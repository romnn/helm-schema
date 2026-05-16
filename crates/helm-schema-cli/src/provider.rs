use std::path::PathBuf;

use helm_schema_ir::{ResourceRef, YamlPath};
use helm_schema_k8s::{
    CrdsCatalogSchemaProvider, K8sSchemaProvider, KubernetesJsonSchemaProvider,
    LocalSchemaProvider, WarningSink,
};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ProviderOptions {
    pub k8s_version: String,
    pub k8s_schema_cache_dir: Option<PathBuf>,
    pub allow_net: bool,
    pub disable_k8s_schemas: bool,
    pub crd_catalog_dir: Option<PathBuf>,
}

pub fn build_provider(
    opts: &ProviderOptions,
    warning_sink: Option<&WarningSink>,
) -> Box<dyn K8sSchemaProvider> {
    let mut providers: Vec<Box<dyn K8sSchemaProvider>> = Vec::new();

    if let Some(dir) = &opts.crd_catalog_dir {
        providers.push(Box::new(LocalSchemaProvider::new(dir)));
    }

    let mut crds_catalog = CrdsCatalogSchemaProvider::new().with_allow_download(opts.allow_net);
    if let Some(dir) = &opts.crd_catalog_dir {
        crds_catalog = crds_catalog.with_cache_dir(dir.clone());
    }
    providers.push(Box::new(crds_catalog));

    if !opts.disable_k8s_schemas {
        let mut upstream = KubernetesJsonSchemaProvider::new(opts.k8s_version.clone())
            .with_allow_download(opts.allow_net);

        if let Some(sink) = warning_sink {
            upstream = upstream.with_warning_sink(sink.clone());
        }

        if let Some(dir) = &opts.k8s_schema_cache_dir {
            upstream = upstream.with_cache_dir(dir);
        }

        providers.push(Box::new(upstream));
    }

    if providers.is_empty() {
        providers.push(Box::new(NullProvider));
    }

    Box::new(MultiProvider { providers })
}

struct NullProvider;

impl K8sSchemaProvider for NullProvider {
    fn schema_for_resource_path(&self, _resource: &ResourceRef, _path: &YamlPath) -> Option<Value> {
        None
    }
}

struct MultiProvider {
    providers: Vec<Box<dyn K8sSchemaProvider>>,
}

impl K8sSchemaProvider for MultiProvider {
    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        // Commit to the first provider that *owns* the resource (has its
        // schema file), even if it can't resolve this specific path —
        // falling through to the next provider on a path miss would emit
        // misleading "no schema found" warnings from that provider when
        // the resource is actually handled fine here. Path-resolution
        // failures inside an owning provider are silent gaps in coverage,
        // not missing-schema errors.
        for p in &self.providers {
            if p.has_resource(resource) {
                return p.schema_for_resource_path(resource, path);
            }
        }

        // No provider owns the resource. Fall back to the legacy "ask
        // each provider in turn" path so a downstream provider that
        // doesn't implement `has_resource` precisely (or any chain
        // surprises) still gets a chance to answer. The K8s OpenAPI
        // provider's warning fires here, which is the right place for
        // genuinely-unrecognised resources.
        for p in &self.providers {
            if let Some(v) = p.schema_for_resource_path(resource, path) {
                return Some(v);
            }
        }
        None
    }

    fn has_resource(&self, resource: &ResourceRef) -> bool {
        self.providers.iter().any(|p| p.has_resource(resource))
    }
}
