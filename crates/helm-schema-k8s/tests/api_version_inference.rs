//! Feature D end-to-end tests.

use std::sync::Arc;

use color_eyre::eyre;
use helm_schema_core::{ProviderSchemaUse, ResourceRef, ResourceSchemaOracle, ValueKind, YamlPath};
use helm_schema_k8s::inference::aggregate;
use helm_schema_k8s::{
    ApiVersionCandidate, ApiVersionInferenceOutcome, Chain, CrdsCatalogSchemaProvider, Diagnostic,
    DiagnosticSink, InferenceSource, K8sSchemaProvider, K8sVersionChain,
    KubernetesJsonSchemaProvider, ProviderLookupResult, ProviderOrigin, source_id_for_url,
};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

/// Shared provider fixtures for K8s integration tests.
pub mod common;
use common::{MockFetcher, MockResponse};

fn tmp_dir(label: &str) -> eyre::Result<std::path::PathBuf> {
    Ok(ScratchDir::new(label)?.keep())
}

fn use_with_kind(kind: &str) -> ProviderSchemaUse {
    ProviderSchemaUse {
        value_path: helm_schema_core::ValuesPath::parse("x"),
        path: YamlPath(vec!["spec".to_string()]),
        kind: ValueKind::Scalar,
        stringified: false,
        resource: ResourceRef::concrete(String::new(), kind.to_string()),
        is_self_range_collection: false,
        template_supplied_member_keys: std::collections::BTreeSet::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        source_null_tolerant: false,
        omitted_members: std::collections::BTreeMap::default(),
        outer_guards: Vec::new(),
    }
}

#[test]
fn inference_skipped_when_api_version_candidates_nonempty() -> eyre::Result<()> {
    let mut use_ = use_with_kind("ServiceMonitor");
    use_.resource
        .api_version_candidates
        .push("monitoring.coreos.com/v1".to_string());

    let crd = CrdsCatalogSchemaProvider::new()
        .with_allow_download(false)
        .with_cache_dir(tmp_dir("inf-skip")?)
        .with_api_version_guess(true);
    let sink = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(crd)])
        .with_inference_enabled(true)
        .with_diagnostic_sink(sink.clone());
    let _ = chain.schema_fragment_for_use(&use_);
    assert!(!sink.snapshot().iter().any(|diagnostic| matches!(
        diagnostic,
        Diagnostic::InferredApiVersion { .. } | Diagnostic::AmbiguousApiVersion { .. }
    )));
    Ok(())
}

#[test]
fn inference_returns_no_match_without_opt_in() -> eyre::Result<()> {
    // An explicit scratch root keeps this check independent of the user cache.
    let fetcher = Arc::new(MockFetcher::new());
    let transport = Arc::clone(&fetcher);
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(tmp_dir("inf-no-opt-in")?)
        .with_allow_download(true)
        .with_fetcher(transport)
        .with_api_version_guess(false);
    let candidates = provider.infer_api_version_candidates("ServiceMonitor");
    assert!(
        candidates.is_empty(),
        "without opt-in, infer_api_version_candidates must return empty"
    );
    sim_assert_eq!(have: fetcher.total_calls(), want: 0);
    Ok(())
}

#[test]
fn api_version_guess_shortlist_resolves_known_kind() -> eyre::Result<()> {
    let crd = tmp_dir("inf-shortlist")?;
    let k8s = tmp_dir("inf-shortlist-k8s")?;
    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let crd_provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(crd)
        .with_fetcher(mock.clone())
        .with_api_version_guess(true);
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(k8s)
        .with_fetcher(mock);
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(crd_provider), Box::new(k8s_provider)])
        .with_inference_enabled(true)
        .with_diagnostic_sink(diagnostics.clone());

    let _ = chain.schema_fragment_for_use(&use_with_kind("ServiceMonitor"));

    let inferred = diagnostics
        .snapshot()
        .into_iter()
        .find_map(|diagnostic| match diagnostic {
            Diagnostic::InferredApiVersion {
                inferred_api_version,
                source,
                origin,
                ..
            } => Some((inferred_api_version, source, origin)),
            _ => None,
        });
    sim_assert_eq!(
        have: inferred,
        want: Some((
            "monitoring.coreos.com/v1".to_string(),
            InferenceSource::Shortlist,
            ProviderOrigin::DefaultCatalog,
        ))
    );
    Ok(())
}

