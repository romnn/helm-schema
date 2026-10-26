use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::ValuesPath;
use std::rc::Rc;

use super::{
    Arena, Datum, Field, FrozenTest, FrozenValue, Heap, HeapVersion, Leaf, MapType, Multiplicity,
    ObjectData, ObjectId, ObjectVersion, Remainder, SnapshotId, Value, ValueNode,
};

const OBJECTS_PER_PAGE: u64 = 64;

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

impl Heap {
    /// A heap whose session normalizes predicates through `predicate_memo`.
    pub(super) fn with_predicate_memo(predicate_memo: Rc<helm_schema_core::PredicateMemo>) -> Self {
        let arena = Rc::new(Arena {
            predicate_memo,
            ..Arena::default()
        });
        Self {
            version: Rc::new(HeapVersion {
                id: arena.snapshot(),
                pages: BTreeMap::new(),
            }),
            arena,
        }
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn fork(&self) -> Self {
        Self {
            version: Rc::new(self.version.as_ref().clone()),
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
        let classes = self
            .version
            .pages
            .values()
            .map(|page| page.len())
            .sum::<usize>();
        if classes >= 4096 {
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
            }) = self.version.object(&id)
            {
                inputs.extend(old.iter().cloned());
            }
            self.install(
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
                fields: BTreeMap::new(),
                remainder,
            },
            Multiplicity::Singleton,
        );
        Value::leaf(Datum::Map {
            view: MapType::Interface,
            target: Some(target),
        })
    }

    pub(super) fn install(&mut self, id: ObjectId, object: ObjectVersion) {
        let version = Rc::make_mut(&mut self.version);
        version.id = self.arena.snapshot();
        let page = version
            .pages
            .entry(id.ordinal / OBJECTS_PER_PAGE)
            .or_default();
        Rc::make_mut(page).insert(id, Rc::new(object));
    }

