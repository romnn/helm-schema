use std::path::PathBuf;

use helm_schema_ir::{ResourceRef, YamlPath};
use helm_schema_k8s::{
    CrdCatalogSchemaProvider, K8sSchemaProvider, UpstreamK8sSchemaProvider, WarningSink,
};
use serde_json::Value;

use crate::error::{CliError, CliResult};

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
    warning_sink: Option<WarningSink>,
) -> CliResult<Box<dyn K8sSchemaProvider>> {
    let mut providers: Vec<Box<dyn K8sSchemaProvider>> = Vec::new();

    if let Some(dir) = &opts.crd_catalog_dir {
        let p = CrdCatalogSchemaProvider::new(dir).ok_or_else(|| CliError::CrdCatalogLoad {
            dir: dir.display().to_string(),
        })?;
        providers.push(Box::new(p));
    }

    if !opts.disable_k8s_schemas {
        let mut upstream = UpstreamK8sSchemaProvider::new(opts.k8s_version.clone())
            .with_allow_download(opts.allow_net);

        if let Some(sink) = warning_sink.clone() {
            upstream = upstream.with_warning_sink(sink);
        }

        if let Some(dir) = &opts.k8s_schema_cache_dir {
            upstream = upstream.with_cache_dir(dir);
        }

        providers.push(Box::new(upstream));
    }

    if providers.is_empty() {
        providers.push(Box::new(NullProvider));
    }

    Ok(Box::new(MultiProvider { providers }))
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
        for p in &self.providers {
            if let Some(v) = p.schema_for_resource_path(resource, path) {
                return Some(v);
            }
        }
        None
    }
}
