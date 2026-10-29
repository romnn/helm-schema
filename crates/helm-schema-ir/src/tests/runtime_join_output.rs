use color_eyre::eyre;
use helm_schema_ast::DefineIndex;
use helm_schema_core::GuardValue;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::Flow;
use crate::value::tests::execute_document;

#[test]
fn output_budget_neighbors_keep_exact_text_and_all_widened_dependencies() {
    use std::collections::BTreeSet;

    use crate::value::{State, Value};
    use helm_schema_core::ValuesPath;

    for count in [127, 128, 129] {
        let mut state = State::new(Value::nil());
        let paths = (0..count)
            .map(|index| ValuesPath::parse(&format!("items.{index}")))
            .collect::<BTreeSet<_>>();
        for path in &paths {
            // Known text can still carry dependencies from the expression that produced it.
            let value = Value::string("x").with_influences(BTreeSet::from([path.clone()]));
            state.output.emit(state.heap.freeze(&value));
        }
        let captured = state
            .heap
            .freeze(&crate::value::text::capture(&state.output, false));

        // Exact neighbors remain renderable; overflow keeps every influencing input.
        sim_assert_eq!(
            have: captured.exact_text(),
            want: (count <= 128).then(|| "x".repeat(count)),
            "count={count}"
        );
        if count > 128 {
            sim_assert_eq!(have: captured.inputs(), want: paths);
        }
    }
}

#[test]
fn thirty_two_element_ranges_keep_both_surviving_output_histories() {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        indoc! {r#"
        {{- $xs := splitList "" "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" -}}
        {{- if .Values.a -}}
          {{- if .Values.b -}}{{- range $xs -}}L{{ . }}{{- end -}}
          {{- else -}}{{- fail "b" -}}{{- end -}}
        {{- else -}}{{- range $xs -}}R{{ . }}{{- end -}}{{- end -}}
    "#},
        None,
        &db,
    );
    for exit in &execution.exits {
        if !matches!(exit.flow, Flow::Normal(_)) {
            continue;
        }
        sim_assert_eq!(have: exit.state.output_widened, want: false);
    }
    let values = execution
        .text()
        .map(|(_, text)| text.as_value())
        .collect::<Vec<_>>();
    sim_assert_eq!(have: values.len(), want: 1);
    let mut pending = values;
    let mut leaves = Vec::new();
    while let Some(value) = pending.pop() {
        match value.node.as_ref() {
            crate::value::ValueNode::Select {
                decision: _,
                yes,
                no,
            } => pending.extend([yes.clone(), no.clone()]),
            crate::value::ValueNode::Leaf(_) => {
                if let Some(GuardValue::String(text)) = value.constant() {
                    leaves.push(text.clone());
                }
            }
        }
    }
    leaves.sort();
    sim_assert_eq!(have: leaves, want: vec!["Lx".repeat(32), "Rx".repeat(32)]);
}

#[test]
fn joined_outputs_charge_only_the_selected_branch_and_keep_suffixes() -> eyre::Result<()> {
    use crate::value::state::ContinuationJoin;
    use crate::value::{Coverage, State, Value};
    use color_eyre::eyre::OptionExt as _;
    use std::rc::Rc;
    for count in [63, 64, 127, 128, 129] {
        let initial = State::new(Value::nil());
        let predicate = helm_schema_core::Predicate::from(helm_schema_core::Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("a"),
        });
        let decision =
            initial
                .heap
                .arena
                .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        let mut yes = initial.fork(Rc::clone(&decision), true);
        let mut no = initial.fork(Rc::clone(&decision), false);
        for _ in 0..count {
            yes.output.bytes("L");
            no.output.bytes("R");
        }
        let mut joined = State::join_branches(
            &ContinuationJoin {
                decision: Rc::clone(&decision),
                active: Vec::new(),
                taken: true,
            },
            &yes,
            &no,
        )
        .ok_or_eyre("compatible branches must join")?;
        for taken in [true, false] {
            sim_assert_eq!(have: joined.output.as_value().under(&decision, taken).constant().cloned(), want: if count <= 128 {Some(GuardValue::String(if taken {"L"} else {"R"}.repeat(count)))} else {None});
        }
        joined.output.bytes("!");
        if count < 128 {
            sim_assert_eq!(have: joined.output.as_value().under(&decision, true).constant().cloned(), want: Some(GuardValue::String(format!("{}!", "L".repeat(count)))));
        }
    }
    Ok(())
}

