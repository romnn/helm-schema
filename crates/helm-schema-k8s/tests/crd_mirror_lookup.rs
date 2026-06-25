//! Feature C — CRD mirror chain + cross-scan + no-substitution
//! contracts.

use std::fs;
use std::sync::Arc;

use helm_schema_core::{ResourceRef, YamlPath};
use helm_schema_k8s::{
    Chain, CrdsCatalogSchemaProvider, Diagnostic, DiagnosticSink, K8sSchemaProvider,
    source_id_for_url,
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

fn crd_doc() -> String {
    r#"{"type":"object","properties":{"spec":{"type":"object","properties":{"x":{"type":"string"}}}}}"#.to_string()
}

#[test]
fn crd_has_resource_does_not_speculatively_download() {
    let cache = tmp_dir("crd-has-resource");
    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache)
        .with_allow_download(true)
        .with_fetcher(mock.clone());

    let resource = ResourceRef {
        api_version: "monitoring.coreos.com/v1".to_string(),
        kind: "ServiceMonitor".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let owns = provider.has_resource(&resource);
    assert!(!owns);
    sim_assert_eq!(have: mock.total_calls(), want: 0, "has_resource must not fetch");
}

#[test]
fn crd_loose_probes_mirrors_for_exact_version() {
    let cache = tmp_dir("crd-loose-mirror");
    let mirror_url = "https://example.com/crds";
    let mock = Arc::new(
        MockFetcher::new()
            // default catalog 404
            .with(
                "https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/monitoring.coreos.com/servicemonitor_v1alpha1.json",
                MockResponse::NotFound,
            )
            // mirror returns 200
            .with_body(
                format!("{mirror_url}/monitoring.coreos.com/servicemonitor_v1alpha1.json"),
                crd_doc().into_bytes(),
            ),
    );
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache)
        .with_allow_download(true)
        .with_loose(true)
        .with_mirrors(vec![mirror_url.to_string()])
        .with_fetcher(mock);

    let resource = ResourceRef {
        api_version: "monitoring.coreos.com/v1alpha1".to_string(),
        kind: "ServiceMonitor".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let schema = provider
        .lookup(&resource, &YamlPath(Vec::new()))
        .into_schema_fragment();
    assert!(
        schema.is_some(),
        "mirror should answer for exact requested version"
    );
}

#[test]
fn crd_loose_never_substitutes_version() {
    let cache = tmp_dir("crd-no-subst");
    // Pre-populate v1 in default cache so the cross-scan tier sees it,
    // but the requested v1alpha1 is genuinely missing.
    let source_id = "default";
    fs::create_dir_all(cache.join(source_id).join("monitoring.coreos.com"))
        .expect("seed group dir");
    fs::write(
        cache
            .join(source_id)
            .join("monitoring.coreos.com/servicemonitor_v1.json"),
        crd_doc(),
    )
    .expect("seed v1 schema");
    // Marker so the cache isn't wiped.
    fs::write(
        cache.join("CACHE_LAYOUT_VERSION"),
        format!("{}\n", helm_schema_k8s::CACHE_LAYOUT_VERSION),
    )
    .expect("seed marker");

    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let diagnostics = DiagnosticSink::new();
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache)
        .with_allow_download(true)
        .with_loose(true)
        .with_fetcher(mock)
        .with_diagnostic_sink(diagnostics.clone());
    let chain = Chain::new(vec![Box::new(provider)]).with_diagnostic_sink(diagnostics.clone());

    let resource = ResourceRef {
        api_version: "monitoring.coreos.com/v1alpha1".to_string(),
        kind: "ServiceMonitor".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let schema = chain.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));
    assert!(schema.is_none(), "v1alpha1 must not be substituted by v1");

    let snapshot = diagnostics.snapshot();
    assert!(
        snapshot.iter().any(|d| matches!(
            d,
            Diagnostic::CrdVersionNotFound { requested_version, .. }
                if requested_version == "v1alpha1"
        )),
        "expected CrdVersionNotFound for v1alpha1; got {snapshot:?}"
    );
    assert!(
        snapshot.iter().any(|d| matches!(
            d,
            Diagnostic::CrdVersionAvailableAtOtherVersions { requested_version, available_versions, .. }
                if requested_version == "v1alpha1" && available_versions.contains(&"v1".to_string())
        )),
        "expected CrdVersionAvailableAtOtherVersions; got {snapshot:?}"
    );
}

