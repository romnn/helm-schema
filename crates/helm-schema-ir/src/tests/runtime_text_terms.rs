use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::TextTerm;
use crate::analysis_db::IrAnalysisDb;
use crate::value::tests::execute_document;
use crate::value::{Datum, FrozenValue, Leaf, ValueNode};

#[test]
fn frozen_indentation_rejects_expansion_before_allocating_output() {
    use std::collections::BTreeSet;

    use crate::value::guard::RuntimeGuard;
    use crate::value::{State, Value};

    let state = State::new(Value::nil());
    let text = ["line"; 17].join("\n");
    let value = Value::frozen_text(
        TextTerm::Indent {
            value: state.heap.freeze(&Value::string(&text)),
            width: 4096,
            newline: true,
        },
        BTreeSet::new(),
    );
    sim_assert_eq!(have: value.under_runtime(&RuntimeGuard::True), want: value.clone());
    sim_assert_eq!(have: state.heap.freeze(&value).exact_text(), want: None);
}

#[test]
fn frozen_yaml_shared_collection_expansion_charges_the_query_budget() -> eyre::Result<()> {
    use std::collections::BTreeSet;

    use crate::value::Value;
    use crate::value::guard::RuntimeGuard;

    let mut source = "{{ $x := dict \"leaf\" \"value\" }}".to_owned();
    for _ in 0..20 {
        source.push_str("{{ $x = dict \"left\" $x \"right\" $x }}");
    }
    source.push_str("{{ $x }}");
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(&source, Some("templates/NOTES.txt"), &db);
    let site = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing collection")?;
    let mut remaining = 64;
    sim_assert_eq!(have: site.value().json_with(&mut crate::value::output::JsonQuery { preserve_nil_collections: false, remaining: Some(&mut remaining), outcome: &|_| None, choice: None }).is_none(), want: true);
    sim_assert_eq!(have: remaining, want: 0);
    let value = Value::frozen_text(
        TextTerm::Yaml {
            value: site.value().clone(),
            pretty: false,
        },
        BTreeSet::new(),
    );
    for _ in 0..3 {
        sim_assert_eq!(have: value.under_runtime(&RuntimeGuard::True), want: value.clone());
    }
    Ok(())
}

#[test]
fn encoded_text_retains_the_operand_snapshot_and_catalog_alphabet() -> eyre::Result<()> {
    use helm_schema_core::ValuesPath;

    use crate::function_semantics::{OutputBehavior, function_semantics};

    for function in ["b64enc", "sha1sum", "sha256sum", "sha512sum", "adler32sum"] {
        let source = format!("{{{{ $encoded := {function} .Values.payload }}}}{{{{ $encoded }}}}");
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document(&source, Some("templates/NOTES.txt"), &db);
        let site = execution
            .sites()
            .filter(|site| site.rendered())
            .last()
            .ok_or_eyre("missing encoded emission")?;
        let TextTerm::Encoded { value, alphabet } = term(site.value())? else {
            eyre::bail!("missing frozen encoded term");
        };
        sim_assert_eq!(have: OutputBehavior::Encoded(*alphabet), want: function_semantics(function).behavior);
        sim_assert_eq!(have: value.identity(), want: Some(&ValuesPath::parse("payload")));
        sim_assert_eq!(have: site.value().inputs(), want: value.inputs());
        sim_assert_eq!(have: site.value().exact_text(), want: None);
    }
    Ok(())
}