#[test]
fn complementary_output_choices_do_not_add_impossible_branch_costs() -> eyre::Result<()> {
    use crate::value::state::ContinuationJoin;
    use crate::value::{Coverage, State, Value};
    use color_eyre::eyre::OptionExt as _;
    use std::rc::Rc;
    let initial = State::new(Value::nil());
    let condition = helm_schema_core::Predicate::from(helm_schema_core::Guard::Truthy {
        path: helm_schema_core::ValuesPath::parse("a"),
    });
    let decision =
        initial
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let plan = ContinuationJoin {
        decision: Rc::clone(&decision),
        active: Vec::new(),
        taken: true,
    };
    let mut yes = initial.fork(Rc::clone(&decision), true);
    let mut no = initial.fork(Rc::clone(&decision), false);
    for _ in 0..100 {
        yes.output.bytes("x");
    }
    no.output.bytes("y");
    let joined = State::join_branches(&plan, &yes, &no).ok_or_eyre("first output join")?;
    let mut yes = joined.fork(Rc::clone(&decision), true);
    let mut no = joined.fork(Rc::clone(&decision), false);
    yes.output.bytes("y");
    for _ in 0..100 {
        no.output.bytes("x");
    }
    let mut joined = State::join_branches(&plan, &yes, &no).ok_or_eyre("second output join")?;
    joined.output.bytes("!");
    for taken in [true, false] {
        sim_assert_eq!(have: joined.output.as_value().under(&decision, taken).constant().cloned(), want: Some(GuardValue::String(if taken {format!("{}y!", "x".repeat(100))} else {format!("y{}!", "x".repeat(100))})));
    }
    Ok(())
}

#[test]
fn observation_union_keeps_full_branch_order_and_reserves_suffix_per_branch() -> eyre::Result<()> {
    use crate::value::state::ContinuationJoin;
    use crate::value::{Coverage, State, Value};
    use color_eyre::eyre::OptionExt as _;
    use std::rc::Rc;
    let initial = State::new(Value::nil());
    let condition = helm_schema_core::Predicate::from(helm_schema_core::Guard::Truthy {
        path: helm_schema_core::ValuesPath::parse("a"),
    });
    let decision =
        initial
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let mut yes = initial.fork(Rc::clone(&decision), true);
    let mut no = initial.fork(Rc::clone(&decision), false);
    for index in 0..100 {
        yes.emit_bytes(helm_schema_syntax::Span::new(index, index + 1), None, "L");
        no.emit_bytes(helm_schema_syntax::Span::new(index, index + 1), None, "R");
    }
    let mut joined = State::join_branches(
        &ContinuationJoin {
            decision: Rc::clone(&decision),
            active: Vec::new(),
            taken: true,
        },
        &yes,
        &no,
    )
    .ok_or_eyre("compatible histories must join")?;
    sim_assert_eq!(have: joined.text_sites.len(), want: 200);
    for index in 100..128 {
        joined.emit_bytes(helm_schema_syntax::Span::new(index, index + 1), None, "!");
    }
    sim_assert_eq!(have: joined.text_sites.len(), want: 228);
    for branch in [&yes, &no] {
        let condition = branch.path_predicate();
        let ordinals = joined
            .text_sites
            .iter()
            .filter(|site| {
                helm_schema_core::Predicate::all(vec![condition.clone(), site.condition()])
                    .normalize_boolean()
                    != helm_schema_core::Predicate::False
            })
            .map(|site| site.ordinal)
            .collect::<Vec<_>>();
        sim_assert_eq!(have: ordinals.len(), want: 128);
        assert!(
            ordinals
                .windows(2)
                .all(|pair| matches!(pair, [first, second] if first < second))
        );
    }
    joined.emit_bytes(helm_schema_syntax::Span::new(128, 129), None, "overflow");
    sim_assert_eq!(have: joined.text_sites.len(), want: 228);
    Ok(())
}