#[test]
fn inference_outcome_local_override_authoritative() {
    let candidates = vec![
        ApiVersionCandidate {
            api_version: "g/v1alpha1".to_string(),
            source: InferenceSource::Shortlist,
            origin: ProviderOrigin::LocalOverride,
        },
        ApiVersionCandidate {
            api_version: "g/v1".to_string(),
            source: InferenceSource::Shortlist,
            origin: ProviderOrigin::DefaultCatalog,
        },
    ];
    match aggregate(candidates) {
        ApiVersionInferenceOutcome::Resolved {
            api_version,
            origin,
            ..
        } => {
            sim_assert_eq!(have: api_version, want: "g/v1alpha1");
            sim_assert_eq!(have: origin, want: ProviderOrigin::LocalOverride);
        }
        other => panic!("expected Resolved from override; got {other:?}"),
    }
}

#[test]
fn inference_outcome_internally_inconsistent_local_override_is_ambiguous() {
    let candidates = vec![
        ApiVersionCandidate {
            api_version: "g/v1alpha1".to_string(),
            source: InferenceSource::Shortlist,
            origin: ProviderOrigin::LocalOverride,
        },
        ApiVersionCandidate {
            api_version: "g/v1beta1".to_string(),
            source: InferenceSource::Shortlist,
            origin: ProviderOrigin::LocalOverride,
        },
    ];
    assert!(matches!(
        aggregate(candidates),
        ApiVersionInferenceOutcome::Ambiguous { .. }
    ));
}

#[test]
fn inference_outcome_ambiguous_when_two_non_override_disagree() {
    let candidates = vec![
        ApiVersionCandidate {
            api_version: "g/v1".to_string(),
            source: InferenceSource::Shortlist,
            origin: ProviderOrigin::DefaultCatalog,
        },
        ApiVersionCandidate {
            api_version: "g/v2".to_string(),
            source: InferenceSource::Shortlist,
            origin: ProviderOrigin::KubernetesOpenApi,
        },
    ];
    match aggregate(candidates) {
        ApiVersionInferenceOutcome::Ambiguous { candidates } => {
            sim_assert_eq!(have: candidates.len(), want: 2);
        }
        other => panic!("expected Ambiguous; got {other:?}"),
    }
}

#[test]
fn inference_outcome_single_global_candidate_resolves() {
    let candidates = vec![ApiVersionCandidate {
        api_version: "g/v1".to_string(),
        source: InferenceSource::Shortlist,
        origin: ProviderOrigin::DefaultCatalog,
    }];
    match aggregate(candidates) {
        ApiVersionInferenceOutcome::Resolved { api_version, .. } => {
            sim_assert_eq!(have: api_version, want: "g/v1");
        }
        other => panic!("expected Resolved; got {other:?}"),
    }
}

#[test]
fn inference_local_override_wins_over_online_probe() {
    let candidates = vec![
        ApiVersionCandidate {
            api_version: "a/v1".to_string(),
            source: InferenceSource::Shortlist,
            origin: ProviderOrigin::LocalOverride,
        },
        ApiVersionCandidate {
            api_version: "b/v1".to_string(),
            source: InferenceSource::OnlineProbe,
            origin: ProviderOrigin::DefaultCatalog,
        },
    ];
    match aggregate(candidates) {
        ApiVersionInferenceOutcome::Resolved {
            api_version,
            origin,
            ..
        } => {
            sim_assert_eq!(have: api_version, want: "a/v1");
            sim_assert_eq!(have: origin, want: ProviderOrigin::LocalOverride);
        }
        other => panic!("expected Resolved from override; got {other:?}"),
    }
}

#[test]
fn api_version_guess_aggregates_across_providers() {
    // The shortlist and online probe agree; the shortlist supplies the diagnostic source.
    let candidates = vec![
        ApiVersionCandidate {
            api_version: "x/v1".to_string(),
            source: InferenceSource::OnlineProbe,
            origin: ProviderOrigin::DefaultCatalog,
        },
        ApiVersionCandidate {
            api_version: "x/v1".to_string(),
            source: InferenceSource::Shortlist,
            origin: ProviderOrigin::KubernetesOpenApi,
        },
    ];
    match aggregate(candidates) {
        ApiVersionInferenceOutcome::Resolved {
            api_version,
            source,
            ..
        } => {
            sim_assert_eq!(have: api_version, want: "x/v1");
            sim_assert_eq!(
                have: source,
                want: InferenceSource::Shortlist,
                "Shortlist must win over OnlineProbe"
            );
        }
        other => panic!("expected Resolved with Shortlist source; got {other:?}"),
    }
}

