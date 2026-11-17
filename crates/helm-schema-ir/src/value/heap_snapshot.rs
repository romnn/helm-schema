//! Frozen-root reads and live-slot consumption over persistent object contents.

use super::work::WorkBudget;
use super::{Field, HeapVersion, ObjectData, ObjectId, ObjectVersion, Remainder};
use ahash::AHashMap;
use helm_schema_core::ValuesPath;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

const MAX_VALUE_WORK: usize = 65_536;

#[derive(Default)]
struct DropQueue {
    active: bool,
    pending: Vec<super::heap_store::Store>,
}

thread_local! {
    static DROPPING: RefCell<DropQueue> = RefCell::default();
}

pub(super) fn track_version(live: &Rc<Cell<usize>>) -> Rc<Cell<usize>> {
    live.set(live.get() + 1);
    tracing::debug_span!(target: "helm_schema_ir::counters::heap_store", "heap_versions_live", count = live.get()).in_scope(|| {});
    Rc::clone(live)
}

impl Drop for HeapVersion {
    fn drop(&mut self) {
        self.live_versions.set(self.live_versions.get() - 1);
        tracing::debug_span!(target: "helm_schema_ir::counters::heap_store", "heap_versions_live", count = self.live_versions.get()).in_scope(|| {});
        if self.store.len() == 0 {
            return;
        }
        // Frozen values can form long acyclic snapshot chains; reclaim them without recursion.
        let drain = DROPPING.with(|queue| {
            let mut queue = queue.borrow_mut();
            queue.pending.push(std::mem::take(&mut self.store));
            let drain = !queue.active;
            queue.active = true;
            drain
        });
        if drain {
            while let Some(store) = DROPPING.with(|queue| queue.borrow_mut().pending.pop()) {
                drop(store);
            }
            // No root or queue allocation survives the synchronous reclamation.
            DROPPING.with(|queue| *queue.borrow_mut() = DropQueue::default());
        }
    }
}

impl HeapVersion {
    pub(super) fn has_allocated_object(&self, object: &ObjectId) -> bool {
        self.store.multiplicity(object).is_some()
    }
    pub(super) fn object_count(&self) -> usize {
        self.store.len()
    }
    #[tracing::instrument(skip_all)]
    pub(super) fn object(&self, object: &ObjectId) -> Option<Rc<ObjectVersion>> {
        self.store.object(object)
    }
    pub(super) fn borrowed_object(&self, object: &ObjectId) -> Option<&ObjectVersion> {
        self.store.borrowed(object)
    }
}

pub(super) fn data_inputs(heap: &Rc<HeapVersion>, data: &ObjectData) -> BTreeSet<ValuesPath> {
    data_inputs_with_budget(heap, data, &mut WorkBudget::new(MAX_VALUE_WORK))
}

fn data_inputs_with_budget(
    heap: &Rc<HeapVersion>,
    data: &ObjectData,
    budget: &mut WorkBudget,
) -> BTreeSet<ValuesPath> {
    let mut fields = Vec::new();
    let mut values = Vec::new();
    let mut inputs = BTreeSet::new();
    match data {
        ObjectData::Map {
            fields: known,
            remainder,
        } => {
            fields.extend(known.values());
            if let Remainder::Unknown { inputs: known } = remainder {
                inputs.extend(known.iter().cloned());
            }
        }
        ObjectData::Strings(slots) => values.extend(slots.iter().cloned()),
        ObjectData::Opaque { reachable } => values.push(super::Value::leaf(super::Datum::Unknown(
            super::UnknownValue {
                types: super::TypeCoverage::Unresolved,
                aliases: reachable.clone(),
            },
        ))),
        ObjectData::Widened { inputs: known } => inputs.extend(known.iter().cloned()),
    }
    while let Some(field) = fields.pop() {
        if !budget.charge(1) {
            return BTreeSet::from([ValuesPath::default()]);
        }
        match field {
            Field::Present(value) => values.push(value.clone()),
            Field::Select {
                decision: _,
                yes,
                no,
            } => fields.extend([yes.as_ref(), no.as_ref()]),
            Field::Unknown | Field::Absent => {}
        }
    }
    let include_unknown_aliases = matches!(data, ObjectData::Opaque { reachable: _ });
    for value in values {
        let frozen = super::FrozenValue::at(
            Rc::clone(heap),
            heap.consume_value(&value, budget, &mut AHashMap::new()),
        );
        inputs.extend(frozen.collect_inputs_with_budget(include_unknown_aliases, budget));
    }
    inputs
}

