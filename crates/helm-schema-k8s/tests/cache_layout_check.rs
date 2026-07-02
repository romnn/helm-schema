//! Cache layout marker / invalidation contract tests.

use std::fs;
use std::sync::Arc;
use test_util::prelude::sim_assert_eq;

use helm_schema_k8s::{
    CACHE_LAYOUT_VERSION, Diagnostic, DiagnosticSink, K8sSchemaProvider,
    KubernetesJsonSchemaProvider, LAYOUT_MARKER_FILENAME,
};

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

#[test]
fn legacy_cache_layout_is_invalidated_in_alpha() {
    let root = tmp_dir("legacy-k8s");
    // Pre-populate the legacy layout: <root>/v1.35.0/foo.json directly,
    // no per-source <default>/ directory and no marker.
    fs::create_dir_all(root.join("v1.35.0")).expect("legacy version dir");
    fs::write(root.join("v1.35.0/foo.json"), "{}").expect("seed legacy file");

    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let diagnostics = DiagnosticSink::new();
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(root.clone())
        .with_allow_download(true)
        .with_fetcher(mock.clone())
        .with_diagnostic_sink(diagnostics.clone());

    // Trigger the layout check via any normal entry point.
    let resource = helm_schema_core::ResourceRef::concrete("v1".to_string(), "Service".to_string());
    let _ = provider.has_resource(&resource);
    let _ = provider.lookup(&resource, &helm_schema_core::YamlPath(Vec::new()));

    // Legacy `<root>/v1.35.0/foo.json` was wiped.
    assert!(
        !root.join("v1.35.0/foo.json").exists(),
        "legacy file should have been wiped"
    );
    // Current marker is present.
    let marker = root.join(LAYOUT_MARKER_FILENAME);
    assert!(marker.exists(), "current marker must be written");
    let marker_value: u32 = fs::read_to_string(&marker)
        .expect("read marker")
        .trim()
        .parse()
        .expect("parse marker");
    sim_assert_eq!(have: marker_value, want: CACHE_LAYOUT_VERSION);

    // CacheLayoutInvalidated diagnostic was emitted.
    let diagnostics = diagnostics.snapshot();
    assert!(
        diagnostics.iter().any(|d| matches!(
            d,
            Diagnostic::CacheLayoutInvalidated { cache_root, .. }
                if cache_root == &root.display().to_string()
        )),
        "expected CacheLayoutInvalidated diagnostic; got {diagnostics:?}"
    );
}

#[test]
fn cache_layout_version_marker_written_after_repopulate() {
    let root = tmp_dir("empty-k8s");
    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let diagnostics = DiagnosticSink::new();
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(root.clone())
        .with_allow_download(true)
        .with_fetcher(mock.clone())
        .with_diagnostic_sink(diagnostics.clone());

    let resource = helm_schema_core::ResourceRef::concrete("v1".to_string(), "Service".to_string());
    let _ = provider.has_resource(&resource);
    let _ = provider.lookup(&resource, &helm_schema_core::YamlPath(Vec::new()));

    let marker = root.join(LAYOUT_MARKER_FILENAME);
    assert!(
        marker.exists(),
        "marker must be written even on first populate"
    );
    let diagnostics = diagnostics.snapshot();
    assert!(
        !diagnostics
            .iter()
            .any(|d| matches!(d, Diagnostic::CacheLayoutInvalidated { .. })),
        "first-populate must NOT emit an invalidation diagnostic"
    );
}

