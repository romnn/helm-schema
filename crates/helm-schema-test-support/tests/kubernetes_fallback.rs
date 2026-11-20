//! The offline validator resolves resource schemas with the product's version chain:
//! the primary release first, then ADR 0032's lookup-only fallback releases; CRDs only at
//! their pinned version (ADR 0033).

use std::path::PathBuf;

use color_eyre::eyre;
use helm_schema_test_support::acceptance::{
    AcceptanceResult, AdjudicatedProbe, HelmFlipVerdict, fold_results,
};
use helm_schema_test_support::helm::adjudication::{
    FallbackJudgment, KUBERNETES_RELEASE, KubernetesVerdict, OfflineKubernetesValidator,
};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

fn bundle() -> PathBuf {
    test_util::workspace_testdata().join("provider-bundle")
}

fn validator() -> eyre::Result<OfflineKubernetesValidator> {
    OfflineKubernetesValidator::with_crd_catalog(
        &bundle().join("kubernetes-json-schema-cache"),
        &bundle().join("crds-catalog-cache"),
        KUBERNETES_RELEASE,
    )
}

/// prometheus-adapter's custom-metrics `APIService`, as its template renders it.
fn api_service(api_version: &str, group_priority_minimum: &Value) -> Value {
    json!({
        "apiVersion": api_version,
        "kind": "APIService",
        "metadata": {"name": "v1beta1.custom.metrics.k8s.io"},
        "spec": {
            "service": {"name": "adapter", "namespace": "default"},
            "group": "custom.metrics.k8s.io",
            "version": "v1beta1",
            "insecureSkipTLSVerify": true,
            "groupPriorityMinimum": group_priority_minimum,
            "versionPriority": 100,
        },
    })
}

fn violation_paths(verdict: KubernetesVerdict) -> eyre::Result<Vec<String>> {
    let KubernetesVerdict::Invalid(violations) = verdict else {
        eyre::bail!("expected proven violations, got {verdict:?}");
    };
    Ok(violations
        .into_iter()
        .map(|violation| format!("{}: {}", violation.instance_path, violation.key.keyword))
        .collect())
}

/// `apiregistration.k8s.io/v1beta1` left Kubernetes in v1.22; v1.15.0's release catalog is the
/// newest snapshot in the bundle that declares it. The judgment names that release.
#[test]
fn a_removed_api_service_is_judged_by_the_newest_release_that_declares_it() -> eyre::Result<()> {
    let validator = validator()?;
    let fallback = vec![FallbackJudgment {
        location: "document 0".to_string(),
        api_version: "apiregistration.k8s.io/v1beta1".to_string(),
        kind: "APIService".to_string(),
        name: Some("v1beta1.custom.metrics.k8s.io".to_string()),
        release: "v1.15.0".to_string(),
    }];
    let valid = api_service("apiregistration.k8s.io/v1beta1", &json!(100));
    sim_assert_eq!(
        have: validator.validate_documents_with_fallback(&[valid]),
        want: (KubernetesVerdict::Valid, fallback.clone()),
    );
    let mistyped = api_service("apiregistration.k8s.io/v1beta1", &json!("high"));
    let (verdict, judged) = validator.validate_documents_with_fallback(&[mistyped]);
    sim_assert_eq!(
        have: violation_paths(verdict)?,
        want: vec!["/spec/groupPriorityMinimum: type".to_string()],
    );
    sim_assert_eq!(have: judged, want: fallback);
    // The fallback derivation is the strict shape the primary release's bundle has.
    let mut unknown_field = api_service("apiregistration.k8s.io/v1beta1", &json!(100));
    unknown_field["spec"]["unknownField"] = json!(true);
    sim_assert_eq!(
        have: violation_paths(validator.validate_documents(&[unknown_field]))?,
        want: vec!["/spec/unknownField: additionalProperties".to_string()],
    );
    Ok(())
}

#[test]
fn a_served_api_service_is_judged_at_the_primary_release() -> eyre::Result<()> {
    let validator = validator()?;
    let valid = api_service("apiregistration.k8s.io/v1", &json!(100));
    sim_assert_eq!(
        have: validator.validate_documents_with_fallback(&[valid]),
        want: (KubernetesVerdict::Valid, Vec::new()),
    );
    let mistyped = api_service("apiregistration.k8s.io/v1", &json!("high"));
    sim_assert_eq!(
        have: violation_paths(validator.validate_documents(&[mistyped]))?,
        want: vec!["/spec/groupPriorityMinimum: type".to_string()],
    );
    Ok(())
}

