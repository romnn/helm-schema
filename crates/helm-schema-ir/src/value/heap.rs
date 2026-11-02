use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::hash::{Hash, Hasher};

use ahash::{AHashMap, AHashSet};
use helm_schema_core::ValuesPath;
use std::rc::Rc;

use super::{
    Arena, Datum, Field, FrozenTest, FrozenValue, Heap, HeapNode, HeapVersion, Leaf, MapType,
    Multiplicity, ObjectData, ObjectId, ObjectVersion, Remainder, SnapshotId, Value, ValueNode,
};

impl HeapNode {
    pub(super) fn parents(&self) -> impl Iterator<Item = &Rc<HeapVersion>> {
        let (first, second) = match self {
            Self::Empty => (None, None),
            Self::Registration { parent, paths: _ }
            | Self::Install {
                parent,
                object: _,
                storage: _,
                kind: _,
            }
            | Self::Write {
                parent,
                object: _,
                key: _,
                field: _,
            }
            | Self::WriteSlot {
                parent,
                object: _,
                index: _,
                value: _,
            }
            | Self::UnknownWrite {
                parent,
                objects: _,
                inputs: _,
            } => (Some(parent), None),
            Self::Join {
                decision: _,
                yes,
                no,
            } => (Some(yes), Some(no)),
        };
        first.into_iter().chain(second)
    }
}

impl Arena {
    pub(super) fn object(&self) -> ObjectId {
        let ordinal = self.next_object.get();
        self.next_object.set(ordinal + 1);
        ObjectId { ordinal }
    }

    pub(super) fn snapshot(&self) -> SnapshotId {
        let ordinal = self.next_snapshot.get();
        self.next_snapshot.set(ordinal + 1);
        SnapshotId { ordinal }
    }
}

impl Multiplicity {
    pub(super) fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Singleton, Self::Singleton) => Self::Singleton,
            (Self::Summary, Self::Singleton | Self::Summary) | (Self::Singleton, Self::Summary) => {
                Self::Summary
            }
        }
    }
}

