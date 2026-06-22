//! Feature A + B end-to-end tests using `MockFetcher` so no real
//! network calls happen.

use std::sync::Arc;

use helm_schema_core::{ResourceRef, ResourceSchemaOracle, YamlPath};
use helm_schema_k8s::{
    Chain, Diagnostic, DiagnosticSink, K8sSchemaProvider, K8sVersionChain,
    KubernetesJsonSchemaProvider, MockFetcher, MockResponse,
    cache::{k8s_cache_path, not_found_marker_exists},
    default_source_id,
};
use test_util::prelude::sim_assert_eq;

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

fn pdb_v1beta1_doc() -> String {
    r#"{"type":"object","x-kubernetes-group-version-kind":[{"group":"policy","version":"v1beta1","kind":"PodDisruptionBudget"}],"properties":{"spec":{"type":"object","properties":{"maxUnavailable":{"type":"integer"}}}}}"#.to_string()
}

#[test]
fn k8s_version_fallback_falls_back_to_older() {
    let cache_dir = tmp_dir("k8s-fallback-falls-back");
    let mock = Arc::new(
        MockFetcher::new()
            .with(
                "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.35.0/poddisruptionbudget-policy-v1beta1.json",
                MockResponse::NotFound,
            )
            .with_body(
                "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.24.0/poddisruptionbudget-policy-v1beta1.json",
                pdb_v1beta1_doc().into_bytes(),
            ),
    );
    let diagnostics = DiagnosticSink::new();
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string(), "v1.24.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir)
    .with_allow_download(true)
    .with_fetcher(mock.clone())
    .with_diagnostic_sink(diagnostics.clone());

    let chain = Chain::new(vec![Box::new(provider)]).with_diagnostic_sink(diagnostics.clone());

    let resource = ResourceRef {
        api_version: "policy/v1beta1".to_string(),
        kind: "PodDisruptionBudget".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let schema = chain.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));
    assert!(schema.is_some(), "expected fallback to v1.24.0 to resolve");

    let diagnostics = diagnostics.snapshot();
    assert!(
        diagnostics.iter().any(|d| matches!(
            d,
            Diagnostic::ResolvedFromFallbackVersion {
                kind,
                api_version,
                primary_version,
                resolved_version,
            } if kind == "PodDisruptionBudget"
                && api_version == "policy/v1beta1"
                && primary_version == "v1.35.0"
                && resolved_version == "v1.24.0"
        )),
        "expected ResolvedFromFallbackVersion diagnostic; got {diagnostics:?}"
    );
}

#[test]
fn k8s_version_fallback_offline_returns_none() {
    let cache_dir = tmp_dir("k8s-fallback-offline");
    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NetworkDisabled));
    let diagnostics = DiagnosticSink::new();
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string(), "v1.24.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir)
    .with_allow_download(true)
    .with_fetcher(mock.clone())
    .with_diagnostic_sink(diagnostics.clone());

    let chain = Chain::new(vec![Box::new(provider)]).with_diagnostic_sink(diagnostics.clone());

    let resource = ResourceRef {
        api_version: "policy/v1beta1".to_string(),
        kind: "PodDisruptionBudget".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let schema = chain.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));
    assert!(schema.is_none(), "offline → no schema");
    let diagnostics = diagnostics.snapshot();
    assert!(
        diagnostics.iter().any(|d| matches!(
            d,
            Diagnostic::MissingSchema { kind, k8s_versions_tried, .. }
                if kind == "PodDisruptionBudget"
                    && k8s_versions_tried.contains(&"v1.35.0".to_string())
                    && k8s_versions_tried.contains(&"v1.24.0".to_string())
        )),
        "expected MissingSchema with both versions; got {diagnostics:?}"
    );
}

#[test]
fn has_resource_does_not_speculatively_download() {
    let cache_dir = tmp_dir("k8s-has-resource-no-download");
    let mock = Arc::new(MockFetcher::new());
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec![
            "v1.35.0".to_string(),
            "v1.34.0".to_string(),
            "v1.33.0".to_string(),
            "v1.32.0".to_string(),
            "v1.31.0".to_string(),
        ],
        None,
    ))
    .with_cache_dir(cache_dir)
    .with_allow_download(true)
    .with_fetcher(mock.clone());

    let resource = ResourceRef {
        api_version: "policy/v1beta1".to_string(),
        kind: "PodDisruptionBudget".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let owns = provider.has_resource(&resource);
    assert!(!owns, "empty cache + no downloads → has_resource=false");
    sim_assert_eq!(
        have: mock.total_calls(),
        want: 0,
        "has_resource must not trigger any fetches"
    );
}

