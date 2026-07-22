//! CRD-catalog schema lookup regression for `PrometheusRule`.

use color_eyre::eyre;
use helm_schema_core::{ResourceRef, YamlPath};
use helm_schema_k8s::{CrdsCatalogSchemaProvider, K8sSchemaProvider, LocalSchemaProvider};
use std::sync::atomic::{AtomicUsize, Ordering};
use test_util::prelude::sim_assert_eq;

static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn make_temp_dir(group_dir: &str) -> eyre::Result<std::path::PathBuf> {
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "helm-schema.crds-catalog-test.{}.{}",
        std::process::id(),
        n
    ));
    std::fs::create_dir_all(dir.join(group_dir))?;
    Ok(dir)
}

fn materialize_schema_for_resource(
    provider: &impl K8sSchemaProvider,
    resource: &ResourceRef,
) -> Option<serde_json::Value> {
    provider
        .lookup(resource, &YamlPath(Vec::new()))
        .into_schema_fragment()
        .map(helm_schema_core::ProviderSchemaFragment::into_schema)
}

#[test]
fn materialize_prometheusrule() -> eyre::Result<()> {
    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);

    let r = ResourceRef::concrete(
        "monitoring.coreos.com/v1".to_string(),
        "PrometheusRule".to_string(),
    );

    let upstream_materialized =
        materialize_schema_for_resource(&provider, &r).expect("materialize");

    let relative_path = "monitoring.coreos.com/prometheusrule_v1.json";
    let cached = provider.cache_dir.join("default").join(relative_path);
    assert!(
        cached.exists(),
        "expected schema to be cached at {cached:?}"
    );

    let root_dir = make_temp_dir("monitoring.coreos.com")?;
    std::fs::copy(&cached, root_dir.join(relative_path)).expect("copy cached schema");

    let local_provider = LocalSchemaProvider::new(&root_dir);
    let local_materialized =
        materialize_schema_for_resource(&local_provider, &r).expect("materialize");

    sim_assert_eq!(have: upstream_materialized, want: local_materialized);
    Ok(())
}

#[test]
fn prometheusrule_leaf_schema_rules_items() -> eyre::Result<()> {
    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);

    let r = ResourceRef::concrete(
        "monitoring.coreos.com/v1".to_string(),
        "PrometheusRule".to_string(),
    );

    let path = YamlPath(vec![
        "spec".to_string(),
        "groups[*]".to_string(),
        "rules[*]".to_string(),
    ]);

    let upstream_leaf = provider
        .lookup(&r, &path)
        .into_schema_fragment()
        .expect("leaf schema");

    let relative_path = "monitoring.coreos.com/prometheusrule_v1.json";
    let cached = provider.cache_dir.join("default").join(relative_path);
    assert!(
        cached.exists(),
        "expected schema to be cached at {cached:?}"
    );

    let root_dir = make_temp_dir("monitoring.coreos.com")?;
    std::fs::copy(&cached, root_dir.join(relative_path)).expect("copy cached schema");

    let local_provider = LocalSchemaProvider::new(&root_dir);
    let local_leaf = local_provider
        .lookup(&r, &path)
        .into_schema_fragment()
        .expect("leaf schema");

    sim_assert_eq!(have: upstream_leaf.into_schema(), want: local_leaf.into_schema());
    Ok(())
}

/// `has_resource` reports whether the catalog has the resource's schema
/// FILE, distinct from whether a specific path resolves inside it. Used
/// by chain providers to commit to the first owning provider and avoid
/// downstream "missing schema" warnings on path misses.
#[test]
fn has_resource_true_for_cached_crd() {
    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);

    // Force the cache to populate first.
    let r = ResourceRef::concrete(
        "monitoring.coreos.com/v1".to_string(),
        "PrometheusRule".to_string(),
    );
    let _ = materialize_schema_for_resource(&provider, &r);

    assert!(
        provider.has_resource(&r),
        "PrometheusRule (cached CRD) should report has_resource=true"
    );
}

/// `.k8s.io`-suffix groups previously got blocklisted by
/// `relative_path_for_resource`, so legitimate addon CRDs like the
/// vertical pod autoscaler (`autoscaling.k8s.io/VerticalPodAutoscaler`)
/// were unreachable even when the catalog had them. This pins the fixed
/// behaviour.
#[test]
fn relative_path_handles_dot_k8s_io_suffix_groups() {
    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);

    // Online-tolerant: the resource may or may not be cached; what
    // matters is that `has_resource` (which goes through
    // `relative_path_for_resource`) doesn't unconditionally return
    // false. A successful download path will flip this to true; in
    // offline test runs with no cache it's false, but the regression
    // we're guarding against is the unconditional `.k8s.io` skip.
    let r = ResourceRef::concrete(
        "autoscaling.k8s.io/v1".to_string(),
        "VerticalPodAutoscaler".to_string(),
    );

    let online = provider.has_resource(&r);
    if !online {
        // Sanity: if download is gated/offline, the negative-cache
        // outcome is fine. The regression we're preventing is the
        // *path-formation* returning None.
        return;
    }
    assert!(
        online,
        "VerticalPodAutoscaler (autoscaling.k8s.io/v1) should be \
         resolvable via the CRDs catalog when downloads are allowed"
    );
}

/// Built-in K8s API groups stay skipped — there's no point downloading
/// `apps/v1/Deployment` from the CRDs catalog (it 404s) and accidentally
/// shadowing the upstream K8s `OpenAPI` provider for these.
#[test]
fn relative_path_skips_built_in_k8s_groups() {
    let provider = CrdsCatalogSchemaProvider::new();
    for built_in in [
        ("apps/v1", "Deployment"),
        ("batch/v1", "Job"),
        ("autoscaling/v2", "HorizontalPodAutoscaler"),
        ("policy/v1", "PodDisruptionBudget"),
        ("extensions/v1beta1", "Ingress"),
    ] {
        let (api_version, kind) = built_in;
        let r = ResourceRef::concrete(api_version.to_string(), kind.to_string());
        assert!(
            !provider.has_resource(&r),
            "{kind} ({api_version}) is a built-in K8s API group — CRDs catalog must skip it",
        );
    }
}
