use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::hash::{Hash, Hasher};

use ahash::{AHashMap, AHashSet};
use helm_schema_core::ValuesPath;
use std::rc::Rc;

use super::{
    Arena, Datum, Field, FrozenTest, FrozenValue, Heap, HeapUpdate, HeapVersion, Leaf, MapType,
    Multiplicity, ObjectData, ObjectId, ObjectVersion, Remainder, SnapshotId, Value, ValueNode,
};

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
        Self {
            version: Rc::new(HeapVersion {
                live_versions: super::heap_snapshot::track_version(&arena.live_versions),
                id,
                store: super::heap_store::Store::default(),
                arena: Rc::downgrade(&arena),
                input_paths,
            }),
            arena,
        }
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
            | ObjectData::Widened {
                inputs: _,
                reachable: _,
            } => None,
        };
        let data = if let Some(reason) = reason {
            tracing::debug!(?reason, "widen object");
            tracing::debug_span!(target: "helm_schema_ir::counters::heap_store", "object_widened", reason = ?reason).in_scope(|| {});
            ObjectData::Widened {
                inputs: Rc::new(self.data_inputs(&data)),
                reachable: data_reach(&data),
            }
        } else {
            data
        };
        let classes = self
            .version
            .object_count()
            .saturating_sub(self.arena.instance_object_base.get());
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
            let mut reachable = data_reach(&data);
            if let Some(ObjectVersion {
                data:
                    ObjectData::Widened {
                        inputs: old,
                        reachable: old_reachable,
                    },
                ..
            }) = self.version.object(&id).as_deref()
            {
                inputs.extend(old.iter().cloned());
                reachable = reachable.union(old_reachable);
            }
            self.install(
                id.clone(),
                ObjectVersion {
                    revision: self.arena.snapshot(),
                    multiplicity: Multiplicity::Summary,
                    data: ObjectData::Widened {
                        inputs: Rc::new(inputs),
                        reachable,
                    },
                },
            );
            return id;
        }
        let id = occurrence.cloned().unwrap_or_else(|| self.arena.object());
        self.install(
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
                fields: Rc::default(),
                remainder,
            },
            Multiplicity::Singleton,
        );
        Value::leaf(Datum::Map {
            view: MapType::Interface,
            target: Some(target),
        })
    }

    fn advance(&mut self, node: &HeapUpdate) {
        let id = self.arena.snapshot();
        let store = super::heap_store::Store::advance(&self.version, node, id, &self.arena);
        self.version = Rc::new(HeapVersion {
            live_versions: super::heap_snapshot::track_version(&self.arena.live_versions),
            id,
            store,
            arena: Rc::downgrade(&self.arena),
            input_paths: Rc::clone(&self.arena.input_paths.borrow()),
        });
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
        let id = self.arena.snapshot();
        Rc::new(HeapVersion {
            live_versions: super::heap_snapshot::track_version(&self.arena.live_versions),
            id,
            store: self.version.store.clone(),
            arena: Rc::downgrade(&self.arena),
            input_paths: paths,
        })
    }

    pub(super) fn install(&mut self, id: ObjectId, object: ObjectVersion) {
        self.advance(&HeapUpdate::Install {
            object: id,
            storage: Rc::new(object),
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
            } | ObjectData::Widened {
                inputs: _,
                reachable: _,
            }
        ) {
            return false;
        }
        let field = match object.multiplicity {
            Multiplicity::Singleton => value,
            Multiplicity::Summary => {
                let decision = self.arena.decision(
                    helm_schema_core::Predicate::False,
                    helm_schema_core::Predicate::False,
                    super::Coverage::Partial,
                );
                let mut budget = super::work::WorkBudget::new(super::work::OBJECT_WORK);
                let previous = self.version.field(id, key);
                let selected = field_join_fits(&value, &previous, &mut budget)
                    .then(|| {
                        super::heap_select::field(&decision, value.clone(), previous, &mut budget)
                    })
                    .flatten();
                let Some(field) = selected else {
                    let added = ObjectData::Map {
                        fields: Rc::new(super::ObjectFields::from_iter([(key.to_owned(), value)])),
                        remainder: Remainder::Closed,
                    };
                    self.widen_written_object(id, &object, &added);
                    return true;
                };
                field
            }
        };
        self.advance(&HeapUpdate::Write {
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
            Multiplicity::Summary => {
                let decision = self.arena.decision(
                    helm_schema_core::Predicate::False,
                    helm_schema_core::Predicate::False,
                    super::Coverage::Partial,
                );
                let mut budget = super::work::WorkBudget::new(super::work::OBJECT_WORK);
                let selected = super::state::value_join_fits_with_budget(&value, slot, &mut budget)
                    .then(|| super::heap_select::value(&decision, &value, slot, &mut budget))
                    .flatten();
                let Some(selected) = selected else {
                    self.widen_written_object(id, &object, &ObjectData::Strings(vec![value]));
                    return true;
                };
                selected
            }
        };
        self.advance(&HeapUpdate::WriteSlot {
            object: id.clone(),
            index,
            value,
        });
        true
    }

    pub(super) fn unknown_write(&mut self, inputs: Rc<BTreeSet<ValuesPath>>) {
        let objects = super::Reach::Top;
        self.advance(&HeapUpdate::UnknownWrite { objects, inputs });
    }

    fn widen_written_object(
        &mut self,
        id: &ObjectId,
        previous: &ObjectVersion,
        added: &ObjectData,
    ) {
        let mut inputs = self.data_inputs(&previous.data);
        inputs.extend(self.data_inputs(added));
        let reachable = data_reach(&previous.data).union(&data_reach(added));
        tracing::debug_span!(target: "helm_schema_ir::counters::heap_store", "object_widened", reason = "summary_write_bound", object = id.ordinal).in_scope(|| {});
        self.install(
            id.clone(),
            ObjectVersion {
                revision: self.arena.snapshot(),
                multiplicity: previous.multiplicity,
                data: ObjectData::Widened {
                    inputs: Rc::new(inputs),
                    reachable,
                },
            },
        );
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn freeze(&self, value: &Value) -> FrozenValue {
        let value = self.version.consume(value);
        let heap = if value.depends_on_heap() {
            self.input_snapshot()
        } else {
            Rc::clone(self.arena.empty_snapshot.get_or_init(|| {
                let id = self.arena.snapshot();

                Rc::new(HeapVersion {
                    live_versions: super::heap_snapshot::track_version(&self.arena.live_versions),
                    id,
                    store: super::heap_store::Store::default(),
                    arena: Rc::downgrade(&self.arena),
                    input_paths: Rc::clone(&self.arena.input_paths.borrow()),
                })
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
                    | ObjectData::Widened {
                        inputs: _,
                        reachable: _,
                    },
            })
            | None => Field::Unknown,
        }
    }
}

impl Remainder {
    pub(super) fn missing(&self) -> Field {
        match self {
            Self::Closed => Field::Absent,
            Self::Open | Self::Input | Self::Unknown { inputs: _ } | Self::Layered(_) => {
                Field::Unknown
            }
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
            self.advance(&HeapUpdate::Join {
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
    budget: &mut super::work::WorkBudget,
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
            Ok(join_slots(decision, yes, no, budget))
        }
        (
            ObjectData::Widened {
                inputs: yes,
                reachable: yes_reachable,
            },
            ObjectData::Widened {
                inputs: no,
                reachable: no_reachable,
            },
        ) => Ok(ObjectData::Widened {
            inputs: join_input_sets(yes, no, input_unions, budget),
            reachable: yes_reachable.union(no_reachable),
        }),
        // A map widened on one arm keeps the other arm's fields under the decision: the widened
        // arm's members are unknown and reference what its contents may reference.
        (ObjectData::Widened { inputs, reachable }, ObjectData::Map { fields, remainder })
        | (ObjectData::Map { fields, remainder }, ObjectData::Widened { inputs, reachable }) => {
            Ok(ObjectData::Widened {
                inputs: Rc::clone(inputs),
                reachable: reachable.union(&map_reach(fields, remainder)),
            })
        }
        // Other storage widened on one arm stays widened: its unknown contents may still
        // reference what either arm references.
        (
            ObjectData::Widened { inputs, reachable },
            other @ (ObjectData::Strings(_) | ObjectData::Opaque { reachable: _ }),
        )
        | (
            other @ (ObjectData::Strings(_) | ObjectData::Opaque { reachable: _ }),
            ObjectData::Widened { inputs, reachable },
        ) => Ok(ObjectData::Widened {
            inputs: Rc::clone(inputs),
            reachable: reachable.union(&data_reach(other)),
        }),
        (ObjectData::Opaque { reachable: yes }, ObjectData::Opaque { reachable: no }) => {
            if !budget.charge(
                super::heap_equality::reach_work(yes) + super::heap_equality::reach_work(no),
            ) {
                return Ok(ObjectData::Widened {
                    inputs: Rc::default(),
                    reachable: yes.union(no),
                });
            }
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

fn join_slots(
    decision: &Rc<FrozenTest>,
    yes: &[Value],
    no: &[Value],
    budget: &mut super::work::WorkBudget,
) -> ObjectData {
    if yes
        .iter()
        .zip(no)
        .any(|(yes, no)| !super::state::value_join_fits_with_budget(yes, no, budget))
    {
        return ObjectData::Widened {
            inputs: Rc::default(),
            reachable: slots_reach(yes).union(&slots_reach(no)),
        };
    }
    let Some(slots) = yes
        .iter()
        .zip(no)
        .map(|(yes, no)| super::heap_select::value(decision, yes, no, budget))
        .collect::<Option<Vec<_>>>()
    else {
        return ObjectData::Widened {
            inputs: Rc::default(),
            reachable: slots_reach(yes).union(&slots_reach(no)),
        };
    };
    ObjectData::Strings(slots)
}

pub(super) fn join_input_sets(
    yes: &Rc<BTreeSet<ValuesPath>>,
    no: &Rc<BTreeSet<ValuesPath>>,
    input_unions: Option<&RefCell<InputUnions>>,
    budget: &mut super::work::WorkBudget,
) -> Rc<BTreeSet<ValuesPath>> {
    if !budget.charge(yes.len() + no.len()) {
        return Rc::new(BTreeSet::from([ValuesPath::default()]));
    }
    if Rc::ptr_eq(yes, no) {
        return Rc::clone(yes);
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
    yes: &super::ObjectFields,
    yes_remainder: Remainder,
    no: &super::ObjectFields,
    no_remainder: Remainder,
    budget: &mut super::work::WorkBudget,
) -> ObjectData {
    let keys = yes
        .keys()
        .chain(no.keys())
        .collect::<std::collections::BTreeSet<_>>();
    // A join that cannot keep the fields keeps what they may reference.
    let widened = || ObjectData::Widened {
        inputs: Rc::default(),
        reachable: map_reach(yes, &yes_remainder).union(&map_reach(no, &no_remainder)),
    };
    if keys.len() > 128 {
        return widened();
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
            return widened();
        }
        let Some(field) = super::heap_select::field(decision, yes_field, no_field, budget) else {
            return widened();
        };
        fields.insert(key.clone(), field);
    }
    let remainder = match (yes_remainder, no_remainder) {
        (Remainder::Unknown { inputs: yes }, Remainder::Unknown { inputs: no }) => {
            Remainder::Unknown {
                inputs: join_input_sets(&yes, &no, None, budget),
            }
        }
        (Remainder::Unknown { inputs }, Remainder::Open | Remainder::Closed | Remainder::Input)
        | (Remainder::Open | Remainder::Closed | Remainder::Input, Remainder::Unknown { inputs }) => {
            Remainder::Unknown { inputs }
        }
        (yes @ Remainder::Layered(_), no) | (yes, no @ Remainder::Layered(_)) => {
            super::layers::join_remainders(decision, &yes, &no, budget)
                .unwrap_or_else(|| super::layers::covering_remainder(&yes, &no))
        }
        (Remainder::Closed, Remainder::Closed) => Remainder::Closed,
        (Remainder::Input, Remainder::Input) => Remainder::Input,
        (Remainder::Open, Remainder::Open | Remainder::Closed | Remainder::Input)
        | (Remainder::Closed | Remainder::Input, Remainder::Open)
        | (Remainder::Input, Remainder::Closed)
        | (Remainder::Closed, Remainder::Input) => Remainder::Open,
    };
    ObjectData::Map {
        fields: Rc::new(fields.into()),
        remainder,
    }
}

impl Heap {
    pub(super) fn data_inputs(&self, data: &ObjectData) -> BTreeSet<ValuesPath> {
        super::heap_snapshot::data_inputs(&self.version, data)
    }
}

fn field_join_fits(yes: &Field, no: &Field, budget: &mut super::work::WorkBudget) -> bool {
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

impl std::fmt::Debug for HeapVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HeapVersion")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl super::Reach {
    /// What either reach may reference.
    pub(super) fn union(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Bounded(left), Self::Bounded(right)) => {
                if right.is_subset(left) {
                    self.clone()
                } else if left.is_subset(right) {
                    other.clone()
                } else {
                    Self::Bounded(Rc::new(left.union(right).cloned().collect()))
                }
            }
            (Self::Top, Self::Bounded(_) | Self::Top) | (Self::Bounded(_), Self::Top) => Self::Top,
        }
    }
}

/// Records the objects `value` references directly and queues the values it holds; false when
/// an unknown part may reference objects nothing records.
pub(super) fn value_references(
    value: &Value,
    pending: &mut Vec<Value>,
    targets: &mut BTreeSet<ObjectId>,
) -> bool {
    match value.node.as_ref() {
        super::ValueNode::Select {
            decision: _,
            yes,
            no,
        } => {
            pending.extend([yes.clone(), no.clone()]);
        }
        super::ValueNode::Leaf(super::Leaf { datum, origin: _ }) => match datum {
            super::Datum::Text(_)
            | super::Datum::Scalar {
                go_type: _,
                content: _,
            }
            | super::Datum::Map {
                view: _,
                target: None,
            } => {}
            super::Datum::Map {
                view: _,
                target: Some(id),
            }
            | super::Datum::Slot {
                backing: id,
                index: _,
            } => {
                targets.insert(id.clone());
            }
            super::Datum::Strings(slice) => {
                targets.insert(slice.backing.clone());
            }
            super::Datum::Sequence(sequence) => pending.extend(
                sequence
                    .storage
                    .iter()
                    .flat_map(|items| items.iter().cloned()),
            ),
            super::Datum::Unknown(unknown) => match &unknown.aliases {
                super::Reach::Bounded(ids) => targets.extend(ids.iter().cloned()),
                super::Reach::Top => return false,
            },
        },
    }
    true
}

/// The objects the values in `values` may reference.
pub(super) fn values_reach<'a>(values: impl IntoIterator<Item = &'a Value>) -> super::Reach {
    let mut pending: Vec<Value> = values.into_iter().cloned().collect();
    let mut targets = BTreeSet::new();
    while let Some(value) = pending.pop() {
        if !value_references(&value, &mut pending, &mut targets) {
            return super::Reach::Top;
        }
    }
    super::Reach::Bounded(Rc::new(targets))
}

fn slots_reach(slots: &[Value]) -> super::Reach {
    values_reach(slots)
}

fn map_reach(fields: &super::ObjectFields, remainder: &Remainder) -> super::Reach {
    let unwritten = match remainder {
        // Unwritten keys hold the input map's own members, reached through it.
        Remainder::Closed | Remainder::Open | Remainder::Input => {
            super::Reach::Bounded(Rc::default())
        }
        Remainder::Layered(layers) => layers.references(),
        Remainder::Unknown { inputs: _ } => return super::Reach::Top,
    };
    if unwritten == super::Reach::Top {
        return super::Reach::Top;
    }
    let mut values = Vec::new();
    let mut pending: Vec<&Field> = fields.values().collect();
    while let Some(field) = pending.pop() {
        match field {
            Field::Absent | Field::Unknown => {}
            Field::Present(value) => values.push(value),
            Field::Select {
                decision: _,
                yes,
                no,
            } => pending.extend([yes.as_ref(), no.as_ref()]),
        }
    }
    values_reach(values).union(&unwritten)
}

/// The objects the contents of `data` may reference: what a summary of them keeps.
pub(super) fn data_reach(data: &ObjectData) -> super::Reach {
    match data {
        ObjectData::Map { fields, remainder } => map_reach(fields, remainder),
        ObjectData::Strings(slots) => slots_reach(slots),
        ObjectData::Opaque { reachable }
        | ObjectData::Widened {
            inputs: _,
            reachable,
        } => reachable.clone(),
    }
}

/// A member of these values: reached through them, it references only what they may.
pub(super) fn unknown_member(values: &[Value], inputs: BTreeSet<ValuesPath>) -> Value {
    Value::with_influences(
        Value::leaf(super::Datum::Unknown(super::UnknownValue {
            types: super::TypeCoverage::Unresolved,
            aliases: values_reach(values),
        })),
        inputs,
    )
}
