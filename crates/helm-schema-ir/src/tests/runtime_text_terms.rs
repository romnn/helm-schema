use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::TextTerm;
use crate::analysis_db::IrAnalysisDb;
use crate::value::{Datum, FrozenValue, Leaf, ValueNode, execute_document};

#[test]
fn repeated_specialization_retains_the_branch_of_a_mixed_text_choice() {
    use std::rc::Rc;

    use helm_schema_core::{GuardValue, Predicate, ValuesPath};

    use crate::value::guard::RuntimeGuard;
    use crate::value::{Coverage, State, TextBuffer, Value};

    for literal_taken in [true, false] {
        let state = State::new(Value::nil());
        let condition = Predicate::truthy_path("enabled");
        let decision =
            state
                .heap
                .arena
                .decision(condition.clone(), condition.negated(), Coverage::Exact);
        let mut literal = TextBuffer::default();
        literal.bytes("fixed");
        let mut input = TextBuffer::default();
        input.emit(
            state
                .heap
                .freeze(&Value::input(ValuesPath::parse("payload"))),
        );
        let graph = if literal_taken {
            TextBuffer::join(Rc::clone(&decision), &literal, &input, &state.heap.arena)
        } else {
            TextBuffer::join(Rc::clone(&decision), &input, &literal, &state.heap.arena)
        };
        let captured = super::capture(&graph, false);
        let expected = if literal_taken {
            Value::select(
                Rc::clone(&decision),
                Value::string("fixed"),
                captured.clone(),
            )
        } else {
            Value::select(
                Rc::clone(&decision),
                captured.clone(),
                Value::string("fixed"),
            )
        };
        let mut actual = captured.under_runtime(&RuntimeGuard::True);

        // One decision and two leaves retain both alternatives across repeated queries.
        for iteration in 0..128 {
            sim_assert_eq!(have: &actual, want: &expected, "literal_taken={literal_taken}, iteration={iteration}");
            sim_assert_eq!(have: actual.under(&decision, literal_taken).constant().cloned(), want: Some(GuardValue::String("fixed".to_owned())));
            sim_assert_eq!(have: actual.under(&decision, !literal_taken), want: captured.clone());
            actual = actual.under_runtime(&RuntimeGuard::True);
        }
    }
}

#[test]
fn mixed_text_truth_and_length_queries_preserve_the_selected_literal() {
    use std::rc::Rc;

    use helm_schema_core::{Predicate, ValuesPath};

    use crate::value::{Coverage, DecisionEdge, State, TextBuffer, Value};

    for literal_taken in [true, false] {
        let state = State::new(Value::nil());
        let condition = Predicate::truthy_path("enabled");
        let decision =
            state
                .heap
                .arena
                .decision(condition.clone(), condition.negated(), Coverage::Exact);
        let mut literal = TextBuffer::default();
        literal.bytes("fixed");
        let mut input = TextBuffer::default();
        input.emit(
            state
                .heap
                .freeze(&Value::input(ValuesPath::parse("payload"))),
        );
        let graph = if literal_taken {
            TextBuffer::join(Rc::clone(&decision), &literal, &input, &state.heap.arena)
        } else {
            TextBuffer::join(Rc::clone(&decision), &input, &literal, &state.heap.arena)
        };
        let captured = super::capture(&graph, false);
        let snapshots = state.heap.arena.next_snapshot.get();

        // Querying a string as a map must terminate without consuming its alternatives again.
        // Truth keeps the known nonempty arm and leaves the input arm uncertain.
        for _ in 0..32 {
            sim_assert_eq!(have: state.map_length(&captured), want: None);
            let truth = state.test(&captured);
            let literal_guard = state.heap.arena.runtime_path(&[DecisionEdge {
                test: Rc::clone(&decision),
                taken: literal_taken,
            }]);
            let input_guard = state.heap.arena.runtime_path(&[DecisionEdge {
                test: Rc::clone(&decision),
                taken: !literal_taken,
            }]);
            sim_assert_eq!(have: literal_guard.implies_test(&truth), want: Some(true));
            sim_assert_eq!(have: input_guard.implies_test(&truth), want: None);
            sim_assert_eq!(have: state.heap.arena.next_snapshot.get(), want: snapshots);
        }
    }
}

