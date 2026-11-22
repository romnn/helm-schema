//! The carried state of a range transfer (ADR 0072 §6): whether the state one pass of a body
//! ends in stays inside the state it started from, and the join that summarizes both.
//!
//! Coverage compares the states the next iteration would start from, not the operations that
//! produced them: every carried object's storage (fields, slots, remainder, installs) by
//! identity, every outer binding the body can still see, and the validity assumptions. The
//! persistent stores' diff only finds the objects whose storage may differ; no body text is
//! inspected. A join only raises carried locations: it records no read and no other fact.

use std::collections::BTreeSet;
use std::rc::Rc;

use helm_schema_core::ValuesPath;

use super::path::LinkKind;
use super::work::WorkBudget;
use super::{
    DecisionEdge, Field, FrozenTest, FrozenValue, ObjectData, ObjectId, ObjectVersion, Reach,
    State, Value, ValueNode,
};

/// The passes after which a transfer stops refining one key at a time and widens whole objects
/// (ADR 0072 §10: the bound widens only the loop's carried locations).
pub(super) const MAX_LOOP_PASSES: usize = 4;

/// Work reading one carried object's provenance may take; a read that exhausts it proves no
/// coverage, so the location widens.
const MAX_COVERAGE_WORK: usize = 1 << 16;

/// How far a join raises the locations a pass left uncovered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Widening {
    /// A changed field of an exact map becomes a summary of both values; a changed key set,
    /// slot, remainder or alias widens the whole object.
    Fields,
    /// Every uncovered object widens whole (the pass bound).
    Objects,
    /// Every uncovered location becomes the top summary, which covers every later value; each
    /// pass then tops a location not topped before, so the transfer terminates.
    Top,
}

/// The outer state a range body can reach: the frames that outlive an iteration, and the names
/// each iteration declares afresh. A binding an inner frame or a fresh declaration shadows can be
/// neither read nor written by the body, so it is not carried; neither is an outer dot, which the
/// iteration's member shadows.
#[derive(Clone, Copy)]
pub(super) struct CarriedScope<'a> {
    pub(super) frames: usize,
    pub(super) rebound: &'a BTreeSet<String>,
}

/// A start state of the Rest passes, raised by the joins of the passes run from it.
pub(super) struct Carried {
    pub(super) state: State,
    assumptions_top: bool,
}

impl Carried {
    pub(super) fn new(state: State) -> Self {
        Self {
            state,
            assumptions_top: false,
        }
    }

    /// Whether `end`, reached by one pass from this start, stays inside it.
    pub(super) fn covers(&self, end: &State, scope: CarriedScope<'_>) -> bool {
        uncovered(&self.state, end, scope, true, self.assumptions_top).is_empty()
    }

    /// Joins `end` into this start: every location `end` leaves uncovered rises to a summary of
    /// both states (ADR 0072 §6), retaining observations independently of that join.
    pub(super) fn widen(&mut self, end: &State, scope: CarriedScope<'_>, widening: Widening) {
        let uncovered = uncovered(&self.state, end, scope, false, self.assumptions_top);
        self.state.retain_loop_facts(end);
        let state = &mut self.state;
        let top = widening == Widening::Top;
        let mut assumptions_top = self.assumptions_top;
        for location in uncovered {
            match location {
                Uncovered::Fields { object, keys } if widening == Widening::Fields => {
                    for key in keys {
                        let before = state.heap.version.field(&object, &key);
                        let after = end.heap.version.field(&object, &key);
                        let mut inputs = field_inputs(state, &before);
                        inputs.extend(field_inputs(end, &after));
                        state
                            .heap
                            .write(&object, &key, Field::Present(Value::influenced(inputs)));
                    }
                }
                Uncovered::Fields { object, keys: _ } | Uncovered::Object(object) => {
                    widen_object(state, end, &object, top);
                }
                Uncovered::Binding { frame, name } => {
                    let before = state.frames.get(frame).and_then(|f| f.bindings.get(&name));
                    let after = end.frames.get(frame).and_then(|f| f.bindings.get(&name));
                    let summary = summary(state, before, end, after, top);
                    if let Some(target) = state.frames.get_mut(frame) {
                        Rc::make_mut(target).bindings.insert(name, summary);
                    }
                }
                Uncovered::Assumption(test) => {
                    // Each assumption a pass states is carried as itself. At Top the set stops
                    // being compared, so fresh assumptions cannot extend the ladder; every later
                    // pass still states its own on its exits. Facts after the loop keep exactly
                    // the assumptions their passes made (ADR 0072 §10).
                    if top && !assumptions_top {
                        let _bound = tracing::debug_span!(
                            target: "helm_schema_ir::counters", "loop_assumption_top"
                        )
                        .entered();
                        assumptions_top = true;
                    }
                    state
                        .active
                        .push(DecisionEdge { test, taken: true }, LinkKind::Assume);
                }
            }
        }
        self.assumptions_top = assumptions_top;
    }
}

