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

fn alternatives(execution: &DocumentExecution) -> BTreeSet<(Predicate, Option<String>)> {
    execution
        .executed_outputs()
        .into_iter()
        .map(|output| (output.condition.normalize_boolean(), output.text))
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
fn a_literal_span_retains_its_executed_equality_guard() {
    let source = "{{ if eq .Values.mode \"left\" }}L{{ end }}";
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
    sim_assert_eq!(
        have: alternatives(&execute(source)),
        want: BTreeSet::from([
            (Predicate::all(vec![guard.clone(), comparable.clone()]).normalize_boolean(), Some("L".to_owned())),
            (Predicate::all(vec![guard.negated(), comparable]).normalize_boolean(), Some(String::new())),
        ])
    );
}

#[test]
fn mutually_exclusive_pieces_at_one_span_are_never_concatenated() -> eyre::Result<()> {
    let guard = Predicate::truthy_path("a");
    let span = Span::new(0, 1);
    let initial = State::new(Value::nil());
    let decision = initial.heap.arena.decision(
        guard.clone(),
        guard.negated(),
        crate::value::Coverage::Exact,
    );
    let mut yes = initial.fork(std::rc::Rc::clone(&decision), true);
    yes.emit_bytes(span, None, "L");
    let mut no = initial.fork(std::rc::Rc::clone(&decision), false);
    no.emit_bytes(span, None, "R");
    let state = State::join_branches(
        &crate::value::state::ContinuationJoin {
            decision,
            active: Vec::new(),
            taken: true,
        },
        &yes,
        &no,
    )
    .ok_or_eyre("mutually exclusive output histories did not join")?;
    sim_assert_eq!(
        have: alternatives(&document(state)),
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
    sim_assert_eq!(have: alternatives(&execute(source)), want: BTreeSet::from(expected));
}

#[test]
fn an_unknown_or_inexact_piece_only_invalidates_its_included_arm() -> eyre::Result<()> {
    let source = "{{ if .Values.a }}{{ .Values.unknown }}{{ end }}";
    let guard = Predicate::truthy_path("a");
    sim_assert_eq!(
        have: alternatives(&execute(source)),
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
    let execution = document(state);
    sim_assert_eq!(have: execution.observed_marker_complete(span), want: false);
    sim_assert_eq!(have: execution.observed_marker_condition(span), want: guard);
    // Observation metadata cannot rewrite the already frozen, unconditional output.
    sim_assert_eq!(
        have: alternatives(&execution),
        want: BTreeSet::from([(Predicate::True, Some("L".to_owned()))])
    );
    Ok(())
}

#[test]
fn bounded_frozen_output_projection_returns_one_explicit_unknown() -> eyre::Result<()> {
    let mut source = String::new();
    for index in 0..16 {
        write!(source, "{{{{ if .Values.b{index} }}}}X{{{{ end }}}}")?;
    }
    sim_assert_eq!(have: alternatives(&execute(&source)), want: BTreeSet::from([(Predicate::True, None)]));
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
        let observed = execution
            .text_sites()
            .filter(|site| site.span().start <= span.start && span.end <= site.span().end)
            .map(|site| {
                let start = span.start - site.span().start;
                let end = span.end - site.span().start;
                (site.coverage_exact(), site.bytes().get(start..end))
            })
            .collect::<Vec<_>>();
        eyre::ensure!(
            !observed.is_empty(),
            "missing literal kind observation: {kind}"
        );
        eyre::ensure!(
            observed
                .iter()
                .all(|(complete, text)| *complete && *text == Some(kind))
        );
    }
    let kinds = execution
        .placed_documents()
        .flat_map(|document| crate::resource_identity::placed_resources(&document, &execution))
        .map(|resource| resource.resource.kind)
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(have: kinds, want: BTreeSet::from(["ConfigMap".to_owned(), "Service".to_owned()]));
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
        .push(crate::value::TextPart::Unknown(BTreeSet::new()).into());
    let execution = document(state);
    let observed = execution
        .text_sites()
        .filter(|site| site.span() == span)
        .map(|site| (site.coverage_exact(), site.bytes().to_owned()))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: observed, want: vec![(true, "Kind".to_owned())]);
    sim_assert_eq!(have: alternatives(&execution), want: BTreeSet::from([(Predicate::True, None)]));
}

#[test]
fn dropped_observations_keep_even_an_earlier_literal_query_incomplete() {
    let span = Span::new(0, 4);
    let mut state = State::new(Value::nil());
    state.emit_bytes(span, None, "Kind");
    for _ in 0..128 {
        state.emit_bytes(Span::new(10, 11), None, "X");
    }
    let execution = document(state);
    sim_assert_eq!(have: execution.observed_marker_complete(span), want: false);
    sim_assert_eq!(have: alternatives(&execution), want: BTreeSet::from([(Predicate::True, None)]));
}
