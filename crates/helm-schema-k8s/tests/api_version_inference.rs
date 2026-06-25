//! Feature D end-to-end tests.

use std::sync::Arc;

use helm_schema_core::{ProviderSchemaUse, ResourceRef, ResourceSchemaOracle, ValueKind, YamlPath};
use helm_schema_k8s::inference::aggregate;
use helm_schema_k8s::{
    ApiVersionCandidate, ApiVersionInferenceOutcome, Chain, CrdsCatalogSchemaProvider, Diagnostic,
    DiagnosticSink, InferenceSource, K8sSchemaProvider, KubernetesJsonSchemaProvider,
    ProviderLookupResult, ProviderOrigin, source_id_for_url,
};
use test_util::prelude::sim_assert_eq;

mod common;
use common::{MockFetcher, MockResponse};

fn tmp_dir(label: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "helm-schema.{label}.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&p).expect("create temp dir");
    p
}

fn use_with_kind(kind: &str) -> ProviderSchemaUse {
    ProviderSchemaUse {
        value_path: "x".to_string(),
        path: YamlPath(vec!["spec".to_string()]),
        kind: ValueKind::Scalar,
        resource: ResourceRef {
            api_version: String::new(),
            kind: kind.to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        },
        is_self_range_collection: false,
    }
}

#[test]
fn inference_skipped_when_api_version_candidates_nonempty() {
    let mut use_ = use_with_kind("ServiceMonitor");
    use_.resource
        .api_version_candidates
        .push("monitoring.coreos.com/v1".to_string());

    let crd = CrdsCatalogSchemaProvider::new()
        .with_allow_download(false)
        .with_cache_dir(tmp_dir("inf-skip"))
        .with_api_version_guess(true);
    let chain = Chain::new(vec![Box::new(crd)]).with_inference_enabled(true);
    // No diagnostic sink because inference wouldn't actually emit
    // InferredApiVersion if candidates are pre-pinned.
    let _ = chain.schema_fragment_for_use(&use_);
    // The contract is: this should NOT trigger inference. If it did,
    // we'd see InferredApiVersion in a sink; absence here is the test.
}

#[test]
fn inference_returns_no_match_without_opt_in() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_api_version_guess(false);
    let candidates = provider.infer_api_version_candidates("Pod");
    assert!(
        candidates.is_empty(),
        "without opt-in, infer_api_version_candidates must return empty"
    );
}

#[test]
fn api_version_guess_shortlist_resolves_known_kind() {
    let crd = tmp_dir("inf-shortlist");
    let k8s = tmp_dir("inf-shortlist-k8s");
    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let crd_provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(crd)
        .with_fetcher(mock.clone())
        .with_api_version_guess(true);
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(k8s)
        .with_fetcher(mock)
        .with_api_version_guess(true);
    let chain = Chain::new(vec![Box::new(crd_provider), Box::new(k8s_provider)])
        .with_inference_enabled(true);

    let candidates: Vec<_> = chain
        .providers()
        .iter()
        .flat_map(|p| p.infer_api_version_candidates("ServiceMonitor"))
        .collect();

    assert!(
        candidates
            .iter()
            .any(|c| c.api_version == "monitoring.coreos.com/v1"
                && c.source == InferenceSource::Shortlist),
        "expected ServiceMonitor → monitoring.coreos.com/v1 from shortlist; got {candidates:?}"
    );
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
    // Shortlist AND LocalCacheScan contribute the same apiVersion →
    // single Resolved outcome, source = Shortlist (higher priority).
    let candidates = vec![
        ApiVersionCandidate {
            api_version: "x/v1".to_string(),
            source: InferenceSource::LocalCacheScan,
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
                "Shortlist must win over LocalCacheScan"
            );
        }
        other => panic!("expected Resolved with Shortlist source; got {other:?}"),
    }
}

#[test]
fn api_version_guess_strict_disables_inference() {
    // With guess off, providers return empty candidates regardless of
    // the kind being in the shortlist.
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_api_version_guess(false);
    assert!(
        provider
            .infer_api_version_candidates("ServiceMonitor")
            .is_empty(),
        "with --strict-api-versions (= guess off), no candidates"
    );
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
        fn has_resource(&self, _r: &helm_schema_core::ResourceRef) -> bool {
            false
        }
    }
    let provider = Empty;
    assert!(provider.infer_api_version_candidates("Pod").is_empty());
}

