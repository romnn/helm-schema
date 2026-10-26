use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{Guard, GuardValue, Predicate};
use test_util::prelude::sim_assert_eq;

use super::*;

fn target(value: &Value) -> eyre::Result<ObjectId> {
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    view: _,
                    target: Some(target),
                },
            inputs: _,
        }) => Ok(target.clone()),
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
        })
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => eyre::bail!("expected nonnil map"),
    }
}

fn test(arena: &Arena, coverage: Coverage) -> Rc<FrozenTest> {
    let path = ValuesPath::parse("enabled");
    arena.decision(
        Predicate::from(Guard::Truthy { path: path.clone() }),
        Predicate::from(Guard::Truthy { path }).negated(),
        coverage,
    )
}

#[test]
fn decisions_preserve_correlation_unknown_arms_and_identity() {
    let arena = Arena::default();
    let decision = test(&arena, Coverage::Exact);
    let yes = Value::string("yes");
    let no = Value::string("no");
    let inner = Value::select(Rc::clone(&decision), yes.clone(), no.clone());
    let joined = Value::select(Rc::clone(&decision), inner.clone(), inner);
    sim_assert_eq!(have: joined.under(&decision, true), want: yes);
    sim_assert_eq!(have: joined.under(&decision, false), want: no);

    // Equal predicates at different instants must not collapse independent observations.
    let later = test(&arena, Coverage::Exact);
    let uncertain = Value::select(Rc::clone(&later), Value::string("yes"), Value::unknown());
    sim_assert_eq!(have: uncertain.under(&decision, false), want: uncertain);
    sim_assert_eq!(have: uncertain.under(&later, false), want: Value::unknown());
    sim_assert_eq!(have: Value::select(decision, yes.clone(), yes.clone()), want: yes);
}

#[test]
fn exact_runtime_types_do_not_require_content_or_conflate_nil_containers() {
    for go_type in [
        ScalarType::Nil,
        ScalarType::Bool,
        ScalarType::String,
        ScalarType::Int,
        ScalarType::Int64,
        ScalarType::Uint8,
        ScalarType::Uint64,
        ScalarType::Float64,
        ScalarType::Complex128,
    ] {
        let value = Value::scalar(go_type, None);
        sim_assert_eq!(have: value.node.as_ref(), want: &ValueNode::Leaf(Leaf { datum: Datum::Scalar { go_type, content: None }, inputs: BTreeSet::new() }));
    }
    let mut heap = Heap::new();
    let empty = heap.map(Remainder::Closed);
    for view in [MapType::Interface, MapType::String, MapType::Values] {
        assert_ne!(Value::leaf(Datum::Map { view, target: None }), empty);
    }
    assert_ne!(
        Value::leaf(Datum::Sequence(None)),
        Value::leaf(Datum::Sequence(Some(Rc::new(Vec::new()))))
    );
    assert_ne!(
        Value::leaf(Datum::Unknown(AliasSet::Known(BTreeSet::new()))),
        Value::unknown()
    );
}

#[test]
fn aliases_share_current_writes_but_snapshots_pin_older_versions() -> eyre::Result<()> {
    let mut heap = Heap::new();
    let map = heap.map(Remainder::Closed);
    let id = target(&map)?;
    assert!(heap.write(&id, "x", Field::Present(Value::string("old"))));
    let alias = map.clone();
    let frozen = heap.freeze(&map);
    let old_version = heap.version.id;
    assert!(heap.write(&id, "x", Field::Present(Value::string("new"))));
    assert_ne!(heap.version.id, old_version);
    sim_assert_eq!(have: frozen.read_field("x").field, want: Field::Present(Value::string("old")));
    sim_assert_eq!(have: heap.freeze(&alias).read_field("x").field, want: Field::Present(Value::string("new")));
    sim_assert_eq!(have: frozen.heap.id, want: old_version);
    Ok(())
}

#[test]
fn fork_isolation_and_allocation_identity_survive_equal_contents() -> eyre::Result<()> {
    let mut heap = Heap::new();
    let map = heap.map(Remainder::Closed);
    let id = target(&map)?;
    let mut fork = heap.fork();
    sim_assert_eq!(have: heap.version.id, want: fork.version.id);
    assert!(Rc::ptr_eq(
        heap.version.pages.get(&0).ok_or_eyre("missing page")?,
        fork.version.pages.get(&0).ok_or_eyre("missing fork page")?
    ));
    assert!(fork.write(&id, "x", Field::Present(Value::string("branch"))));
    sim_assert_eq!(have: heap.version.field(&id, "x"), want: Field::Absent);
    sim_assert_eq!(have: fork.version.field(&id, "x"), want: Field::Present(Value::string("branch")));
    assert_ne!(
        target(&heap.map(Remainder::Closed))?,
        target(&fork.map(Remainder::Closed))?
    );
    Ok(())
}