#[test]
fn api_version_guess_strict_disables_inference() -> eyre::Result<()> {
    let fetcher = Arc::new(MockFetcher::new());
    let transport = Arc::clone(&fetcher);
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(tmp_dir("inf-strict")?)
        .with_allow_download(true)
        .with_fetcher(transport)
        .with_api_version_guess(true);
    let sink = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(provider)])
        .with_inference_enabled(false)
        .with_diagnostic_sink(sink.clone());
    assert!(
        chain
            .schema_fragment_for_use(&use_with_kind("ServiceMonitor"))
            .is_none()
    );
    assert!(!sink.snapshot().iter().any(|diagnostic| matches!(
        diagnostic,
        Diagnostic::InferredApiVersion { .. } | Diagnostic::AmbiguousApiVersion { .. }
    )));
    sim_assert_eq!(have: fetcher.total_calls(), want: 0);
    Ok(())
}

#[test]
fn infer_api_version_candidates_default_impl_empty() {
    // A custom K8sSchemaProvider that doesn't override
    // infer_api_version_candidates returns empty via the trait default.
    #[derive(Debug)]
    struct Empty;
    impl helm_schema_k8s::K8sSchemaProvider for Empty {
        fn origin(&self) -> helm_schema_k8s::ProviderOrigin {
            helm_schema_k8s::ProviderOrigin::DefaultCatalog
        }
        fn lookup(
            &self,
            _r: &helm_schema_core::ResourceRef,
            _p: &helm_schema_core::YamlPath,
        ) -> ProviderLookupResult {
            ProviderLookupResult::NotOwned
        }
    }
    let provider = Empty;
    sim_assert_eq!(
        have: provider.infer_api_version_candidates("Pod"),
        want: [] as [ApiVersionCandidate; 0]
    );
}

// A version-dependent kind needs structural chart evidence or an explicit local schema.
#[test]
fn shortlist_does_not_resolve_pod_disruption_budget() {
    use helm_schema_k8s::inference::shortlist::canonical_api_version_for_kind;
    sim_assert_eq!(
        have: canonical_api_version_for_kind("PodDisruptionBudget"),
        want: None,
        "version-dependent kinds must abstain without structural or explicit local evidence"
    );
}

// InferredApiVersion is informational and must NOT fire for built-in
// K8s kinds whose
// canonical apiVersion is obvious (ConfigMap→v1, Deployment→apps/v1,
// ClusterRole→rbac.authorization.k8s.io/v1, …). Inference resolution
// still runs (so the schema is still loaded) — only the diagnostic
// noise is suppressed. For CRD inferences (monitoring.coreos.com/v1
// for ServiceMonitor, etc.) the diagnostic remains useful and DOES
// fire — covered by the existing `inference_emits_diagnostic_through_chain`
// test plus the loose-mode acceptance fixture.
#[test]
fn inference_for_builtin_kind_does_not_emit_diagnostic() -> eyre::Result<()> {
    // A throwaway cache root, so what this asserts cannot change with
    // whatever a previous run left in the shared per-user cache.
    let crd_provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(tmp_dir("inf-builtin-kind-crd")?)
        .with_allow_download(false)
        .with_api_version_guess(true);
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(tmp_dir("inf-builtin-kind")?)
        .with_allow_download(false);
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(crd_provider), Box::new(k8s_provider)])
        .with_inference_enabled(true)
        .with_diagnostic_sink(diagnostics.clone());

    // ConfigMap with NO apiVersion → inference runs → shortlist
    // resolves to `v1` (core group, built-in).
    let use_ = ProviderSchemaUse {
        value_path: helm_schema_core::ValuesPath::parse("x"),
        path: YamlPath(vec!["data".to_string()]),
        kind: ValueKind::Scalar,
        stringified: false,
        resource: ResourceRef::concrete(String::new(), "ConfigMap".to_string()),
        is_self_range_collection: false,
        template_supplied_member_keys: std::collections::BTreeSet::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        source_null_tolerant: false,
        omitted_members: std::collections::BTreeMap::default(),
        outer_guards: Vec::new(),
    };
    let _ = chain.schema_fragment_for_use(&use_);

    let snapshot = diagnostics.snapshot();
    let any_inferred = snapshot
        .iter()
        .any(|d| matches!(d, Diagnostic::InferredApiVersion { .. }));
    assert!(
        !any_inferred,
        "InferredApiVersion MUST NOT fire for built-in kinds; sink: {snapshot:?}"
    );
    Ok(())
}