#[test]
fn cache_invalidation_emits_diagnostic() {
    // Same setup as `legacy_cache_layout_is_invalidated_in_alpha`, but
    // asserting the exact diagnostic shape (previous_marker=None on
    // first sweep, current_marker = compiled-in constant).
    let root = tmp_dir("cache-inv-diag");
    fs::create_dir_all(root.join("v1.35.0")).expect("legacy version dir");
    fs::write(root.join("v1.35.0/foo.json"), "{}").expect("seed legacy file");

    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let diagnostics = DiagnosticSink::new();
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(root.clone())
        .with_allow_download(true)
        .with_fetcher(mock)
        .with_diagnostic_sink(diagnostics.clone());

    let resource = helm_schema_core::ResourceRef::concrete("v1".to_string(), "Service".to_string());
    let _ = provider.lookup(&resource, &helm_schema_core::YamlPath(Vec::new()));

    let snapshot = diagnostics.snapshot();
    let inv = snapshot
        .iter()
        .find_map(|d| match d {
            Diagnostic::CacheLayoutInvalidated {
                cache_root,
                previous_marker,
                current_marker,
            } => Some((cache_root.clone(), *previous_marker, *current_marker)),
            _ => None,
        })
        .expect("CacheLayoutInvalidated must be present");
    sim_assert_eq!(have: inv.0, want: root.display().to_string());
    sim_assert_eq!(have: inv.1, want: None, "legacy layout has no prior marker");
    sim_assert_eq!(have: inv.2, want: CACHE_LAYOUT_VERSION);
}

#[test]
fn forward_incompat_cache_emits_diagnostic() {
    // Companion to `cache_layout_version_newer_marker_refuses_mutation`
    // that asserts the diagnostic shape (on_disk_marker > compiled).
    let root = tmp_dir("forward-incompat-diag");
    fs::write(
        root.join(LAYOUT_MARKER_FILENAME),
        format!("{}\n", CACHE_LAYOUT_VERSION + 999),
    )
    .expect("seed marker");

    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let diagnostics = DiagnosticSink::new();
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(root.clone())
        .with_allow_download(true)
        .with_fetcher(mock)
        .with_diagnostic_sink(diagnostics.clone());

    let resource = helm_schema_core::ResourceRef::concrete("v1".to_string(), "Service".to_string());
    let _ = provider.lookup(&resource, &helm_schema_core::YamlPath(Vec::new()));

    let snapshot = diagnostics.snapshot();
    let payload = snapshot
        .iter()
        .find_map(|d| match d {
            Diagnostic::CacheLayoutForwardIncompatible {
                cache_root,
                on_disk_marker,
                compiled_marker,
            } => Some((cache_root.clone(), *on_disk_marker, *compiled_marker)),
            _ => None,
        })
        .expect("CacheLayoutForwardIncompatible must be present");
    sim_assert_eq!(have: payload.0, want: root.display().to_string());
    sim_assert_eq!(have: payload.1, want: CACHE_LAYOUT_VERSION + 999);
    sim_assert_eq!(have: payload.2, want: CACHE_LAYOUT_VERSION);
}