#[test]
fn a_kind_no_release_declares_stays_undecided() -> eyre::Result<()> {
    let document =
        json!({"apiVersion": "apps/v1", "kind": "Undeclared", "metadata": {"name": "x"}});
    sim_assert_eq!(
        have: validator()?.validate_documents(&[document]),
        want: KubernetesVerdict::Uncertain(vec![
            "document 0: apps/v1/Undeclared x: pinned resource schema not found".to_string(),
        ]),
    );
    Ok(())
}

/// The catalog pins `ServiceMonitor` only at `v1`; another version is never substituted.
#[test]
fn a_custom_resource_is_never_substituted() -> eyre::Result<()> {
    let document = json!({
        "apiVersion": "monitoring.coreos.com/v1alpha1",
        "kind": "ServiceMonitor",
        "metadata": {"name": "x"},
        "spec": {"endpoints": "not a list"},
    });
    sim_assert_eq!(
        have: validator()?.validate_documents(&[document]),
        want: KubernetesVerdict::Uncertain(vec![
            "document 0: monitoring.coreos.com/v1alpha1/ServiceMonitor x: pinned CRD schema not found"
                .to_string(),
        ]),
    );
    Ok(())
}

/// The primary release authoritatively omits `policy/v1beta1`, so the server rejects the
/// resource before any schema applies; v1.24.0's catalog declaring it changes nothing.
#[test]
fn a_kind_the_primary_release_does_not_serve_is_not_graded_at_a_fallback() -> eyre::Result<()> {
    let document = json!({
        "apiVersion": "policy/v1beta1",
        "kind": "PodDisruptionBudget",
        "metadata": {"name": "x"},
        "spec": {"minAvailable": 1},
    });
    sim_assert_eq!(
        have: violation_paths(validator()?.validate_documents(&[document]))?,
        want: vec!["/apiVersion: served".to_string()],
    );
    Ok(())
}

/// The coverage report counts each verdict once, even with contributions from both namespaces.
#[test]
fn the_coverage_counts_each_verdict_a_fallback_release_decided() -> eyre::Result<()> {
    let primary_namespace = "default: document 10: apiregistration.k8s.io/v1beta1/APIService \
                             v1beta1.custom.metrics.k8s.io: judged by fallback release v1.15.0";
    let second_namespace = "second: document 10: apiregistration.k8s.io/v1beta1/APIService \
                            v1beta1.custom.metrics.k8s.io: judged by fallback release v1.15.0";
    let probe = |flip: usize, verdict: HelmFlipVerdict, fallback: &[&str]| {
        (
            (0, flip + 1),
            AcceptanceResult::Probe(Box::new(AdjudicatedProbe {
                flip,
                key: format!("flip {flip}"),
                case: format!("chart: flip {flip}"),
                document_kind: None,
                kubernetes_version: "1.29.0".to_string(),
                input: None,
                outcome: Ok((verdict, PathBuf::from("evidence"))),
                decisive_kubernetes_fallback: fallback.iter().map(ToString::to_string).collect(),
                adjudication_ms: 0,
            })),
        )
    };
    let comparison = fold_results(
        vec![
            probe(
                0,
                HelmFlipVerdict::TighteningMatchedKubernetesRejection,
                &[primary_namespace, second_namespace],
            ),
            probe(1, HelmFlipVerdict::TighteningMatchedHelmAbort, &[]),
            probe(
                2,
                HelmFlipVerdict::CandidateAcceptsKubernetesRejects,
                &[second_namespace],
            ),
        ],
        true,
    )?;
    let coverage = serde_json::to_value(&comparison.helm_adjudication)?;
    sim_assert_eq!(
        have: json!([
            coverage["verdicts_decided_at_fallback_releases"],
            coverage["fallback_release_verdicts"],
        ]),
        want: json!([2, [
            {
                "case": "chart: flip 0",
                "verdict": "TighteningMatchedKubernetesRejection",
                "resources": [primary_namespace, second_namespace],
            },
            {
                "case": "chart: flip 2",
                "verdict": "CandidateAcceptsKubernetesRejects",
                "resources": [second_namespace],
            },
        ]]),
    );
    Ok(())
}
