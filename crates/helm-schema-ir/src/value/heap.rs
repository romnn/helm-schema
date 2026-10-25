use std::collections::BTreeMap;
use std::rc::Rc;

use super::{
    Arena, Datum, Field, FrozenTest, FrozenValue, Heap, HeapVersion, Leaf, MapType, Multiplicity,
    ObjectData, ObjectId, ObjectVersion, Remainder, SnapshotId, Value, ValueNode,
};

const OBJECTS_PER_PAGE: u64 = 64;

impl Arena {
    pub(super) fn snapshot(&self) -> SnapshotId {
        let ordinal = self.next_snapshot.get();
        self.next_snapshot.set(ordinal + 1);
        SnapshotId { ordinal }
    }
}

impl Heap {
    pub(super) fn new() -> Self {
        let arena = Rc::new(Arena::default());
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
                inputs: self.data_inputs(&data),
            }
        } else {
            data
        };
        if self.arena.next_object.get() >= 4096 {
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
                    data: ObjectData::Widened { inputs },
                },
            );
            return id;
        }
        let ordinal = self.arena.next_object.get();
        self.arena.next_object.set(ordinal + 1);
        let id = ObjectId { ordinal };
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
            let mut inputs = inputs.clone();
            if let Field::Present(value) = &value {
                inputs.extend(self.freeze(value).inputs());
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
                    data: ObjectData::Widened { inputs },
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
        FrozenValue {
            heap: Rc::clone(&self.version),
            value: self.version.consume(value),
        }
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
                Value::select(Rc::clone(decision), self.consume(yes), self.consume(no))
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
                    data:
                        ObjectData::Map {
                            fields: _,
                            remainder: _,
                        }
                        | ObjectData::Opaque { reachable: _ }
                        | ObjectData::Widened { inputs: _ },
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

impl FrozenValue {
    fn field(&self, key: &str) -> Field {
        match self.value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        view: _,
                        target: Some(id),
                    },
                origin: _,
            }) => self.heap.field(id, key),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Scalar {
                        go_type: _,
                        content: _,
                    }
                    | Datum::Map {
                        view: _,
                        target: None,
                    }
                    | Datum::Sequence(_)
                    | Datum::Strings(_)
                    | Datum::Slot {
                        backing: _,
                        index: _,
                    }
                    | Datum::Unknown(_),
                origin: _,
            }) => Field::Unknown,
            ValueNode::Select { decision, yes, no } => Field::select(
                Rc::clone(decision),
                Self {
                    heap: Rc::clone(&self.heap),
                    value: yes.clone(),
                }
                .field(key),
                Self {
                    heap: Rc::clone(&self.heap),
                    value: no.clone(),
                }
                .field(key),
            ),
        }
    }
}