#[test]
fn frozen_yaml_exact_subset_composes_with_indentation() -> eyre::Result<()> {
    use helm_schema_core::GuardValue;

    use crate::value::guard::RuntimeGuard;

    for (expression, normal, pretty) in [
        ("true", "true", "true"),
        ("17", "17", "17"),
        (r#""hello""#, "hello", "hello"),
        (r#""yes""#, "\"yes\"", "\"yes\""),
        ("dict", "{}", "{}"),
        ("list", "[]", "[]"),
        (r#"fromJsonArray "null""#, "null", "[]"),
        (r#"fromJson "null""#, "null", "{}"),
        (
            r#"dict "items" (fromJsonArray "null")"#,
            "items: null",
            "items: []",
        ),
        (r#"list (fromJson "null")"#, "- null", "- {}"),
        (
            r#"dict "items" (list "alpha" "beta")"#,
            indoc::indoc! {"
                items:
                - alpha
                - beta"},
            indoc::indoc! {"
                items:
                  - alpha
                  - beta"},
        ),
        (
            r#"list (dict "alpha" (list 1 2))"#,
            indoc::indoc! {"
                - alpha:
                  - 1
                  - 2"},
            indoc::indoc! {"
                - alpha:
                    - 1
                    - 2"},
        ),
    ] {
        for (function, expected) in [("toYaml", normal), ("toYamlPretty", pretty)] {
            let source = format!("{{{{ {function} ({expression}) | indent 2 | nindent 1 }}}}");
            let db = IrAnalysisDb::new(&DefineIndex::new());
            let execution = execute_document(&source, Some("templates/NOTES.txt"), &db);
            let site = execution
                .sites()
                .filter(|site| site.rendered())
                .last()
                .ok_or_eyre("missing YAML emission")?;
            let actual = site.value().value.under_runtime(&RuntimeGuard::True);
            let expected = super::indent(&super::indent(expected, 2, false), 1, true);
            sim_assert_eq!(have: actual.constant().cloned(), want: Some(GuardValue::String(expected)), "{source}");
        }
    }
    Ok(())
}

#[test]
fn frozen_yaml_long_map_keys_keep_explicit_key_spelling_unknown() -> eyre::Result<()> {
    for length in [128, 129] {
        let key = "a".repeat(length);
        for function in ["toYaml", "toYamlPretty"] {
            let source = format!("{{{{ {function} (dict {key:?} \"value\") }}}}");
            let db = IrAnalysisDb::new(&DefineIndex::new());
            let execution = execute_document(&source, Some("templates/NOTES.txt"), &db);
            let site = execution
                .sites()
                .filter(|site| site.rendered())
                .last()
                .ok_or_eyre("missing YAML emission")?;
            let expected = (length == 128).then(|| format!("{key}: value"));
            sim_assert_eq!(have: site.value().exact_text(), want: expected, "{function}, length={length}");
        }
    }
    Ok(())
}

#[test]
fn frozen_yaml_unsupported_spelling_keeps_the_frozen_term() -> eyre::Result<()> {
    use crate::value::guard::RuntimeGuard;

    for expression in [r#""line\nbreak""#, r#"dict "a10" 1 "a2" 2"#, "1.25"] {
        let source = format!("{{{{ toYaml ({expression}) }}}}");
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document(&source, Some("templates/NOTES.txt"), &db);
        let site = execution
            .sites()
            .filter(|site| site.rendered())
            .last()
            .ok_or_eyre("missing YAML emission")?;
        sim_assert_eq!(have: site.value().value.under_runtime(&RuntimeGuard::True), want: site.value().value.clone());
    }
    Ok(())
}

#[test]
fn frozen_yaml_resolves_outer_choices_without_changing_the_snapshot() {
    use std::collections::BTreeSet;

    use helm_schema_core::{GuardValue, Predicate};

    use crate::value::guard::RuntimeGuard;
    use crate::value::{Coverage, State, Value};

    let state = State::new(Value::nil());
    let predicate = Predicate::truthy_path("enabled");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let selected = Value::select(
        std::rc::Rc::clone(&decision),
        Value::string("yes"),
        Value::string("hello"),
    );
    let frozen = state.heap.freeze(&selected);
    let serialized = Value::frozen_text(
        TextTerm::Yaml {
            value: frozen.clone(),
            pretty: false,
        },
        BTreeSet::new(),
    );
    let projected = serialized.under_runtime(&RuntimeGuard::True);
    for (taken, text) in [(true, "\"yes\""), (false, "hello")] {
        sim_assert_eq!(have: projected.under(&decision, taken).constant().cloned(), want: Some(GuardValue::String(text.to_owned())));
    }
    sim_assert_eq!(have: frozen.value, want: selected);
    for pretty in [false, true] {
        let scalar_nil = Value::frozen_text(
            TextTerm::Yaml {
                value: state.heap.freeze(&Value::nil()),
                pretty,
            },
            BTreeSet::new(),
        );
        sim_assert_eq!(have: scalar_nil.under_runtime(&RuntimeGuard::True).constant().cloned(), want: Some(GuardValue::String("null".to_owned())));
    }
}

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
        for before in ["before", "before-value"] {
            let source = indoc::formatdoc! {r#"
            {{{{- $object := dict "t" "{before}" -}}}}
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
            let expected_text = (before == "before").then(|| super::indent("t: before", 5, true));
            sim_assert_eq!(have: site.value().exact_text(), want: expected_text.clone());
            if expected_text.is_some() {
                continue;
            }
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
            sim_assert_eq!(have: value.json(), want: Some(json!({"t": before})));
        }
    }
    Ok(())
}