/// Whether `end`, a state a pass from `start` reached, stays inside `start`: every carried object
/// and visible outer binding holds what `start` holds or what a summary there covers, and the
/// pass assumed nothing `start` does not.
pub(super) fn covers(start: &State, end: &State, scope: CarriedScope<'_>) -> bool {
    uncovered(start, end, scope, true, false).is_empty()
}

/// A carried location whose value at a pass's end the start does not cover.
enum Uncovered {
    /// Present fields of an exact map, none aliasing storage.
    Fields {
        object: ObjectId,
        keys: Vec<String>,
    },
    Object(ObjectId),
    Binding {
        frame: usize,
        name: String,
    },
    Assumption(Rc<FrozenTest>),
}

/// The locations `end` leaves uncovered; with `first`, stops at the first one.
fn uncovered(
    start: &State,
    end: &State,
    scope: CarriedScope<'_>,
    first: bool,
    assumptions_top: bool,
) -> Vec<Uncovered> {
    let mut found = Vec::new();
    let known = start
        .active
        .links()
        .into_iter()
        .filter(|link| link.kind() == LinkKind::Assume)
        .map(|link| Rc::as_ptr(&link.edge().test))
        .collect::<BTreeSet<_>>();
    for link in end.active.links() {
        if !assumptions_top
            && link.kind() == LinkKind::Assume
            && !known.contains(&Rc::as_ptr(&link.edge().test))
        {
            found.push(Uncovered::Assumption(Rc::clone(&link.edge().test)));
            if first {
                return found;
            }
        }
    }
    // Innermost frame first: a name is visible in the outermost frame not shadowed by a fresh
    // declaration of the iteration or by a frame inside it.
    let mut shadowed = scope.rebound.iter().collect::<BTreeSet<_>>();
    let frames = start
        .frames
        .iter()
        .zip(&end.frames)
        .take(scope.frames)
        .enumerate()
        .collect::<Vec<_>>();
    for (index, (before, after)) in frames.into_iter().rev() {
        let names = before
            .bindings
            .keys()
            .chain(after.bindings.keys())
            .collect::<BTreeSet<_>>();
        for name in names {
            if !shadowed.insert(name) || Rc::ptr_eq(before, after) {
                continue;
            }
            if !value_covers(before.bindings.get(name), end, after.bindings.get(name)) {
                found.push(Uncovered::Binding {
                    frame: index,
                    name: name.clone(),
                });
                if first {
                    return found;
                }
            }
        }
    }
    for object in changed_objects(start, end) {
        if let Some(location) = object_uncovered(start, end, object) {
            found.push(location);
            if first {
                return found;
            }
        }
    }
    found
}

/// The carried objects whose storage may differ between `start` and `end`: those whose identity
/// or contents the persistent stores do not share, and every object after an unknown write
/// changed the epoch. Carried objects exist at the start or are input maps, whose identity
/// outlives the pass that first read them; objects the pass allocated are reached only through a
/// carried location.
fn changed_objects(start: &State, end: &State) -> BTreeSet<ObjectId> {
    let start_version = &start.heap.version;
    let end_version = &end.heap.version;
    super::heap_store::Store::differing(&start_version.store, &end_version.store)
        .into_iter()
        .filter(|object| {
            start_version.has_allocated_object(object)
                || end_version.input_paths.get(object).is_some()
        })
        .cloned()
        .collect()
}

