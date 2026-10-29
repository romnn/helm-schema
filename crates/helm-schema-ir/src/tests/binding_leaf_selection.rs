//! Adversarial binding selection witnesses observed through frozen executed values.

use std::collections::BTreeSet;

use helm_schema_ast::DefineIndex;
use helm_schema_core::{Conjunction, Guard, GuardValue, Predicate, ValuesPath};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::observed_facts::CaptureKind;

fn execute(source: &str) -> crate::value::DocumentExecution {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "selection.tpl",
        indoc! {r#"
        {{- define "unknown" -}}{{ .Values.unknown }}{{- end -}}
        {{- define "rooted" -}}{{ $.Values.child }}{{- end -}}
    "#},
    );
    let db = IrAnalysisDb::new(&defines);
    crate::value::tests::execute_document(source, None, &db)
}

fn records_unresolved_width(value: &crate::value::FrozenValue) -> bool {
    value.alternatives().iter().any(|arm| {
        !arm.coverage_exact()
            || (arm.value().literal().is_none() && arm.value().identity().is_none())
    })
}

/// A loop's unknown remainder must survive both whole-binding and member reads.
#[test]
fn selector_read_of_an_unresolved_binding_keeps_the_unknown_remainder() {
    let source = indoc! {r"
        {{- $cfg := fromYaml .Values.unknown -}}
        {{- range .Values.entries -}}{{- $cfg = $.Values.first -}}{{- end -}}
        {{- $cfg -}}{{- $cfg.child -}}
    "};
    let execution = execute(source);
    let sites = execution
        .sites()
        .filter(|site| site.rendered())
        .collect::<Vec<_>>();
    sim_assert_eq!(have: sites.len(), want: 2);
    for site in sites {
        assert!(
            records_unresolved_width(site.value()),
            "binding read lost its unresolved remainder: {site:?}"
        );
    }
}

/// Only a proved selection subset may contribute an exact values identity.
#[test]
fn mixed_selection_read_reports_the_proven_arm_and_the_unresolved_remainder() {
    let execution = execute(indoc! {r#"
        {{- $cfg := .Values.unproven -}}
        {{- if or .Values.flag (include "unknown" .) -}}{{- $cfg = .Values.proven -}}{{- end -}}
        {{- $cfg -}}
    "#});
    let sites = execution
        .sites()
        .filter(|site| site.rendered() && site.source_path().is_none())
        .collect::<Vec<_>>();
    assert!(!sites.is_empty());
    let proven = sites
        .iter()
        .flat_map(|site| site.value().alternatives())
        .filter(super::super::value::ValueAlternative::coverage_exact)
        .filter_map(|arm| arm.value().identity().cloned())
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(have: proven, want: BTreeSet::from([ValuesPath::parse("proven")]));
    assert!(
        sites
            .iter()
            .any(|site| records_unresolved_width(site.value()))
    );
}

/// Mutually exclusive wrapped and bare bindings must not become one conjunction.
#[test]
fn a_proven_arm_does_not_co_assert_a_sibling_binding_of_the_same_path() {
    let execution = execute(indoc! {r#"
        {{- $secrets := .Values.pullSecrets -}}
        {{- if or .Values.named (include "unknown" .) -}}
          {{- $secrets = dict "name" .Values.pullSecrets -}}
        {{- end -}}
        {{- $secrets -}}
    "#});
    let outputs = execution
        .sites()
        .filter(|site| site.rendered() && site.source_path().is_none())
        .collect::<Vec<_>>();
    assert!(!outputs.is_empty());
    for site in outputs {
        let alternatives = site.value().alternatives();
        let exact = alternatives
            .iter()
            .filter(|arm| arm.coverage_exact())
            .collect::<Vec<_>>();
        assert!(
            !exact
                .iter()
                .any(|arm| arm.value().identity() == Some(&ValuesPath::parse("pullSecrets"))),
            "unproved bare binding was published as exact"
        );
    }
}

/// Strict obligations remain scoped to the provable selection subset.
#[test]
#[ignore = "R3b regression vs main, package A (consumed/comparison provenance): mixed selection proves only the decidable arm; tracked in plan/value-algebra-r3b.md follow-ups"]
fn mixed_selection_proves_only_the_decidable_arm() {
    let execution = execute(indoc! {r#"
        {{- $cfg := .Values.unproven -}}
        {{- if or .Values.flag (include "unknown" .) -}}{{- $cfg = .Values.proven -}}{{- end -}}
        {{- semverCompare "<1.0.0" $cfg -}}
    "#});
    let captures = &execution.observed_facts().captures;
    assert!(!captures.is_empty(), "strict consumer lost all obligations");
    assert!(
        captures.iter().all(|capture| capture
            .conjunction
            .contains(&Predicate::truthy_path("flag"))),
        "strict obligation escaped its proven arm: {captures:?}"
    );
}

#[test]
fn values_rooted_selector_on_a_values_path_root_records_strict_member_hosts() {
    let execution = execute(r#"{{ include "rooted" .Values.sub }}"#);
    assert_values_rooted_hosts(&execution);
}

#[test]
fn values_rooted_direct_selector_on_a_values_path_root_records_strict_member_hosts() {
    let execution = execute("{{ $ctx := .Values.sub }}{{ $ctx.Values.child }}");
    assert_values_rooted_hosts(&execution);
}

fn assert_values_rooted_hosts(execution: &crate::value::DocumentExecution) {
    // Below the supplied argument, Values is an ordinary member. An invalid entry stays nil-safe.
    let live = Predicate::Or(vec![
        Predicate::from(Guard::Absent {
            path: ValuesPath::parse("sub"),
        }),
        Predicate::from(Guard::Eq {
            path: ValuesPath::parse("sub"),
            value: GuardValue::Null,
        }),
    ])
    .negated()
    .normalize_boolean();
    let expected = ["sub", "sub.Values"]
        .map(|path| {
            Conjunction::new([
                live.clone(),
                Predicate::from(Guard::TypeIs {
                    path: ValuesPath::parse(path),
                    schema_type: "object".to_owned(),
                })
                .negated(),
            ])
        })
        .into_iter()
        .collect::<BTreeSet<_>>();
    let actual = execution
        .observed_facts()
        .captures
        .iter()
        .filter(|capture| matches!(capture.kind, CaptureKind::MemberAccess { .. }))
        .map(|capture| capture.conjunction.clone())
        .collect::<BTreeSet<_>>();
    sim_assert_eq!(have: actual, want: expected);
}
