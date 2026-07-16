//! Round-trip + canonicalisation tests for the typed Diagnostic enum.

use helm_schema_k8s::{
    ApiVersionCandidate, Diagnostic, DiagnosticSink, InferenceSource, ProviderOrigin,
    format_diagnostic_json, format_diagnostic_text,
};
use test_util::prelude::sim_assert_eq;

fn sample_variants() -> Vec<Diagnostic> {
    vec![
        Diagnostic::MissingSchema {
            kind: "Foo".to_string(),
            api_version: "x/v1".to_string(),
            k8s_versions_tried: vec!["v1.35.0".to_string()],
            tried_filenames: vec!["foo.json".to_string()],
            available_in_cache_versions: vec![],
            suggested_k8s_version: Some("v1.24.0".to_string()),
            hint: Some("hint".to_string()),
        },
        Diagnostic::ResolvedFromFallbackVersion {
            kind: "Bar".to_string(),
            api_version: "policy/v1beta1".to_string(),
            primary_version: "v1.35.0".to_string(),
            resolved_version: "v1.24.0".to_string(),
        },
        Diagnostic::InferredApiVersion {
            kind: "ServiceMonitor".to_string(),
            inferred_api_version: "monitoring.coreos.com/v1".to_string(),
            source: InferenceSource::Shortlist,
            origin: ProviderOrigin::KubernetesOpenApi,
        },
        Diagnostic::AmbiguousApiVersion {
            kind: "Ingress".to_string(),
            candidates: vec![ApiVersionCandidate {
                api_version: "networking.k8s.io/v1".to_string(),
                source: InferenceSource::Shortlist,
                origin: ProviderOrigin::KubernetesOpenApi,
            }],
        },
        Diagnostic::CrdVersionNotFound {
            group: "g".to_string(),
            kind: "K".to_string(),
            requested_version: "v1alpha1".to_string(),
            locations_tried: vec!["url-a".to_string()],
        },
        Diagnostic::CrdVersionAvailableAtOtherVersions {
            group: "g".to_string(),
            kind: "K".to_string(),
            requested_version: "v1alpha1".to_string(),
            available_versions: vec!["v1".to_string()],
        },
        Diagnostic::LocalOverrideUnreadable {
            kind: "K".to_string(),
            api_version: "g/v1".to_string(),
            override_path: "/path/to/file".to_string(),
            io_error: "permission denied".to_string(),
        },
        Diagnostic::CacheLayoutInvalidated {
            cache_root: "/cache".to_string(),
            previous_marker: None,
            current_marker: 1,
        },
        Diagnostic::CacheLayoutForwardIncompatible {
            cache_root: "/cache".to_string(),
            on_disk_marker: 99,
            compiled_marker: 1,
        },
        Diagnostic::InputChannelNumericRangeAmbiguity {
            value_path: "servers".to_string(),
        },
    ]
}

#[test]
fn diagnostic_json_round_trip_per_variant() {
    for d in sample_variants() {
        let json = format_diagnostic_json(&d).expect("serialize");
        let parsed: Diagnostic = serde_json::from_str(&json).expect("round-trip parse");
        sim_assert_eq!(have: d, want: parsed, "round-trip lossy for {d:?}");
    }
}

#[test]
fn diagnostic_text_format_per_variant() {
    for d in sample_variants() {
        let text = format_diagnostic_text(&d);
        assert!(
            text.starts_with("warning:") || text.starts_with("info:"),
            "diagnostic text must start with warning: or info:; got {text:?}"
        );
    }
}

#[test]
fn diagnostic_payload_canonicalised_on_insert() {
    let sink = DiagnosticSink::new();
    sink.push(Diagnostic::MissingSchema {
        kind: "Foo".to_string(),
        api_version: "v1".to_string(),
        k8s_versions_tried: vec!["v1.35.0".to_string()],
        tried_filenames: vec!["b.json".to_string(), "a.json".to_string()],
        available_in_cache_versions: vec![],
        suggested_k8s_version: None,
        hint: None,
    });
    sink.push(Diagnostic::MissingSchema {
        kind: "Foo".to_string(),
        api_version: "v1".to_string(),
        k8s_versions_tried: vec!["v1.35.0".to_string()],
        tried_filenames: vec!["a.json".to_string(), "b.json".to_string()],
        available_in_cache_versions: vec![],
        suggested_k8s_version: None,
        hint: None,
    });
    let snapshot = sink.snapshot();
    sim_assert_eq!(have: snapshot.len(), want: 1, "dedupe by key");
    match &snapshot[0] {
        Diagnostic::MissingSchema {
            tried_filenames, ..
        } => {
            sim_assert_eq!(
                have: tried_filenames,
                want: &vec!["a.json".to_string(), "b.json".to_string()]
            );
        }
        other => panic!("unexpected variant: {other:?}"),
    }
}

#[test]
fn diagnostic_dedupe_per_resource() {
    // 5 ResolvedFromFallbackVersion events for the same (kind, api_version,
    // resolved_version) triple → exactly one entry in the sink.
    let sink = DiagnosticSink::new();
    for _ in 0..5 {
        sink.push(Diagnostic::ResolvedFromFallbackVersion {
            kind: "PodDisruptionBudget".to_string(),
            api_version: "policy/v1beta1".to_string(),
            primary_version: "v1.35.0".to_string(),
            resolved_version: "v1.24.0".to_string(),
        });
    }
    sim_assert_eq!(
        have: sink.len(),
        want: 1,
        "dedupe by (kind, api_version, resolved_version)"
    );
}

#[test]
fn diagnostic_iteration_order_deterministic() {
    let mut orders = Vec::new();
    for _ in 0..3 {
        let sink = DiagnosticSink::new();
        for variant in sample_variants() {
            sink.push(variant);
        }
        orders.push(
            sink.snapshot()
                .into_iter()
                .map(|d| format!("{:?}", d.key()))
                .collect::<Vec<_>>(),
        );
    }
    sim_assert_eq!(have: orders[0], want: orders[1]);
    sim_assert_eq!(have: orders[1], want: orders[2]);
}
