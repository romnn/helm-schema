use std::collections::BTreeSet;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;

fn execute(source: &str) -> crate::value::DocumentExecution {
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    crate::value::execute_document(source, None, &db)
}

fn selected_condition(execution: &crate::value::DocumentExecution) -> Predicate {
    Predicate::Or(
        execution
            .text()
            .filter_map(|(condition, text)| {
                (text.exact_text().as_deref() == Some("selected")).then_some(condition)
            })
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
fn partial_truth_becomes_approximate_without_inverting_its_sound_subset() -> eyre::Result<()> {
    let execution = execute(r#"{{ or .Values.alpha (printf "%q" .Values.opaque) }}"#);
    let site = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing partial output")?;
    let alternatives = site.value().alternatives();
    assert!(
        alternatives.iter().any(|arm| !arm.coverage_exact()),
        "unknown formatting became exact"
    );
    assert!(
        alternatives
            .iter()
            .filter(|arm| !arm.coverage_exact())
            .all(|arm| arm.condition() != &Predicate::truthy_path("alpha").negated()),
        "partial knowledge was inverted into an exact complement"
    );
    Ok(())
}

#[test]
fn condition_polarities_preserve_independent_partial_proofs() {
    let positive =
        execute(r#"{{ if or .Values.enabled (printf "%q" .Values.opaque) }}selected{{ end }}"#);
    let negative = execute(
        r#"{{ if not (or .Values.enabled (printf "%q" .Values.opaque)) }}selected{{ end }}"#,
    );
    let positive = selected_condition(&positive);
    let negative = selected_condition(&negative);
    assert!(
        positive.contains_approximation() || positive == Predicate::True,
        "partial positive condition became exact: {positive:?}"
    );
    assert!(
        negative.contains_approximation() || negative == Predicate::True,
        "partial negative condition became exact: {negative:?}"
    );
}

#[test]
fn complete_subset_claim_requires_a_total_disjoint_partition() {
    let execution = execute("{{ if .Values.selected }}selected{{ else }}fallback{{ end }}");
    let states = execution
        .text()
        .map(|(condition, _)| condition)
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
    assert!(selected_condition(&partial) != Predicate::truthy_path("alpha"));
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
    assert!(selected_condition(&partial) != Predicate::truthy_path("primary").negated());
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
    let (_, output) = execution
        .text()
        .next()
        .ok_or_eyre("missing eager-effect witness")?;
    sim_assert_eq!(have: output.exact_text(), want: Some("selectedtrue".to_string()));
    Ok(())
}