impl HeapVersion {
    /// Resolves live slots against this version without revisiting shared value nodes.
    #[tracing::instrument(skip_all)]
    pub(super) fn consume(&self, value: &super::Value) -> super::Value {
        if matches!(
            value.node.as_ref(),
            super::ValueNode::Leaf(super::Leaf {
                datum: super::Datum::Text(_)
                    | super::Datum::Scalar {
                        go_type: _,
                        content: _
                    }
                    | super::Datum::Map { view: _, target: _ }
                    | super::Datum::Sequence(_)
                    | super::Datum::Strings(_)
                    | super::Datum::Unknown(_),
                origin: _,
            })
        ) {
            return value.clone();
        }
        self.consume_value(
            value,
            &mut WorkBudget::new(MAX_VALUE_WORK),
            &mut AHashMap::new(),
        )
    }

    fn consume_value(
        &self,
        value: &super::Value,
        budget: &mut WorkBudget,
        memo: &mut AHashMap<*const super::ValueNode, super::Value>,
    ) -> super::Value {
        let key = Rc::as_ptr(&value.node);
        if let Some(value) = memo.get(&key) {
            return value.clone();
        }
        if !budget.charge(1) {
            return super::Value::influenced(BTreeSet::from([ValuesPath::default()]));
        }
        let consumed = match value.node.as_ref() {
            super::ValueNode::Select { decision, yes, no } => {
                let consumed_yes = self.consume_value(yes, budget, memo);
                let consumed_no = self.consume_value(no, budget, memo);
                if Rc::ptr_eq(&yes.node, &consumed_yes.node)
                    && Rc::ptr_eq(&no.node, &consumed_no.node)
                {
                    value.clone()
                } else {
                    super::heap_select::value(decision, &consumed_yes, &consumed_no, budget)
                        .unwrap_or_else(|| {
                            super::Value::influenced(BTreeSet::from([ValuesPath::default()]))
                        })
                }
            }
            super::ValueNode::Leaf(super::Leaf {
                datum: super::Datum::Slot { backing, index },
                origin: _,
            }) => match self.object(backing).as_deref() {
                Some(ObjectVersion {
                    revision: _,
                    multiplicity: _,
                    data: ObjectData::Strings(slots),
                }) => slots
                    .get(*index)
                    .cloned()
                    .unwrap_or_else(super::Value::unknown),
                Some(ObjectVersion {
                    revision: _,
                    multiplicity: _,
                    data: ObjectData::Widened { inputs },
                }) => super::Value {
                    node: Rc::new(super::ValueNode::Leaf(super::Leaf {
                        datum: super::Datum::Scalar {
                            go_type: super::ScalarType::String,
                            content: None,
                        },
                        origin: super::Origin {
                            derivation: None,
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
                | None => super::Value::unknown(),
            },
            super::ValueNode::Leaf(super::Leaf {
                datum:
                    super::Datum::Text(_)
                    | super::Datum::Scalar {
                        go_type: _,
                        content: _,
                    }
                    | super::Datum::Map { view: _, target: _ }
                    | super::Datum::Sequence(_)
                    | super::Datum::Strings(_)
                    | super::Datum::Unknown(_),
                origin: _,
            }) => value.clone(),
        };
        memo.insert(key, consumed.clone());
        consumed
    }
}