#[test]
fn exhausted_text_projection_retains_one_frozen_graph_across_repeated_queries() {
    use std::collections::BTreeSet;
    use std::rc::Rc;

    use helm_schema_core::{GuardValue, Predicate};

    use crate::value::guard::RuntimeGuard;
    use crate::value::{Coverage, DecisionEdge, State, TextBuffer, Value};

    let state = State::new(Value::nil());
    let mut graph = TextBuffer::default();
    let mut decisions = Vec::new();
    for index in 0..12 {
        let condition = Predicate::truthy_path(format!("choice{index}"));
        let decision =
            state
                .heap
                .arena
                .decision(condition.clone(), condition.negated(), Coverage::Exact);
        let mut yes = graph.clone();
        yes.bytes("L");
        let mut no = graph.clone();
        no.bytes("R");
        graph = TextBuffer::join(Rc::clone(&decision), &yes, &no, &state.heap.arena);
        decisions.push(decision);
    }
    let captured = super::capture(&graph, false);
    let mut actual = captured.clone();
    let mut sizes = Vec::new();

    // Every execution is literal, but enumerating 4096 strings exceeds the query's work budget.
    // Publishing a partial frontier would give its captured leaves fresh budgets on each query.
    for _ in 0..3 {
        actual = actual.under_runtime(&RuntimeGuard::True);
        let mut nodes = BTreeSet::new();
        let mut pending = vec![&actual];
        while let Some(value) = pending.pop() {
            if !nodes.insert(Rc::as_ptr(&value.node)) {
                continue;
            }
            match value.node.as_ref() {
                ValueNode::Select {
                    decision: _,
                    yes,
                    no,
                } => pending.extend([yes, no]),
                ValueNode::Leaf(_) => {}
            }
        }
        sizes.push(nodes.len());
    }
    sim_assert_eq!(have: sizes, want: vec![1, 1, 1]);
    sim_assert_eq!(have: &actual, want: &captured);

    // Complete contexts still resolve the retained graph to its exact text.
    for (taken, text) in [(true, "L"), (false, "R")] {
        let edges = decisions
            .iter()
            .map(|test| DecisionEdge {
                test: Rc::clone(test),
                taken,
            })
            .collect::<Vec<_>>();
        let selected = captured.under_runtime(&state.heap.arena.runtime_path(&edges));
        sim_assert_eq!(have: selected.constant().cloned(), want: Some(GuardValue::String(text.repeat(12))));
    }
}

fn term(value: &FrozenValue) -> eyre::Result<&TextTerm> {
    match value.value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Text(text),
            origin: _,
        }) => Ok(text.0.as_ref()),
        other => eyre::bail!("expected a frozen text term, got {other:?}"),
    }
}

#[test]
fn shared_capture_debug_is_bounded_and_independent_of_allocation_addresses() -> eyre::Result<()> {
    use helm_schema_core::ValuesPath;

    use crate::value::{State, TextBuffer, Value};

    for _ in 0..2 {
        let state = State::new(Value::nil());
        let mut graph = TextBuffer::default();
        graph.emit(
            state
                .heap
                .freeze(&Value::input(ValuesPath::parse("payload"))),
        );
        let mut captured = super::capture(&graph, false);
        for depth in 1..=32 {
            let previous = state.heap.freeze(&captured);
            let mut graph = TextBuffer::default();
            graph.emit(previous.clone());
            graph.emit(previous);
            captured = super::capture(&graph, false);

            // Each level shares its predecessor twice; formatting must not expand those paths.
            if [1, 8, 32].contains(&depth) {
                let frozen = state.heap.freeze(&captured);
                let rendered = format!("{:?}", term(&frozen)?);
                sim_assert_eq!(have: rendered.as_str(), want: "Capture { terms: 2, strip_marker: false, .. }", "depth={depth}");
            }
        }
    }
    Ok(())
}

#[test]
fn serialization_and_indentation_keep_the_frozen_operand_and_operation_order() -> eyre::Result<()> {
    for (function, expected_pretty) in [("toYaml", false), ("toYamlPretty", true)] {
        let source = indoc::formatdoc! {r#"
            {{{{- $object := dict "t" "before" -}}}}
            {{{{- $text := {function} $object -}}}}
            {{{{- $_ := set $object "t" "after" -}}}}
            {{{{ $text | indent 2 | nindent 3 }}}}
        "#};
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document(&source, Some("templates/NOTES.txt"), &db);
        let site = execution
            .sites()
            .filter(|site| site.rendered())
            .last()
            .ok_or_eyre("missing emission")?;
        let TextTerm::Indent {
            value,
            width,
            newline,
        } = term(site.value())?
        else {
            eyre::bail!("missing outer nindent");
        };
        sim_assert_eq!(have: (*width, *newline), want: (3, true));
        let TextTerm::Indent {
            value,
            width,
            newline,
        } = term(value)?
        else {
            eyre::bail!("missing inner indent");
        };
        sim_assert_eq!(have: (*width, *newline), want: (2, false));
        let TextTerm::Yaml { value, pretty } = term(value)? else {
            eyre::bail!("missing frozen YAML serialization");
        };
        sim_assert_eq!(have: *pretty, want: expected_pretty);
        sim_assert_eq!(have: value.json(), want: Some(json!({"t": "before"})));
    }
    Ok(())
}
