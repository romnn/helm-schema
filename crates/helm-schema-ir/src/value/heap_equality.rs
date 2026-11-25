//! Exact structural comparisons charged to one object's construction budget.

use std::rc::Rc;

use ahash::AHashSet;

use super::work::WorkBudget;
use super::{
    Datum, Field, FrozenValue, InterfaceSequence, Leaf, ObjectData, ObjectVersion, Origin,
    SequenceStorage, Value, ValueNode,
};

enum Pair<'value> {
    Value {
        yes: &'value ValueNode,
        no: &'value ValueNode,
    },
    Field {
        yes: &'value Field,
        no: &'value Field,
    },
    Storage {
        yes: &'value SequenceStorage,
        no: &'value SequenceStorage,
    },
}

#[derive(PartialEq, Eq, Hash)]
struct SeenPair {
    yes: *const (),
    no: *const (),
    kind: u8,
}

pub(super) fn values(yes: &Value, no: &Value, budget: &mut WorkBudget) -> Option<bool> {
    compare(
        vec![Pair::Value {
            yes: &yes.node,
            no: &no.node,
        }],
        budget,
    )
}

pub(super) fn fields(yes: &Field, no: &Field, budget: &mut WorkBudget) -> Option<bool> {
    compare(vec![Pair::Field { yes, no }], budget)
}

pub(super) fn frozen(yes: &FrozenValue, no: &FrozenValue, budget: &mut WorkBudget) -> Option<bool> {
    if !Rc::ptr_eq(&yes.heap, &no.heap) {
        return Some(false);
    }
    values(&yes.value, &no.value, budget)
}

pub(super) fn objects(
    yes: &ObjectVersion,
    no: &ObjectVersion,
    budget: &mut WorkBudget,
) -> Option<bool> {
    let ObjectVersion {
        revision,
        multiplicity,
        data,
    } = yes;
    let ObjectVersion {
        revision: other_revision,
        multiplicity: other_multiplicity,
        data: other_data,
    } = no;
    if revision != other_revision || multiplicity != other_multiplicity {
        return Some(false);
    }
    let mut pending = Vec::new();
    match (data, other_data) {
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
            if yes.len() != no.len()
                || !super::layers::equal_remainders(yes_remainder, no_remainder, budget)?
            {
                return Some(false);
            }
            if !budget.charge(yes.len()) {
                return None;
            }
            for ((yes_key, yes), (no_key, no)) in yes.iter().zip(no.iter()) {
                if yes_key != no_key {
                    return Some(false);
                }
                pending.push(Pair::Field { yes, no });
            }
        }
        (ObjectData::Strings(yes), ObjectData::Strings(no)) => {
            if yes.len() != no.len() {
                return Some(false);
            }
            if !budget.charge(yes.len()) {
                return None;
            }
            pending.extend(yes.iter().zip(no).map(|(yes, no)| Pair::Value {
                yes: &yes.node,
                no: &no.node,
            }));
        }
        (ObjectData::Opaque { reachable: yes }, ObjectData::Opaque { reachable: no }) => {
            if !budget.charge(reach_work(yes) + reach_work(no)) {
                return None;
            }
            return Some(yes == no);
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
        ) => {
            if !budget
                .charge(yes.len() + no.len() + reach_work(yes_reachable) + reach_work(no_reachable))
            {
                return None;
            }
            return Some(yes == no && yes_reachable == no_reachable);
        }
        (
            ObjectData::Map { .. }
            | ObjectData::Strings(_)
            | ObjectData::Opaque { .. }
            | ObjectData::Widened { .. },
            _,
        ) => return Some(false),
    }
    compare(pending, budget)
}

pub(super) fn reach_work(reach: &super::Reach) -> usize {
    match reach {
        super::Reach::Top => 1,
        super::Reach::Bounded(objects) => objects.len() + 1,
    }
}

fn compare(mut pending: Vec<Pair<'_>>, budget: &mut WorkBudget) -> Option<bool> {
    let mut seen = AHashSet::new();
    while let Some(pair) = pending.pop() {
        if !budget.charge(1) {
            return None;
        }
        let pointers = match &pair {
            Pair::Value { yes, no } => SeenPair {
                yes: std::ptr::from_ref(*yes).cast(),
                no: std::ptr::from_ref(*no).cast(),
                kind: 0,
            },
            Pair::Field { yes, no } => SeenPair {
                yes: std::ptr::from_ref(*yes).cast(),
                no: std::ptr::from_ref(*no).cast(),
                kind: 1,
            },
            Pair::Storage { yes, no } => SeenPair {
                yes: std::ptr::from_ref(*yes).cast(),
                no: std::ptr::from_ref(*no).cast(),
                kind: 2,
            },
        };
        if pointers.yes == pointers.no || !seen.insert(pointers) {
            continue;
        }
        let equal = match pair {
            Pair::Field { yes, no } => compare_fields(yes, no, &mut pending),
            Pair::Value { yes, no } => compare_values(yes, no, &mut pending, budget)?,
            Pair::Storage { yes, no } => compare_storage(yes, no, &mut pending, budget)?,
        };
        if !equal {
            return Some(false);
        }
    }
    Some(true)
}