    pub(super) fn write(&mut self, id: &ObjectId, key: &str, value: Field) -> bool {
        let Some(object) = self.version.object(id) else {
            return false;
        };
        if let ObjectData::Widened { inputs } = &object.data {
            let mut inputs = Rc::clone(inputs);
            if let Field::Present(value) = &value {
                let added = self.freeze(value).inputs();
                if !added.is_subset(&inputs) {
                    Rc::make_mut(&mut inputs).extend(added);
                }
            }
            let multiplicity = object.multiplicity;
            self.install(
                id.clone(),
                ObjectVersion {
                    revision: self.arena.snapshot(),
                    multiplicity,
                    data: ObjectData::Widened { inputs },
                },
            );
            return true;
        }
        let ObjectVersion {
            revision: _,
            multiplicity,
            data,
        } = object;
        let ObjectData::Map { fields, remainder } = data else {
            return false;
        };
        let mut fields = fields.clone();
        let value = match multiplicity {
            Multiplicity::Singleton => value,
            Multiplicity::Summary => Field::select(
                self.arena.decision(
                    helm_schema_core::Predicate::False,
                    helm_schema_core::Predicate::False,
                    super::Coverage::Partial,
                ),
                value,
                fields
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| remainder.missing()),
            ),
        };
        fields.insert(key.to_owned(), value);
        if fields.len() > 128 {
            tracing::debug!(reason = ?super::BoundReason::ObjectFields, "widen map write");
            let inputs = self.data_inputs(&ObjectData::Map {
                fields,
                remainder: *remainder,
            });
            let multiplicity = *multiplicity;
            self.install(
                id.clone(),
                ObjectVersion {
                    revision: self.arena.snapshot(),
                    multiplicity,
                    data: ObjectData::Widened {
                        inputs: Rc::new(inputs),
                    },
                },
            );
            return true;
        }
        let object = ObjectVersion {
            revision: self.arena.snapshot(),
            multiplicity: *multiplicity,
            data: ObjectData::Map {
                fields,
                remainder: *remainder,
            },
        };
        self.install(id.clone(), object);
        true
    }

    pub(super) fn write_slot(&mut self, id: &ObjectId, index: usize, value: Value) -> bool {
        let Some(object) = self.version.object(id) else {
            return false;
        };
        let ObjectVersion {
            revision: _,
            multiplicity,
            data,
        } = object;
        let ObjectData::Strings(slots) = data else {
            return false;
        };
        let mut slots = slots.clone();
        let Some(slot) = slots.get_mut(index) else {
            return false;
        };
        *slot = match multiplicity {
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
        self.install(
            id.clone(),
            ObjectVersion {
                revision: self.arena.snapshot(),
                multiplicity: *multiplicity,
                data: ObjectData::Strings(slots),
            },
        );
        true
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn freeze(&self, value: &Value) -> FrozenValue {
        let value = self.version.consume(value);
        let heap = if value.depends_on_heap() {
            Rc::clone(&self.version)
        } else {
            Rc::clone(self.arena.empty_snapshot.get_or_init(|| {
                Rc::new(HeapVersion {
                    id: self.arena.snapshot(),
                    pages: BTreeMap::new(),
                })
            }))
        };
        FrozenValue { heap, value }
    }
}

impl Value {
    fn depends_on_heap(&self) -> bool {
        let mut pending = vec![self];
        let mut seen = BTreeSet::new();
        while let Some(value) = pending.pop() {
            if !seen.insert(Rc::as_ptr(&value.node)) {
                continue;
            }
            match value.node.as_ref() {
                ValueNode::Select {
                    decision: _,
                    yes,
                    no,
                } => pending.extend([yes, no]),
                ValueNode::Leaf(Leaf { datum, origin: _ }) => match datum {
                    Datum::Scalar {
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
                        if let Some(storage) = storage {
                            pending.extend(storage.iter());
                        }
                    }
                    Datum::Unknown(super::UnknownValue { types: _, aliases }) => match aliases {
                        super::AliasSet::Known(objects) if objects.is_empty() => {}
                        super::AliasSet::Known(_) | super::AliasSet::RenderGraph => return true,
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
    pub(super) fn object(&self, id: &ObjectId) -> Option<&ObjectVersion> {
        self.pages
            .get(&(id.ordinal / OBJECTS_PER_PAGE))?
            .get(id)
            .map(Rc::as_ref)
    }

    pub(super) fn field(&self, id: &ObjectId, key: &str) -> Field {
        match self.object(id) {
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

    pub(super) fn consume(&self, value: &Value) -> Value {
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => {
                let consumed_yes = self.consume(yes);
                let consumed_no = self.consume(no);
                if Rc::ptr_eq(&yes.node, &consumed_yes.node)
                    && Rc::ptr_eq(&no.node, &consumed_no.node)
                {
                    value.clone()
                } else {
                    Value::select(Rc::clone(decision), consumed_yes, consumed_no)
                }
            }
            ValueNode::Leaf(Leaf {
                datum: Datum::Slot { backing, index },
                origin: _,
            }) => match self.object(backing) {
                Some(ObjectVersion {
                    revision: _,
                    multiplicity: _,
                    data: ObjectData::Strings(slots),
                }) => slots.get(*index).cloned().unwrap_or_else(Value::unknown),
                Some(ObjectVersion {
                    revision: _,
                    multiplicity: _,
                    data: ObjectData::Widened { inputs },
                }) => Value {
                    node: Rc::new(ValueNode::Leaf(Leaf {
                        datum: Datum::Scalar {
                            go_type: super::ScalarType::String,
                            content: None,
                        },
                        origin: super::Origin {
                            identity: None,
                            influences: inputs.as_ref().clone(),
                        },
                    })),
                },
                Some(ObjectVersion {
                    revision: _,
                    multiplicity: _,
                    data:
                        ObjectData::Map {
                            fields: _,
                            remainder: _,
                        }
                        | ObjectData::Opaque { reachable: _ },
                })
                | None => Value::unknown(),
            },
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Scalar {
                        go_type: _,
                        content: _,
                    }
                    | Datum::Map { view: _, target: _ }
                    | Datum::Sequence(_)
                    | Datum::Strings(_)
                    | Datum::Unknown(_),
                origin: _,
            }) => value.clone(),
        }
    }
}

impl Remainder {
    pub(super) fn missing(self) -> Field {
        match self {
            Self::Closed => Field::Absent,
            Self::Open | Self::Input => Field::Unknown,
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
        let Heap {
            version: yes_version,
            arena: yes_arena,
        } = yes;
        let Heap {
            version: no_version,
            arena: no_arena,
        } = no;
        let HeapVersion {
            id: yes_id,
            pages: yes_pages,
        } = yes_version.as_ref();
        let HeapVersion {
            id: no_id,
            pages: no_pages,
        } = no_version.as_ref();
        let Heap {
            version: _,
            arena: target_arena,
        } = &*self;
        if !Rc::ptr_eq(target_arena, yes_arena) || !Rc::ptr_eq(target_arena, no_arena) {
            return Err(JoinError::DifferentSessions);
        }
        if yes_id == no_id {
            self.version = Rc::clone(yes_version);
            return Ok(());
        }
        let page_ids = yes_pages
            .keys()
            .chain(no_pages.keys())
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let mut input_unions = BTreeMap::new();
        for page_id in page_ids {
            if yes_pages
                .get(&page_id)
                .zip(no_pages.get(&page_id))
                .is_some_and(|(yes, no)| Rc::ptr_eq(yes, no))
            {
                continue;
            }
            let ids = yes_pages
                .get(&page_id)
                .into_iter()
                .chain(no_pages.get(&page_id))
                .flat_map(|page| page.keys().cloned())
                .collect::<std::collections::BTreeSet<_>>();
            for id in ids {
                let Some(object) = self.join_object(decision, &id, yes, no, &mut input_unions)?
                else {
                    continue;
                };
                if self.version.object(&id) != Some(&object) {
                    self.install(id, object);
                }
            }
        }
        Ok(())
    }
    fn join_object(
        &self,
        decision: &Rc<FrozenTest>,
        id: &ObjectId,
        yes: &Self,
        no: &Self,
        input_unions: &mut BTreeMap<InputPair, Rc<BTreeSet<ValuesPath>>>,
    ) -> Result<Option<ObjectVersion>, JoinError> {
        let input_path = self.arena.input_paths.borrow().get(id).cloned();
        let baseline = ObjectVersion {
            revision: self.arena.snapshot(),
            multiplicity: Multiplicity::Singleton,
            data: ObjectData::Map {
                fields: BTreeMap::new(),
                remainder: Remainder::Input,
            },
        };
        let yes_object = yes
            .version
            .object(id)
            .or_else(|| input_path.as_ref().map(|_| &baseline));
        let no_object = no
            .version
            .object(id)
            .or_else(|| input_path.as_ref().map(|_| &baseline));
        let object = match (yes_object, no_object) {
            (Some(yes), Some(no)) if std::ptr::eq(yes, no) || yes == no => yes.clone(),
            (Some(yes_object), Some(no_object)) => {
                let ObjectVersion {
                    revision: _,
                    multiplicity: yes_multiplicity,
                    data: yes_data,
                } = yes_object;
                let ObjectVersion {
                    revision: _,
                    multiplicity: no_multiplicity,
                    data: no_data,
                } = no_object;
                let mut data = join_data(decision, yes_data, no_data, input_unions)?;
                if let ObjectData::Widened { inputs } = &mut data {
                    let mut added = BTreeSet::new();
                    for branch in [yes, no] {
                        match branch.version.object(id).map(|object| &object.data) {
                            Some(ObjectData::Widened { inputs: _ }) | None => {}
                            Some(
                                ObjectData::Map {
                                    fields: _,
                                    remainder: _,
                                }
                                | ObjectData::Strings(_)
                                | ObjectData::Opaque { reachable: _ },
                            ) => {
                                added.extend(branch.object_inputs(id));
                            }
                        }
                    }
                    added.extend(input_path.iter().cloned());
                    if !added.is_subset(inputs) {
                        Rc::make_mut(inputs).extend(added);
                    }
                }
                if matches!(&data, ObjectData::Map { fields, remainder: _ } if fields.len() > 128) {
                    let mut inputs = yes.object_inputs(id);
                    inputs.extend(no.object_inputs(id));
                    inputs.extend(input_path.iter().cloned());
                    data = ObjectData::Widened {
                        inputs: Rc::new(inputs),
                    };
                    tracing::debug!(reason = ?super::BoundReason::ObjectFields, "widen joined map");
                }
                ObjectVersion {
                    revision: self.arena.snapshot(),
                    multiplicity: match (*yes_multiplicity, *no_multiplicity) {
                        (Multiplicity::Singleton, Multiplicity::Singleton) => {
                            Multiplicity::Singleton
                        }
                        (
                            Multiplicity::Summary,
                            Multiplicity::Singleton | Multiplicity::Summary,
                        )
                        | (Multiplicity::Singleton, Multiplicity::Summary) => Multiplicity::Summary,
                    },
                    data,
                }
            }
            (Some(object), None) | (None, Some(object)) => object.clone(),
            (None, None) => return Ok(None),
        };
        Ok(Some(object))
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum JoinError {
    #[error("cannot join heaps from different sessions")]
    DifferentSessions,
    #[error("object storage changed across a fork")]
    StorageChanged,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct InputPair {
    yes: *const BTreeSet<ValuesPath>,
    no: *const BTreeSet<ValuesPath>,
}

fn join_data(
    decision: &Rc<FrozenTest>,
    yes: &ObjectData,
    no: &ObjectData,
    input_unions: &mut BTreeMap<InputPair, Rc<BTreeSet<ValuesPath>>>,
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
            *yes_remainder,
            no,
            *no_remainder,
        )),
        (ObjectData::Strings(yes), ObjectData::Strings(no)) if yes.len() == no.len() => {
            if yes
                .iter()
                .zip(no)
                .any(|(yes, no)| !super::state::value_join_fits(yes, no))
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
                inputs: join_input_sets(yes, no, input_unions),
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
                    (super::AliasSet::Known(yes), super::AliasSet::Known(no)) => {
                        super::AliasSet::Known(yes.union(no).cloned().collect())
                    }
                    (
                        super::AliasSet::RenderGraph,
                        super::AliasSet::Known(_) | super::AliasSet::RenderGraph,
                    )
                    | (super::AliasSet::Known(_), super::AliasSet::RenderGraph) => {
                        super::AliasSet::RenderGraph
                    }
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

fn join_input_sets(
    yes: &Rc<BTreeSet<ValuesPath>>,
    no: &Rc<BTreeSet<ValuesPath>>,
    input_unions: &mut BTreeMap<InputPair, Rc<BTreeSet<ValuesPath>>>,
) -> Rc<BTreeSet<ValuesPath>> {
    if Rc::ptr_eq(yes, no) || no.is_subset(yes) {
        Rc::clone(yes)
    } else if yes.is_subset(no) {
        Rc::clone(no)
    } else {
        let key = InputPair {
            yes: Rc::as_ptr(yes),
            no: Rc::as_ptr(no),
        };
        Rc::clone(
            input_unions
                .entry(key)
                .or_insert_with(|| Rc::new(yes.union(no).cloned().collect())),
        )
    }
}

fn join_map_data(
    decision: &Rc<FrozenTest>,
    yes: &BTreeMap<String, Field>,
    yes_remainder: Remainder,
    no: &BTreeMap<String, Field>,
    no_remainder: Remainder,
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
        if !field_join_fits(&yes_field, &no_field) {
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
        (Remainder::Closed, Remainder::Closed) => Remainder::Closed,
        (Remainder::Input, Remainder::Input) => Remainder::Input,
        (Remainder::Open, Remainder::Open | Remainder::Closed | Remainder::Input)
        | (Remainder::Closed | Remainder::Input, Remainder::Open)
        | (Remainder::Input, Remainder::Closed)
        | (Remainder::Closed, Remainder::Input) => Remainder::Open,
    };
    ObjectData::Map { fields, remainder }
}

impl Heap {
    fn data_inputs(
        &self,
        data: &ObjectData,
    ) -> std::collections::BTreeSet<helm_schema_core::ValuesPath> {
        let mut inputs = std::collections::BTreeSet::new();
        match data {
            ObjectData::Map {
                fields,
                remainder: _,
            } => {
                for field in fields.values() {
                    collect_field_inputs(self, field, &mut inputs);
                }
            }
            ObjectData::Strings(slots) => {
                for value in slots {
                    inputs.extend(self.freeze(value).inputs());
                }
            }
            ObjectData::Opaque { reachable } => {
                let reference = Value::leaf(Datum::Unknown(super::UnknownValue {
                    types: super::TypeCoverage::Unresolved,
                    aliases: reachable.clone(),
                }));
                inputs.extend(self.freeze(&reference).reachable_inputs());
            }
            ObjectData::Widened { inputs: known } => inputs.extend(known.iter().cloned()),
        }
        inputs
    }
}
fn collect_field_inputs(
    heap: &Heap,
    field: &Field,
    inputs: &mut std::collections::BTreeSet<helm_schema_core::ValuesPath>,
) {
    match field {
        Field::Present(value) => inputs.extend(heap.freeze(value).inputs()),
        Field::Select {
            decision: _,
            yes,
            no,
        } => {
            collect_field_inputs(heap, yes, inputs);
            collect_field_inputs(heap, no, inputs);
        }
        Field::Absent | Field::Unknown => {}
    }
}

impl Heap {
    fn object_inputs(
        &self,
        id: &ObjectId,
    ) -> std::collections::BTreeSet<helm_schema_core::ValuesPath> {
        let reference = Value::leaf(Datum::Unknown(super::UnknownValue {
            types: super::TypeCoverage::Unresolved,
            aliases: super::AliasSet::Known(std::collections::BTreeSet::from([id.clone()])),
        }));
        self.freeze(&reference).reachable_inputs()
    }
}

fn field_join_fits(yes: &Field, no: &Field) -> bool {
    if let (Field::Present(yes), Field::Present(no)) = (yes, no)
        && Rc::ptr_eq(&yes.node, &no.node)
    {
        return true;
    }
    let mut pending = vec![yes, no];
    let mut leaves = 0;
    while let Some(field) = pending.pop() {
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
