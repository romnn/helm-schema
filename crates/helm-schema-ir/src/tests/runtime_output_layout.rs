use std::collections::BTreeSet;

use color_eyre::eyre;

use std::rc::Rc;

use helm_schema_core::{PlainScalarPlacement, Predicate, PredicateMemo, ValuesPath};
use helm_schema_syntax::Span;
use test_util::prelude::sim_assert_eq;

use super::OutputPlacement;
use crate::value::{Coverage, Value};
use crate::value::{ExecutedSite, ObservationId, OutputNode, State, TextBuffer, TextPart};

struct Fixture {
    state: State,
    site: ExecutedSite,
    memo: PredicateMemo,
}

fn fixture(prefix: &str, suffix: &str) -> Fixture {
    let mut state = State::new(Value::nil());
    let value = state.heap.freeze(&Value::input(ValuesPath::parse("value")));
    state.output.bytes(prefix);
    state.output.parts.push(OutputNode {
        occurrences: vec![ObservationId {
            program: state.program,
            ordinal: 0,
            span: Span::new(0, 1),
        }],
        part: TextPart::Frozen(value.clone()),
    });
    state.output.bytes(suffix);
    let site = ExecutedSite {
        program: state.program,
        coverage_exact: true,
        rendered_unchanged: true,
        plain_scalar_placement: PlainScalarPlacement::Unknown,
        consumption_binder: Vec::new(),
        ordinal: 0,
        span: Span::new(0, 1),
        source_path: None,
        condition: Predicate::True.into(),
        value,
        rendered: true,
    };
    Fixture {
        state,
        site,
        memo: PredicateMemo::new(),
    }
}

fn placement(fixture: &Fixture) -> PlainScalarPlacement {
    OutputPlacement::new(
        &Rc::new(fixture.state.output.clone()),
        &Predicate::True,
        &fixture.memo,
        crate::value::FileOutput::Manifest,
        &mut super::SyntaxContexts::default(),
        &fixture.state.heap.arena.input_paths.borrow(),
        &fixture.state.heap.arena.input_objects.borrow(),
    )
    .plain_placement(&fixture.site, &fixture.memo)
}