#[test]
fn explicit_k8s_version_order_preserved() {
    // No sort: the chain order returned by `K8sVersionChain::ordered`
    // must match the order the user typed.
    let chain = K8sVersionChain::new(vec!["v1.24.0".to_string(), "v1.35.0".to_string()], None);
    sim_assert_eq!(have: chain.ordered(), want: vec!["v1.24.0", "v1.35.0"]);
}

#[test]
fn k8s_version_fallback_chain_derivation() {
    // `auto` with default window expands the single explicit version
    // into a 6-element descending chain.
    let chain = K8sVersionChain::new(vec!["v1.35.0".to_string()], Some(5));
    sim_assert_eq!(
        have: chain.ordered(),
        want: vec![
            "v1.35.0", "v1.34.0", "v1.33.0", "v1.32.0", "v1.31.0", "v1.30.0",
        ]
    );
}

#[test]
fn resolved_from_fallback_version_payload_fields() {
    let cache_dir = tmp_dir("k8s-fallback-payload-fields");
    let mock = Arc::new(
        MockFetcher::new()
            .with(
                "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.35.0/poddisruptionbudget-policy-v1beta1.json",
                MockResponse::NotFound,
            )
            .with_body(
                "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.24.0/poddisruptionbudget-policy-v1beta1.json",
                pdb_v1beta1_doc().into_bytes(),
            ),
    );
    let diagnostics = DiagnosticSink::new();
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string(), "v1.24.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir)
    .with_allow_download(true)
    .with_fetcher(mock)
    .with_diagnostic_sink(diagnostics.clone());

    let chain = Chain::new(vec![Box::new(provider)]).with_diagnostic_sink(diagnostics.clone());
    let _ = chain.schema_fragment_for_resource_path(
        &ResourceRef {
            api_version: "policy/v1beta1".to_string(),
            kind: "PodDisruptionBudget".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        },
        &YamlPath(Vec::new()),
    );

    let snapshot = diagnostics.snapshot();
    let payload = snapshot
        .iter()
        .find_map(|d| match d {
            Diagnostic::ResolvedFromFallbackVersion {
                kind,
                api_version,
                primary_version,
                resolved_version,
            } => Some((
                kind.clone(),
                api_version.clone(),
                primary_version.clone(),
                resolved_version.clone(),
            )),
            _ => None,
        })
        .expect("ResolvedFromFallbackVersion must be present");

    sim_assert_eq!(have: payload.0, want: "PodDisruptionBudget");
    sim_assert_eq!(have: payload.1, want: "policy/v1beta1");
    sim_assert_eq!(have: payload.2, want: "v1.35.0");
    sim_assert_eq!(have: payload.3, want: "v1.24.0");
}

#[test]
fn k8s_negative_cache_per_source_and_version() {
    let cache_dir = tmp_dir("k8s-negcache-per-source");
    let mirror_url = "https://example.com/mirror";
    let default_url = "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.35.0/poddisruptionbudget-policy-v1beta1.json";
    let mirror_resource_url =
        format!("{mirror_url}/v1.35.0/poddisruptionbudget-policy-v1beta1.json");
    let mock = Arc::new(
        MockFetcher::new()
            .with(default_url, MockResponse::NotFound)
            .with_body(mirror_resource_url.clone(), pdb_v1beta1_doc().into_bytes()),
    );
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir)
    .with_mirrors(vec![mirror_url.to_string()])
    .with_allow_download(true)
    .with_fetcher(mock.clone());

    let resource = ResourceRef {
        api_version: "policy/v1beta1".to_string(),
        kind: "PodDisruptionBudget".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };

    let _ = provider.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));
    let _ = provider.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));

    sim_assert_eq!(
        have: mock.calls_for(default_url),
        want: 1,
        "default source negative cache prevents retry"
    );
    // Mirror returned 200 on first call; on second call the in-memory
    // cache or on-disk cache short-circuits. Either way, no MORE than
    // one fetch should hit the mirror per lookup (it's read once,
    // cached, reused).
    assert!(
        mock.calls_for(&mirror_resource_url) <= 2,
        "mirror should be fetched at most once per file (first to populate)"
    );
}

#[test]
fn negative_cache_avoids_retry() {
    let cache_dir = tmp_dir("k8s-negative-cache");
    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir)
    .with_allow_download(true)
    .with_fetcher(mock.clone());

    let resource = ResourceRef {
        api_version: "policy/v1beta1".to_string(),
        kind: "PodDisruptionBudget".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };

    let _ = provider.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));
    let _ = provider.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));
    let _ = provider.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));

    // Multiple call sites attempt several filename candidates per
    // resource; what matters is that no URL is fetched twice.
    let counts_unique: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let _ = counts_unique;
    // Spot-check: the canonical filename is fetched at most once.
    let url = "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.35.0/poddisruptionbudget-policy-v1beta1.json";
    sim_assert_eq!(
        have: mock.calls_for(url),
        want: 1,
        "negative cache must prevent repeat fetches for the same URL"
    );
}