#[test]
fn cache_invalidation_is_per_root() {
    use helm_schema_k8s::{Chain, CrdsCatalogSchemaProvider};

    // K8s root: legacy layout; CRD root: current marker + valid entry.
    let k8s_root = tmp_dir("per-root-k8s");
    let crd_root = tmp_dir("per-root-crd");
    fs::create_dir_all(k8s_root.join("v1.35.0")).expect("legacy K8s");
    fs::write(k8s_root.join("v1.35.0/foo.json"), "{}").expect("seed legacy K8s");

    fs::write(
        crd_root.join(LAYOUT_MARKER_FILENAME),
        format!("{}\n", CACHE_LAYOUT_VERSION),
    )
    .expect("seed CRD marker");
    fs::create_dir_all(crd_root.join("default/monitoring.coreos.com")).expect("seed CRD group dir");
    let crd_file = crd_root.join("default/monitoring.coreos.com/servicemonitor_v1.json");
    fs::write(&crd_file, r#"{"type":"object"}"#).expect("seed CRD file");

    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let diagnostics = DiagnosticSink::new();
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(k8s_root.clone())
        .with_allow_download(true)
        .with_fetcher(mock.clone())
        .with_diagnostic_sink(diagnostics.clone());
    let crd_provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(crd_root.clone())
        .with_allow_download(true)
        .with_fetcher(mock)
        .with_diagnostic_sink(diagnostics.clone());

    let chain = Chain::new(vec![Box::new(crd_provider), Box::new(k8s_provider)])
        .with_diagnostic_sink(diagnostics.clone());

    // Probe a K8s resource → triggers K8s layout check (invalidation).
    let _ = chain.schema_fragment_for_resource_path(
        &helm_schema_core::ResourceRef::concrete("v1".to_string(), "Service".to_string()),
        &helm_schema_core::YamlPath(Vec::new()),
    );
    // Probe a CRD resource → triggers CRD layout check (no-op).
    let _ = chain.schema_fragment_for_resource_path(
        &helm_schema_core::ResourceRef::concrete(
            "monitoring.coreos.com/v1".to_string(),
            "ServiceMonitor".to_string(),
        ),
        &helm_schema_core::YamlPath(Vec::new()),
    );

    let snapshot = diagnostics.snapshot();
    let k8s_invalidated = snapshot.iter().any(|d| {
        matches!(
            d,
            Diagnostic::CacheLayoutInvalidated { cache_root, .. }
                if cache_root == &k8s_root.display().to_string()
        )
    });
    let crd_invalidated = snapshot.iter().any(|d| {
        matches!(
            d,
            Diagnostic::CacheLayoutInvalidated { cache_root, .. }
                if cache_root == &crd_root.display().to_string()
        )
    });
    assert!(
        k8s_invalidated,
        "K8s root must be invalidated (legacy layout)"
    );
    assert!(
        !crd_invalidated,
        "CRD root must NOT be invalidated (current marker)"
    );
    assert!(crd_file.exists(), "CRD content must survive untouched");
}

#[test]
fn cache_forward_incompat_one_root_does_not_block_other() {
    use helm_schema_k8s::{Chain, CrdsCatalogSchemaProvider};

    // K8s root: forward-incompat. CRD root: empty.
    let k8s_root = tmp_dir("forward-block-k8s");
    let crd_root = tmp_dir("forward-block-crd");
    fs::write(
        k8s_root.join(LAYOUT_MARKER_FILENAME),
        format!("{}\n", CACHE_LAYOUT_VERSION + 1),
    )
    .expect("seed forward marker");

    // CRD has the schema we'll ask for.
    let mock = Arc::new(MockFetcher::new().with_body(
        "https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/monitoring.coreos.com/servicemonitor_v1.json",
        r#"{"type":"object","properties":{"spec":{"type":"object"}}}"#.as_bytes().to_vec(),
    ));
    let diagnostics = DiagnosticSink::new();
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(k8s_root.clone())
        .with_allow_download(true)
        .with_fetcher(mock.clone())
        .with_diagnostic_sink(diagnostics.clone());
    let crd_provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(crd_root.clone())
        .with_allow_download(true)
        .with_fetcher(mock)
        .with_diagnostic_sink(diagnostics.clone());

    let chain = Chain::new(vec![Box::new(crd_provider), Box::new(k8s_provider)])
        .with_diagnostic_sink(diagnostics.clone());

    // First, probe a K8s resource so K8s's layout check fires
    // (forward-incompat marker → diagnostic + NotOwned).
    let _ = chain.schema_fragment_for_resource_path(
        &helm_schema_core::ResourceRef::concrete("v1".to_string(), "Service".to_string()),
        &helm_schema_core::YamlPath(Vec::new()),
    );

    // Then, probe a CRD resource — K8s is forward-incompat, but CRD
    // resolution must still succeed.
    let schema = chain.schema_fragment_for_resource_path(
        &helm_schema_core::ResourceRef::concrete(
            "monitoring.coreos.com/v1".to_string(),
            "ServiceMonitor".to_string(),
        ),
        &helm_schema_core::YamlPath(Vec::new()),
    );
    assert!(
        schema.is_some(),
        "CRD must still resolve despite forward-incompat K8s root"
    );
    let snapshot = diagnostics.snapshot();
    let k8s_incompat = snapshot
        .iter()
        .filter(|d| {
            matches!(
                d,
                Diagnostic::CacheLayoutForwardIncompatible { cache_root, .. }
                    if cache_root == &k8s_root.display().to_string()
            )
        })
        .count();
    let crd_incompat = snapshot
        .iter()
        .filter(|d| {
            matches!(
                d,
                Diagnostic::CacheLayoutForwardIncompatible { cache_root, .. }
                    if cache_root == &crd_root.display().to_string()
            )
        })
        .count();
    sim_assert_eq!(
        have: k8s_incompat,
        want: 1,
        "K8s root must emit exactly one forward-incompat"
    );
    sim_assert_eq!(have: crd_incompat, want: 0, "no forward-incompat for CRD root");
}

#[test]
fn override_dir_never_invalidated() {
    use helm_schema_k8s::{Chain, CrdsCatalogSchemaProvider, LocalSchemaProvider};

    let override_dir = tmp_dir("override-untouched");
    let cache_dir = tmp_dir("cache-wiped");

    // Hand-maintained override schema.
    fs::create_dir_all(override_dir.join("g.io")).expect("override group dir");
    fs::write(
        override_dir.join("g.io/foo_v1alpha1.json"),
        r#"{"type":"object","properties":{"spec":{"type":"object"}}}"#,
    )
    .expect("seed override");

    // Pre-populate the *cache* dir with a legacy layout (no marker).
    fs::create_dir_all(cache_dir.join("monitoring.coreos.com")).expect("legacy crd cache");
    fs::write(
        cache_dir.join("monitoring.coreos.com/servicemonitor_v1.json"),
        r#"{"type":"object"}"#,
    )
    .expect("seed legacy crd file");

    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let diagnostics = DiagnosticSink::new();
    let crd = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache_dir.clone())
        .with_allow_download(true)
        .with_fetcher(mock)
        .with_diagnostic_sink(diagnostics.clone());
    let local = LocalSchemaProvider::new(override_dir.clone());
    let chain =
        Chain::new(vec![Box::new(local), Box::new(crd)]).with_diagnostic_sink(diagnostics.clone());

    // Trigger the layout check by probing any CRD-shaped resource.
    let _ = chain.schema_fragment_for_resource_path(
        &helm_schema_core::ResourceRef::concrete(
            "monitoring.coreos.com/v1".to_string(),
            "ServiceMonitor".to_string(),
        ),
        &helm_schema_core::YamlPath(Vec::new()),
    );

    // Override file is untouched.
    assert!(
        override_dir.join("g.io/foo_v1alpha1.json").exists(),
        "override file must NOT be wiped"
    );
    // Cache dir's legacy file was wiped (separate concern, included here
    // to ensure the test exercises the actual invalidation pathway).
    assert!(
        !cache_dir
            .join("monitoring.coreos.com/servicemonitor_v1.json")
            .exists(),
        "cache dir's legacy file should have been wiped"
    );
    // No marker was written to the override dir.
    assert!(
        !override_dir.join(LAYOUT_MARKER_FILENAME).exists(),
        "override dir must not gain a CACHE_LAYOUT_VERSION marker"
    );
}

