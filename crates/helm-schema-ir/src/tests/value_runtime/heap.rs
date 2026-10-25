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

    pub(super) fn fork(&self) -> Self {
        Self {
            version: Rc::new(self.version.as_ref().clone()),
            arena: Rc::clone(&self.arena),
        }
    }

    pub(super) fn allocate(&mut self, data: ObjectData, multiplicity: Multiplicity) -> ObjectId {
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
                data: ObjectData::Strings(_),
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
                inputs: _,
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
                        },
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
                    | Datum::Unknown(_),
                inputs: _,
            }) => value.clone(),
        }
    }
}

impl Remainder {
    pub(super) fn missing(self) -> Field {
        match self {
            Self::Closed => Field::Absent,
            Self::Open => Field::Unknown,
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
                inputs: _,
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
                    | Datum::Slot {
                        backing: _,
                        index: _,
                    }
                    | Datum::Unknown(_),
                inputs: _,
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
    pub(super) fn join(
        &mut self,
        decision: Rc<FrozenTest>,
        yes: &Self,
        no: &Self,
    ) -> Result<(), JoinError> {
        if !Rc::ptr_eq(&self.arena, &yes.arena) || !Rc::ptr_eq(&self.arena, &no.arena) {
            return Err(JoinError::DifferentSessions);
        }
        let ids = yes
            .version
            .pages
            .values()
            .chain(no.version.pages.values())
            .flat_map(|page| page.keys().cloned())
            .collect::<std::collections::BTreeSet<_>>();
        for id in ids {
            let object = match (yes.version.object(&id), no.version.object(&id)) {
                (Some(yes), Some(no)) if yes == no => yes.clone(),
                (Some(yes), Some(no)) => ObjectVersion {
                    revision: self.arena.snapshot(),
                    multiplicity: match (yes.multiplicity, no.multiplicity) {
                        (Multiplicity::Singleton, Multiplicity::Singleton) => {
                            Multiplicity::Singleton
                        }
                        (
                            Multiplicity::Summary,
                            Multiplicity::Singleton | Multiplicity::Summary,
                        )
                        | (Multiplicity::Singleton, Multiplicity::Summary) => Multiplicity::Summary,
                    },
                    data: join_data(Rc::clone(&decision), &yes.data, &no.data)?,
                },
                (Some(object), None) | (None, Some(object)) => object.clone(),
                (None, None) => continue,
            };
            self.install(id, object);
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
            let mut fields = BTreeMap::new();
            for key in keys {
                fields.insert(
                    key.clone(),
                    Field::select(
                        Rc::clone(&decision),
                        yes.get(key)
                            .cloned()
                            .unwrap_or_else(|| yes_remainder.missing()),
                        no.get(key)
                            .cloned()
                            .unwrap_or_else(|| no_remainder.missing()),
                    ),
                );
            }
            let remainder = match (yes_remainder, no_remainder) {
                (Remainder::Closed, Remainder::Closed) => Remainder::Closed,
                (Remainder::Open, Remainder::Open | Remainder::Closed)
                | (Remainder::Closed, Remainder::Open) => Remainder::Open,
            };
            Ok(ObjectData::Map { fields, remainder })
        }
        (ObjectData::Strings(yes), ObjectData::Strings(no)) if yes.len() == no.len() => {
            Ok(ObjectData::Strings(
                yes.iter()
                    .zip(no)
                    .map(|(yes, no)| Value::select(Rc::clone(&decision), yes.clone(), no.clone()))
                    .collect(),
            ))
        }
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
