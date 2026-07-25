//! Live fetch-on-demand against the real upstream schema sources.
//!
//! Every other test in this workspace reads the vendored bundle under
//! `testdata/provider-bundle/` so its result cannot depend on the network or
//! on cache warmth. That leaves one thing unproven, and it is the thing a
//! first-time user hits: with an EMPTY cache and downloads enabled, does the
//! tool actually reach upstream, parse what it gets, and write a cache entry
//! the next run can use?
//!
//! These tests answer that, so they are the only ones allowed to touch the
//! network. Each one starts from a fresh temp cache directory — pointing at
//! the ambient user cache would let a warm machine pass without ever issuing
//! a request, which is precisely the coverage gap this file exists to close.
//!
//! They run under the `network` nextest profile (`task test:network`) and
//! inside the all-inclusive `ci` profile (`task test:all`) via its
//! per-binary override. Both retry them — an upstream blip is an outage,
//! not a defect in this repo — while every offline test keeps `retries = 0`.

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_core::{ResourceRef, YamlPath};
use helm_schema_k8s::{CrdsCatalogSchemaProvider, K8sSchemaProvider, KubernetesJsonSchemaProvider};
use test_util::prelude::sim_assert_eq;

/// Pinned so a fetch failure is never confused with "this version moved".
const K8S_VERSION: &str = "v1.35.0";

fn cold_cache(label: &str) -> eyre::Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix(&format!("helm-schema-network-{label}."))
        .tempdir()
        .wrap_err("create empty cache root")
}

fn root_schema(
    provider: &impl K8sSchemaProvider,
    resource: &ResourceRef,
) -> Option<serde_json::Value> {
    provider
        .lookup(resource, &YamlPath(Vec::new()))
        .into_schema_fragment()
        .map(helm_schema_core::ProviderSchemaFragment::into_schema)
}

/// The full first-run path for a built-in Kubernetes kind: empty cache →
/// upstream fetch → schema returned → cache entry written → a second,
/// download-disabled provider serves the same schema from that entry.
///
/// The offline half is what makes this more than a connectivity check: it
/// proves the bytes written during the fetch are a usable cache entry, which
/// is what every subsequent run of the tool depends on.
#[test]
fn k8s_schema_is_fetched_into_an_empty_cache_and_reused_offline() -> eyre::Result<()> {
    let cache = cold_cache("k8s")?;
    let resource = ResourceRef::concrete("apps/v1".to_string(), "Deployment".to_string());

    let online = KubernetesJsonSchemaProvider::new(K8S_VERSION.to_string())
        .with_cache_dir(cache.path().to_path_buf())
        .with_allow_download(true);
    let fetched = root_schema(&online, &resource)
        .ok_or_eyre("upstream fetch returned no schema for apps/v1 Deployment")?;

    assert!(
        fetched.pointer("/properties/spec").is_some(),
        "fetched Deployment schema has no spec property: {fetched}"
    );

    let cached_file = cache
        .path()
        .join("default")
        .join(K8S_VERSION)
        .join("deployment-apps-v1.json");
    assert!(
        cached_file.is_file(),
        "fetch must leave a reusable cache entry at {}",
        cached_file.display()
    );

    let offline = KubernetesJsonSchemaProvider::new(K8S_VERSION.to_string())
        .with_cache_dir(cache.path().to_path_buf())
        .with_allow_download(false);
    sim_assert_eq!(
        have: root_schema(&offline, &resource).ok_or_eyre("cache entry unusable offline")?,
        want: fetched
    );
    Ok(())
}

/// The same first-run path for the CRD catalog, which is a different
/// upstream host and a different on-disk layout.
#[test]
fn crd_schema_is_fetched_into_an_empty_cache_and_reused_offline() -> eyre::Result<()> {
    let cache = cold_cache("crd")?;
    let resource = ResourceRef::concrete(
        "monitoring.coreos.com/v1".to_string(),
        "PrometheusRule".to_string(),
    );

    let online = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache.path().to_path_buf())
        .with_allow_download(true);
    let fetched = root_schema(&online, &resource).ok_or_eyre(
        "upstream fetch returned no schema for monitoring.coreos.com/v1 PrometheusRule",
    )?;

    let cached_file = cache
        .path()
        .join("default")
        .join("monitoring.coreos.com")
        .join("prometheusrule_v1.json");
    assert!(
        cached_file.is_file(),
        "fetch must leave a reusable cache entry at {}",
        cached_file.display()
    );

    let offline = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache.path().to_path_buf())
        .with_allow_download(false);
    sim_assert_eq!(
        have: root_schema(&offline, &resource).ok_or_eyre("cache entry unusable offline")?,
        want: fetched
    );
    Ok(())
}

/// An API upstream genuinely does not carry must come back as an
/// authoritative absence and be recorded on disk, so a later offline run can
/// tell "absent upstream" apart from "not fetched yet". A cold cache alone
/// must never be read as absence.
///
/// `autoscaling/v2beta1 HorizontalPodAutoscaler` was removed long before
/// this bundle version, so upstream answers a real 404 for it.
#[test]
fn upstream_absence_is_recorded_as_authoritative() -> eyre::Result<()> {
    let cache = cold_cache("absent")?;
    let resource = ResourceRef::concrete(
        "autoscaling/v2beta1".to_string(),
        "HorizontalPodAutoscaler".to_string(),
    );

    let online = KubernetesJsonSchemaProvider::new(K8S_VERSION.to_string())
        .with_cache_dir(cache.path().to_path_buf())
        .with_allow_download(true);
    assert!(
        root_schema(&online, &resource).is_none(),
        "an API removed before {K8S_VERSION} must not resolve to a schema"
    );

    let marker = cache
        .path()
        .join("default")
        .join(K8S_VERSION)
        .join("horizontalpodautoscaler-autoscaling-v2beta1.json.not-found");
    assert!(
        marker.is_file(),
        "an authoritative upstream 404 must be recorded at {} so later offline \
         runs can distinguish 'absent upstream' from 'not fetched yet'",
        marker.display()
    );
    Ok(())
}

/// `.k8s.io`-suffix groups were once blocklisted by path formation, making
/// legitimate addon CRDs unreachable even when the catalog had them. Proving
/// the fix needs a real catalog response, so it belongs here rather than in
/// the bundle-backed suite where it could only assert "did not return false
/// for the wrong reason".
#[test]
fn dot_k8s_io_suffix_groups_resolve_against_the_live_catalog() -> eyre::Result<()> {
    let cache = cold_cache("k8s-io-suffix")?;
    let provider = CrdsCatalogSchemaProvider::new()
        .with_cache_dir(cache.path().to_path_buf())
        .with_allow_download(true);

    let resource = ResourceRef::concrete(
        "autoscaling.k8s.io/v1".to_string(),
        "VerticalPodAutoscaler".to_string(),
    );
    // Populate first: `has_resource` reports on-disk state by contract and
    // never fetches.
    let _ = root_schema(&provider, &resource);

    assert!(
        provider.has_resource(&resource),
        "VerticalPodAutoscaler (autoscaling.k8s.io/v1) must be resolvable \
         through the CRDs catalog — a `.k8s.io` suffix is not a built-in group"
    );
    Ok(())
}