// Counterexample: a CRD (non-built-in group) inference DOES emit
// InferredApiVersion. Pins that the suppression is scoped to
// built-ins only and doesn't accidentally hide useful CRD diagnostics.
#[test]
fn inference_for_crd_kind_still_emits_diagnostic() -> eyre::Result<()> {
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(tmp_dir("inf-crd-emits")?)
        .with_allow_download(false)
        .with_api_version_guess(true);
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(provider)])
        .with_inference_enabled(true)
        .with_diagnostic_sink(diagnostics.clone());

    // ServiceMonitor with NO apiVersion → shortlist resolves to
    // monitoring.coreos.com/v1 (CRD group, NOT built-in).
    let use_ = ProviderSchemaUse {
        value_path: helm_schema_core::ValuesPath::parse("x"),
        path: YamlPath(vec!["spec".to_string()]),
        kind: ValueKind::Scalar,
        stringified: false,
        resource: ResourceRef::concrete(String::new(), "ServiceMonitor".to_string()),
        is_self_range_collection: false,
        template_supplied_member_keys: std::collections::BTreeSet::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        source_null_tolerant: false,
        omitted_members: std::collections::BTreeMap::default(),
        outer_guards: Vec::new(),
    };
    let _ = chain.schema_fragment_for_use(&use_);

    let snapshot = diagnostics.snapshot();
    let inferred = snapshot.iter().find_map(|d| match d {
        Diagnostic::InferredApiVersion {
            kind,
            inferred_api_version,
            ..
        } => Some((kind.clone(), inferred_api_version.clone())),
        _ => None,
    });
    sim_assert_eq!(
        have: inferred,
        want: Some((
            "ServiceMonitor".to_string(),
            "monitoring.coreos.com/v1".to_string()
        )),
        "CRD inference MUST still emit InferredApiVersion; sink: {snapshot:?}"
    );
    Ok(())
}

#[test]
fn api_version_guess_online_probe_kind_scoped() -> eyre::Result<()> {
    // Shortlist-known kind: online probe answers; an unknown kind: no
    // probe attempted at all.
    let cache = tmp_dir("inf-online-probe")?;
    let known_kind_url = "https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/monitoring.coreos.com/servicemonitor_v1.json";
    let mock = Arc::new(MockFetcher::new().with_body(known_kind_url, crd_doc().into_bytes()));
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache)
        .with_allow_download(true)
        .with_api_version_guess(true)
        .with_fetcher(mock.clone());

    let known = provider.infer_api_version_candidates("ServiceMonitor");
    let online = known
        .iter()
        .find(|c| c.source == InferenceSource::OnlineProbe);
    assert!(online.is_some(), "online probe must fire for known kinds");

    let baseline = mock.total_calls();
    let unknown = provider.infer_api_version_candidates("TotallyUnknownKindXYZ");
    let after = mock.total_calls();
    sim_assert_eq!(
        have: after,
        want: baseline,
        "online probe must not fire for kinds outside the shortlist"
    );
    assert!(
        !unknown
            .iter()
            .any(|c| c.source == InferenceSource::OnlineProbe),
        "no online candidate for unknown kind"
    );
    Ok(())
}

