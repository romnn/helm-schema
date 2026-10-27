use std::rc::Rc;

use helm_schema_core::{PlainScalarPlacement, Predicate, PredicateMemo, ValuesPath};
use helm_schema_syntax::Span;
use test_util::prelude::sim_assert_eq;

use super::OutputPlacement;
use crate::value::{Coverage, Value};
use crate::value::{ExecutedSite, ObservationId, OutputNode, State, TextBuffer, TextPart};

impl super::FilePlacement {
    pub(in crate::value) fn output_graphs(&self) -> impl Iterator<Item = &TextBuffer> {
        self.outputs.iter().map(|output| output.graph.as_ref())
    }
}

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
        ordinal: 0,
        span: Span::new(0, 1),
        source_path: None,
        condition: Predicate::True,
        value,
        raw_format_inputs: Vec::new(),
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
        Rc::new(fixture.state.output.clone()),
        Predicate::True,
        &fixture.memo,
        crate::value::FileOutput::Manifest,
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
    fixture.site.condition = condition;
    let decisions = fixture.state.heap.arena.next_decision.get();
    sim_assert_eq!(have: placement(&fixture), want: PlainScalarPlacement::BlockValue);
    sim_assert_eq!(have: fixture.state.heap.arena.next_decision.get(), want: decisions);
    fixture.site.condition = Predicate::True;
    sim_assert_eq!(have: placement(&fixture), want: PlainScalarPlacement::Unknown);
}

#[test]
fn marker_provenance_uses_graph_boundaries_for_manifests_and_notes() {
    let db = crate::analysis_db::IrAnalysisDb::new(&helm_schema_ast::DefineIndex::new());
    for path in ["templates/probe.yaml", "templates/NOTES.txt"] {
        for (source, literal_unchanged) in [
            (">safe{{ .Values.tail }}", true),
            ("<no {{ .Values.tail }}", false),
        ] {
            let execution = crate::value::execute_document(source, Some(path), &db);
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
        let execution = crate::value::execute_document(
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
        let execution = crate::value::execute_document(
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
