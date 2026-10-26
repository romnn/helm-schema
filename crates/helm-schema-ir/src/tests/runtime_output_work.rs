use super::{TextBuffer, TextPart, TextQuery, TextRetention};
use crate::value::{Coverage, State, Value};
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

struct SharedOutput {
    state: State,
    buffer: TextBuffer,
}

fn shared_output(depth: usize, unknown: bool) -> SharedOutput {
    let state = State::new(Value::nil());
    let mut left = TextBuffer::default();
    left.bytes("x");
    let mut right = TextBuffer::default();
    if unknown {
        right.push(TextPart::Unknown(std::collections::BTreeSet::from([
            ValuesPath::parse("input"),
        ])));
    } else {
        right.emit(state.heap.freeze(&Value::string("x")));
    }
    for index in 0..depth {
        let condition = Predicate::truthy_path(format!("left{index}"));
        let left_decision =
            state
                .heap
                .arena
                .decision(condition.clone(), condition.negated(), Coverage::Exact);
        let condition = Predicate::truthy_path(format!("right{index}"));
        let right_decision =
            state
                .heap
                .arena
                .decision(condition.clone(), condition.negated(), Coverage::Exact);
        let next_left = TextBuffer::join(left_decision, &left, &right, &state.heap.arena);
        right = TextBuffer::join(right_decision, &left, &right, &state.heap.arena);
        left = next_left;
    }
    SharedOutput {
        state,
        buffer: left,
    }
}

#[test]
fn exact_shared_output_queries_visit_buffers_instead_of_execution_paths() {
    for depth in [8, 16, 32, 48] {
        let SharedOutput { state, buffer } = shared_output(depth, false);
        let mut query = TextQuery::default();
        sim_assert_eq!(have: query.is_widened_under(&buffer, &crate::value::guard::RuntimeGuard::True, &state.heap.arena), want: false);
        assert!(query.uncertainty.len() <= 4 * depth + 3);
        sim_assert_eq!(have: query.widened.len(), want: 0);
        sim_assert_eq!(have: query.exact_text(&buffer), want: Some("x".to_owned()));
        assert!(query.text.len() <= 4 * depth + 3);
        sim_assert_eq!(have: query.as_value(&buffer).constant().cloned(), want: Some(helm_schema_core::GuardValue::String("x".to_owned())));
    }
}

#[test]
fn uncertain_shared_output_conversion_reuses_scalar_and_text_subqueries() {
    for depth in [8, 16, 32] {
        let SharedOutput { state: _, buffer } = shared_output(depth, true);
        let mut query = TextQuery::default();
        sim_assert_eq!(have: query.exact_text(&buffer), want: None);
        let value = query.as_value(&buffer);
        sim_assert_eq!(have: value.inputs(), want: std::collections::BTreeSet::from([ValuesPath::parse("input")]));
        assert!(query.text.len() <= 4 * depth + 3);
        assert!(query.values.len() <= 4 * depth + 3);
        sim_assert_eq!(have: buffer.inputs(), want: std::collections::BTreeSet::from([ValuesPath::parse("input")]));
    }
}

#[test]
fn repeated_output_overflow_releases_unreachable_frozen_heap_snapshots() -> eyre::Result<()> {
    use crate::value::{Field, Remainder};
    use std::rc::Rc;
    for selected in [false, true] {
        let initial = State::new(Value::nil());
        let mut state = if selected {
            let condition = Predicate::truthy_path("active");
            let decision = initial.heap.arena.decision(
                condition.clone(),
                condition.negated(),
                Coverage::Exact,
            );
            initial.fork(decision, true)
        } else {
            initial
        };
        state.output.restrict_budget(
            &state.heap.arena.runtime_path(&state.active),
            &state.heap.arena,
        );
        let map = state.heap.map(Remainder::Closed);
        let object = state
            .map_target(&map)
            .ok_or_eyre("mutable output map missing")?;
        let mut snapshots = Vec::new();
        for index in 1..=385 {
            state.heap.write(
                &object,
                "value",
                Field::Present(Value::string(&index.to_string())),
            );
            snapshots.push(Rc::downgrade(&state.heap.version));
            state.output.emit(state.heap.freeze(&map));
            if [129, 257, 385].contains(&index) {
                sim_assert_eq!(have: state.output.parts.len(), want: 1);
                assert!(matches!(
                    state.output.parts.as_slice(),
                    [TextPart::Unknown(_)]
                ));
                assert!(
                    snapshots
                        .iter()
                        .take(index - 1)
                        .all(|snapshot| snapshot.upgrade().is_none()),
                    "selected={selected}, emission={index}"
                );
            }
        }
        state
            .heap
            .write(&object, "value", Field::Present(Value::string("after")));
        assert!(
            snapshots
                .iter()
                .all(|snapshot| snapshot.upgrade().is_none()),
            "selected={selected}"
        );
    }
    Ok(())
}

#[test]
fn phase_offset_overflows_release_snapshots_after_both_paths_widen() -> eyre::Result<()> {
    use crate::value::{Field, Remainder};
    use std::rc::Rc;
    let mut state = State::new(Value::nil());
    let condition = Predicate::truthy_path("phase");
    let decision =
        state
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let mut first_phase = TextBuffer::default();
    first_phase.bytes("seed");
    state.output = TextBuffer::join(
        decision,
        &first_phase,
        &TextBuffer::default(),
        &state.heap.arena,
    );
    let map = state.heap.map(Remainder::Closed);
    let object = state
        .map_target(&map)
        .ok_or_eyre("mutable phase output map missing")?;
    let mut snapshots = Vec::new();
    for index in 1..=385 {
        state.heap.write(
            &object,
            "value",
            Field::Present(Value::string(&index.to_string())),
        );
        snapshots.push(Rc::downgrade(&state.heap.version));
        state.output.emit(state.heap.freeze(&map));
        if [129, 257, 385].contains(&index) {
            assert!(
                snapshots
                    .iter()
                    .take(index - 1)
                    .all(|snapshot| snapshot.upgrade().is_none()),
                "emission={index}"
            );
        }
    }
    Ok(())
}