#[test]
fn chain_caches_api_version_inference_by_kind() -> eyre::Result<()> {
    let cache = tmp_dir("inf-chain-cache")?;
    let service_monitor_url = "https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/monitoring.coreos.com/servicemonitor_v1.json";
    let mock = Arc::new(MockFetcher::new().with_body(service_monitor_url, crd_doc().into_bytes()));
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache)
        .with_allow_download(true)
        .with_api_version_guess(true)
        .with_fetcher(mock.clone());
    let chain = Chain::new(vec![Box::new(provider)]).with_inference_enabled(true);
    let use_ = use_with_kind("ServiceMonitor");

    let first = chain.schema_fragment_for_use(&use_);
    assert!(first.is_some(), "setup must resolve ServiceMonitor");
    let calls_after_first_lookup = mock.calls_for(service_monitor_url);
    assert!(
        calls_after_first_lookup > 0,
        "setup should exercise the online CRD inference/lookup path"
    );

    let second = chain.schema_fragment_for_use(&use_);
    assert!(second.is_some(), "second lookup must still resolve");
    sim_assert_eq!(
        have: mock.calls_for(service_monitor_url),
        want: calls_after_first_lookup,
        "repeated uses of the same kind must reuse the chain inference cache"
    );
    Ok(())
}

fn crd_doc() -> String {
    r#"{"type":"object","properties":{"spec":{"type":"object"}}}"#.to_string()
}

#[test]
fn cache_contents_do_not_change_shortlisted_crd_inference() -> eyre::Result<()> {
    #[derive(Debug, PartialEq)]
    struct Observation {
        schema: Option<serde_json::Value>,
        diagnostics: Vec<Diagnostic>,
    }

    let cache = ScratchDir::new("inf-shortlist-cache-independent")?;
    let url = "https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/monitoring.coreos.com/servicemonitor_v1.json";
    let fetcher = Arc::new(MockFetcher::new().with_body(url, crd_doc().into_bytes()));
    let infer = || {
        let sink = DiagnosticSink::new();
        let transport = Arc::clone(&fetcher);
        let provider = CrdsCatalogSchemaProvider::new()
            .with_cache_dir(cache.path().to_path_buf())
            .with_allow_download(true)
            .with_fetcher(transport)
            .with_api_version_guess(true);
        let chain = Chain::new(vec![Box::new(provider)])
            .with_inference_enabled(true)
            .with_diagnostic_sink(sink.clone());
        let fragment = chain.schema_fragment_for_use(&use_with_kind("ServiceMonitor"));
        Observation {
            schema: fragment.map(|fragment| fragment.schema().clone()),
            diagnostics: sink.snapshot(),
        }
    };
    let cold = infer();
    sim_assert_eq!(have: cold.schema, want: Some(serde_json::json!({"type": "object"})));
    assert!(cold.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic,
        Diagnostic::InferredApiVersion {
            source: InferenceSource::Shortlist,
            ..
        }
    )));

    // A previous lookup of another version must not add an inference alternative.
    std::fs::write(
        cache
            .path()
            .join("default/monitoring.coreos.com/servicemonitor_v2.json"),
        crd_doc(),
    )?;
    sim_assert_eq!(have: infer(), want: cold);
    Ok(())
}

#[test]
fn explicit_override_inference_survives_remote_cache_conflicts() -> eyre::Result<()> {
    let root = ScratchDir::new("inf-local-override")?;
    let overrides = root.path().join("overrides");
    std::fs::create_dir_all(overrides.join("local.example"))?;
    std::fs::write(overrides.join("local.example/widget_v1.json"), crd_doc())?;
    let cache = root.path().join("cache");
    std::fs::create_dir_all(cache.join("default/remote.example"))?;
    std::fs::write(
        cache.join("default/remote.example/widget_v9.json"),
        crd_doc(),
    )?;
    let local =
        helm_schema_k8s::LocalSchemaProvider::new(overrides.clone()).with_api_version_guess(true);
    let remote = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache)
        .with_allow_download(false)
        .with_api_version_guess(true);
    let sink = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(local), Box::new(remote)])
        .with_inference_enabled(true)
        .with_diagnostic_sink(sink.clone());
    let fragment = chain.schema_fragment_for_use(&use_with_kind("Widget"));
    sim_assert_eq!(
        have: fragment.map(|fragment| fragment.schema().clone()),
        want: Some(serde_json::json!({"type": "object"}))
    );
    assert!(sink.snapshot().iter().any(|diagnostic| matches!(
        diagnostic,
        Diagnostic::InferredApiVersion { inferred_api_version, origin: ProviderOrigin::LocalOverride, .. }
            if inferred_api_version == "local.example/v1"
    )));

    // Two authored versions remain ambiguous even though either schema can load.
    std::fs::write(overrides.join("local.example/widget_v2.json"), crd_doc())?;
    let local = helm_schema_k8s::LocalSchemaProvider::new(overrides).with_api_version_guess(true);
    assert!(matches!(
        aggregate(local.infer_api_version_candidates("Widget")),
        ApiVersionInferenceOutcome::Ambiguous { .. }
    ));
    Ok(())
}