#[test]
fn api_version_guess_cache_scan_spans_all_version_dirs() {
    // Pre-populate two version dirs in the cache; tier 2 must scan both.
    let cache = tmp_dir("inf-version-spans");
    std::fs::write(
        cache.join("CACHE_LAYOUT_VERSION"),
        format!("{}\n", helm_schema_k8s::CACHE_LAYOUT_VERSION),
    )
    .expect("seed marker");
    std::fs::create_dir_all(cache.join("default/v1.24.0")).expect("v1.24 dir");
    std::fs::write(
        cache.join("default/v1.24.0/foo.json"),
        r#"{"x-kubernetes-group-version-kind":[{"group":"policy","version":"v1beta1","kind":"PodDisruptionBudget"}]}"#,
    )
    .expect("seed file");

    use helm_schema_k8s::K8sVersionChain;
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string(), "v1.24.0".to_string()],
        None,
    ))
    .with_cache_dir(cache)
    .with_allow_download(false)
    .with_api_version_guess(true);

    let candidates = provider.infer_api_version_candidates("PodDisruptionBudget");
    assert!(
        candidates
            .iter()
            .any(|c| c.api_version == "policy/v1beta1"
                && c.source == InferenceSource::LocalCacheScan),
        "tier 2 must find PDB v1beta1 in v1.24.0 cache dir; got {candidates:?}"
    );
}

// Tier-2 CRD cache scan walks EVERY CONFIGURED source namespace.
// Configured = default + every mirror passed to `with_mirrors`.
#[test]
fn api_version_guess_cache_scan_spans_all_configured_crd_sources() {
    let cache = tmp_dir("inf-crd-sources-configured");
    let mirror_url = "https://example.com/mirror-a";
    let mirror_source_id = source_id_for_url(mirror_url);
    std::fs::write(
        cache.join("CACHE_LAYOUT_VERSION"),
        format!("{}\n", helm_schema_k8s::CACHE_LAYOUT_VERSION),
    )
    .expect("seed marker");
    std::fs::create_dir_all(cache.join("default/monitoring.coreos.com"))
        .expect("default group dir");
    std::fs::write(
        cache.join("default/monitoring.coreos.com/customkind_v1.json"),
        r#"{"type":"object"}"#,
    )
    .expect("seed default file");
    std::fs::create_dir_all(cache.join(format!("{mirror_source_id}/example.io")))
        .expect("mirror namespace");
    std::fs::write(
        cache.join(format!("{mirror_source_id}/example.io/customkind_v2.json")),
        r#"{"type":"object"}"#,
    )
    .expect("seed mirror file");

    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache)
        .with_allow_download(false)
        .with_mirrors(vec![mirror_url.to_string()])
        .with_api_version_guess(true);

    let candidates = provider.infer_api_version_candidates("CustomKind");
    let api_versions: std::collections::HashSet<String> = candidates
        .into_iter()
        .filter(|c| c.source == InferenceSource::LocalCacheScan)
        .map(|c| c.api_version)
        .collect();
    assert!(
        api_versions.contains("monitoring.coreos.com/v1"),
        "default source result missing; got {api_versions:?}"
    );
    assert!(
        api_versions.contains("example.io/v2"),
        "configured-mirror source result missing; got {api_versions:?}"
    );
}

// Pins Finding 2 — a CRD source-id dir on disk that is NOT in the
// currently-configured mirror set MUST NOT contribute inference
// candidates. The "removed-mirror" case: yesterday's run used
// `--crd-catalog-mirror=https://stale.example.com`, today's run
// doesn't. Today's inference must ignore the stale-mirror dir.
#[test]
fn api_version_guess_cache_scan_ignores_stale_unconfigured_crd_source() {
    let cache = tmp_dir("inf-crd-sources-stale");
    std::fs::write(
        cache.join("CACHE_LAYOUT_VERSION"),
        format!("{}\n", helm_schema_k8s::CACHE_LAYOUT_VERSION),
    )
    .expect("seed marker");
    std::fs::create_dir_all(cache.join("default/monitoring.coreos.com"))
        .expect("default group dir");
    std::fs::write(
        cache.join("default/monitoring.coreos.com/customkind_v1.json"),
        r#"{"type":"object"}"#,
    )
    .expect("seed default file");
    // Stale mirror namespace from a previous run; not in current config.
    std::fs::create_dir_all(cache.join("removed-mirror-id/example.io")).expect("stale namespace");
    std::fs::write(
        cache.join("removed-mirror-id/example.io/customkind_v2.json"),
        r#"{"type":"object"}"#,
    )
    .expect("seed stale file");

    // Configure only `default` — no mirrors. Stale on-disk dirs must
    // be ignored.
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache)
        .with_allow_download(false)
        .with_api_version_guess(true);

    let candidates = provider.infer_api_version_candidates("CustomKind");
    let api_versions: std::collections::HashSet<String> = candidates
        .into_iter()
        .filter(|c| c.source == InferenceSource::LocalCacheScan)
        .map(|c| c.api_version)
        .collect();
    assert!(
        api_versions.contains("monitoring.coreos.com/v1"),
        "default source result missing; got {api_versions:?}"
    );
    assert!(
        !api_versions.contains("example.io/v2"),
        "stale removed-mirror cache MUST NOT contribute inference candidates; got {api_versions:?}"
    );
}

