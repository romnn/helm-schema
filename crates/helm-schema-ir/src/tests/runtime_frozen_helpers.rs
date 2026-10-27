use std::rc::Rc;

use super::{Datum, Field, FrozenValue, HeapVersion, Leaf, ValueNode};

#[derive(Clone, Debug)]
pub(super) struct FrozenField {
    pub(super) heap: Rc<HeapVersion>,
    pub(super) field: Field,
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
                    | Datum::Text(_)
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

impl FrozenValue {
    pub(super) fn read_field(&self, key: &str) -> FrozenField {
        FrozenField {
            heap: Rc::clone(&self.heap),
            field: self.field(key),
        }
    }
}

impl FrozenField {
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
