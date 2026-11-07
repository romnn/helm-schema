//! A bundle's missing per-kind document proves absence only against its release's API catalog.

use std::sync::Arc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{ApiPresenceQuery, ResourceRef};
use helm_schema_k8s::{K8sSchemaProvider as _, KubernetesJsonSchemaProvider};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

/// Shared provider fixtures for K8s integration tests.
pub mod common;
use common::{MockFetcher, bundled_k8s_provider};

const UPSTREAM: &str = "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master";

fn has(provider: &KubernetesJsonSchemaProvider, api: &str) -> eyre::Result<Option<bool>> {
    let query =
        ApiPresenceQuery::parse_helm_literal(api).ok_or_eyre("invalid capability literal")?;
    Ok(provider.capability_has_query_at_primary_version(&query))
}

fn answers(provider: &KubernetesJsonSchemaProvider) -> eyre::Result<Value> {
    let crd = ResourceRef::concrete(
        "apiextensions.k8s.io/v1".to_string(),
        "CustomResourceDefinition".to_string(),
    );
    let cron_job = ResourceRef::concrete("batch/v1beta1".to_string(), "CronJob".to_string());
    Ok(json!({
        "crd_group_version": has(provider, "apiextensions.k8s.io/v1")?,
        "crd_kind": has(provider, "apiextensions.k8s.io/v1/CustomResourceDefinition")?,
        "crd_served": provider.resource_is_served_at_primary_version(&crd),
        "retired_group_version": has(provider, "batch/v1beta1")?,
        "retired_served": provider.resource_is_served_at_primary_version(&cron_job),
        "meta_type": has(provider, "batch/v1beta1/DeleteOptions")?,
        "alpha_root": has(provider, "internal.apiserver.k8s.io/v1alpha1/StorageVersion")?,
    }))
}

/// The standalone-strict bundle has no `CustomResourceDefinition` document because that schema
/// refers to itself, yet Kubernetes v1.29 serves `apiextensions.k8s.io/v1` and Helm's default
/// capabilities report it. The vendored bundle records the upstream miss as a negative entry; the
/// release catalog declares the kind, so the miss proves nothing. Unprobed kinds stay unknown.
#[test]
fn the_vendored_standalone_bundle_leaves_custom_resource_definitions_undecided() -> eyre::Result<()>
{
    let provider = bundled_k8s_provider("v1.29.0-standalone-strict");
    sim_assert_eq!(
        have: answers(&provider)?,
        want: json!({
            "crd_group_version": null,
            "crd_kind": null,
            "crd_served": null,
            "retired_group_version": false,
            "retired_served": false,
            "meta_type": null,
            "alpha_root": null,
        })
    );
    Ok(())
}

/// Online, a variant bundle's miss is checked against the release catalog that the plain release
/// directory publishes. Only a kind no single-root definition declares is absent: a declared kind
/// (a served CRD, an alpha root that is off by default) and a miss without a catalog stay unknown.
#[test]
fn a_bundle_miss_is_absence_only_when_no_single_root_declares_the_kind() -> eyre::Result<()> {
    let catalog = json!({"definitions": {
        "io.k8s.apiextensions-apiserver.pkg.apis.apiextensions.v1.CustomResourceDefinition": {
            "x-kubernetes-group-version-kind": [
                {"group": "apiextensions.k8s.io", "kind": "CustomResourceDefinition", "version": "v1"},
            ],
        },
        "io.k8s.api.apiserverinternal.v1alpha1.StorageVersion": {
            "x-kubernetes-group-version-kind": [
                {"group": "internal.apiserver.k8s.io", "kind": "StorageVersion", "version": "v1alpha1"},
            ],
        },
        // Generic meta types list retired group/versions without declaring a resource there.
        "io.k8s.apimachinery.pkg.apis.meta.v1.DeleteOptions": {
            "x-kubernetes-group-version-kind": [
                {"group": "batch", "kind": "DeleteOptions", "version": "v1beta1"},
                {"group": "extensions", "kind": "DeleteOptions", "version": "v1beta1"},
            ],
        },
    }});
    let catalog_url = format!("{UPSTREAM}/v1.29.0/_definitions.json");
    let with_catalog =
        Arc::new(MockFetcher::new().with_body(catalog_url.clone(), serde_json::to_vec(&catalog)?));
    let cache = ScratchDir::new("capability-release-catalog")?;
    let online = KubernetesJsonSchemaProvider::new("v1.29.0-standalone-strict")
        .with_cache_dir(cache.path())
        .with_allow_download(true)
        .with_fetcher(with_catalog.clone());
    let without_catalog_cache = ScratchDir::new("capability-release-no-catalog")?;
    let without_catalog = KubernetesJsonSchemaProvider::new("v1.29.0-standalone-strict")
        .with_cache_dir(without_catalog_cache.path())
        .with_allow_download(true)
        .with_fetcher(Arc::new(MockFetcher::new()));
    let offline = KubernetesJsonSchemaProvider::new("v1.29.0-standalone-strict")
        .with_cache_dir(cache.path())
        .with_allow_download(false);
    sim_assert_eq!(
        have: json!({
            "online": answers(&online)?,
            "catalog_fetches": with_catalog.calls_for(&catalog_url),
            "offline_after_online": answers(&offline)?,
            "without_catalog": answers(&without_catalog)?,
        }),
        want: json!({
            "online": {
                "crd_group_version": null,
                "crd_kind": null,
                "crd_served": null,
                "retired_group_version": false,
                "retired_served": false,
                "meta_type": false,
                "alpha_root": null,
            },
            "catalog_fetches": 1,
            "offline_after_online": {
                "crd_group_version": null,
                "crd_kind": null,
                "crd_served": null,
                "retired_group_version": false,
                "retired_served": false,
                "meta_type": false,
                "alpha_root": null,
            },
            "without_catalog": {
                "crd_group_version": null,
                "crd_kind": null,
                "crd_served": null,
                "retired_group_version": null,
                "retired_served": null,
                "meta_type": null,
                "alpha_root": null,
            },
        })
    );
    Ok(())
}