#[test]
fn invocation_union_admits_a_suffix_only_on_branches_with_capacity() -> eyre::Result<()> {
    use crate::value::state::ContinuationJoin;
    use crate::value::{Coverage, ExecutedInvocationRecord, State, Value};
    use color_eyre::eyre::OptionExt as _;
    use std::rc::Rc;
    fn record(state: &mut State, invocation: u64) {
        for slot in state.reserve_invocation() {
            state.invocations.push(ExecutedInvocationRecord {
                condition: slot.condition,
                ordinal: slot.ordinal,
                invocation,
                program: 0,
                site: invocation,
                function: "print".to_owned(),
                wrapper: None,
            });
        }
    }
    let initial = State::new(Value::nil());
    let condition = helm_schema_core::Predicate::from(helm_schema_core::Guard::Truthy {
        path: helm_schema_core::ValuesPath::parse("a"),
    });
    let decision =
        initial
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let mut yes = initial.fork(Rc::clone(&decision), true);
    let mut no = initial.fork(Rc::clone(&decision), false);
    for invocation in 0..128 {
        record(&mut yes, invocation);
    }
    for invocation in 128..192 {
        record(&mut no, invocation);
    }
    let mut joined = State::join_branches(
        &ContinuationJoin {
            decision: Rc::clone(&decision),
            active: Vec::new(),
            taken: true,
        },
        &yes,
        &no,
    )
    .ok_or_eyre("invocation histories join")?;
    sim_assert_eq!(have: joined.invocations.len(), want: 192);
    record(&mut joined, 192);
    sim_assert_eq!(have: joined.invocations.len(), want: 193);
    let suffix = joined
        .invocations
        .iter()
        .filter(|record| record.invocation == 192)
        .map(|record| (record.ordinal, record.condition.clone()))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: suffix, want: vec![(192, no.path_predicate().into())]);
    sim_assert_eq!(have: joined.fork(Rc::clone(&decision), true).invocations_complete(), want: false);
    sim_assert_eq!(have: joined.fork(decision, false).invocations_complete(), want: true);
    Ok(())
}

#[test]
fn restricting_a_join_keeps_the_under_budget_output_and_observations_exact() -> eyre::Result<()> {
    use crate::value::state::ContinuationJoin;
    use crate::value::{Coverage, DocumentExecution, Exit, Normal, State, Value};
    use color_eyre::eyre::OptionExt as _;
    use std::rc::Rc;
    let initial = State::new(Value::nil());
    let condition = helm_schema_core::Predicate::from(helm_schema_core::Guard::Truthy {
        path: helm_schema_core::ValuesPath::parse("a"),
    });
    let decision =
        initial
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let span = helm_schema_syntax::Span::new(0, 1);
    let mut yes = initial.fork(Rc::clone(&decision), true);
    let mut no = initial.fork(Rc::clone(&decision), false);
    for _ in 0..128 {
        yes.emit_bytes(span, None, "L");
    }
    no.emit_bytes(span, None, "R");
    let mut joined = State::join_branches(
        &ContinuationJoin {
            decision: Rc::clone(&decision),
            active: Vec::new(),
            taken: true,
        },
        &yes,
        &no,
    )
    .ok_or_eyre("unequal histories join")?;
    joined.emit_bytes(span, None, "!");
    let mut low = joined.fork(Rc::clone(&decision), false);
    low.emit_bytes(span, None, "?");
    sim_assert_eq!(have: low.output_widened, want: false);
    sim_assert_eq!(have: low.output.as_value().under(&decision, false).constant().cloned(), want: Some(GuardValue::String("R!?".to_owned())));
    let execution = DocumentExecution::new(
        vec![Exit {
            state: low,
            flow: Flow::Normal(Normal {
                value: Value::nil(),
            }),
        }],
        crate::value::FileOutput::Manifest,
    );
    sim_assert_eq!(have: execution.executed_outputs().into_iter().map(|output| (output.condition, output.text)).collect::<Vec<_>>(), want: vec![(no.path_predicate(), Some("R!?".to_owned()))]);
    Ok(())
}

#[test]
fn a_budget_fork_never_restores_intrinsically_incomplete_invocation_history() {
    use crate::value::{Coverage, State, Value};
    use std::rc::Rc;
    let mut state = State::new(Value::nil());
    state.invocation_records_complete = false;
    let decision = state.heap.arena.decision(
        helm_schema_core::Predicate::False,
        helm_schema_core::Predicate::False,
        Coverage::Partial,
    );
    sim_assert_eq!(have: state.fork(Rc::clone(&decision), true).invocations_complete(), want: false);
    sim_assert_eq!(have: state.fork(decision, false).invocations_complete(), want: false);
}