impl Heap {
    /// A heap whose session normalizes predicates through `predicate_memo`.
    pub(super) fn with_predicate_memo(predicate_memo: Rc<helm_schema_core::PredicateMemo>) -> Self {
        let arena = Rc::new(Arena {
            predicate_memo,
            ..Arena::default()
        });
        let id = arena.snapshot();
        let input_paths = Rc::clone(&arena.input_paths.borrow());
        let heap = Self {
            version: Rc::new(HeapVersion {
                id,
                allocations: super::heap_headers::AllocationIndex::default(),
                arena: Rc::downgrade(&arena),
                input_paths,
                node: HeapNode::Empty,
                index: super::heap_index::HeapIndex::default(),
                borrowed_objects: super::heap_query::BorrowedObjects::default(),
            }),
            arena,
        };
        super::stage0_diagnostics::heap(&heap.version);
        heap
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn fork(&self) -> Self {
        Self {
            version: Rc::clone(&self.version),
            arena: Rc::clone(&self.arena),
        }
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn allocate(&mut self, data: ObjectData, multiplicity: Multiplicity) -> ObjectId {
        self.allocate_at(None, data, multiplicity)
    }

    pub(super) fn allocate_at(
        &mut self,
        occurrence: Option<&ObjectId>,
        data: ObjectData,
        multiplicity: Multiplicity,
    ) -> ObjectId {
        let reason = match &data {
            ObjectData::Map {
                fields,
                remainder: _,
            } if fields.len() > 128 => Some(super::BoundReason::ObjectFields),
            ObjectData::Strings(slots) if slots.len() > 128 => {
                Some(super::BoundReason::StringSlots)
            }
            ObjectData::Map {
                fields: _,
                remainder: _,
            }
            | ObjectData::Strings(_)
            | ObjectData::Opaque { reachable: _ }
            | ObjectData::Widened { inputs: _ } => None,
        };
        let data = if let Some(reason) = reason {
            tracing::debug!(?reason, "widen object");
            ObjectData::Widened {
                inputs: Rc::new(self.data_inputs(&data)),
            }
        } else {
            data
        };
        let classes = self.version.object_count();
        let existing = occurrence.is_some_and(|id| self.version.has_allocated_object(id));
        if !existing && classes >= 4096 {
            tracing::debug!(reason = ?super::BoundReason::ObjectClasses, "widen allocation");
            let ordinal = if let Some(ordinal) = self.arena.aggregate_summary.get() {
                ordinal
            } else {
                let ordinal = self.arena.next_object.get();
                self.arena.next_object.set(ordinal + 1);
                self.arena.aggregate_summary.set(Some(ordinal));
                ordinal
            };
            let id = ObjectId { ordinal };
            let mut inputs = self.data_inputs(&data);
            if let Some(ObjectVersion {
                data: ObjectData::Widened { inputs: old },
                ..
            }) = self.version.object(&id).as_deref()
            {
                inputs.extend(old.iter().cloned());
            }
            let kind = if self.version.has_allocated_object(&id) {
                super::InstallKind::Mutation
            } else {
                super::InstallKind::Baseline
            };
            self.install_with_kind(
                kind,
                id.clone(),
                ObjectVersion {
                    revision: self.arena.snapshot(),
                    multiplicity: Multiplicity::Summary,
                    data: ObjectData::Widened {
                        inputs: Rc::new(inputs),
                    },
                },
            );
            return id;
        }
        let id = occurrence.cloned().unwrap_or_else(|| self.arena.object());
        let kind = if existing {
            super::InstallKind::Mutation
        } else {
            super::InstallKind::Baseline
        };
        self.install_with_kind(
            kind,
            id.clone(),
            ObjectVersion {
                revision: self.arena.snapshot(),
                multiplicity,
                data,
            },
        );
        id
    }

    pub(super) fn map(&mut self, remainder: Remainder) -> Value {
        let target = self.allocate(
            ObjectData::Map {
                fields: Rc::new(BTreeMap::new()),
                remainder,
            },
            Multiplicity::Singleton,
        );
        Value::leaf(Datum::Map {
            view: MapType::Interface,
            target: Some(target),
        })
    }

    fn advance(&mut self, node: HeapNode) {
        let id = self.arena.snapshot();
        let allocations = super::heap_headers::AllocationIndex::for_node(&node);
        let index = super::heap_index::HeapIndex::for_node(&node, &self.arena.input_paths.borrow());
        self.version = Rc::new(HeapVersion {
            id,
            allocations,
            arena: Rc::downgrade(&self.arena),
            input_paths: Rc::clone(&self.arena.input_paths.borrow()),
            node,
            index,
            borrowed_objects: super::heap_query::BorrowedObjects::default(),
        });
        super::stage0_diagnostics::heap(&self.version);
    }

    pub(super) fn register_input(&mut self, object: ObjectId, path: ValuesPath) {
        let mut paths = self.arena.input_paths.borrow_mut();
        if paths.get(&object) != Some(&path) {
            Rc::make_mut(&mut paths).insert(object, path);
        }
        drop(paths);
        self.refresh_input_paths();
    }

    fn refresh_input_paths(&mut self) {
        self.version = self.input_snapshot();
    }

    fn input_snapshot(&self) -> Rc<HeapVersion> {
        let paths = Rc::clone(&self.arena.input_paths.borrow());
        if Rc::ptr_eq(&paths, &self.version.input_paths) {
            return Rc::clone(&self.version);
        }
        let node = HeapNode::Registration {
            parent: Rc::clone(&self.version),
            paths: Rc::clone(&paths),
        };
        let id = self.arena.snapshot();
        let index = super::heap_index::HeapIndex::for_node(&node, &paths);
        let version = Rc::new(HeapVersion {
            id,
            allocations: self.version.allocations.clone(),
            arena: Rc::downgrade(&self.arena),
            input_paths: paths,
            node,
            index,
            borrowed_objects: super::heap_query::BorrowedObjects::default(),
        });
        super::stage0_diagnostics::heap(&version);
        version
    }

    pub(super) fn install(&mut self, id: ObjectId, object: ObjectVersion) {
        self.install_with_kind(super::InstallKind::Mutation, id, object);
    }

    pub(super) fn register_storage(&mut self, id: ObjectId, object: ObjectVersion) {
        self.install_with_kind(super::InstallKind::Baseline, id, object);
    }

    fn install_with_kind(&mut self, kind: super::InstallKind, id: ObjectId, object: ObjectVersion) {
        self.advance(HeapNode::Install {
            parent: Rc::clone(&self.version),
            object: id,
            storage: Rc::new(object),
            kind,
        });
    }

    pub(super) fn write(&mut self, id: &ObjectId, key: &str, value: Field) -> bool {
        let Some(object) = self.version.object(id) else {
            return false;
        };
        if !matches!(
            object.data,
            ObjectData::Map {
                fields: _,
                remainder: _
            } | ObjectData::Widened { inputs: _ }
        ) {
            return false;
        }
        let field = match object.multiplicity {
            Multiplicity::Singleton => value,
            Multiplicity::Summary => Field::select(
                self.arena.decision(
                    helm_schema_core::Predicate::False,
                    helm_schema_core::Predicate::False,
                    super::Coverage::Partial,
                ),
                value,
                self.version.field(id, key),
            ),
        };
        self.advance(HeapNode::Write {
            parent: Rc::clone(&self.version),
            object: id.clone(),
            key: key.to_owned(),
            field,
        });
        true
    }

    pub(super) fn write_slot(&mut self, id: &ObjectId, index: usize, value: Value) -> bool {
        let Some(object) = self.version.object(id) else {
            return false;
        };
        let ObjectData::Strings(slots) = &object.data else {
            return false;
        };
        let Some(slot) = slots.get(index) else {
            return false;
        };
        let value = match object.multiplicity {
            Multiplicity::Singleton => value,
            Multiplicity::Summary => Value::select(
                self.arena.decision(
                    helm_schema_core::Predicate::False,
                    helm_schema_core::Predicate::False,
                    super::Coverage::Partial,
                ),
                value,
                slot.clone(),
            ),
        };
        self.advance(HeapNode::WriteSlot {
            parent: Rc::clone(&self.version),
            object: id.clone(),
            index,
            value,
        });
        true
    }

    pub(super) fn unknown_write(&mut self, inputs: Rc<BTreeSet<ValuesPath>>) {
        let objects = super::Reach::Top;
        self.advance(HeapNode::UnknownWrite {
            parent: Rc::clone(&self.version),
            objects,
            inputs,
        });
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn freeze(&self, value: &Value) -> FrozenValue {
        let value = self.version.consume(value);
        let heap = if value.depends_on_heap() {
            self.input_snapshot()
        } else {
            Rc::clone(self.arena.empty_snapshot.get_or_init(|| {
                let id = self.arena.snapshot();
                let version = Rc::new(HeapVersion {
                    id,
                    allocations: super::heap_headers::AllocationIndex::default(),
                    arena: Rc::downgrade(&self.arena),
                    input_paths: Rc::clone(&self.arena.input_paths.borrow()),
                    node: HeapNode::Empty,
                    index: super::heap_index::HeapIndex::default(),
                    borrowed_objects: super::heap_query::BorrowedObjects::default(),
                });
                super::stage0_diagnostics::heap(&version);
                version
            }))
        };
        FrozenValue::at(heap, value)
    }
}

impl FrozenValue {
    pub(super) fn at(heap: Rc<HeapVersion>, value: Value) -> Self {
        Self { heap, value }
    }
}

impl Value {
    fn depends_on_heap(&self) -> bool {
        let mut pending = Vec::new();
        let mut next = Some(self);
        let mut seen = AHashSet::new();
        while let Some(value) = next.take().or_else(|| pending.pop()) {
            match value.node.as_ref() {
                ValueNode::Select {
                    decision: _,
                    yes,
                    no,
                } => {
                    if seen.insert(Rc::as_ptr(&value.node)) {
                        next = Some(yes);
                        pending.push(no);
                    }
                }
                ValueNode::Leaf(Leaf { datum, origin: _ }) => match datum {
                    Datum::Text(_)
                    | Datum::Scalar {
                        go_type: _,
                        content: _,
                    }
                    | Datum::Map {
                        view: _,
                        target: None,
                    } => {}
                    Datum::Sequence(super::InterfaceSequence {
                        storage,
                        start: _,
                        length: _,
                        capacity: _,
                    }) => {
                        if let Some(storage) = storage
                            && seen.insert(Rc::as_ptr(&value.node))
                        {
                            pending.extend(storage.iter());
                        }
                    }
                    Datum::Unknown(super::UnknownValue { types: _, aliases }) => match aliases {
                        super::Reach::Bounded(objects) if objects.is_empty() => {}
                        super::Reach::Bounded(_) | super::Reach::Top => return true,
                    },
                    Datum::Map {
                        view: _,
                        target: Some(_),
                    }
                    | Datum::Strings(_)
                    | Datum::Slot {
                        backing: _,
                        index: _,
                    } => return true,
                },
            }
        }
        false
    }
}

impl HeapVersion {
    pub(super) fn field(&self, id: &ObjectId, key: &str) -> Field {
        match self.object(id).as_deref() {
            Some(ObjectVersion {
                revision: _,
                multiplicity: _,
                data: ObjectData::Map { fields, remainder },
            }) => fields
                .get(key)
                .cloned()
                .unwrap_or_else(|| remainder.missing()),
            Some(ObjectVersion {
                revision: _,
                multiplicity: _,
                data:
                    ObjectData::Strings(_)
                    | ObjectData::Opaque { reachable: _ }
                    | ObjectData::Widened { inputs: _ },
            })
            | None => Field::Unknown,
        }
    }
}

impl Remainder {
    pub(super) fn missing(&self) -> Field {
        match self {
            Self::Closed => Field::Absent,
            Self::Open | Self::Input | Self::Unknown { inputs: _ } => Field::Unknown,
        }
    }
}

impl Heap {
    #[tracing::instrument(skip_all)]
    pub(super) fn join(
        &mut self,
        decision: &Rc<FrozenTest>,
        yes: &Self,
        no: &Self,
    ) -> Result<(), JoinError> {
        if !Rc::ptr_eq(&self.arena, &yes.arena) || !Rc::ptr_eq(&self.arena, &no.arena) {
            return Err(JoinError::DifferentSessions);
        }
        if Rc::ptr_eq(&yes.version, &no.version) {
            self.version = Rc::clone(&yes.version);
        } else {
            self.advance(HeapNode::Join {
                decision: Rc::clone(decision),
                yes: Rc::clone(&yes.version),
                no: Rc::clone(&no.version),
            });
        }
        self.refresh_input_paths();
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum JoinError {
    #[error("cannot join heaps from different sessions")]
    DifferentSessions,
    #[error("object storage changed across a fork")]
    StorageChanged,
}

#[derive(Clone)]
struct InputPair {
    yes: Rc<BTreeSet<ValuesPath>>,
    no: Rc<BTreeSet<ValuesPath>>,
}

impl PartialEq for InputPair {
    fn eq(&self, other: &Self) -> bool {
        let Self { yes, no } = self;
        let Self {
            yes: other_yes,
            no: other_no,
        } = other;
        Rc::ptr_eq(yes, other_yes) && Rc::ptr_eq(no, other_no)
    }
}

impl Eq for InputPair {}

impl Hash for InputPair {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self { yes, no } = self;
        Rc::as_ptr(yes).hash(state);
        Rc::as_ptr(no).hash(state);
    }
}

#[derive(Default)]
pub(super) struct InputUnions {
    entries: AHashMap<InputPair, std::rc::Weak<BTreeSet<ValuesPath>>>,
    order: VecDeque<InputPair>,
    payload: usize,
}

impl std::fmt::Debug for InputUnions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InputUnions")
            .field("entries", &self.entries.len())
            .field("payload", &self.payload)
            .finish_non_exhaustive()
    }
}

impl InputUnions {
    fn union(
        &mut self,
        yes: &Rc<BTreeSet<ValuesPath>>,
        no: &Rc<BTreeSet<ValuesPath>>,
    ) -> Rc<BTreeSet<ValuesPath>> {
        let key = InputPair {
            yes: Rc::clone(yes),
            no: Rc::clone(no),
        };
        if let Some(entry) = self.entries.get_mut(&key) {
            if let Some(value) = entry.upgrade() {
                return value;
            }
            let value = union_inputs(yes, no);
            *entry = Rc::downgrade(&value);
            return value;
        }
        let weight = yes.len() + no.len();
        let value = union_inputs(yes, no);
        if weight <= 262_144 {
            while self.entries.len() >= 4096 || self.payload + weight > 262_144 {
                let Some(key) = self.order.pop_front() else {
                    break;
                };
                self.entries.remove(&key);
                self.payload -= key.yes.len() + key.no.len();
            }
            self.payload += weight;
            self.entries.insert(key.clone(), Rc::downgrade(&value));
            self.order.push_back(key);
        }
        value
    }
}

pub(super) fn join_data(
    decision: &Rc<FrozenTest>,
    yes: &ObjectData,
    no: &ObjectData,
    input_unions: Option<&RefCell<InputUnions>>,
    budget: &mut super::heap_query::QueryBudget,
) -> Result<ObjectData, JoinError> {
    match (yes, no) {
        (
            ObjectData::Map {
                fields: yes,
                remainder: yes_remainder,
            },
            ObjectData::Map {
                fields: no,
                remainder: no_remainder,
            },
        ) => Ok(join_map_data(
            decision,
            yes,
            yes_remainder.clone(),
            no,
            no_remainder.clone(),
            budget,
        )),
        (ObjectData::Strings(yes), ObjectData::Strings(no)) if yes.len() == no.len() => {
            if yes
                .iter()
                .zip(no)
                .any(|(yes, no)| !super::state::value_join_fits_with_budget(yes, no, budget))
            {
                return Ok(ObjectData::Widened {
                    inputs: Rc::default(),
                });
            }
            Ok(ObjectData::Strings(
                yes.iter()
                    .zip(no)
                    .map(|(yes, no)| Value::select(Rc::clone(decision), yes.clone(), no.clone()))
                    .collect(),
            ))
        }
        (ObjectData::Widened { inputs: yes }, ObjectData::Widened { inputs: no }) => {
            Ok(ObjectData::Widened {
                inputs: join_input_sets(yes, no, input_unions, budget),
            })
        }
        (
            ObjectData::Widened { inputs },
            ObjectData::Map {
                fields: _,
                remainder: _,
            }
            | ObjectData::Strings(_)
            | ObjectData::Opaque { reachable: _ },
        )
        | (
            ObjectData::Map {
                fields: _,
                remainder: _,
            }
            | ObjectData::Strings(_)
            | ObjectData::Opaque { reachable: _ },
            ObjectData::Widened { inputs },
        ) => Ok(ObjectData::Widened {
            inputs: Rc::clone(inputs),
        }),
        (ObjectData::Opaque { reachable: yes }, ObjectData::Opaque { reachable: no }) => {
            Ok(ObjectData::Opaque {
                reachable: match (yes, no) {
                    (super::Reach::Bounded(yes), super::Reach::Bounded(no)) => {
                        super::Reach::Bounded(Rc::new(yes.union(no).cloned().collect()))
                    }
                    (super::Reach::Top, super::Reach::Bounded(_) | super::Reach::Top)
                    | (super::Reach::Bounded(_), super::Reach::Top) => super::Reach::Top,
                },
            })
        }
        (
            ObjectData::Opaque { reachable: _ } | ObjectData::Strings(_),
            ObjectData::Map {
                fields: _,
                remainder: _,
            }
            | ObjectData::Strings(_),
        )
        | (
            ObjectData::Map {
                fields: _,
                remainder: _,
            }
            | ObjectData::Strings(_),
            ObjectData::Opaque { reachable: _ },
        )
        | (
            ObjectData::Map {
                fields: _,
                remainder: _,
            },
            ObjectData::Strings(_),
        ) => Err(JoinError::StorageChanged),
    }
}

pub(super) fn join_input_sets(
    yes: &Rc<BTreeSet<ValuesPath>>,
    no: &Rc<BTreeSet<ValuesPath>>,
    input_unions: Option<&RefCell<InputUnions>>,
    budget: &mut super::heap_query::QueryBudget,
) -> Rc<BTreeSet<ValuesPath>> {
    if Rc::ptr_eq(yes, no) {
        return Rc::clone(yes);
    }
    if !budget.charge(yes.len() + no.len()) {
        return Rc::new(BTreeSet::from([ValuesPath::default()]));
    }
    if let Some(input_unions) = input_unions {
        input_unions.borrow_mut().union(yes, no)
    } else {
        union_inputs(yes, no)
    }
}

fn union_inputs(
    yes: &Rc<BTreeSet<ValuesPath>>,
    no: &Rc<BTreeSet<ValuesPath>>,
) -> Rc<BTreeSet<ValuesPath>> {
    if no.is_subset(yes) {
        Rc::clone(yes)
    } else if yes.is_subset(no) {
        Rc::clone(no)
    } else {
        Rc::new(yes.union(no).cloned().collect())
    }
}

fn join_map_data(
    decision: &Rc<FrozenTest>,
    yes: &BTreeMap<String, Field>,
    yes_remainder: Remainder,
    no: &BTreeMap<String, Field>,
    no_remainder: Remainder,
    budget: &mut super::heap_query::QueryBudget,
) -> ObjectData {
    let keys = yes
        .keys()
        .chain(no.keys())
        .collect::<std::collections::BTreeSet<_>>();
    if keys.len() > 128 {
        return ObjectData::Widened {
            inputs: Rc::default(),
        };
    }
    let mut fields = BTreeMap::new();
    for key in keys {
        let yes_field = yes
            .get(key)
            .cloned()
            .unwrap_or_else(|| yes_remainder.missing());
        let no_field = no
            .get(key)
            .cloned()
            .unwrap_or_else(|| no_remainder.missing());
        if !field_join_fits(&yes_field, &no_field, budget) {
            return ObjectData::Widened {
                inputs: Rc::default(),
            };
        }
        fields.insert(
            key.clone(),
            Field::select(Rc::clone(decision), yes_field, no_field),
        );
    }
    let remainder = match (yes_remainder, no_remainder) {
        (Remainder::Unknown { inputs: yes }, Remainder::Unknown { inputs: no }) => {
            Remainder::Unknown {
                inputs: Rc::new(yes.union(&no).cloned().collect()),
            }
        }
        (Remainder::Unknown { inputs }, Remainder::Open | Remainder::Closed | Remainder::Input)
        | (Remainder::Open | Remainder::Closed | Remainder::Input, Remainder::Unknown { inputs }) => {
            Remainder::Unknown { inputs }
        }
        (Remainder::Closed, Remainder::Closed) => Remainder::Closed,
        (Remainder::Input, Remainder::Input) => Remainder::Input,
        (Remainder::Open, Remainder::Open | Remainder::Closed | Remainder::Input)
        | (Remainder::Closed | Remainder::Input, Remainder::Open)
        | (Remainder::Input, Remainder::Closed)
        | (Remainder::Closed, Remainder::Input) => Remainder::Open,
    };
    ObjectData::Map {
        fields: Rc::new(fields),
        remainder,
    }
}

impl Heap {
    pub(super) fn data_inputs(&self, data: &ObjectData) -> BTreeSet<ValuesPath> {
        super::heap_query::data_inputs(&self.version, data)
    }
}

fn field_join_fits(yes: &Field, no: &Field, budget: &mut super::heap_query::QueryBudget) -> bool {
    if let (Field::Present(yes), Field::Present(no)) = (yes, no)
        && Rc::ptr_eq(&yes.node, &no.node)
    {
        return true;
    }
    let mut pending = vec![yes, no];
    let mut leaves = 0;
    while let Some(field) = pending.pop() {
        if !budget.charge(4) {
            return false;
        }
        match field {
            Field::Absent | Field::Unknown => {
                leaves += 1;
                if leaves > 128 {
                    return false;
                }
            }
            Field::Select {
                decision: _,
                yes,
                no,
            } => {
                pending.push(yes);
                pending.push(no);
            }
            Field::Present(value) => {
                let mut values = vec![value];
                while let Some(value) = values.pop() {
                    if !budget.charge(4) {
                        return false;
                    }
                    match value.node.as_ref() {
                        ValueNode::Leaf(Leaf {
                            datum: _,
                            origin: _,
                        }) => {
                            leaves += 1;
                            if leaves > 128 {
                                return false;
                            }
                        }
                        ValueNode::Select {
                            decision: _,
                            yes,
                            no,
                        } => {
                            values.push(yes);
                            values.push(no);
                        }
                    }
                }
            }
        }
    }
    true
}