#[test]
fn a_single_unknown_block_mapping_or_sequence_value_is_certified() {
    for (prefix, suffix) in [
        (
            indoc::indoc! {r"
            data:
              value: "},
            "\n",
        ),
        (
            indoc::indoc! {r"
            values:
              - "},
            "\n",
        ),
        (
            indoc::indoc! {r"
            data:
              value: "},
            indoc::indoc! {r"
            
            <no value>"},
        ),
    ] {
        let fixture = fixture(prefix, suffix);
        sim_assert_eq!(have: placement(&fixture), want: PlainScalarPlacement::BlockValue);
    }
}

#[test]
fn flow_quoted_partial_key_and_root_positions_never_gain_a_block_certificate() {
    for (prefix, suffix) in [
        ("data: {value: ", "}\n"),
        ("data: &anchor ", "\n"),
        (
            indoc::indoc! {r"
            data: {
              value: "},
            indoc::indoc! {r"
            
            }
            "},
        ),
        (
            indoc::indoc! {r"
            data:
              values: ["},
            "]\n",
        ),
        (
            indoc::indoc! {r"
            data:
              values: [
            "},
            indoc::indoc! {r"
            
            ]
            "},
        ),
        (
            indoc::indoc! {r#"
            data:
              value: ""#},
            "\"\n",
        ),
        (
            indoc::indoc! {r"
            data:
              value: prefix"},
            "\n",
        ),
        (
            indoc::indoc! {r"
            data:
              value: "},
            "suffix\n",
        ),
        ("", ": value\n"),
        ("", "\n"),
        ("data: [<no value>", "]\n"),
    ] {
        let fixture = fixture(prefix, suffix);
        sim_assert_eq!(have: placement(&fixture), want: PlainScalarPlacement::Unknown, "prefix={prefix:?}; suffix={suffix:?}");
    }
}

#[test]
fn two_unknown_emissions_abstain_even_when_the_frozen_value_is_identical() {
    let mut fixture = fixture(
        "first: ",
        indoc::indoc! {r"
        
        second: "},
    );
    fixture.state.output.emit(fixture.site.value.clone());
    fixture.state.output.bytes("\n");
    sim_assert_eq!(have: placement(&fixture), want: PlainScalarPlacement::Unknown);
}

#[test]
fn another_unknown_output_and_recovery_or_size_overflow_abstain() {
    let mut other = fixture(
        "first: ",
        indoc::indoc! {r"
        
        second: "},
    );
    other.state.output.emit(
        other
            .state
            .heap
            .freeze(&Value::input(ValuesPath::parse("other"))),
    );
    sim_assert_eq!(have: placement(&other), want: PlainScalarPlacement::Unknown);
    let broken = fixture(
        indoc::indoc! {r"
        data: [
         value: "},
        "\n",
    );
    sim_assert_eq!(have: placement(&broken), want: PlainScalarPlacement::Unknown);
    let oversized = fixture(&"x".repeat(65 * 1024), "\n");
    sim_assert_eq!(have: placement(&oversized), want: PlainScalarPlacement::Unknown);
}

#[test]
fn selected_output_uses_the_target_condition_without_forking_state() {
    let mut fixture = fixture(
        indoc::indoc! {r"
        data:
          value: "},
        "\n",
    );
    let condition = Predicate::truthy_path("enabled");
    let decision = fixture.state.heap.arena.decision(
        condition.clone(),
        condition.clone().negated(),
        Coverage::Exact,
    );
    let selected = fixture.state.output.clone();
    let mut other = TextBuffer::default();
    other.bytes("other: known\n");
    fixture.state.output = TextBuffer {
        parts: vec![
            TextPart::Select {
                decision,
                yes: Rc::new(selected),
                no: Rc::new(other),
            }
            .into(),
        ],
        ..TextBuffer::default()
    };
    fixture.site.condition = condition.into();
    let decisions = fixture.state.heap.arena.next_decision.get();
    sim_assert_eq!(have: placement(&fixture), want: PlainScalarPlacement::BlockValue);
    sim_assert_eq!(have: fixture.state.heap.arena.next_decision.get(), want: decisions);
    fixture.site.condition = Predicate::True.into();
    sim_assert_eq!(have: placement(&fixture), want: PlainScalarPlacement::Unknown);
}

#[test]
fn partial_choices_only_preserve_states_proved_by_both_arms() {
    use super::quoted::LexicalState;

    let fixture = fixture("value: ", "\n");
    let decision =
        fixture
            .state
            .heap
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial);
    for (other, preserves) in [
        (TextPart::Bytes("other".to_owned()), true),
        (TextPart::Bytes("\" # repaired".to_owned()), false),
        (TextPart::Unknown(BTreeSet::new()), false),
    ] {
        let selected = TextPart::Select {
            decision: Rc::clone(&decision),
            yes: Rc::new(TextBuffer {
                parts: vec![TextPart::Bytes("safe".to_owned()).into()],
                ..TextBuffer::default()
            }),
            no: Rc::new(TextBuffer {
                parts: vec![other.into()],
                ..TextBuffer::default()
            }),
        };
        let input_paths = fixture.state.heap.arena.input_paths.borrow();
        let input_objects = fixture.state.heap.arena.input_objects.borrow();
        let mut transfers = super::LexicalTransfers::new(&input_paths, &input_objects);
        let states = transfers.part(&selected, LexicalState::Double, &[], &fixture.memo);
        sim_assert_eq!(have: states.get(&LexicalState::Double).cloned().map(|condition| fixture.memo.normalize(condition)), want: preserves.then_some(Predicate::True));
        sim_assert_eq!(have: states.len(), want: usize::from(preserves));
    }
}

#[test]
fn partial_control_does_not_hide_output_failure_or_a_possible_survivor() {
    use crate::value::{AbortReason, DecisionEdge, Exit, FileOutput, Flow, Normal};

    let mut fixture = fixture("value: \"", "\"");
    let expected = OutputPlacement::new(
        &Rc::new(fixture.state.output.clone()),
        &Predicate::True,
        &fixture.memo,
        FileOutput::Manifest,
        &mut super::SyntaxContexts::default(),
        &fixture.state.heap.arena.input_paths.borrow(),
        &fixture.state.heap.arena.input_objects.borrow(),
    )
    .invalid(&fixture.memo);
    assert_ne!(expected, Predicate::False);
    let decision =
        fixture
            .state
            .heap
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial);
    Rc::make_mut(&mut fixture.state.active).push(DecisionEdge {
        test: decision,
        taken: true,
    });
    let normal = Exit {
        state: fixture.state.clone(),
        flow: Flow::Normal(Normal {
            value: Value::nil(),
        }),
    };
    let alone = super::FilePlacement::seal(std::slice::from_ref(&normal), FileOutput::Manifest);
    sim_assert_eq!(have: alone.abort, want: expected);

    // A partial abort may represent continuing execution and must remain a survivor.
    let uncertain = Exit {
        state: fixture.state,
        flow: Flow::Abort(AbortReason::Fail),
    };
    let joined = super::FilePlacement::seal(&[normal, uncertain], FileOutput::Manifest);
    sim_assert_eq!(have: joined.abort, want: Predicate::False);
}

#[test]
fn marker_provenance_uses_graph_boundaries_for_manifests_and_notes() {
    let db = crate::analysis_db::IrAnalysisDb::new(&helm_schema_ast::DefineIndex::new());
    for path in ["templates/probe.yaml", "templates/NOTES.txt"] {
        for (source, literal_unchanged) in [
            (">safe{{ .Values.tail }}", true),
            ("<no {{ .Values.tail }}", false),
        ] {
            let execution = crate::value::tests::execute_document(source, Some(path), &db);
            sim_assert_eq!(
                have: execution.text_sites().map(|site| site.rendered_unchanged).collect::<Vec<_>>(),
                want: vec![literal_unchanged],
                "path={path}, source={source}"
            );
            sim_assert_eq!(
                have: execution.sites().filter(|site| site.rendered()).map(|site| site.rendered_unchanged).collect::<Vec<_>>(),
                want: vec![false],
                "path={path}, source={source}"
            );
        }
    }
}

#[test]
fn complete_marker_provenance_demotes_split_emissions_but_keeps_safe_neighbours() {
    let db = crate::analysis_db::IrAnalysisDb::new(&helm_schema_ast::DefineIndex::new());
    for path in ["templates/probe.yaml", "templates/NOTES.txt"] {
        let execution = crate::value::tests::execute_document(
            r#"A<no {{ "value" }}>B{{ "untouched" }}"#,
            Some(path),
            &db,
        );
        sim_assert_eq!(have: execution.text_sites().map(|site| site.rendered_unchanged).collect::<Vec<_>>(), want: vec![false, false]);
        sim_assert_eq!(have: execution.sites().map(|site| site.rendered_unchanged).collect::<Vec<_>>(), want: vec![false, true]);
        sim_assert_eq!(have: execution.text().map(|(_, output)| output.exact_text()).collect::<Vec<_>>(), want: vec![Some("ABuntouched".to_owned())]);
    }
}

#[test]
fn nested_capture_contexts_share_the_marker_provenance() {
    let mut defines = helm_schema_ast::DefineIndex::new();
    defines.add_file_source(
        "templates/_helpers.tpl",
        indoc::indoc! {r#"
            {{- define "inner" -}}<no {{ .Values.part }}{{- end -}}
            {{- define "outer" -}}{{ include "inner" . }}{{- end -}}
        "#},
    );
    let db = crate::analysis_db::IrAnalysisDb::new(&defines);
    for path in ["templates/probe.yaml", "templates/NOTES.txt"] {
        let execution = crate::value::tests::execute_document(
            r#"A{{ include "outer" . }}B{{ "safe" }}"#,
            Some(path),
            &db,
        );
        sim_assert_eq!(have: execution.sites().filter(|site| site.rendered()).map(|site| site.rendered_unchanged).collect::<Vec<_>>(), want: vec![false, true]);
        sim_assert_eq!(have: execution.text_sites().map(|site| site.rendered_unchanged).collect::<Vec<_>>(), want: vec![true, true]);
        sim_assert_eq!(
            have: execution.all_sites().filter(|site| site.rendered() && site.source_path() == Some("templates/_helpers.tpl")).map(|site| site.rendered_unchanged).collect::<Vec<_>>(),
            want: vec![false, false]
        );
    }
}

#[test]
fn independent_scalar_choices_share_one_ownership_context() {
    let mut fixture = fixture("", "");
    fixture.state.output = TextBuffer::default();
    for index in 0..12 {
        fixture.state.output.bytes(&format!("value{index}: \""));
        let predicate = Predicate::truthy_path(format!("enabled{index}"));
        let decision = fixture.state.heap.arena.decision(
            predicate.clone(),
            predicate.negated(),
            Coverage::Exact,
        );
        let mut branches = Vec::new();
        for side in ["left", "right"] {
            let mut branch = TextBuffer::default();
            branch.emit(
                fixture
                    .state
                    .heap
                    .freeze(&Value::input(ValuesPath::parse(&format!("{side}{index}")))),
            );
            branches.push(Rc::new(branch));
        }
        let mut branches = branches.into_iter();
        if let (Some(yes), Some(no)) = (branches.next(), branches.next()) {
            fixture
                .state
                .output
                .parts
                .push(TextPart::Select { decision, yes, no }.into());
        }
        fixture.state.output.bytes("\"\n");
    }
    let placement = OutputPlacement::new(
        &Rc::new(fixture.state.output),
        &Predicate::True,
        &fixture.memo,
        crate::value::FileOutput::Manifest,
        &mut super::SyntaxContexts::default(),
        &fixture.state.heap.arena.input_paths.borrow(),
        &fixture.state.heap.arena.input_objects.borrow(),
    );
    sim_assert_eq!(have: placement.layouts.len(), want: 1);
    sim_assert_eq!(have: placement.unknown, want: Predicate::False);
    let inputs = placement
        .layouts
        .iter()
        .flat_map(|layout| {
            layout.documents.iter().flat_map(move |document| {
                let placed = super::PlacedDocument {
                    condition: document.condition.clone(),
                    ownership: &document.ownership,
                    layout,
                    document,
                };
                placed
                    .terms(Span::new(0, document.ownership.skeleton.text.len()))
                    .filter(|term| term.placeholder)
                    .map(|term| term.inputs())
                    .collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    sim_assert_eq!(have: inputs.len(), want: 12);
    for (index, inputs) in inputs.into_iter().enumerate() {
        sim_assert_eq!(have: inputs, want: BTreeSet::from([
            ValuesPath::parse(&format!("left{index}")), ValuesPath::parse(&format!("right{index}")),
        ]));
    }
}

#[test]
fn shared_layouts_keep_concatenated_emissions_separate() {
    let source = "value: {{ if .Values.enabled }}{{ .Values.a }}{{ .Values.b }}{{ else }}{{ .Values.c }}{{ .Values.d }}{{ end }}";
    let db = crate::analysis_db::IrAnalysisDb::new(&helm_schema_ast::DefineIndex::new());
    let execution =
        crate::value::tests::execute_document(source, Some("templates/probe.yaml"), &db);
    let slots = execution
        .placed_documents()
        .map(|document| {
            document
                .terms(Span::new(0, document.ownership.skeleton.text.len()))
                .filter(|term| term.placeholder)
                .count()
        })
        .collect::<Vec<_>>();
    sim_assert_eq!(have: slots, want: vec![2]);
    sim_assert_eq!(have: execution.sites().filter(|site| site.rendered()).map(|site| site.plain_scalar_placement).collect::<Vec<_>>(),
        want: vec![PlainScalarPlacement::Unknown; 4]);
}

#[test]
fn infeasible_structural_choices_do_not_consume_the_layout_budget() {
    let fixture = fixture("value: \"", "\"\n");
    let mut impossible = TextBuffer::default();
    impossible.bytes("unused: ");
    for index in 0..6 {
        let guard = Predicate::truthy_path(format!("branch{index}"));
        let mut yes = TextBuffer::default();
        yes.bytes("a");
        let mut no = TextBuffer::default();
        no.bytes("b");
        impossible.parts.push(
            TextPart::Select {
                decision: fixture.state.heap.arena.decision(
                    guard.clone(),
                    guard.negated(),
                    Coverage::Exact,
                ),
                yes: Rc::new(yes),
                no: Rc::new(no),
            }
            .into(),
        );
    }
    let active = Predicate::truthy_path("active");
    let graph = TextBuffer {
        parts: vec![
            TextPart::Select {
                decision: fixture.state.heap.arena.decision(
                    active.clone().negated(),
                    active.clone(),
                    Coverage::Exact,
                ),
                yes: Rc::new(impossible),
                no: Rc::new(fixture.state.output),
            }
            .into(),
        ],
        ..TextBuffer::default()
    };
    let placement = OutputPlacement::new(
        &Rc::new(graph),
        &active,
        &fixture.memo,
        crate::value::FileOutput::Manifest,
        &mut super::SyntaxContexts::default(),
        &fixture.state.heap.arena.input_paths.borrow(),
        &fixture.state.heap.arena.input_objects.borrow(),
    );
    sim_assert_eq!(have: placement.layouts.len(), want: 1);
    sim_assert_eq!(have: placement.unknown, want: Predicate::False);
}

#[test]
fn exhausted_shared_occurrence_queries_remain_unknown() {
    let fixture = fixture("value: ", "\n");
    let id = ObservationId {
        program: fixture.site.program,
        ordinal: fixture.site.ordinal,
        span: fixture.site.span,
    };
    let mut graph = Rc::new(TextBuffer {
        parts: vec![OutputNode {
            occurrences: vec![id],
            part: TextPart::Frozen(fixture.site.value),
        }],
        ..TextBuffer::default()
    });
    for index in 0..8 {
        let guard = Predicate::truthy_path(format!("branch{index}"));
        graph = Rc::new(TextBuffer {
            parts: vec![OutputNode {
                occurrences: vec![id],
                part: TextPart::Select {
                    decision: fixture.state.heap.arena.decision(
                        guard.clone(),
                        guard.negated(),
                        Coverage::Exact,
                    ),
                    yes: Rc::clone(&graph),
                    no: graph,
                },
            }],
            ..TextBuffer::default()
        });
    }
    let node = super::NodeRef {
        buffer: graph,
        index: 0,
        start: 0,
        indentation: 0,
        context: Rc::new(Vec::new()),
        serializer: None,
    };
    let term = super::PlacedTerm {
        range: Span::new(0, 8),
        placeholder: true,
        source_offset: None,
        node: &node,
    };
    sim_assert_eq!(have: term.occurrence_guard(id), want: None);
    sim_assert_eq!(have: term.inputs(), want: BTreeSet::from([ValuesPath::parse("value")]));
}

#[test]
fn recovered_frozen_continuations_do_not_certify_resource_placement() {
    let mut fixture = fixture("value: \"", "\"\n");
    let continuation = fixture.state.heap.freeze(&Value::string("\\\n"));
    fixture
        .state
        .output
        .parts
        .insert(1, TextPart::Frozen(continuation).into());
    let placed = OutputPlacement::new(
        &Rc::new(fixture.state.output),
        &Predicate::True,
        &fixture.memo,
        crate::value::FileOutput::Manifest,
        &mut super::SyntaxContexts::default(),
        &fixture.state.heap.arena.input_paths.borrow(),
        &fixture.state.heap.arena.input_objects.borrow(),
    );
    assert_ne!(placed.invalid(&fixture.memo), Predicate::False);
    sim_assert_eq!(have: placed.layouts.iter().map(|layout| layout.documents.len()).sum::<usize>(), want: 0);
}

#[test]
fn shared_syntax_keeps_frozen_inputs_and_guards_separate() {
    let first = fixture("value: \"", "\"\n");
    let mut second = fixture("value: \"", "\"\n");
    second.state.output.parts[1].part = TextPart::Frozen(
        second
            .state
            .heap
            .freeze(&Value::input(ValuesPath::parse("other"))),
    );
    let mut syntax = super::SyntaxContexts::default();
    let first = OutputPlacement::new(
        &Rc::new(first.state.output),
        &Predicate::True,
        &first.memo,
        crate::value::FileOutput::Manifest,
        &mut syntax,
        &first.state.heap.arena.input_paths.borrow(),
        &first.state.heap.arena.input_objects.borrow(),
    );
    let guard = Predicate::truthy_path("flag");
    let second = OutputPlacement::new(
        &Rc::new(second.state.output),
        &guard,
        &second.memo,
        crate::value::FileOutput::Manifest,
        &mut syntax,
        &second.state.heap.arena.input_paths.borrow(),
        &second.state.heap.arena.input_objects.borrow(),
    );
    assert!(Rc::ptr_eq(
        &first.layouts[0].documents[0].ownership,
        &second.layouts[0].documents[0].ownership,
    ));
    sim_assert_eq!(have: &first.layouts[0].documents[0].condition, want: &Predicate::True);
    sim_assert_eq!(have: &second.layouts[0].documents[0].condition, want: &guard);
    let memo = PredicateMemo::new();
    sim_assert_eq!(have: first.invalid(&memo).value_paths(), want: BTreeSet::from([ValuesPath::parse("value")]));
    sim_assert_eq!(have: second.invalid(&memo).value_paths(), want: BTreeSet::from([ValuesPath::parse("other"), ValuesPath::parse("flag")]));
}

#[test]
fn stringification_does_not_reapply_an_overwritten_input_members_text() -> eyre::Result<()> {
    use crate::value::kinds::GoType;
    use crate::value::relationship::ValueOperation;
    use crate::value::{Field, ScalarType};
    use color_eyre::eyre::OptionExt as _;

    let mut state = State::new(Value::nil());
    state
        .map_target(&Value::input(ValuesPath::default()))
        .ok_or_eyre("root backing missing")?;
    let source = crate::value::kinds::refined_parameter(
        &Value::input(ValuesPath::parse("payload")),
        GoType::Map,
    );
    let target = state
        .map_target(&source)
        .ok_or_eyre("map backing missing")?;
    state
        .heap
        .write(&target, "x", Field::Present(Value::string("safe")));
    let stringified = Value::scalar(ScalarType::String, None).derived(
        state.heap.freeze(&source),
        ValueOperation::Format(crate::value::relationship::TextFormat::Value),
    );
    let frozen = state.heap.freeze(&stringified);
    let input_paths = state.heap.arena.input_paths.borrow();
    let input_objects = state.heap.arena.input_objects.borrow();
    let mut transfers = super::LexicalTransfers::new(&input_paths, &input_objects);
    // An original x containing a quote is overwritten before formatting, so it cannot
    // establish an invalid output quote through its old input identity.
    let states = transfers.value(&frozen, super::LexicalState::Double, &PredicateMemo::new());
    sim_assert_eq!(have: states.get(&super::LexicalState::Invalid), want: None);
    Ok(())
}