#[test]
fn open_maps_distinguish_tombstones_present_nil_and_unseen_fields() -> eyre::Result<()> {
    let mut heap = Heap::new();
    let map = heap.map(Remainder::Open);
    let id = target(&map)?;
    sim_assert_eq!(have: heap.version.field(&id, "x"), want: Field::Unknown);
    let nil = Value::scalar(
        ScalarType::Nil,
        Some(ScalarContent::Literal(GuardValue::Null)),
    );
    assert!(heap.write(&id, "x", Field::Present(nil.clone())));
    sim_assert_eq!(have: heap.version.field(&id, "x"), want: Field::Present(nil));
    assert!(heap.write(&id, "x", Field::Absent));
    sim_assert_eq!(have: heap.version.field(&id, "x"), want: Field::Absent);
    sim_assert_eq!(have: heap.version.field(&id, "other"), want: Field::Unknown);
    Ok(())
}

#[test]
fn summary_updates_keep_the_old_absence_alternative() -> eyre::Result<()> {
    let mut heap = Heap::new();
    let id = heap.allocate(
        ObjectData::Map {
            fields: BTreeMap::new(),
            remainder: Remainder::Closed,
        },
        Multiplicity::Summary,
    );
    assert!(heap.write(&id, "x", Field::Present(Value::string("maybe"))));
    let field = heap.version.field(&id, "x");
    let Field::Select { decision, yes, no } = field else {
        eyre::bail!("weak write discarded a feasible absence");
    };
    sim_assert_eq!(have: decision.coverage, want: Coverage::Partial);
    sim_assert_eq!(have: *yes, want: Field::Present(Value::string("maybe")));
    sim_assert_eq!(have: *no, want: Field::Absent);
    assert!(heap.write(&id, "x", Field::Absent));
    assert_ne!(heap.version.field(&id, "x"), Field::Absent);
    Ok(())
}

#[test]
fn scalar_consumption_and_emission_detach_live_string_slots() {
    let mut heap = Heap::new();
    let backing = heap.allocate(
        ObjectData::Strings(vec![Value::string("old")]),
        Multiplicity::Singleton,
    );
    let slot = Value::leaf(Datum::Slot {
        backing: backing.clone(),
        index: 0,
    });
    let copied = heap.version.consume(&slot);
    let emitted = heap.freeze(&slot);
    assert!(heap.write_slot(&backing, 0, Value::string("new")));
    sim_assert_eq!(have: heap.version.consume(&slot), want: Value::string("new"));
    sim_assert_eq!(have: copied, want: Value::string("old"));
    sim_assert_eq!(have: emitted.value, want: Value::string("old"));
    assert!(!heap.write_slot(&backing, 1, Value::string("outside")));
}

#[test]
fn lexical_assignment_survives_scope_and_call_boundaries_isolate_locals() -> eyre::Result<()> {
    let root = Value::string("root");
    let mut state = State::new(root.clone());
    state.bind("outer".to_owned(), Value::string("old"));
    state.enter_scope(None);
    state.bind("header".to_owned(), Value::string("header"));
    assert!(state.assign("outer", Value::string("assigned")));
    sim_assert_eq!(have: state.dot(), want: Some(&root));
    assert!(state.leave_scope());
    assert!(state.read_local("header").is_none());
    sim_assert_eq!(have: state.read_local("outer"), want: Some(&Value::string("assigned")));

    // Root is an ordinary binding; rebinding it does not change dot.
    assert!(state.assign("$", Value::string("rebound")));
    sim_assert_eq!(have: state.read_root(), want: Some(&Value::string("rebound")));
    sim_assert_eq!(have: state.dot(), want: Some(&root));
    let map = state.heap.map(Remainder::Closed);
    let id = target(&map)?;
    let caller_loop = state.loop_id(0);
    state.enter_call(map.clone());
    assert!(state.read_local("outer").is_none());
    assert!(!state.assign("outer", Value::string("must not escape")));
    assert_ne!(state.loop_id(0), caller_loop);
    sim_assert_eq!(have: state.read_root(), want: Some(&map));
    sim_assert_eq!(have: state.dot(), want: Some(&map));
    state.enter_scope(Some(Value::string("with")));
    assert!(
        state
            .heap
            .write(&id, "x", Field::Present(Value::string("helper")))
    );
    assert!(state.leave_call());
    sim_assert_eq!(have: state.heap.version.field(&id, "x"), want: Field::Present(Value::string("helper")));
    sim_assert_eq!(have: state.dot(), want: Some(&root));
    assert!(!state.leave_call());
    assert!(!state.leave_scope());
    Ok(())
}

