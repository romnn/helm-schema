use std::rc::Rc;

use helm_schema_core::Predicate;

use super::{
    Datum, DocumentExecution, ExecutedSite, Field, Flow, FrozenValue, HeapVersion, Leaf,
    ObjectData, TextBuffer, ValueNode,
};

#[derive(Clone, Debug)]
pub(crate) struct FrozenField {
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
    pub(crate) fn read_field(&self, key: &str) -> FrozenField {
        FrozenField {
            heap: Rc::clone(&self.heap),
            field: self.field(key),
        }
    }
}

impl FrozenField {
    pub(crate) fn present(&self) -> Option<FrozenValue> {
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

impl FrozenValue {
    pub(crate) fn sequence_items(&self) -> Option<Vec<Self>> {
        let values = match self.value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum: Datum::Sequence(sequence),
                origin: _,
            }) => sequence.items()?,
            ValueNode::Leaf(Leaf {
                datum: Datum::Strings(slice),
                origin: _,
            }) => {
                let super::ObjectVersion {
                    data: ObjectData::Strings(slots),
                    revision: _,
                    multiplicity: _,
                } = self.heap.object(&slice.backing)?
                else {
                    return None;
                };
                slots.get(slice.start..slice.start.checked_add(slice.length)?)?
            }
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Scalar {
                        go_type: _,
                        content: _,
                    }
                    | Datum::Map { view: _, target: _ }
                    | Datum::Slot {
                        backing: _,
                        index: _,
                    }
                    | Datum::Text(_)
                    | Datum::Unknown(_),
                origin: _,
            })
            | ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => return None,
        };
        Some(
            values
                .iter()
                .map(|value| Self {
                    heap: Rc::clone(&self.heap),
                    value: self.heap.consume(value),
                })
                .collect(),
        )
    }
}

impl DocumentExecution {
    pub(crate) fn text_sites(&self) -> impl Iterator<Item = &super::ExecutedText> {
        self.exits
            .iter()
            .filter(|exit| matches!(exit.flow, Flow::Normal(_)))
            .flat_map(|exit| exit.state.text_sites.iter())
            .filter(|site| self.root_programs.contains(&site.program))
    }
    pub(crate) fn all_sites(&self) -> impl Iterator<Item = &ExecutedSite> {
        self.exits.iter().flat_map(|exit| exit.state.sites.iter())
    }
    pub(crate) fn text(&self) -> impl Iterator<Item = (Predicate, &TextBuffer)> {
        self.exits.iter().filter_map(|exit| match &exit.flow {
            Flow::Normal(_) => Some((exit.state.path_predicate(), &exit.state.output)),
            Flow::Abort(_) | Flow::Break(_) | Flow::Continue(_) => None,
        })
    }
}
