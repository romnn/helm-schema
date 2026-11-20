//! `release_definition` resolves a resource version-first over the chain and names the release
//! that answered: the resource's document first, else the release catalog's single-root
//! declaration (ADR 0032's lookup-only fallback).

use std::fs;
use std::path::Path;

use color_eyre::eyre;
use helm_schema_core::ResourceRef;
use helm_schema_k8s::cache::k8s_cache_path;
use helm_schema_k8s::{
    K8sVersionChain, KubernetesJsonSchemaProvider, ReleaseDefinition, ReleaseSchemaSource,
    default_source_id,
};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

/// Shared provider fixtures for Kubernetes integration tests.
pub mod common;

const PRIMARY: &str = "v1.29.0-standalone-strict";

/// The validator's provider: the vendored bundle offline, `PRIMARY` plus the auto window.
fn bundled_chain_provider() -> KubernetesJsonSchemaProvider {
    KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
        vec![PRIMARY.to_string()],
        Some(K8sVersionChain::AUTO_FALLBACK_WINDOW),
    ))
    .with_cache_dir(
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
    )
    .with_allow_download(false)
}

/// The answering release and what answered there, as comparable JSON.
fn answer(provider: &KubernetesJsonSchemaProvider, api_version: &str, kind: &str) -> Value {
    let resource = ResourceRef::concrete(api_version.to_string(), kind.to_string());
    match provider.release_definition(&resource) {
        None => Value::Null,
        Some(ReleaseDefinition {
            release,
            source: ReleaseSchemaSource::Document(fragment),
        }) => json!({
            "release": release,
            "document": fragment.source().map(|source| {
                json!([source.version(), source.filename(), source.pointer()])
            }),
        }),
        Some(ReleaseDefinition {
            release,
            source: ReleaseSchemaSource::Catalog { definitions, .. },
        }) => json!({"release": release, "catalog": definitions}),
    }
}

#[test]
fn the_bundle_answers_each_resource_at_the_first_release_that_defines_it() {
    let provider = bundled_chain_provider();
    sim_assert_eq!(
        have: json!([
            answer(&provider, "apiregistration.k8s.io/v1", "APIService"),
            answer(&provider, "apiextensions.k8s.io/v1", "CustomResourceDefinition"),
            answer(&provider, "apiregistration.k8s.io/v1beta1", "APIService"),
            answer(&provider, "apps/v1", "Undeclared"),
        ]),
        want: json!([
            {
                "release": PRIMARY,
                "document": [PRIMARY, "apiservice-apiregistration-v1.json", ""],
            },
            // The standalone bundle drops the self-referencing CRD; its release catalog declares it.
            {
                "release": PRIMARY,
                "catalog": [
                    "io.k8s.apiextensions-apiserver.pkg.apis.apiextensions.v1.CustomResourceDefinition",
                ],
            },
            // Removed in v1.22; v1.15.0 is the newest snapshot in the bundle that declares it.
            {
                "release": "v1.15.0",
                "catalog": ["io.k8s.kube-aggregator.pkg.apis.apiregistration.v1beta1.APIService"],
            },
            null,
        ]),
    );
}

fn write_json(path: &Path, value: &Value) -> eyre::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| eyre::eyre!("{} has no parent", path.display()))?;
    fs::create_dir_all(parent)?;
    fs::write(path, serde_json::to_vec(value)?)?;
    Ok(())
}

fn widget_catalog() -> Value {
    json!({"definitions": {"io.example.Widget": {
        "type": "object",
        "x-kubernetes-group-version-kind": [{"group": "apps", "kind": "Widget", "version": "v1"}],
    }}})
}

/// Version-first: an earlier release's catalog declaration beats a later release's document,
/// and within one release the document beats the catalog.
#[test]
fn an_earlier_release_answers_before_a_later_one_and_its_document_before_its_catalog()
-> eyre::Result<()> {
    let cache = ScratchDir::new("release-definition")?;
    let path = |version: &str, filename: &str| {
        k8s_cache_path(cache.path(), default_source_id(), version, filename)
    };
    let document = json!({"type": "object", "properties": {"kind": {"enum": ["Widget"]}}});
    write_json(&path("v1.20.0", "_definitions.json"), &widget_catalog())?;
    write_json(&path("v1.19.0", "widget-apps-v1.json"), &document)?;
    let provider = |primary: &str| {
        KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
            vec![primary.to_string()],
            Some(2),
        ))
        .with_cache_dir(cache.path())
        .with_allow_download(false)
    };
    let primary_catalog = answer(&provider("v1.20.0"), "apps/v1", "Widget");
    let fallback_catalog = answer(&provider("v1.21.0"), "apps/v1", "Widget");
    write_json(&path("v1.20.0", "widget-apps-v1.json"), &document)?;
    let primary_document = answer(&provider("v1.20.0"), "apps/v1", "Widget");
    sim_assert_eq!(
        have: json!([primary_catalog, fallback_catalog, primary_document]),
        want: json!([
            {"release": "v1.20.0", "catalog": ["io.example.Widget"]},
            {"release": "v1.20.0", "catalog": ["io.example.Widget"]},
            {"release": "v1.20.0", "document": ["v1.20.0", "widget-apps-v1.json", ""]},
        ]),
    );
    Ok(())
}

#[test]
fn release_definition_abstains_for_download_enabled_providers() -> eyre::Result<()> {
    use std::sync::Arc;

    use common::{MockFetcher, MockResponse};

    let cache = ScratchDir::new("online-release-definition")?;
    let path = |version: &str| {
        k8s_cache_path(
            cache.path(),
            default_source_id(),
            version,
            "widget-apps-v1.json",
        )
    };
    fs::write(
        cache
            .path()
            .join(helm_schema_k8s::cache::LAYOUT_MARKER_FILENAME),
        helm_schema_k8s::cache::CACHE_LAYOUT_VERSION.to_string(),
    )?;
    let document = json!({"type": "object", "properties": {"kind": {"enum": ["Widget"]}}});
    write_json(&path("v1.19.0"), &document)?;
    let fetcher =
        Arc::new(MockFetcher::new().with_default(MockResponse::Transport("unavailable".into())));
    let provider = || {
        KubernetesJsonSchemaProvider::with_versions(K8sVersionChain::new(
            vec!["v1.20.0".into(), "v1.19.0".into()],
            None,
        ))
        .with_cache_dir(cache.path())
        .with_allow_download(true)
        .with_fetcher(fetcher.clone())
    };
    let cold = answer(&provider(), "apps/v1", "Widget");
    write_json(&path("v1.20.0"), &document)?;
    let warm = answer(&provider(), "apps/v1", "Widget");
    sim_assert_eq!(have: [cold, warm], want: [Value::Null, Value::Null]);
    sim_assert_eq!(have: fetcher.total_calls(), want: 0);
    Ok(())
}