#[test]
fn flow_never_treats_unknown_break_continue_or_truncation_as_abort() -> eyre::Result<()> {
    let state = State::new(Value::unknown());
    let loop_id = state.loop_id(0).ok_or_eyre("missing call frame")?;
    for flow in [
        Flow::Normal(Normal {
            value: Value::unknown(),
            output: Vec::new(),
        }),
        Flow::Break(loop_id),
        Flow::Continue(loop_id),
    ] {
        match &flow {
            Flow::Normal(Normal { value, output }) => {
                sim_assert_eq!(have: value, want: &Value::unknown());
                assert!(output.is_empty());
            }
            Flow::Break(id) | Flow::Continue(id) => sim_assert_eq!(have: id, want: &loop_id),
            Flow::Abort(reason) => eyre::bail!("unexpected abort: {reason:?}"),
        }
        assert!(
            !CallOutcome {
                exits: vec![Exit {
                    state: state.clone(),
                    flow
                }],
                completeness: Completeness::Complete
            }
            .definitely_aborts()
        );
    }
    for reason in [
        AbortReason::Arity,
        AbortReason::Type,
        AbortReason::Required,
        AbortReason::Fail,
    ] {
        let outcome = CallOutcome {
            exits: vec![Exit {
                state: state.clone(),
                flow: Flow::Abort(reason),
            }],
            completeness: Completeness::Complete,
        };
        assert!(outcome.definitely_aborts());
    }
    assert!(
        !CallOutcome {
            exits: vec![Exit {
                state: state.clone(),
                flow: Flow::Abort(AbortReason::Fail)
            }],
            completeness: Completeness::Truncated
        }
        .definitely_aborts()
    );
    assert!(
        !CallOutcome {
            exits: Vec::new(),
            completeness: Completeness::Complete
        }
        .definitely_aborts()
    );
    let partial = state
        .heap
        .arena
        .decision(Predicate::False, Predicate::False, Coverage::Partial);
    assert!(
        !CallOutcome {
            exits: vec![Exit {
                state: state.fork(partial, true),
                flow: Flow::Abort(AbortReason::Fail)
            }],
            completeness: Completeness::Complete
        }
        .definitely_aborts()
    );
    Ok(())
}

#[test]
fn invocation_owns_resolved_operands_and_never_syntax_or_replayed_writes() {
    for callee in [Callee::Function("set".to_owned()), Callee::Program(2)] {
        let invocation = Invocation {
            callee,
            operands: vec![
                Operand {
                    value: Value::string("literal"),
                    site: 4,
                    form: ArgumentForm::DirectLiteral,
                },
                Operand {
                    value: Value::unknown(),
                    site: 5,
                    form: ArgumentForm::Evaluated,
                },
            ],
            site: 3,
        };
        let Invocation {
            callee,
            operands,
            site,
        } = invocation;
        sim_assert_eq!(have: site, want: 3);
        match callee {
            Callee::Function(name) => sim_assert_eq!(have: name, want: "set"),
            Callee::Program(id) => sim_assert_eq!(have: id, want: 2),
        }
        let fields = operands
            .into_iter()
            .map(|Operand { value, site, form }| (value, site, form))
            .collect::<Vec<_>>();
        sim_assert_eq!(have: fields, want: vec![(Value::string("literal"), 4, ArgumentForm::DirectLiteral), (Value::unknown(), 5, ArgumentForm::Evaluated)]);
    }
}

#[test]
fn heap_join_uses_one_frozen_decision_for_aliases_slots_and_absence() -> eyre::Result<()> {
    let mut heap = Heap::new();
    let map = heap.map(Remainder::Closed);
    let id = target(&map)?;
    let strings = heap.allocate(
        ObjectData::Strings(vec![Value::string("before")]),
        Multiplicity::Singleton,
    );
    let frozen = heap.freeze(&map);
    let decision = test(&heap.arena, Coverage::Exact);
    let mut yes = heap.fork();
    let mut no = heap.fork();
    assert!(yes.write(&id, "x", Field::Present(Value::string("yes"))));
    assert!(yes.write_slot(&strings, 0, Value::string("yes")));
    assert!(no.write_slot(&strings, 0, Value::string("no")));
    let child = yes.map(Remainder::Closed);
    assert!(yes.write(&id, "child", Field::Present(child.clone())));
    heap.join(&decision, &yes, &no)?;
    sim_assert_eq!(have: heap.version.field(&id, "x"), want: Field::select(Rc::clone(&decision), Field::Present(Value::string("yes")), Field::Absent));
    sim_assert_eq!(have: frozen.read_field("x").field, want: Field::Absent);
    let slot = Value::leaf(Datum::Slot {
        backing: strings,
        index: 0,
    });
    let joined = heap.version.consume(&slot);
    sim_assert_eq!(have: joined.under(&decision, true), want: Value::string("yes"));
    sim_assert_eq!(have: joined.under(&decision, false), want: Value::string("no"));
    assert!(heap.version.object(&target(&child)?).is_some());
    assert!(heap.join(&decision, &yes, &Heap::new()).is_err());
    Ok(())
}