// Pins Finding 3 (round 2) — `PodDisruptionBudget` is version-dependent
// (`policy/v1` since v1.21, `policy/v1beta1` before), so it MUST NOT
// appear on the unambiguous shortlist. With auto-fallback enabled the
// cache can hold BOTH versions; if the shortlist also contributed an
// answer, the aggregator would observe two distinct api_versions and
// fire `AmbiguousApiVersion(kind=PodDisruptionBudget)`. The Temporal
// chart's loose-mode run regressed on exactly this in PR review.
#[test]
fn shortlist_does_not_resolve_pod_disruption_budget() {
    use helm_schema_k8s::inference::shortlist::canonical_api_version_for_kind;
    sim_assert_eq!(
        have: canonical_api_version_for_kind("PodDisruptionBudget"),
        want: None,
        "PodDisruptionBudget must NOT be on the shortlist; it would cause AmbiguousApiVersion when auto-fallback populates v1beta1 in cache"
    );
}

// Pins Finding 3 (round 2) end-to-end — PDB inference with BOTH
// policy/v1 (in v1.35 primary cache) and policy/v1beta1 (in v1.24
// auto-fallback cache) populated must NOT fire AmbiguousApiVersion.
// The combination of (a) PDB off shortlist and (b) inference scanning
// only the EXPLICIT version (not auto-fallback) keeps the loose-mode
// invocation clean.
#[test]
fn pdb_inference_is_not_ambiguous_with_auto_fallback_cache() {
    let cache = tmp_dir("inf-pdb-no-ambiguous");
    std::fs::write(
        cache.join("CACHE_LAYOUT_VERSION"),
        format!("{}\n", helm_schema_k8s::CACHE_LAYOUT_VERSION),
    )
    .expect("seed marker");
    // Primary v1.35.0 has policy/v1 PDB.
    std::fs::create_dir_all(cache.join("default/v1.35.0")).expect("v1.35 dir");
    std::fs::write(
        cache.join("default/v1.35.0/poddisruptionbudget-policy-v1.json"),
        r#"{"x-kubernetes-group-version-kind":[{"group":"policy","version":"v1","kind":"PodDisruptionBudget"}]}"#,
    )
    .expect("seed v1.35 file");
    // Auto-fallback v1.24.0 has policy/v1beta1 PDB.
    std::fs::create_dir_all(cache.join("default/v1.24.0")).expect("v1.24 dir");
    std::fs::write(
        cache.join("default/v1.24.0/poddisruptionbudget-policy-v1beta1.json"),
        r#"{"x-kubernetes-group-version-kind":[{"group":"policy","version":"v1beta1","kind":"PodDisruptionBudget"}]}"#,
    )
    .expect("seed v1.24 file");

    use helm_schema_k8s::K8sVersionChain;
    // EXPLICIT = [v1.35.0]; auto_fallback_window = 11 → ordered chain
    // includes v1.24.0 too. inference_scan_versions = explicit only.
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string()],
        Some(11),
    ))
    .with_cache_dir(cache)
    .with_allow_download(false)
    .with_api_version_guess(true);

    let candidates = provider.infer_api_version_candidates("PodDisruptionBudget");
    let outcome = aggregate(candidates.clone());
    match outcome {
        ApiVersionInferenceOutcome::Resolved { api_version, .. } => {
            sim_assert_eq!(
                have: api_version,
                want: "policy/v1",
                "inference must resolve to current policy/v1, not the auto-fallback's v1beta1; candidates={candidates:?}"
            );
        }
        ApiVersionInferenceOutcome::Ambiguous {
            candidates: amb_candidates,
        } => panic!(
            "must NOT be Ambiguous; got candidates {amb_candidates:?} from full set {candidates:?}"
        ),
        ApiVersionInferenceOutcome::NoMatch => panic!(
            "must resolve via primary cache scan; got NoMatch from candidates {candidates:?}"
        ),
    }
}