/// How `end` leaves `object` uncovered, if it does.
fn object_uncovered(start: &State, end: &State, object: ObjectId) -> Option<Uncovered> {
    let before = start.heap.version.object(&object);
    if let Some(before) = &before
        && let ObjectData::Widened { inputs } = &before.data
        && inputs.contains(&ValuesPath::default())
    {
        // The top summary covers every storage.
        return None;
    }
    let after = end.heap.version.object(&object);
    let (before, after) = match (before, after) {
        (None, None) => return None,
        (None, Some(_)) => {
            // Materialize the entry's actual remainder, including inherited merge layers.
            // The arena gives this input the same identity in both versions.
            let Some(path) = end.heap.version.input_paths.get(&object) else {
                return Some(Uncovered::Object(object));
            };
            let mut baseline = start.clone();
            baseline.input_object(path);
            return object_uncovered(&baseline, end, object);
        }
        (Some(_), None) => return Some(Uncovered::Object(object)),
        (Some(before), Some(after)) => (before, after),
    };
    if Rc::ptr_eq(&before, &after) || before.data == after.data {
        return None;
    }
    match (&before.data, &after.data) {
        (ObjectData::Widened { inputs }, _) => object_inputs(end, &object, &after)
            .is_none_or(|after_inputs| !after_inputs.is_subset(inputs))
            .then_some(Uncovered::Object(object)),
        (
            ObjectData::Map {
                fields: before_fields,
                remainder: before_remainder,
            },
            ObjectData::Map {
                fields: after_fields,
                remainder: after_remainder,
            },
        ) if before_remainder == after_remainder => {
            let keys = before_fields
                .keys()
                .chain(after_fields.keys())
                .collect::<BTreeSet<_>>();
            let mut changed = Vec::new();
            for key in keys {
                match (before_fields.get(key), after_fields.get(key)) {
                    (Some(before_field), Some(after_field)) => {
                        if field_covers(before_field, end, after_field) {
                            continue;
                        }
                        match (before_field, after_field) {
                            (Field::Present(before_value), Field::Present(after_value))
                                if !aliases(before_value) && !aliases(after_value) =>
                            {
                                changed.push(key.clone());
                            }
                            _ => return Some(Uncovered::Object(object)),
                        }
                    }
                    (None, None) => {}
                    (Some(_), None) | (None, Some(_)) => return Some(Uncovered::Object(object)),
                }
            }
            (!changed.is_empty()).then_some(Uncovered::Fields {
                object,
                keys: changed,
            })
        }
        (
            ObjectData::Map {
                fields: _,
                remainder: _,
            }
            | ObjectData::Strings(_)
            | ObjectData::Opaque { reachable: _ },
            _,
        ) => Some(Uncovered::Object(object)),
    }
}

/// Raises `object` in `state` to a widened map over both states' contents; `top`, or contents
/// whose inputs exceed the allowance, add the root path, so the result covers every later
/// storage.
fn widen_object(state: &mut State, end: &State, object: &ObjectId, top: bool) {
    if !state.heap.version.has_allocated_object(object)
        && let Some(path) = end.heap.version.input_paths.get(object).cloned()
    {
        // An input map the pass read first starts from the map the input holds.
        state.input_object(&path);
    }
    let Some(before) = state.heap.version.object(object) else {
        return;
    };
    let mut inputs = object_inputs(state, object, &before)
        .unwrap_or_else(|| BTreeSet::from([ValuesPath::default()]));
    if let Some(after) = end.heap.version.object(object) {
        inputs.extend(
            object_inputs(end, object, &after)
                .unwrap_or_else(|| BTreeSet::from([ValuesPath::default()])),
        );
    }
    if top {
        inputs.insert(ValuesPath::default());
    }
    // The join abstracts the state; it is not a read of the program, so it records none.
    let revision = state.heap.arena.snapshot();
    state.heap.install(
        object.clone(),
        ObjectVersion {
            revision,
            multiplicity: before.multiplicity,
            data: ObjectData::Widened {
                inputs: Rc::new(inputs),
            },
        },
    );
}