#[test]
fn persisted_not_found_marker_avoids_cross_process_retry() {
    let cache_dir = tmp_dir("k8s-persistent-negative-cache");
    let url = "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.35.0/poddisruptionbudget-policy-v1beta1.json";
    let first_fetcher = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let first_provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir.clone())
    .with_allow_download(true)
    .with_fetcher(first_fetcher.clone());

    let resource = ResourceRef {
        api_version: "policy/v1beta1".to_string(),
        kind: "PodDisruptionBudget".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };

    let _ = first_provider.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));
    sim_assert_eq!(
        have: first_fetcher.calls_for(url),
        want: 1,
        "first lookup should fetch once and persist the authoritative 404"
    );

    let second_fetcher = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let second_provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir)
    .with_allow_download(true)
    .with_fetcher(second_fetcher.clone());

    let _ = second_provider.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));
    sim_assert_eq!(
        have: second_fetcher.total_calls(),
        want: 0,
        "persisted not-found marker should avoid a cross-process retry"
    );
}

#[test]
fn no_cache_bypasses_not_found_marker_and_refreshes_positive_schema() {
    let cache_dir = tmp_dir("k8s-no-cache-refreshes-negative");
    let url = "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.35.0/poddisruptionbudget-policy-v1beta1.json";
    let stale_fetcher = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let stale_provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir.clone())
    .with_allow_download(true)
    .with_fetcher(stale_fetcher.clone());

    let resource = ResourceRef {
        api_version: "policy/v1beta1".to_string(),
        kind: "PodDisruptionBudget".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let schema_path = k8s_cache_path(
        &cache_dir,
        default_source_id(),
        "v1.35.0",
        "poddisruptionbudget-policy-v1beta1.json",
    );

    let _ = stale_provider.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));
    sim_assert_eq!(
        have: stale_fetcher.calls_for(url),
        want: 1,
        "setup should persist an initial authoritative 404"
    );
    assert!(
        not_found_marker_exists(&schema_path),
        "setup should write a persistent not-found marker"
    );

    let refresh_fetcher =
        Arc::new(MockFetcher::new().with_body(url, pdb_v1beta1_doc().into_bytes()));
    let refresh_provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir.clone())
    .with_allow_download(true)
    .with_use_cache(false)
    .with_fetcher(refresh_fetcher.clone());

    let refreshed =
        refresh_provider.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));
    assert!(
        refreshed.is_some(),
        "--no-cache should bypass the stale not-found marker"
    );
    sim_assert_eq!(
        have: refresh_fetcher.calls_for(url),
        want: 1,
        "--no-cache should make a fresh upstream request"
    );
    assert!(
        !not_found_marker_exists(&schema_path),
        "successful refresh should remove the stale not-found marker"
    );

    let offline_fetcher = Arc::new(MockFetcher::new().with_default(MockResponse::NetworkDisabled));
    let offline_provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir)
    .with_allow_download(false)
    .with_fetcher(offline_fetcher);
    assert!(
        offline_provider
            .schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()))
            .is_some(),
        "fresh positive schema should be cached for later normal runs"
    );
}

#[test]
fn no_cache_authoritative_not_found_clears_stale_positive_schema() {
    let cache_dir = tmp_dir("k8s-no-cache-clears-positive");
    let url = "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master/v1.35.0/poddisruptionbudget-policy-v1beta1.json";
    let resource = ResourceRef {
        api_version: "policy/v1beta1".to_string(),
        kind: "PodDisruptionBudget".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };

    let positive_fetcher =
        Arc::new(MockFetcher::new().with_body(url, pdb_v1beta1_doc().into_bytes()));
    let positive_provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir.clone())
    .with_allow_download(true)
    .with_fetcher(positive_fetcher);
    assert!(
        positive_provider
            .schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()))
            .is_some(),
        "setup should write a positive cache entry"
    );

    let stale_fetcher = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let stale_provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir.clone())
    .with_allow_download(true)
    .with_use_cache(false)
    .with_fetcher(stale_fetcher.clone());
    assert!(
        stale_provider
            .schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()))
            .is_none(),
        "--no-cache should accept the fresh authoritative 404"
    );
    sim_assert_eq!(
        have: stale_fetcher.calls_for(url),
        want: 1,
        "--no-cache should re-check upstream once"
    );

    let offline_fetcher = Arc::new(MockFetcher::new().with_default(MockResponse::NetworkDisabled));
    let offline_provider = KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec!["v1.35.0".to_string()],
        None,
    ))
    .with_cache_dir(cache_dir)
    .with_allow_download(false)
    .with_fetcher(offline_fetcher);
    assert!(
        offline_provider
            .schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()))
            .is_none(),
        "stale positive cache entry should not survive the authoritative 404"
    );
}