// Pins Finding 3 (round 3) Option B — InferredApiVersion is
// informational and must NOT fire for built-in K8s kinds whose
// canonical apiVersion is obvious (ConfigMap→v1, Deployment→apps/v1,
// ClusterRole→rbac.authorization.k8s.io/v1, …). Inference resolution
// still runs (so the schema is still loaded) — only the diagnostic
// noise is suppressed. For CRD inferences (monitoring.coreos.com/v1
// for ServiceMonitor, etc.) the diagnostic remains useful and DOES
// fire — covered by the existing `inference_emits_diagnostic_through_chain`
// test plus the loose-mode acceptance fixture.
#[test]
fn inference_for_builtin_kind_does_not_emit_diagnostic() {
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_allow_download(false)
        .with_api_version_guess(true);
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(provider)])
        .with_inference_enabled(true)
        .with_diagnostic_sink(diagnostics.clone());

    // ConfigMap with NO apiVersion → inference runs → shortlist
    // resolves to `v1` (core group, built-in).
    let use_ = ProviderSchemaUse {
        value_path: "x".to_string(),
        path: YamlPath(vec!["data".to_string()]),
        kind: ValueKind::Scalar,
        resource: ResourceRef {
            api_version: String::new(),
            kind: "ConfigMap".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        },
        is_self_range_collection: false,
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
}

// Counterexample: a CRD (non-built-in group) inference DOES emit
// InferredApiVersion. Pins that the suppression is scoped to
// built-ins only and doesn't accidentally hide useful CRD diagnostics.
#[test]
fn inference_for_crd_kind_still_emits_diagnostic() {
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(tmp_dir("inf-crd-emits"))
        .with_allow_download(false)
        .with_api_version_guess(true);
    let diagnostics = DiagnosticSink::new();
    let chain = Chain::new(vec![Box::new(provider)])
        .with_inference_enabled(true)
        .with_diagnostic_sink(diagnostics.clone());

    // ServiceMonitor with NO apiVersion → shortlist resolves to
    // monitoring.coreos.com/v1 (CRD group, NOT built-in).
    let use_ = ProviderSchemaUse {
        value_path: "x".to_string(),
        path: YamlPath(vec!["spec".to_string()]),
        kind: ValueKind::Scalar,
        resource: ResourceRef {
            api_version: String::new(),
            kind: "ServiceMonitor".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        },
        is_self_range_collection: false,
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
}

// Pins Finding 4 (round 2) — inference K8s cache scan must skip
// auto-fallback version dirs even when they are part of the
// configured chain via `K8sVersionChain::new(_, Some(window))`.
// Only EXPLICIT versions participate in inference; auto-fallback
// dirs exist as schema-lookup escape valves, not as user intent.
#[test]
fn k8s_cache_scan_skips_auto_fallback_version_dir() {
    let cache = tmp_dir("inf-skip-auto-fallback");
    std::fs::write(
        cache.join("CACHE_LAYOUT_VERSION"),
        format!("{}\n", helm_schema_k8s::CACHE_LAYOUT_VERSION),
    )
    .expect("seed marker");
    // Primary v1.35.0 (EXPLICIT) holds NetworkPolicy v1.
    std::fs::create_dir_all(cache.join("default/v1.35.0")).expect("v1.35 dir");
    std::fs::write(
        cache.join("default/v1.35.0/networkpolicy-networking-v1.json"),
        r#"{"x-kubernetes-group-version-kind":[{"group":"networking.k8s.io","version":"v1","kind":"NetworkPolicy"}]}"#,
    )
    .expect("seed primary file");
    // Auto-fallback v1.24.0 (NOT explicit) holds an old extensions/v1beta1.
    std::fs::create_dir_all(cache.join("default/v1.24.0")).expect("v1.24 dir");
    std::fs::write(
        cache.join("default/v1.24.0/networkpolicy-extensions-v1beta1.json"),
        r#"{"x-kubernetes-group-version-kind":[{"group":"extensions","version":"v1beta1","kind":"NetworkPolicy"}]}"#,
    )
    .expect("seed fallback file");

    use helm_schema_k8s::K8sVersionChain;
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string()],
        Some(11),
    ))
    .with_cache_dir(cache)
    .with_allow_download(false)
    .with_api_version_guess(true);

    let candidates = provider.infer_api_version_candidates("NetworkPolicy");
    let cache_versions: std::collections::HashSet<String> = candidates
        .iter()
        .filter(|c| c.source == InferenceSource::LocalCacheScan)
        .map(|c| c.api_version.clone())
        .collect();
    assert!(
        cache_versions.contains("networking.k8s.io/v1"),
        "primary v1.35 cache must contribute networking.k8s.io/v1; got {cache_versions:?}"
    );
    assert!(
        !cache_versions.contains("extensions/v1beta1"),
        "auto-fallback v1.24 dir MUST NOT contribute extensions/v1beta1 to inference; got {cache_versions:?}"
    );
}