#[test]
fn crd_loose_available_at_other_versions_local_cache_only() {
    let cache = tmp_dir("crd-cache-only-hint");
    fs::write(
        cache.join("CACHE_LAYOUT_VERSION"),
        format!("{}\n", helm_schema_k8s::CACHE_LAYOUT_VERSION),
    )
    .expect("seed marker");

    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let diagnostics = DiagnosticSink::new();
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache)
        .with_allow_download(true)
        .with_loose(true)
        .with_fetcher(mock);
    let chain = Chain::new(vec![Box::new(provider)]).with_diagnostic_sink(diagnostics.clone());

    let resource = ResourceRef {
        api_version: "monitoring.coreos.com/v1alpha1".to_string(),
        kind: "ServiceMonitor".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let _ = chain.schema_fragment_for_resource_path(&resource, &YamlPath(Vec::new()));

    let snapshot = diagnostics.snapshot();
    assert!(
        snapshot
            .iter()
            .any(|d| matches!(d, Diagnostic::CrdVersionNotFound { .. })),
        "CrdVersionNotFound must fire"
    );
    assert!(
        !snapshot
            .iter()
            .any(|d| matches!(d, Diagnostic::CrdVersionAvailableAtOtherVersions { .. })),
        "CrdVersionAvailableAtOtherVersions must NOT fire without local-cache evidence"
    );
}

#[test]
fn crd_strict_suppresses_cross_scan_keeps_mirrors() {
    // Strict mode: cross-scan + CrdVersionAvailableAtOtherVersions
    // are suppressed, but mirrors are still consulted.
    let cache = tmp_dir("crd-strict-keeps-mirrors");
    fs::write(
        cache.join("CACHE_LAYOUT_VERSION"),
        format!("{}\n", helm_schema_k8s::CACHE_LAYOUT_VERSION),
    )
    .expect("seed marker");
    // Pre-populate v1 in the default namespace (cross-scan would find this).
    fs::create_dir_all(cache.join("default/monitoring.coreos.com")).expect("seed group dir");
    fs::write(
        cache.join("default/monitoring.coreos.com/servicemonitor_v1.json"),
        crd_doc(),
    )
    .expect("seed v1");

    let mirror_url = "https://example.com/strict-mirror";
    let mirror_resource_url =
        format!("{mirror_url}/monitoring.coreos.com/servicemonitor_v1alpha1.json");
    let mock = Arc::new(
        MockFetcher::new()
            .with(
                "https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/monitoring.coreos.com/servicemonitor_v1alpha1.json",
                MockResponse::NotFound,
            )
            .with_body(mirror_resource_url, crd_doc().into_bytes()),
    );
    let diagnostics = DiagnosticSink::new();
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache)
        .with_allow_download(true)
        .with_loose(false) // strict
        .with_mirrors(vec![mirror_url.to_string()])
        .with_fetcher(mock)
        .with_diagnostic_sink(diagnostics.clone());

    let resource = ResourceRef {
        api_version: "monitoring.coreos.com/v1alpha1".to_string(),
        kind: "ServiceMonitor".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let schema = provider
        .lookup(&resource, &YamlPath(Vec::new()))
        .into_schema_fragment();
    assert!(schema.is_some(), "mirror must answer in strict mode");
    let snapshot = diagnostics.snapshot();
    assert!(
        !snapshot
            .iter()
            .any(|d| matches!(d, Diagnostic::CrdVersionAvailableAtOtherVersions { .. })),
        "strict mode must NOT emit CrdVersionAvailableAtOtherVersions"
    );
}

#[test]
fn crd_mirror_does_not_mask_default_catalog() {
    // Default returns 200 (its own content), mirror also returns 200
    // with different content. Default wins per precedence; mirror
    // entry is namespaced separately, never returned.
    let cache = tmp_dir("crd-default-wins");
    let mirror_url = "https://example.com/mirror-different-content";
    let default_url = "https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/monitoring.coreos.com/servicemonitor_v1alpha1.json";
    let mirror_resource_url =
        format!("{mirror_url}/monitoring.coreos.com/servicemonitor_v1alpha1.json");
    let mock = Arc::new(
        MockFetcher::new()
            .with_body(
                default_url,
                r#"{"type":"object","properties":{"default_field":{"type":"string"}}}"#
                    .as_bytes()
                    .to_vec(),
            )
            .with_body(
                mirror_resource_url,
                r#"{"type":"object","properties":{"mirror_only_field":{"type":"string"}}}"#
                    .as_bytes()
                    .to_vec(),
            ),
    );
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache)
        .with_allow_download(true)
        .with_mirrors(vec![mirror_url.to_string()])
        .with_fetcher(mock);

    let schema = provider
        .lookup(
            &ResourceRef {
                api_version: "monitoring.coreos.com/v1alpha1".to_string(),
                kind: "ServiceMonitor".to_string(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            },
            &YamlPath(Vec::new()),
        )
        .into_schema_fragment()
        .expect("default answered")
        .into_schema();
    let props = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .expect("properties");
    assert!(
        props.contains_key("default_field"),
        "default catalog's content must win"
    );
    assert!(
        !props.contains_key("mirror_only_field"),
        "mirror content must NOT be returned in front-of default"
    );
}