#[test]
fn cache_layout_version_newer_marker_refuses_mutation() {
    let root = tmp_dir("forward-incompat");
    fs::write(
        root.join(LAYOUT_MARKER_FILENAME),
        format!("{}\n", CACHE_LAYOUT_VERSION + 999),
    )
    .expect("seed forward-incompat marker");

    let mock = Arc::new(MockFetcher::new().with_default(MockResponse::NotFound));
    let diagnostics = DiagnosticSink::new();
    let provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(root.clone())
        .with_allow_download(true)
        .with_fetcher(mock.clone())
        .with_diagnostic_sink(diagnostics.clone());

    let resource = helm_schema_core::ResourceRef::concrete("v1".to_string(), "Service".to_string());
    let _ = provider.has_resource(&resource);
    let _ = provider.lookup(&resource, &helm_schema_core::YamlPath(Vec::new()));

    // Marker untouched.
    let marker_value: u32 = fs::read_to_string(root.join(LAYOUT_MARKER_FILENAME))
        .expect("read marker")
        .trim()
        .parse()
        .expect("parse marker");
    sim_assert_eq!(have: marker_value, want: CACHE_LAYOUT_VERSION + 999);

    let diagnostics = diagnostics.snapshot();
    assert!(
        diagnostics.iter().any(|d| matches!(
            d,
            Diagnostic::CacheLayoutForwardIncompatible { cache_root, .. }
                if cache_root == &root.display().to_string()
        )),
        "expected forward-incompatible diagnostic; got {diagnostics:?}"
    );
}