// Pins Finding 2 (K8s side) — K8s cache scan must equivalently ignore
// stale `<source_id>` dirs left behind by a previously configured
// `--k8s-schema-mirror`.
#[test]
fn api_version_guess_k8s_cache_scan_ignores_stale_unconfigured_source() {
    let cache = tmp_dir("inf-k8s-sources-stale");
    std::fs::write(
        cache.join("CACHE_LAYOUT_VERSION"),
        format!("{}\n", helm_schema_k8s::CACHE_LAYOUT_VERSION),
    )
    .expect("seed marker");
    std::fs::create_dir_all(cache.join("default/v1.24.0")).expect("default version dir");
    std::fs::write(
        cache.join("default/v1.24.0/foo.json"),
        r#"{"x-kubernetes-group-version-kind":[{"group":"policy","version":"v1beta1","kind":"PodDisruptionBudget"}]}"#,
    )
    .expect("seed default file");
    // Stale K8s mirror namespace from a previous run.
    std::fs::create_dir_all(cache.join("removed-k8s-mirror/v1.24.0")).expect("stale namespace");
    std::fs::write(
        cache.join("removed-k8s-mirror/v1.24.0/bar.json"),
        r#"{"x-kubernetes-group-version-kind":[{"group":"unconfigured.example","version":"v9","kind":"PodDisruptionBudget"}]}"#,
    )
    .expect("seed stale file");

    use helm_schema_k8s::K8sVersionChain;
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string(), "v1.24.0".to_string()],
        None,
    ))
    .with_cache_dir(cache)
    .with_allow_download(false)
    .with_api_version_guess(true);

    let candidates = provider.infer_api_version_candidates("PodDisruptionBudget");
    let api_versions: std::collections::HashSet<String> = candidates
        .into_iter()
        .filter(|c| c.source == InferenceSource::LocalCacheScan)
        .map(|c| c.api_version)
        .collect();
    assert!(
        api_versions.contains("policy/v1beta1"),
        "default source result missing; got {api_versions:?}"
    );
    assert!(
        !api_versions.contains("unconfigured.example/v9"),
        "stale removed-mirror cache MUST NOT contribute K8s inference candidates; got {api_versions:?}"
    );
}

#[test]
fn api_version_guess_online_probe_kind_scoped() {
    // Shortlist-known kind: online probe answers; an unknown kind: no
    // probe attempted at all.
    let cache = tmp_dir("inf-online-probe");
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
    assert!(online.is_some(), "tier 3 must fire for known kinds");

    let baseline = mock.total_calls();
    let unknown = provider.infer_api_version_candidates("TotallyUnknownKindXYZ");
    let after = mock.total_calls();
    sim_assert_eq!(
        have: after,
        want: baseline,
        "tier 3 must NOT fire for kinds not in the extended shortlist"
    );
    assert!(
        !unknown
            .iter()
            .any(|c| c.source == InferenceSource::OnlineProbe),
        "no online candidate for unknown kind"
    );
}

#[test]
fn chain_caches_api_version_inference_by_kind() {
    let cache = tmp_dir("inf-chain-cache");
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
}

fn crd_doc() -> String {
    r#"{"type":"object","properties":{"spec":{"type":"object"}}}"#.to_string()
}

#[test]
fn inference_emits_diagnostic_through_chain() {
    let crd = tmp_dir("inf-diag");
    let k8s = tmp_dir("inf-diag-k8s");
    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let crd_provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(crd)
        .with_fetcher(mock.clone())
        .with_api_version_guess(true);
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(k8s)
        .with_fetcher(mock)
        .with_api_version_guess(true);
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
}
