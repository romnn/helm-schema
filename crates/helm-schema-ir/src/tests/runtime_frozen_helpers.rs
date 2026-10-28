use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use super::scalar_value::ScalarContent;
use crate::abstract_value::AbstractValue;
use helm_schema_core::{GuardValue, Predicate};

use super::{
    Datum, DocumentExecution, ExecutedSite, Field, Flow, FrozenValue, HeapVersion, Leaf,
    ObjectData, TextBuffer, Value, ValueNode,
};

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

impl super::ExecutedSite {
    pub(crate) fn projected_value(&self) -> AbstractValue {
        if self.coverage_exact {
            self.value.projected_value()
        } else {
            AbstractValue::Widened(self.value.inputs())
        }
    }
}

impl FrozenValue {
    pub(crate) fn projected_value(&self) -> AbstractValue {
        project(&self.value, &self.heap, &mut BTreeSet::new(), 0)
    }
}

fn project_sequence(
    value: &Value,
    sequence: &super::InterfaceSequence,
    heap: &super::HeapVersion,
    seen: &BTreeSet<super::ObjectId>,
    depth: usize,
) -> AbstractValue {
    if sequence.storage.is_none() {
        return AbstractValue::Widened(value.inputs());
    }
    let Some(items) = sequence.items() else {
        return AbstractValue::Widened(value.inputs());
    };
    AbstractValue::List(
        items
            .iter()
            .map(|value| project(value, heap, &mut seen.clone(), depth + 1))
            .collect(),
    )
}

fn project_string_slice(
    value: &Value,
    slice: &super::StringSlice,
    heap: &super::HeapVersion,
    seen: &BTreeSet<super::ObjectId>,
    depth: usize,
) -> AbstractValue {
    let Some(super::ObjectVersion {
        data: ObjectData::Strings(slots),
        revision: _,
        multiplicity: _,
    }) = heap.object(&slice.backing)
    else {
        return AbstractValue::Unknown;
    };
    let Some(slots) = slots.get(slice.start..slice.start.saturating_add(slice.length)) else {
        return AbstractValue::Widened(value.inputs());
    };
    AbstractValue::List(
        slots
            .iter()
            .map(|value| project(value, heap, &mut seen.clone(), depth + 1))
            .collect(),
    )
}

fn project(
    value: &Value,
    heap: &super::HeapVersion,
    seen: &mut BTreeSet<super::ObjectId>,
    depth: usize,
) -> AbstractValue {
    if depth > 32 {
        return AbstractValue::Widened(value.inputs());
    }
    if let Some(path) = value.identity() {
        return AbstractValue::ValuesPath(path.clone());
    }
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(slice),
            origin: _,
        }) => project_string_slice(value, slice, heap, seen, depth),
        ValueNode::Select {
            decision: _,
            yes,
            no,
        } => AbstractValue::Choice(BTreeSet::from([
            project(yes, heap, &mut seen.clone(), depth + 1),
            project(no, heap, &mut seen.clone(), depth + 1),
        ])),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    target: Some(id),
                    view: _,
                },
            origin: _,
        }) if seen.insert(id.clone()) => {
            let Some(super::ObjectVersion {
                data: ObjectData::Map { fields, remainder },
                revision: _,
                multiplicity: _,
            }) = heap.object(id)
            else {
                return AbstractValue::Unknown;
            };
            let entries: BTreeMap<_, _> = fields
                .iter()
                .filter_map(|(key, field)| match field {
                    super::Field::Present(value) => Some((
                        key.clone(),
                        project(value, heap, &mut seen.clone(), depth + 1),
                    )),
                    super::Field::Absent
                    | super::Field::Unknown
                    | super::Field::Select {
                        decision: _,
                        yes: _,
                        no: _,
                    } => None,
                })
                .collect();
            if *remainder == super::Remainder::Closed {
                AbstractValue::Dict(entries)
            } else {
                AbstractValue::Overlay {
                    entries,
                    fallback: Box::new(AbstractValue::Unknown),
                }
            }
        }
        ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(sequence),
            origin: _,
        }) => project_sequence(value, sequence, heap, seen, depth),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Scalar {
                    content: Some(ScalarContent::Literal(GuardValue::String(text))),
                    go_type: _,
                },
            origin: _,
        }) => AbstractValue::StringSet(BTreeSet::from([text.clone()])),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Text(_)
                | Datum::Scalar {
                    go_type: _,
                    content: _,
                }
                | Datum::Map { view: _, target: _ }
                | Datum::Slot {
                    backing: _,
                    index: _,
                }
                | Datum::Unknown(_),
            origin: _,
        }) => AbstractValue::Widened(value.inputs()),
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