#[test]
fn crd_negative_cache_per_source() {
    // Default 404, mirror 404 — second call must not re-fetch.
    let cache = tmp_dir("crd-negcache-per-source");
    let mirror_url = "https://example.com/mirror-404";
    let default_url = "https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/monitoring.coreos.com/servicemonitor_v1alpha1.json";
    let mirror_resource_url =
        format!("{mirror_url}/monitoring.coreos.com/servicemonitor_v1alpha1.json");
    let mock = Arc::new(
        MockFetcher::new()
            .with(default_url, MockResponse::NotFound)
            .with(mirror_resource_url.clone(), MockResponse::NotFound),
    );
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache)
        .with_allow_download(true)
        .with_mirrors(vec![mirror_url.to_string()])
        .with_fetcher(mock.clone());

    let resource = ResourceRef {
        api_version: "monitoring.coreos.com/v1alpha1".to_string(),
        kind: "ServiceMonitor".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let _ = provider.lookup(&resource, &YamlPath(Vec::new()));
    let _ = provider.lookup(&resource, &YamlPath(Vec::new()));

    sim_assert_eq!(
        have: mock.calls_for(default_url),
        want: 1,
        "default 404 must be cached negatively"
    );
    sim_assert_eq!(
        have: mock.calls_for(&mirror_resource_url),
        want: 1,
        "mirror 404 must be cached negatively independently"
    );
}

#[test]
fn crd_cache_record_source_writes_meta_sidecar() {
    let cache = tmp_dir("crd-record-source");
    let default_url = "https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/monitoring.coreos.com/servicemonitor_v1alpha1.json";
    let mock = Arc::new(MockFetcher::new().with_body(default_url, crd_doc().into_bytes()));
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache.clone())
        .with_allow_download(true)
        .with_record_source(true)
        .with_fetcher(mock);

    let resource = ResourceRef {
        api_version: "monitoring.coreos.com/v1alpha1".to_string(),
        kind: "ServiceMonitor".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let _ = provider.lookup(&resource, &YamlPath(Vec::new()));

    let meta = cache.join("default/monitoring.coreos.com/servicemonitor_v1alpha1.json.meta");
    assert!(meta.exists(), "expected meta sidecar at {meta:?}");
    let content = fs::read_to_string(&meta).expect("read sidecar");
    assert!(content.contains("source_url:"), "sidecar lists source URL");
    assert!(
        content.contains("fetched_at:"),
        "sidecar lists fetch timestamp"
    );
    assert!(
        content.contains(default_url),
        "sidecar carries the actual URL"
    );
}

#[test]
fn crd_diagnostic_json_format() {
    // CrdVersionNotFound serializes as a single JSON object that
    // round-trips back to the same variant.
    use helm_schema_k8s::format_diagnostic_json;
    let diagnostic = Diagnostic::CrdVersionNotFound {
        group: "g".to_string(),
        kind: "K".to_string(),
        requested_version: "v1alpha1".to_string(),
        locations_tried: vec!["url-a".to_string()],
    };
    let json = format_diagnostic_json(&diagnostic).expect("serialize");
    sim_assert_eq!(have: json.lines().count(), want: 1, "JSON output must be a single line");
    let parsed: Diagnostic = serde_json::from_str(&json).expect("round-trip");
    assert!(matches!(parsed, Diagnostic::CrdVersionNotFound { .. }));
}

#[test]
fn crd_mirror_cache_per_source_namespaced() {
    let cache = tmp_dir("crd-mirror-namespace");
    let mirror_url = "https://example.com/crds";
    let mirror_id = source_id_for_url(mirror_url);
    let mock = Arc::new(
        MockFetcher::new()
            .with_body(
                "https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/monitoring.coreos.com/servicemonitor_v1alpha1.json",
                r#"{"type":"object","properties":{"spec":{"type":"object"}}}"#.as_bytes().to_vec(),
            )
            .with_body(
                format!("{mirror_url}/monitoring.coreos.com/servicemonitor_v1alpha1.json"),
                r#"{"type":"object","properties":{"spec":{"type":"object","properties":{"mirror_only_field":{"type":"string"}}}}}"#.as_bytes().to_vec(),
            ),
    );
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache.clone())
        .with_allow_download(true)
        .with_mirrors(vec![mirror_url.to_string()])
        .with_fetcher(mock);

    let resource = ResourceRef {
        api_version: "monitoring.coreos.com/v1alpha1".to_string(),
        kind: "ServiceMonitor".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    // Force the default to populate first.
    let _ = provider.lookup(&resource, &YamlPath(Vec::new()));

    let default_path = cache.join("default/monitoring.coreos.com/servicemonitor_v1alpha1.json");
    let mirror_path = cache.join(format!(
        "{mirror_id}/monitoring.coreos.com/servicemonitor_v1alpha1.json"
    ));
    assert!(
        default_path.exists(),
        "default content cached at default/..."
    );
    // Precedence is default-first, so the mirror is never consulted on a
    // hit by the default source. The mirror namespace dir must NOT
    // appear under the cache root; this is what proves the per-source
    // layout actually keeps mirror bodies from masking the default.
    assert!(
        !mirror_path.exists(),
        "mirror path must not be cached when default already answered (got {})",
        mirror_path.display(),
    );

    // And the on-disk shapes must be different file paths (sanity check
    // that the source_id derivation isn't producing the same dir).
    assert_ne!(
        default_path, mirror_path,
        "default and mirror must namespace to distinct paths"
    );
}