#[test]
fn rejection_needs_every_exact_path_and_every_runtime_alternative() {
    let state = State::new(Value::unknown());
    let decision = test(&state.heap.arena, Coverage::Exact);
    let yes = state.fork(Rc::clone(&decision), true);
    let no = state.fork(decision, false);
    let first = Exit {
        state: yes,
        flow: Flow::Abort(AbortReason::Required),
    };
    let mut outcome = CallOutcome {
        exits: vec![first],
        completeness: Completeness::Complete,
    };
    assert!(!outcome.definitely_aborts());
    outcome.exits.push(Exit {
        state: no,
        flow: Flow::Abort(AbortReason::Type),
    });
    assert!(outcome.definitely_aborts());
    outcome.exits.push(Exit {
        state,
        flow: Flow::Normal(Normal {
            value: Value::unknown(),
            output: Vec::new(),
        }),
    });
    assert!(!outcome.definitely_aborts());
}

#[test]
fn frozen_nested_queries_keep_the_heap_of_the_original_observation() -> eyre::Result<()> {
    let mut heap = Heap::new();
    let outer = heap.map(Remainder::Closed);
    let nested = heap.map(Remainder::Closed);
    let backing = heap.allocate(
        ObjectData::Strings(vec![Value::string("old")]),
        Multiplicity::Singleton,
    );
    let slot = Value::leaf(Datum::Slot {
        backing: backing.clone(),
        index: 0,
    });
    assert!(heap.write(&target(&nested)?, "slot", Field::Present(slot)));
    assert!(heap.write(&target(&outer)?, "nested", Field::Present(nested)));
    let frozen = heap.freeze(&outer);
    assert!(heap.write_slot(&backing, 0, Value::string("new")));
    let old_nested = frozen
        .read_field("nested")
        .present()
        .ok_or_eyre("missing frozen nested map")?;
    let old_slot = old_nested
        .read_field("slot")
        .present()
        .ok_or_eyre("missing frozen nested slot")?;
    sim_assert_eq!(have: old_slot.value, want: Value::string("old"));
    let current_nested = heap
        .freeze(&outer)
        .read_field("nested")
        .present()
        .ok_or_eyre("missing current nested map")?;
    let current_slot = current_nested
        .read_field("slot")
        .present()
        .ok_or_eyre("missing current nested slot")?;
    sim_assert_eq!(have: current_slot.value, want: Value::string("new"));
    Ok(())
}

#[test]
fn summary_slot_write_keeps_old_content_and_frozen_history() -> eyre::Result<()> {
    let mut heap = Heap::new();
    let backing = heap.allocate(
        ObjectData::Strings(vec![Value::string("old")]),
        Multiplicity::Summary,
    );
    let slot = Value::leaf(Datum::Slot {
        backing: backing.clone(),
        index: 0,
    });
    let frozen = heap.freeze(&slot);
    assert!(heap.write_slot(&backing, 0, Value::string("new")));
    let value = heap.version.consume(&slot);
    let ValueNode::Select { decision, yes, no } = value.node.as_ref() else {
        eyre::bail!("summary slot lost an alternative");
    };
    sim_assert_eq!(have: decision.coverage, want: Coverage::Partial);
    sim_assert_eq!(have: yes, want: &Value::string("new"));
    sim_assert_eq!(have: no, want: &Value::string("old"));
    sim_assert_eq!(have: frozen.value, want: Value::string("old"));
    Ok(())
}

#[test]
fn separate_calls_at_equal_depth_have_distinct_loop_identities() {
    let mut state = State::new(Value::unknown());
    state.enter_call(Value::unknown());
    let first = state.loop_id(0);
    assert!(state.leave_call());
    state.enter_call(Value::unknown());
    assert_ne!(state.loop_id(0), first);
}
