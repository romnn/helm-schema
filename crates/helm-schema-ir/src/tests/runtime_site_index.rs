use std::collections::BTreeSet;
use std::rc::Rc;

use helm_schema_core::{GuardValue, Predicate};
use helm_schema_syntax::Span;
use test_util::prelude::sim_assert_eq;

use crate::value::{
    Coverage, DocumentExecution, Exit, FileOutput, Flow, Normal, ObservationId, State, Value,
};

use super::Observation;

fn exit(state: State) -> Exit {
    Exit {
        state,
        flow: Flow::Normal(Normal {
            value: Value::nil(),
        }),
    }
}

#[test]
fn repeated_source_sites_keep_each_occurrence_across_history_chunks() {
    let span = Span::new(2, 4);
    let path = Some("templates/repeated.yaml");
    let mut state = State::new(Value::nil());
    state.program = 17;
    state.emit_bytes(Span::new(0, 1), path, "L");
    for index in 0..34 {
        state.observe(span, path, &Value::string(&index.to_string()), true);
    }
    let execution = DocumentExecution::new(vec![exit(state)], FileOutput::Text);

    // Each iteration has the same parsed source site and a distinct observed value.
    sim_assert_eq!(have: (1..35).flat_map(|ordinal| execution.observations(ObservationId { program: 17, ordinal, span })).filter_map(|observation| match observation { Observation::Value(site) => Some(site.ordinal), Observation::Text(_) => None }).collect::<Vec<_>>(), want: (1..35).collect::<Vec<_>>());
    sim_assert_eq!(have: (1..35).flat_map(|ordinal| execution.observations(ObservationId { program: 17, ordinal, span })).filter_map(|observation| match observation { Observation::Value(site) => Some(site.value.literal().cloned()), Observation::Text(_) => None }).collect::<Vec<_>>(), want: (0..34).map(|index| Some(GuardValue::String(index.to_string()))).collect::<Vec<_>>());
    sim_assert_eq!(have: execution.observations(ObservationId { program: 17, ordinal: 0, span: Span::new(0, 1) }).filter_map(|observation| match observation { Observation::Text(site) => Some(site.bytes.clone()), Observation::Value(_) => None }).collect::<Vec<_>>(), want: vec!["L".to_owned()]);
}

#[test]
fn equal_program_and_ordinal_keep_distinct_guarded_records() {
    let span = Span::new(2, 4);
    let path = Some("templates/branch.yaml");
    let mut initial = State::new(Value::nil());
    initial.program = 23;
    let guard = Predicate::truthy_path("choose");
    let decision =
        initial
            .heap
            .arena
            .decision(guard.clone(), guard.clone().negated(), Coverage::Exact);
    let mut left = initial.fork(Rc::clone(&decision), true);
    let mut right = initial.fork(decision, false);
    left.observe(span, path, &Value::string("left"), true);
    right.observe(span, path, &Value::string("right"), true);
    let execution = DocumentExecution::new(vec![exit(left), exit(right)], FileOutput::Text);

    // Neither an equal source span nor an equal branch ordinal merges the observations.
    sim_assert_eq!(have: execution.observations(ObservationId { program: 23, ordinal: 0, span }).filter_map(|observation| match observation { Observation::Value(site) => Some(site.ordinal), Observation::Text(_) => None }).collect::<Vec<_>>(), want: vec![0, 0]);
    sim_assert_eq!(have: execution.observations(ObservationId { program: 23, ordinal: 0, span }).filter_map(|observation| match observation { Observation::Value(site) => Some(site.condition()), Observation::Text(_) => None }).collect::<Vec<_>>(), want: vec![guard.clone().normalize_boolean(), guard.negated().normalize_boolean()]);
    sim_assert_eq!(have: execution.observations(ObservationId { program: 23, ordinal: 0, span }).filter_map(|observation| match observation { Observation::Value(site) => site.value.literal().cloned(), Observation::Text(_) => None }).collect::<Vec<_>>(), want: vec![GuardValue::String("left".to_owned()), GuardValue::String("right".to_owned())]);
}

