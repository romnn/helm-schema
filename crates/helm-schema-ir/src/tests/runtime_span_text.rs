use std::collections::BTreeSet;
use std::fmt::Write as _;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
use helm_schema_syntax::Span;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::{DocumentExecution, Exit, Flow, Normal, State, Value, execute_document};

fn execute(source: &str) -> DocumentExecution {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    execute_document(source, None, &db)
}

fn alternatives(
    execution: &DocumentExecution,
    span: Span,
) -> BTreeSet<(Predicate, Option<String>)> {
    execution
        .text_for_span(span, None)
        .map(|(condition, text)| (condition.normalize_boolean(), text))
        .collect()
}

fn document(state: State) -> DocumentExecution {
    DocumentExecution::new(
        vec![Exit {
            state,
            flow: Flow::Normal(Normal {
                value: Value::nil(),
            }),
        }],
        crate::value::FileOutput::Manifest,
    )
}

#[test]
fn a_literal_span_retains_its_executed_equality_guard() -> eyre::Result<()> {
    let source = "{{ if eq .Values.mode \"left\" }}L{{ end }}";
    let start = source.find('L').ok_or_eyre("literal span missing")?;
    let guard = Predicate::from(Guard::Eq {
        path: ValuesPath::parse("mode"),
        value: GuardValue::String("left".to_owned()),
    });
    // A string comparison succeeds only for comparable raw inputs; both text arms keep that domain.
    let comparable = Predicate::Or(vec![
        Predicate::from(Guard::Eq {
            path: ValuesPath::parse("mode"),
            value: GuardValue::Null,
        }),
        Predicate::from(Guard::Absent {
            path: ValuesPath::parse("mode"),
        }),
        Predicate::from(Guard::TypeIs {
            path: ValuesPath::parse("mode"),
            schema_type: "string".to_owned(),
        }),
    ]);
    let successful = Predicate::Or(vec![guard.clone(), comparable.clone()]);
    sim_assert_eq!(
        have: alternatives(&execute(source), Span::new(start, start + 1)),
        want: BTreeSet::from([
            (Predicate::all(vec![guard.clone(), comparable]).normalize_boolean(), Some("L".to_owned())),
            (Predicate::all(vec![guard.negated(), successful]).normalize_boolean(), Some(String::new())),
        ])
    );
    Ok(())
}

#[test]
fn mutually_exclusive_pieces_at_one_span_are_never_concatenated() -> eyre::Result<()> {
    let guard = Predicate::truthy_path("a");
    let span = Span::new(0, 1);
    let mut state = State::new(Value::nil());
    state.emit_bytes(span, None, "L");
    state
        .text_sites
        .iter_mut()
        .next()
        .ok_or_eyre("missing first text site")?
        .condition = guard.clone();
    state.emit_bytes(span, None, "R");
    state
        .text_sites
        .iter_mut()
        .nth(1)
        .ok_or_eyre("missing second text site")?
        .condition = guard.negated();
    sim_assert_eq!(
        have: alternatives(&document(state), span),
        want: BTreeSet::from([
            (guard.clone(), Some("L".to_owned())),
            (guard.negated().normalize_boolean(), Some("R".to_owned())),
        ])
    );
    Ok(())
}

#[test]
fn independent_executed_forks_keep_all_four_text_alternatives() {
    let source = "{{ if .Values.a }}A{{ else }}Z{{ end }}{{ if .Values.b }}B{{ else }}Y{{ end }}";
    let a = Predicate::truthy_path("a");
    let b = Predicate::truthy_path("b");
    let expected = [
        (true, true, "AB"),
        (true, false, "AY"),
        (false, true, "ZB"),
        (false, false, "ZY"),
    ]
    .map(|(left, right, text)| {
        (
            Predicate::all(vec![
                if left { a.clone() } else { a.negated() },
                if right { b.clone() } else { b.negated() },
            ])
            .normalize_boolean(),
            Some(text.to_owned()),
        )
    });
    sim_assert_eq!(have: alternatives(&execute(source), Span::new(0, source.len())), want: BTreeSet::from(expected));
}

