use std::collections::BTreeSet;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::ExecutedText;

fn execute(source: &str) -> crate::value::DocumentExecution {
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    crate::value::tests::execute_document(source, None, &db)
}

fn selected_condition(execution: &crate::value::DocumentExecution) -> Predicate {
    Predicate::Or(
        execution
            .text_sites()
            .filter(|site| site.bytes() == "selected")
            .map(ExecutedText::condition)
            .collect(),
    )
    .normalize_boolean()
}

#[test]
fn truth_and_dispatch_adapters_preserve_exactness_and_truth_source() {
    for (source, expected) in [
        (
            "{{ if .Values.alpha }}selected{{ end }}",
            Predicate::truthy_path("alpha"),
        ),
        (
            "{{ if not .Values.alpha }}selected{{ end }}",
            Predicate::truthy_path("alpha").negated(),
        ),
    ] {
        sim_assert_eq!(have: selected_condition(&execute(source)), want: expected);
    }
}

#[test]
fn condition_reachability_distinguishes_raw_identities_from_rendered_scalars() {
    sim_assert_eq!(have: selected_condition(&execute("{{ if false }}selected{{ end }}")), want: Predicate::False);
    sim_assert_eq!(have: selected_condition(&execute(r#"{{ if printf "%s" false }}selected{{ end }}"#)), want: Predicate::True);
}

#[test]
fn partial_truth_keeps_its_positive_proof_without_inverting_the_unknown_remainder() {
    let positive = execute(
        r#"{{ if or .Values.alpha (eq (randAlphaNum 1) "a") }}{{ fail "selected" }}{{ end }}"#,
    );
    let negative = execute(
        r#"{{ if not (or .Values.alpha (eq (randAlphaNum 1) "a")) }}{{ fail "selected" }}{{ end }}"#,
    );
    sim_assert_eq!(have: positive.abort_predicate(), want: Predicate::truthy_path("alpha"));
    sim_assert_eq!(have: negative.abort_predicate(), want: Predicate::False);
}

#[test]
fn condition_polarities_preserve_independent_partial_proofs() {
    let positive =
        execute(r#"{{ if or .Values.enabled (eq (randAlphaNum 1) "a") }}# selected{{ end }}"#);
    let negative = execute(
        r#"{{ if not (or .Values.enabled (eq (randAlphaNum 1) "a")) }}# selected{{ end }}"#,
    );
    let positive = positive
        .text_sites()
        .filter(|site| site.bytes() == "# selected")
        .collect::<Vec<_>>();
    let negative = negative
        .text_sites()
        .filter(|site| site.bytes() == "# selected")
        .collect::<Vec<_>>();
    assert!(
        positive
            .iter()
            .any(|site| site.coverage_exact()
                && site.condition() == Predicate::truthy_path("enabled")),
        "the known positive arm was lost"
    );
    assert!(
        positive.iter().any(|site| !site.coverage_exact()),
        "the random remainder became exact"
    );
    assert!(
        !negative.is_empty(),
        "the possible negative arm was dropped"
    );
    assert!(
        negative.iter().all(|site| !site.coverage_exact()),
        "the random remainder became an exact complement"
    );
}

#[test]
fn complete_subset_claim_requires_a_total_disjoint_partition() {
    let execution = execute("{{ if .Values.selected }}selected{{ else }}fallback{{ end }}");
    let states = execution
        .executed_outputs()
        .into_iter()
        .map(|output| output.condition)
        .collect::<Vec<_>>();
    sim_assert_eq!(have: Predicate::Or(states.clone()).normalize_boolean(), want: Predicate::True);
    sim_assert_eq!(have: Predicate::all(states).normalize_boolean(), want: Predicate::False);
}

#[test]
fn approximate_reachability_lowers_distinct_output_and_execution_roles() -> eyre::Result<()> {
    let execution = execute(
        r#"{{ $value := printf "%q" .Values.alpha }}{{ $value }}{{ if $value }}selected{{ end }}"#,
    );
    let site = execution
        .all_sites()
        .find(|site| !site.rendered())
        .ok_or_eyre("missing frozen assignment")?;
    sim_assert_eq!(have: site.value().identity(), want: None);
    sim_assert_eq!(have: site.value().inputs(), want: BTreeSet::from([ValuesPath::parse("alpha")]));
    assert!(site.value().literal().is_none());
    let condition = selected_condition(&execution);
    assert!(condition.contains_approximation() || condition == Predicate::True);
    Ok(())
}

#[test]
fn exactness_demotes_approximation_and_canonicalizes_true_subsets() {
    let exact = execute(r#"{{ if or true (printf "%q" .Values.alpha) }}selected{{ end }}"#);
    sim_assert_eq!(have: selected_condition(&exact), want: Predicate::True);
    assert!(exact.reads().is_empty(), "dead operand was evaluated");
    let partial =
        execute(r#"{{ if or .Values.alpha (printf "%q" .Values.beta) }}selected{{ end }}"#);
    assert_ne!(
        selected_condition(&partial),
        Predicate::truthy_path("alpha")
    );
}

#[test]
fn complement_preserves_only_invertible_selection_knowledge() {
    for (source, expected) in [
        ("{{ if not true }}selected{{ end }}", Predicate::False),
        ("{{ if not false }}selected{{ end }}", Predicate::True),
        (
            "{{ if not .Values.primary }}selected{{ end }}",
            Predicate::truthy_path("primary").negated(),
        ),
    ] {
        sim_assert_eq!(have: selected_condition(&execute(source)), want: expected);
    }
    let partial = execute(r#"{{ if not (printf "%q" .Values.primary) }}selected{{ end }}"#);
    assert_ne!(
        selected_condition(&partial),
        Predicate::truthy_path("primary").negated()
    );
}

#[test]
fn default_selection_adapter_exposes_all_states_with_owned_truth_sources() -> eyre::Result<()> {
    for (source, expected) in [
        (r#"{{ default "fallback" "" }}"#, "fallback"),
        (r#"{{ default "fallback" "set" }}"#, "set"),
    ] {
        let execution = execute(source);
        let (_, text) = execution
            .text()
            .next()
            .ok_or_eyre("missing default output")?;
        sim_assert_eq!(have: text.exact_text(), want: Some(expected.to_string()));
    }
    let raw = execute(r#"{{ default "fallback" .Values.alpha }}"#);
    let site = raw
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing selected value")?;
    let alternatives = site.value().alternatives();
    assert!(alternatives.iter().any(
        |arm| arm.value().exact_text().as_deref() == Some("fallback")
            && arm.condition() == &Predicate::truthy_path("alpha").negated()
    ));
    let rendered = execute(r#"{{ default "fallback" (printf "%s" false) }}"#);
    assert!(
        rendered
            .text()
            .all(|(_, text)| text.exact_text().as_deref() != Some("fallback"))
    );
    Ok(())
}

#[test]
fn dead_output_selection_retains_eager_effects() -> eyre::Result<()> {
    let execution = execute(
        r#"{{- $state := dict -}}{{- default (set $state "fallback" true) "selected" -}}{{- $state.fallback -}}"#,
    );
    let output = execution
        .executed_outputs()
        .into_iter()
        .next()
        .ok_or_eyre("missing eager-effect witness")?;
    sim_assert_eq!(have: output.text, want: Some("selectedtrue".to_string()));
    Ok(())
}
