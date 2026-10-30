use std::rc::Rc;

use super::{ExecutedSite, ScalarContent};
use crate::value::{Coverage, ScalarType, State, Value};
use color_eyre::eyre;
use helm_schema_core::{GuardValue, Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

pub(crate) struct ExecutedOutput {
    pub(crate) condition: Predicate,
    pub(crate) text: Option<String>,
}

impl crate::value::DocumentExecution {
    pub(crate) fn executed_outputs(&self) -> Vec<ExecutedOutput> {
        let memo = helm_schema_core::PredicateMemo::new();
        let mut outputs = Vec::new();
        for exit in &self.exits {
            if !matches!(exit.flow, crate::value::Flow::Normal(_)) {
                continue;
            }
            let value = crate::value::text::capture(&exit.state.output, false)
                .under_runtime(&crate::value::guard::RuntimeGuard::True);
            for alternative in exit.state.heap.freeze(&value).alternatives() {
                let condition = memo.normalize(Predicate::all(vec![
                    exit.state.path_predicate(),
                    alternative.condition().clone(),
                ]));
                if condition != Predicate::False {
                    outputs.push(ExecutedOutput {
                        condition,
                        text: alternative
                            .coverage_exact()
                            .then(|| alternative.value().exact_text())
                            .flatten(),
                    });
                }
            }
        }
        outputs
    }

    pub(crate) fn observed_marker_condition(&self, span: helm_schema_syntax::Span) -> Predicate {
        Predicate::Or(
            self.text_sites()
                .filter(|site| site.span().start < span.end && span.start < site.span().end)
                .map(|site| site.condition().clone())
                .collect(),
        )
        .normalize_boolean()
    }

    pub(crate) fn observed_marker_sufficient_condition(
        &self,
        span: helm_schema_syntax::Span,
    ) -> Predicate {
        Predicate::Or(
            self.text_sites()
                .filter(|site| site.span().start < span.end && span.start < site.span().end)
                .map(|site| {
                    let condition = &site.condition;
                    let ambient = condition
                        .arena
                        .budget_test(condition.arena.runtime_path(&condition.ambient));
                    Predicate::all(vec![
                        ambient.when_true.clone(),
                        condition
                            .restricted_test()
                            .map_or(Predicate::True, |test| test.when_true.clone()),
                    ])
                })
                .collect(),
        )
        .normalize_boolean()
    }

    pub(crate) fn observed_marker_complete(&self, span: helm_schema_syntax::Span) -> bool {
        self.exits
            .iter()
            .filter(|exit| matches!(exit.flow, crate::value::Flow::Normal(_)))
            .all(|exit| {
                !exit.state.path_predicate().contains_approximation()
                    && !exit.state.observation_count.overflow_under(
                        &exit.state.heap.arena.runtime_path(&exit.state.active),
                        &exit.state.heap.arena,
                    )
                    && exit
                        .state
                        .active
                        .iter()
                        .all(|edge| edge.test.coverage == Coverage::Exact)
            })
            && self
                .text_sites()
                .filter(|site| site.span().start < span.end && span.start < site.span().end)
                .all(|site| {
                    site.coverage_exact()
                        && site.rendered_unchanged
                        && !site.condition().contains_approximation()
                })
    }
}

fn site(state: &State, value: &Value) -> ExecutedSite {
    ExecutedSite {
        program: 0,
        coverage_exact: true,
        rendered_unchanged: true,
        ordinal: 0,
        span: helm_schema_syntax::Span::new(0, 1),
        source_path: None,
        condition: Predicate::True.into(),
        value: state.heap.freeze(value),
        plain_scalar_placement: helm_schema_core::PlainScalarPlacement::Unknown,
        rendered: true,
    }
}

#[test]
fn exact_scalar_selections_keep_all_nested_arm_guards() {
    let state = State::new(Value::nil());
    let outer = Predicate::truthy_path("outer");
    let inner = Predicate::truthy_path("inner");
    let outer_decision =
        state
            .heap
            .arena
            .decision(outer.clone(), outer.clone().negated(), Coverage::Exact);
    let inner_decision =
        state
            .heap
            .arena
            .decision(inner.clone(), inner.clone().negated(), Coverage::Exact);
    let value = Value::select(
        outer_decision,
        Value::select(inner_decision, Value::string("x"), Value::string("y")),
        Value::input(ValuesPath::parse("fallback")),
    );
    let projected = site(&state, &value).projected_scalar();
    let expected = crate::value::ScalarValueDispatch::bind_content(
        vec![
            (
                Predicate::all(vec![outer.clone(), inner.clone()]).normalize_boolean(),
                ScalarContent::Literal(GuardValue::String("x".to_owned())),
            ),
            (
                Predicate::all(vec![outer.clone(), inner.negated()]).normalize_boolean(),
                ScalarContent::Literal(GuardValue::String("y".to_owned())),
            ),
            (
                outer.negated(),
                ScalarContent::Identity(ValuesPath::parse("fallback")),
            ),
        ],
        true,
    );
    sim_assert_eq!(have: projected, want: Some(expected));
}

#[test]
fn every_scalar_content_variant_survives_projection() {
    let state = State::new(Value::nil());
    let path = ValuesPath::parse("input");
    for content in [
        ScalarContent::Literal(GuardValue::String("x".to_owned())),
        ScalarContent::Identity(path.clone()),
        ScalarContent::Rendered(vec![crate::value::scalar_value::ScalarRenderPart::Text(
            "x".to_owned(),
        )]),
        ScalarContent::PrintfStringIdentity(path.clone()),
        ScalarContent::SplitLength {
            value: Box::new(ScalarContent::Identity(path.clone())),
            separator: ":".to_owned(),
        },
        ScalarContent::Length(Box::new(ScalarContent::Identity(path))),
    ] {
        let value = Value::scalar(ScalarType::String, Some(content.clone()));
        sim_assert_eq!(have: site(&state, &value).projected_scalar(), want: Some(crate::value::ScalarValueDispatch::bind_content(vec![(Predicate::True, content)], true)));
    }
}

#[test]
fn unknown_scalar_arms_keep_known_evidence_incomplete() {
    let state = State::new(Value::nil());
    let condition = Predicate::truthy_path("choose");
    let decision = state.heap.arena.decision(
        condition.clone(),
        condition.clone().negated(),
        Coverage::Exact,
    );
    let value = Value::select(decision, Value::string("known"), Value::unknown());
    sim_assert_eq!(have: site(&state, &value).projected_scalar(), want: Some(crate::value::ScalarValueDispatch::bind_content(vec![(condition, ScalarContent::Literal(GuardValue::String("known".to_owned())))], false)));
}

#[test]
fn partial_decisions_and_inexact_sites_abstain() {
    let state = State::new(Value::nil());
    let decision = state
        .heap
        .arena
        .decision(Predicate::False, Predicate::False, Coverage::Partial);
    let value = Value::select(Rc::clone(&decision), Value::string("x"), Value::string("y"));
    sim_assert_eq!(have: site(&state, &value).projected_scalar(), want: None);
    let mut inexact = site(&state, &Value::string("x"));
    inexact.coverage_exact = false;
    sim_assert_eq!(have: inexact.projected_scalar(), want: None);
}

#[test]
fn changed_output_keeps_only_marker_safe_scalar_placement() {
    let state = State::new(Value::nil());
    let path = ValuesPath::parse("input");
    let mut output = site(&state, &Value::input(path.clone()));
    output.rendered_unchanged = false;
    sim_assert_eq!(
        have: output.rendered_scalar(),
        want: Some(crate::value::ScalarValueDispatch::bind_content(
            vec![(
                super::marker_preserving_scalar(&path, &state.heap.arena.predicate_memo),
                ScalarContent::Identity(path),
            )],
            false,
        )),
    );
}

#[test]
fn marker_overlap_language_includes_both_emission_boundaries() -> eyre::Result<()> {
    let pattern = regex::Regex::new(super::marker_overlap_pattern())?;
    for text in ["1<no value>", "value>", "<no ", "value", "", " "] {
        assert!(
            pattern.is_match(text),
            "marker overlap must abstain: {text:?}"
        );
    }
    for text in ["true", "false", "443", "repo: bad", "yeah", "_443", "0x10"] {
        assert!(
            !pattern.is_match(text),
            "unchanged scalar keeps evidence: {text:?}"
        );
    }
    Ok(())
}

#[test]
fn rendered_input_tolerance_keeps_the_original_execution_guard() {
    let db = crate::analysis_db::IrAnalysisDb::new(&helm_schema_ast::DefineIndex::new());
    let execution = crate::value::tests::execute_document(
        "{{ if .Values.emit }}port: {{ .Values.port }}{{ end }}",
        None,
        &db,
    );
    let uses = execution.projection_reads();
    let uses: Vec<_> = uses
        .iter()
        .filter(|read| read.kind == crate::ValueKind::Serialized)
        .collect();
    sim_assert_eq!(have: uses.len(), want: 1);
    for read in uses {
        sim_assert_eq!(have: &read.values_path, want: &ValuesPath::parse("port"));
        let condition = Predicate::Or(
            read.condition
                .disjuncts()
                .iter()
                .map(|branch| Predicate::all(branch.iter().cloned().collect()))
                .collect(),
        );
        let expected = Predicate::truthy_path("emit");
        assert!(condition.exactly_implies(&expected));
        assert!(expected.exactly_implies(&condition));
    }
}