#[test]
fn an_unknown_or_inexact_piece_only_invalidates_its_included_arm() -> eyre::Result<()> {
    let source = "{{ if .Values.a }}{{ .Values.unknown }}{{ end }}";
    let guard = Predicate::truthy_path("a");
    sim_assert_eq!(
        have: alternatives(&execute(source), Span::new(0, source.len())),
        want: BTreeSet::from([
            (guard.clone(), None),
            (guard.negated().normalize_boolean(), Some(String::new())),
        ])
    );
    let span = Span::new(0, 1);
    let mut state = State::new(Value::nil());
    state.emit_bytes(span, None, "L");
    let site = state
        .text_sites
        .iter_mut()
        .next()
        .ok_or_eyre("missing text site")?;
    site.condition = guard.clone();
    site.coverage_exact = false;
    sim_assert_eq!(
        have: alternatives(&document(state), span),
        want: BTreeSet::from([
            (guard.clone(), None),
            (guard.negated().normalize_boolean(), Some(String::new())),
        ])
    );
    Ok(())
}

#[test]
fn too_many_span_alternatives_return_one_explicit_unknown() -> eyre::Result<()> {
    let mut source = String::new();
    for index in 0..8 {
        write!(source, "{{{{ if .Values.b{index} }}}}X{{{{ end }}}}")?;
    }
    sim_assert_eq!(have: alternatives(&execute(&source), Span::new(0, source.len())), want: BTreeSet::from([(Predicate::True, None)]));
    Ok(())
}

#[test]
fn literal_resource_kinds_survive_unknown_sibling_values() -> eyre::Result<()> {
    let source = indoc::indoc! {r#"
        {{- if eq .Values.local.kind "ConfigMap" }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        immutable: {{ .Values.local.setting }}
        {{- else if eq .Values.local.kind "Service" }}
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          type: {{ .Values.local.setting }}
        {{- end }}
    "#};
    let execution = execute(source);
    for kind in ["ConfigMap", "Service"] {
        let start = source
            .find(&format!("kind: {kind}"))
            .ok_or_eyre("kind header missing")?
            + "kind: ".len();
        let span = Span::new(start, start + kind.len());
        let texts = alternatives(&execution, span);
        let history = execution
            .exits
            .iter()
            .map(|exit| {
                (
                    exit.state.output_widened,
                    exit.state.path_predicate(),
                    exit.state
                        .text_sites
                        .iter()
                        .filter(|site| site.span.start < span.end && span.start < site.span.end)
                        .map(|site| {
                            (
                                site.coverage_exact,
                                site.rendered_unchanged,
                                &site.condition,
                                &site.bytes,
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        eyre::ensure!(
            texts.iter().all(|(_, text)| text.is_some()),
            "kind={kind}, texts={texts:#?}, history={history:#?}"
        );
        eyre::ensure!(
            texts.iter().any(|(_, text)| text.as_deref() == Some(kind)),
            "kind={kind}, texts={texts:#?}, history={history:#?}"
        );
    }
    Ok(())
}

#[test]
fn later_unknown_output_does_not_discard_complete_earlier_literal_history() {
    let span = Span::new(0, 4);
    let mut state = State::new(Value::nil());
    state.emit_bytes(span, None, "Kind");
    state.output_widened = true;
    state
        .output
        .parts
        .push(crate::value::TextPart::Unknown(BTreeSet::new()));
    sim_assert_eq!(
        have: alternatives(&document(state), span),
        want: BTreeSet::from([(Predicate::True, Some("Kind".to_owned()))])
    );
}

#[test]
fn dropped_observations_keep_even_an_earlier_literal_query_incomplete() {
    let span = Span::new(0, 4);
    let mut state = State::new(Value::nil());
    state.emit_bytes(span, None, "Kind");
    for _ in 0..128 {
        state.emit_bytes(Span::new(10, 11), None, "X");
    }
    sim_assert_eq!(
        have: alternatives(&document(state), span),
        want: BTreeSet::from([(Predicate::True, None)])
    );
}