#[test]
fn unshortlisted_kinds_ignore_remote_cache_inventory() -> eyre::Result<()> {
    let root = ScratchDir::new("inf-remote-inventory")?;
    let mirror = "https://example.com/schemas";
    let mirror_id = source_id_for_url(mirror);
    for namespace in ["default", &mirror_id, "removed-mirror"] {
        for version in ["v1.35.0", "v1.24.0"] {
            let directory = root.path().join(format!("k8s/{namespace}/{version}"));
            std::fs::create_dir_all(&directory)?;
            std::fs::write(
                directory.join("poddisruptionbudget-policy-v1.json"),
                r#"{"x-kubernetes-group-version-kind":[{"group":"policy","version":"v1","kind":"PodDisruptionBudget"}]}"#,
            )?;
        }
        let directory = root.path().join(format!("crds/{namespace}/example.io"));
        std::fs::create_dir_all(&directory)?;
        for version in ["v1", "v2"] {
            std::fs::write(directory.join(format!("widget_{version}.json")), crd_doc())?;
        }
    }
    for cache in ["k8s", "crds"] {
        std::fs::write(
            root.path().join(cache).join("CACHE_LAYOUT_VERSION"),
            format!("{}\n", helm_schema_k8s::CACHE_LAYOUT_VERSION),
        )?;
    }
    let fetcher = Arc::new(MockFetcher::new());
    // Cached namespaces and releases are storage history, including configured mirrors.
    for allow_download in [false, true] {
        for use_cache in [false, true] {
            for explicit in [
                vec!["v1.35.0".into()],
                vec!["v1.35.0".into(), "v1.24.0".into()],
            ] {
                let transport = Arc::clone(&fetcher);
                let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
                    explicit,
                    Some(11),
                ))
                .with_cache_dir(root.path().join("k8s"))
                .with_mirrors(vec![mirror.to_string()])
                .with_allow_download(allow_download)
                .with_use_cache(use_cache)
                .with_fetcher(transport);
                sim_assert_eq!(
                    have: provider.infer_api_version_candidates("PodDisruptionBudget"),
                    want: Vec::<ApiVersionCandidate>::new()
                );
            }
        }
        let transport = Arc::clone(&fetcher);
        let provider = CrdsCatalogSchemaProvider::new()
            .with_cache_dir(root.path().join("crds"))
            .with_mirrors(vec![mirror.to_string()])
            .with_allow_download(allow_download)
            .with_fetcher(transport)
            .with_api_version_guess(true);
        sim_assert_eq!(
            have: provider.infer_api_version_candidates("Widget"),
            want: Vec::<ApiVersionCandidate>::new()
        );
    }
    sim_assert_eq!(have: fetcher.total_calls(), want: 0);
    Ok(())
}

#[test]
fn inference_emits_diagnostic_through_chain() -> eyre::Result<()> {
    let crd = tmp_dir("inf-diag")?;
    let k8s = tmp_dir("inf-diag-k8s")?;
    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let crd_provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(crd)
        .with_fetcher(mock.clone())
        .with_api_version_guess(true);
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(k8s)
        .with_fetcher(mock);
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(crd_provider), Box::new(k8s_provider)])
        .with_inference_enabled(true)
        .with_diagnostic_sink(diagnostics.clone());

    let _ = chain.schema_fragment_for_use(&use_with_kind("ServiceMonitor"));

    let snapshot = diagnostics.snapshot();
    let inferred = snapshot
        .iter()
        .find(|d| matches!(d, Diagnostic::InferredApiVersion { kind, .. } if kind == "ServiceMonitor"))
        .cloned();
    assert!(
        inferred.is_some(),
        "expected InferredApiVersion diagnostic for ServiceMonitor; got {snapshot:?}"
    );
    Ok(())
}
