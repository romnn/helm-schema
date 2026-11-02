//! One bounded walk over the contents visible to JSON serialization.

use std::collections::BTreeSet;

use super::{
    Datum, Field, FrozenTest, HeapVersion, Leaf, ObjectData, ObjectId, Remainder, Value, ValueNode,
};

pub(super) trait Fact {
    type Output;
    fn charge(&mut self, depth: usize) -> bool;
    fn unknown(&self) -> Self::Output;
    fn examine(&mut self, value: &Value, heap: &HeapVersion, depth: usize) -> Option<Self::Output>;
    fn key(&self, key: &str) -> Self::Output;
    fn join(&self, values: Vec<Self::Output>) -> Self::Output;
    fn select(&self, decision: &FrozenTest, yes: Self::Output, no: Self::Output) -> Self::Output;
    fn decisive(&self, value: &Self::Output) -> bool;
}

pub(super) fn value<F: Fact>(
    source: &Value,
    heap: &HeapVersion,
    depth: usize,
    active: &mut BTreeSet<ObjectId>,
    fact: &mut F,
) -> F::Output {
    if !fact.charge(depth) {
        return fact.unknown();
    }
    if let Some(result) = fact.examine(source, heap, depth) {
        return result;
    }
    let datum = match source.node.as_ref() {
        ValueNode::Select { decision, yes, no } => {
            let yes = value(yes, heap, depth + 1, active, fact);
            let no = value(no, heap, depth + 1, active, fact);
            return fact.select(decision, yes, no);
        }
        ValueNode::Leaf(Leaf { datum, origin: _ }) => datum,
    };
    match datum {
        Datum::Map {
            target: Some(target),
            view: _,
        } => {
            let object = heap.object(target);
            let Some(ObjectData::Map { fields, remainder }) =
                object.as_deref().map(|object| &object.data)
            else {
                return fact.unknown();
            };
            let mut results = Vec::new();
            if *remainder != Remainder::Closed {
                let unknown = fact.unknown();
                if fact.decisive(&unknown) {
                    return unknown;
                }
                results.push(unknown);
            }
            if !active.insert(target.clone()) {
                return fact.unknown();
            }
            for (key, child) in fields.iter() {
                results.push(field(child, key, heap, depth + 1, active, fact));
            }
            active.remove(target);
            fact.join(results)
        }
        Datum::Sequence(sequence) => {
            let Some(children) = sequence.items() else {
                return fact.unknown();
            };
            let results = children
                .iter()
                .map(|child| value(child, heap, depth + 1, active, fact))
                .collect();
            fact.join(results)
        }
        Datum::Strings(slice) => {
            let object = heap.object(&slice.backing);
            let children = match object.as_deref().map(|object| &object.data) {
                Some(ObjectData::Strings(slots)) => slice
                    .start
                    .checked_add(slice.length)
                    .and_then(|end| slots.get(slice.start..end)),
                _ => None,
            };
            match children {
                Some(children) => {
                    if !active.insert(slice.backing.clone()) {
                        return fact.unknown();
                    }
                    let results = children
                        .iter()
                        .map(|child| value(child, heap, depth + 1, active, fact))
                        .collect();
                    active.remove(&slice.backing);
                    fact.join(results)
                }
                None => fact.unknown(),
            }
        }
        Datum::Text(_)
        | Datum::Scalar {
            go_type: _,
            content: _,
        }
        | Datum::Unknown(_)
        | Datum::Map {
            target: None,
            view: _,
        }
        | Datum::Slot {
            backing: _,
            index: _,
        } => fact.unknown(),
    }
}

pub(super) fn field<F: Fact>(
    source: &Field,
    key: &str,
    heap: &HeapVersion,
    depth: usize,
    active: &mut BTreeSet<ObjectId>,
    fact: &mut F,
) -> F::Output {
    if !fact.charge(depth) {
        return fact.unknown();
    }
    match source {
        Field::Absent => fact.join(Vec::new()),
        Field::Unknown => fact.unknown(),
        Field::Present(child) => {
            let key = fact.key(key);
            if fact.decisive(&key) {
                return key;
            }
            let child = value(child, heap, depth + 1, active, fact);
            fact.join(vec![key, child])
        }
        Field::Select { decision, yes, no } => {
            let yes = field(yes, key, heap, depth + 1, active, fact);
            let no = field(no, key, heap, depth + 1, active, fact);
            fact.select(decision, yes, no)
        }
    }
}