#[test]
fn retaining_an_unrelated_domain_keeps_the_original_shared_output_identity() {
    use crate::value::DecisionEdge;
    use std::rc::Rc;
    let SharedOutput { state, buffer } = shared_output(24, false);
    let original = Rc::new(buffer);
    let condition = Predicate::truthy_path("unrelated");
    let decision =
        state
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let allowed = state.heap.arena.runtime_path(&[DecisionEdge {
        test: decision,
        taken: true,
    }]);
    let mut retention = TextRetention::default();
    let retained = retention.retain(&original, &allowed);
    assert!(Rc::ptr_eq(&original, &retained));
    assert!(retention.buffers.len() <= 4 * 24 + 3);
}

#[test]
fn retention_rewrites_only_the_forced_path_and_keeps_untouched_sibling_identity() -> eyre::Result<()>
{
    use crate::value::DecisionEdge;
    use std::rc::Rc;
    let state = State::new(Value::nil());
    let condition = Predicate::truthy_path("inner");
    let inner = state
        .heap
        .arena
        .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let allowed = state.heap.arena.runtime_path(&[DecisionEdge {
        test: Rc::clone(&inner),
        taken: true,
    }]);
    let condition = Predicate::truthy_path("outer");
    let outer = state
        .heap
        .arena
        .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let mut yes = TextBuffer::default();
    yes.bytes("yes");
    let mut no = TextBuffer::default();
    no.bytes("no");
    let nested = TextBuffer::join(inner, &yes, &no, &state.heap.arena);
    let mut untouched = TextBuffer::default();
    untouched.bytes("untouched");
    let original = Rc::new(TextBuffer::join(
        outer,
        &nested,
        &untouched,
        &state.heap.arena,
    ));
    let [
        TextPart::Select {
            decision: _,
            yes: original_yes,
            no: original_no,
        },
    ] = original.parts.as_slice()
    else {
        eyre::bail!("original selection missing");
    };
    let retained = TextRetention::default().retain(&original, &allowed);
    assert!(!Rc::ptr_eq(&original, &retained));
    let [
        TextPart::Select {
            decision: _,
            yes: retained_yes,
            no: retained_no,
        },
    ] = retained.parts.as_slice()
    else {
        eyre::bail!("retained outer selection missing");
    };
    assert!(!Rc::ptr_eq(original_yes, retained_yes));
    assert!(Rc::ptr_eq(original_no, retained_no));
    sim_assert_eq!(have: retained_yes.exact_text(), want: Some("yes".to_owned()));
    Ok(())
}

#[test]
fn immutable_output_children_do_not_retain_historical_budget_guards() -> eyre::Result<()> {
    use crate::value::DecisionEdge;
    use crate::value::guard::RuntimeGuard;
    use std::rc::Rc;
    let state = State::new(Value::nil());
    let mut edges = Vec::new();
    for path in ["first", "second"] {
        let condition = Predicate::truthy_path(path);
        let test =
            state
                .heap
                .arena
                .decision(condition.clone(), condition.negated(), Coverage::Exact);
        edges.push(DecisionEdge { test, taken: true });
    }
    let historical = state.heap.arena.runtime_path(&edges);
    let RuntimeGuard::Branch(node) = &historical else {
        eyre::bail!("historical composite guard missing");
    };
    let weak = Rc::downgrade(node);
    let mut yes = TextBuffer::default();
    yes.bytes("yes");
    yes.restrict_budget(&historical, &state.heap.arena);
    let mut no = TextBuffer::default();
    no.bytes("no");
    no.restrict_budget(&historical, &state.heap.arena);
    let condition = Predicate::truthy_path("choice");
    let decision =
        state
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let mut output = TextBuffer::join(Rc::clone(&decision), &yes, &no, &state.heap.arena);
    assert!(!output.counts.is_empty());
    assert!(output.arena.is_some());
    let [
        TextPart::Select {
            decision: _,
            yes: child_yes,
            no: child_no,
        },
    ] = output.parts.as_slice()
    else {
        eyre::bail!("joined selection missing");
    };
    for child in [child_yes, child_no] {
        assert!(child.counts.is_empty());
        assert!(child.arena.is_none());
    }
    drop(yes);
    drop(no);
    drop(historical);
    output.counts = super::BranchCounts::default();
    // The bounded cache working set releases guards at its normal collection threshold.
    for index in 0..4096 {
        let condition = Predicate::truthy_path(format!("temporary{index}"));
        let test =
            state
                .heap
                .arena
                .decision(condition.clone(), condition.negated(), Coverage::Exact);
        state
            .heap
            .arena
            .runtime_path(&[DecisionEdge { test, taken: true }]);
    }
    state.heap.arena.budget_test(RuntimeGuard::True);
    assert!(weak.upgrade().is_none());
    sim_assert_eq!(have: output.as_value().under(&decision, true).constant().cloned(), want: Some(helm_schema_core::GuardValue::String("yes".to_owned())));
    sim_assert_eq!(have: output.as_value().under(&decision, false).constant().cloned(), want: Some(helm_schema_core::GuardValue::String("no".to_owned())));
    Ok(())
}