#[test]
fn different_sites_at_one_partial_branch_ordinal_resolve_separately() {
    let left_span = Span::new(2, 4);
    let right_span = Span::new(8, 10);
    let path = Some("templates/partial.yaml");
    let mut initial = State::new(Value::nil());
    initial.program = 41;
    let decision =
        initial
            .heap
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial);
    let mut left = initial.fork(Rc::clone(&decision), true);
    let mut right = initial.fork(decision, false);
    left.observe(left_span, path, &Value::string("same"), true);
    right.observe(right_span, path, &Value::string("same"), true);
    let left_id = ObservationId {
        program: 41,
        ordinal: 0,
        span: left_span,
    };
    let right_id = ObservationId {
        program: 41,
        ordinal: 0,
        span: right_span,
    };

    // Equal output bytes and partial guards cannot identify which parsed site emitted a node.
    sim_assert_eq!(have: left.output.parts.iter().flat_map(|node| node.occurrences.iter().copied()).collect::<Vec<_>>(), want: vec![left_id]);
    sim_assert_eq!(have: right.output.parts.iter().flat_map(|node| node.occurrences.iter().copied()).collect::<Vec<_>>(), want: vec![right_id]);
    let execution = DocumentExecution::new(vec![exit(left), exit(right)], FileOutput::Text);
    for id in [left_id, right_id] {
        sim_assert_eq!(have: execution.observations(id).filter_map(|observation| match observation { Observation::Value(site) => Some(site.span), Observation::Text(_) => None }).collect::<Vec<_>>(), want: vec![id.span]);
        sim_assert_eq!(have: execution.observations(id).filter_map(|observation| match observation { Observation::Value(site) => Some(site.coverage_exact), Observation::Text(_) => None }).collect::<Vec<_>>(), want: vec![false]);
    }
}

#[test]
fn occurrence_lookup_preserves_program_and_file_identity() {
    struct ProgramCase {
        program: u64,
        text: &'static str,
        source: &'static str,
    }
    let span = Span::new(2, 4);
    let mut first = State::new(Value::nil());
    first.program = 31;
    first.observe(
        span,
        Some("templates/first.yaml"),
        &Value::string("first"),
        true,
    );
    let mut second = State::new(Value::nil());
    second.program = 32;
    second.observe(
        span,
        Some("templates/second.yaml"),
        &Value::string("second"),
        true,
    );
    let mut execution = DocumentExecution::new(vec![exit(first), exit(second)], FileOutput::Text);

    // Body views retain the shared index, including provenance from other programs.
    for ProgramCase {
        program,
        text,
        source,
    } in [
        ProgramCase {
            program: 31,
            text: "first",
            source: "templates/first.yaml",
        },
        ProgramCase {
            program: 32,
            text: "second",
            source: "templates/second.yaml",
        },
    ] {
        execution.root_programs = BTreeSet::from([program]);
        let id = ObservationId {
            program,
            ordinal: 0,
            span,
        };
        sim_assert_eq!(have: execution.is_root_observation(id), want: true);
        sim_assert_eq!(have: execution.observations(id).filter_map(|observation| match observation { Observation::Value(site) => Some(site.value.literal().cloned()), Observation::Text(_) => None }).collect::<Vec<_>>(), want: vec![Some(GuardValue::String(text.to_owned()))]);
        sim_assert_eq!(have: execution.observations(id).filter_map(|observation| match observation { Observation::Value(site) => Some(site.source_path.as_deref()), Observation::Text(_) => None }).collect::<Vec<_>>(), want: vec![Some(source)]);
    }
    sim_assert_eq!(have: execution.observations(ObservationId { program: 0, ordinal: 0, span }).count(), want: 0);
}