/// Every input `storage` of `object` in `state` depends on, through its fields, slots, unknown
/// regions and lazy merge layers; `None` when that exceeds the comparison's allowance, which
/// proves no coverage.
fn object_inputs(
    state: &State,
    object: &ObjectId,
    storage: &Rc<ObjectVersion>,
) -> Option<BTreeSet<ValuesPath>> {
    let mut budget = WorkBudget::new(MAX_COVERAGE_WORK);
    let inputs = FrozenValue::object_inputs(&state.heap.version, object, storage, &mut budget);
    (!budget.is_exhausted()).then_some(inputs)
}

fn field_inputs(state: &State, field: &Field) -> BTreeSet<ValuesPath> {
    match field {
        Field::Present(value) => state.heap.freeze(value).inputs(),
        Field::Absent | Field::Unknown => BTreeSet::new(),
        Field::Select {
            decision: _,
            yes,
            no,
        } => {
            let mut inputs = field_inputs(state, yes);
            inputs.extend(field_inputs(state, no));
            inputs
        }
    }
}

/// A summary of both values at a carried binding: no identity, aliasing anything, influenced by
/// every input either reaches (ADR 0072 §6).
fn summary(
    start: &State,
    before: Option<&Value>,
    end: &State,
    after: Option<&Value>,
    top: bool,
) -> Value {
    let mut inputs = before.map_or_else(BTreeSet::new, |value| start.heap.freeze(value).inputs());
    inputs.extend(after.map_or_else(BTreeSet::new, |value| end.heap.freeze(value).inputs()));
    if top {
        inputs.insert(ValuesPath::default());
    }
    Value::influenced(inputs)
}

fn field_covers(before: &Field, end: &State, after: &Field) -> bool {
    before == after
        || matches!((before, after), (Field::Present(before), Field::Present(after))
            if value_covers(Some(before), end, Some(after)))
}

/// Whether `before`, a value at a carried location of a start state, covers `after` in `end`:
/// equal, or a summary whose influences include `after`'s (the root path covers every value).
fn value_covers(before: Option<&Value>, end: &State, after: Option<&Value>) -> bool {
    match (before, after) {
        (None, None) => true,
        (Some(before), Some(after)) if before == after => true,
        (Some(before), Some(after)) => summary_inputs(before).is_some_and(|have| {
            have.contains(&ValuesPath::default()) || end.heap.freeze(after).inputs().is_subset(have)
        }),
        (None, Some(_)) | (Some(_), None) => false,
    }
}

/// The influences of a summary: a value with no identity that may alias anything.
fn summary_inputs(value: &Value) -> Option<&BTreeSet<ValuesPath>> {
    match value.node.as_ref() {
        ValueNode::Leaf(super::Leaf {
            datum:
                super::Datum::Unknown(super::UnknownValue {
                    types: super::TypeCoverage::Unresolved,
                    aliases: Reach::Top,
                }),
            origin:
                super::Origin {
                    derivation: None,
                    identity: None,
                    influences,
                },
        }) => Some(influences),
        ValueNode::Leaf(_)
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => None,
    }
}

/// Whether a value can reach mutable storage, so a summary of its influences would lose an alias.
fn aliases(value: &Value) -> bool {
    match value.node.as_ref() {
        ValueNode::Select {
            decision: _,
            yes,
            no,
        } => aliases(yes) || aliases(no),
        ValueNode::Leaf(super::Leaf { datum, origin: _ }) => match datum {
            super::Datum::Map { view: _, target } => target.is_some(),
            super::Datum::Strings(_)
            | super::Datum::Slot {
                backing: _,
                index: _,
            }
            | super::Datum::Sequence(_) => true,
            super::Datum::Text(_)
            | super::Datum::Scalar {
                go_type: _,
                content: _,
            }
            | super::Datum::Unknown(_) => false,
        },
    }
}