fn compare_fields<'value>(
    yes: &'value Field,
    no: &'value Field,
    pending: &mut Vec<Pair<'value>>,
) -> bool {
    match yes {
        Field::Absent => {
            if !matches!(no, Field::Absent) {
                return false;
            }
        }
        Field::Unknown => {
            if !matches!(no, Field::Unknown) {
                return false;
            }
        }
        Field::Present(yes) => {
            let Field::Present(no) = no else {
                return false;
            };
            pending.push(Pair::Value {
                yes: &yes.node,
                no: &no.node,
            });
        }
        Field::Select {
            decision,
            yes,
            no: alternative,
        } => {
            let Field::Select {
                decision: other_decision,
                yes: other_yes,
                no: other_no,
            } = no
            else {
                return false;
            };
            if decision != other_decision {
                return false;
            }
            pending.extend([
                Pair::Field { yes, no: other_yes },
                Pair::Field {
                    yes: alternative,
                    no: other_no,
                },
            ]);
        }
    }
    true
}

fn compare_values<'value>(
    yes: &'value ValueNode,
    no: &'value ValueNode,
    pending: &mut Vec<Pair<'value>>,
    budget: &mut WorkBudget,
) -> Option<bool> {
    match yes {
        ValueNode::Select {
            decision,
            yes,
            no: alternative,
        } => {
            let ValueNode::Select {
                decision: other_decision,
                yes: other_yes,
                no: other_no,
            } = no
            else {
                return Some(false);
            };
            if decision != other_decision {
                return Some(false);
            }
            pending.extend([
                Pair::Value {
                    yes: &yes.node,
                    no: &other_yes.node,
                },
                Pair::Value {
                    yes: &alternative.node,
                    no: &other_no.node,
                },
            ]);
        }
        ValueNode::Leaf(Leaf { datum, origin }) => {
            let ValueNode::Leaf(Leaf {
                datum: other_datum,
                origin: other_origin,
            }) = no
            else {
                return Some(false);
            };
            if !origins(origin, other_origin, pending, budget)? {
                return Some(false);
            }
            match datum {
                Datum::Sequence(InterfaceSequence {
                    storage,
                    start,
                    length,
                    capacity,
                }) => {
                    let Datum::Sequence(InterfaceSequence {
                        storage: other_storage,
                        start: other_start,
                        length: other_length,
                        capacity: other_capacity,
                    }) = other_datum
                    else {
                        return Some(false);
                    };
                    if start != other_start || length != other_length || capacity != other_capacity
                    {
                        return Some(false);
                    }
                    match (storage, other_storage) {
                        (Some(yes), Some(no)) => pending.push(Pair::Storage { yes, no }),
                        (None, None) => {}
                        (None, Some(_)) | (Some(_), None) => return Some(false),
                    }
                }
                Datum::Text(_)
                | Datum::Scalar { .. }
                | Datum::Map { .. }
                | Datum::Strings(_)
                | Datum::Slot { .. }
                | Datum::Unknown(_) => {
                    if !budget.charge(datum_work(datum) + datum_work(other_datum)) {
                        return None;
                    }
                    if datum != other_datum {
                        return Some(false);
                    }
                }
            }
        }
    }
    Some(true)
}

fn compare_storage<'value>(
    yes: &'value SequenceStorage,
    no: &'value SequenceStorage,
    pending: &mut Vec<Pair<'value>>,
    budget: &mut WorkBudget,
) -> Option<bool> {
    match yes {
        SequenceStorage::Interfaces { values: yes } => {
            let SequenceStorage::Interfaces { values: no } = no else {
                return Some(false);
            };
            if yes.len() != no.len() {
                return Some(false);
            }
            if !budget.charge(yes.len()) {
                return None;
            }
            pending.extend(yes.iter().zip(no).map(|(yes, no)| Pair::Value {
                yes: &yes.node,
                no: &no.node,
            }));
        }
        SequenceStorage::Bytes {
            content: yes,
            values: _,
        } => {
            let SequenceStorage::Bytes {
                content: no,
                values: _,
            } = no
            else {
                return Some(false);
            };
            if !budget.charge(yes.as_ref().map_or(0, Vec::len) + no.as_ref().map_or(0, Vec::len)) {
                return None;
            }
            if yes != no {
                return Some(false);
            }
        }
    }
    Some(true)
}

fn origins<'value>(
    yes: &'value Origin,
    no: &'value Origin,
    pending: &mut Vec<Pair<'value>>,
    budget: &mut WorkBudget,
) -> Option<bool> {
    let Origin {
        derivation,
        identity,
        influences,
    } = yes;
    let Origin {
        derivation: other_derivation,
        identity: other_identity,
        influences: other_influences,
    } = no;
    if !budget.charge(influences.len() + other_influences.len() + 1) {
        return None;
    }
    if identity != other_identity || influences != other_influences {
        return Some(false);
    }
    match (derivation, other_derivation) {
        (Some(yes), Some(no)) => {
            let super::relationship::Derivation { source, operation } = yes.as_ref();
            let super::relationship::Derivation {
                source: other_source,
                operation: other_operation,
            } = no.as_ref();
            if operation != other_operation || !Rc::ptr_eq(&source.heap, &other_source.heap) {
                return Some(false);
            }
            pending.push(Pair::Value {
                yes: &source.value.node,
                no: &other_source.value.node,
            });
        }
        (None, None) => {}
        (None, Some(_)) | (Some(_), None) => return Some(false),
    }
    Some(true)
}

fn datum_work(datum: &Datum) -> usize {
    match datum {
        Datum::Scalar {
            go_type: _,
            content:
                Some(super::scalar_value::ScalarContent::Literal(helm_schema_core::GuardValue::String(
                    text,
                ))),
        } => text.len() + 1,
        Datum::Unknown(super::UnknownValue { types: _, aliases }) => reach_work(aliases),
        Datum::Text(_)
        | Datum::Scalar { .. }
        | Datum::Map { .. }
        | Datum::Sequence(_)
        | Datum::Strings(_)
        | Datum::Slot { .. } => 1,
    }
}