impl Heap {
    #[tracing::instrument(skip_all)]
    pub(super) fn join(
        &mut self,
        decision: Rc<FrozenTest>,
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
                let input_path = self.arena.input_paths.borrow().get(&id).cloned();
                if input_path
                    .as_ref()
                    .is_some_and(|path| path.segments().next().is_some())
                    && yes_version.object(&id).is_some() != no_version.object(&id).is_some()
                {
                    return Err(JoinError::ConditionalInputType);
                }
                let baseline = ObjectVersion {
                    revision: self.arena.snapshot(),
                    multiplicity: Multiplicity::Singleton,
                    data: ObjectData::Map {
                        fields: BTreeMap::new(),
                        remainder: Remainder::Input,
                    },
                };
                let yes_object = yes_version
                    .object(&id)
                    .or_else(|| input_path.as_ref().map(|_| &baseline));
                let no_object = no_version
                    .object(&id)
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
                        let mut data = join_data(Rc::clone(&decision), yes_data, no_data)?;
                        if let ObjectData::Widened { inputs } = &mut data {
                            inputs.extend(yes.object_inputs(&id));
                            inputs.extend(no.object_inputs(&id));
                            inputs.extend(input_path.iter().cloned());
                        }
                        if matches!(&data, ObjectData::Map { fields, remainder: _ } if fields.len() > 128)
                        {
                            let mut inputs = yes.object_inputs(&id);
                            inputs.extend(no.object_inputs(&id));
                            inputs.extend(input_path.iter().cloned());
                            data = ObjectData::Widened { inputs };
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
                                | (Multiplicity::Singleton, Multiplicity::Summary) => {
                                    Multiplicity::Summary
                                }
                            },
                            data,
                        }
                    }
                    (Some(object), None) | (None, Some(object)) => object.clone(),
                    (None, None) => continue,
                };
                if self.version.object(&id) != Some(&object) {
                    self.install(id, object);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum JoinError {
    #[error("cannot join heaps from different sessions")]
    DifferentSessions,
    #[error("object storage changed across a fork")]
    StorageChanged,
    #[error("input container materialization does not cover both branches")]
    ConditionalInputType,
}

fn join_data(
    decision: Rc<FrozenTest>,
    yes: &ObjectData,
    no: &ObjectData,
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
        ) => {
            let keys = yes
                .keys()
                .chain(no.keys())
                .collect::<std::collections::BTreeSet<_>>();
            if keys.len() > 128 {
                return Ok(ObjectData::Widened {
                    inputs: Default::default(),
                });
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
                    return Ok(ObjectData::Widened {
                        inputs: Default::default(),
                    });
                }
                fields.insert(
                    key.clone(),
                    Field::select(Rc::clone(&decision), yes_field, no_field),
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
            Ok(ObjectData::Map { fields, remainder })
        }
        (ObjectData::Strings(yes), ObjectData::Strings(no)) if yes.len() == no.len() => {
            if yes
                .iter()
                .zip(no)
                .any(|(yes, no)| !super::state::value_join_fits(yes, no))
            {
                return Ok(ObjectData::Widened {
                    inputs: Default::default(),
                });
            }
            Ok(ObjectData::Strings(
                yes.iter()
                    .zip(no)
                    .map(|(yes, no)| Value::select(Rc::clone(&decision), yes.clone(), no.clone()))
                    .collect(),
            ))
        }
        (ObjectData::Widened { inputs: yes }, ObjectData::Widened { inputs: no }) => {
            Ok(ObjectData::Widened {
                inputs: yes.union(no).cloned().collect(),
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
            inputs: inputs.clone(),
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
            ObjectData::Opaque { reachable: _ },
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
        ) => Err(JoinError::StorageChanged),
        (ObjectData::Strings(_), ObjectData::Strings(_))
        | (
            ObjectData::Map {
                fields: _,
                remainder: _,
            },
            ObjectData::Strings(_),
        )
        | (
            ObjectData::Strings(_),
            ObjectData::Map {
                fields: _,
                remainder: _,
            },
        ) => Err(JoinError::StorageChanged),
    }
}

impl FrozenValue {
    pub(super) fn read_field(&self, key: &str) -> super::FrozenField {
        super::FrozenField {
            heap: Rc::clone(&self.heap),
            field: self.field(key),
        }
    }
}

impl super::FrozenField {
    pub(super) fn present(&self) -> Option<FrozenValue> {
        match &self.field {
            Field::Present(value) => Some(FrozenValue {
                heap: Rc::clone(&self.heap),
                value: self.heap.consume(value),
            }),
            Field::Absent
            | Field::Unknown
            | Field::Select {
                decision: _,
                yes: _,
                no: _,
            } => None,
        }
    }
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
                inputs.extend(self.freeze(&reference).inputs());
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
        self.freeze(&reference).inputs()
    }
}

fn field_join_fits(yes: &Field, no: &Field) -> bool {
    if let (Field::Present(yes), Field::Present(no)) = (yes, no) {
        if Rc::ptr_eq(&yes.node, &no.node) {
            return true;
        }
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
