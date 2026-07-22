use std::path::PathBuf;
use std::sync::Arc;

use helm_schema_k8s::{
    Chain, ChartLocalCrdSchemaProvider, CrdsCatalogSchemaProvider, DiagnosticSink,
    K8sSchemaProvider, K8sVersionChain, KubernetesJsonSchemaProvider, LayoutChecker,
    LocalSchemaProvider, LocalSchemaUniverse, NegativeCache,
};
use tracing::instrument;

/// Options consumed by `build_provider`. Comes from the CLI surface or
/// from library callers.
#[derive(Debug, Clone, Default)]
pub struct ProviderOptions {
    /// User-ordered K8s version list; the first entry is the primary.
    pub k8s_versions: Vec<String>,
    /// Auto-fallback window of older K8s minors. `None` = disabled.
    pub k8s_version_fallback_window: Option<u32>,
    /// Additional K8s schema mirror URLs.
    pub k8s_schema_mirrors: Vec<String>,
    /// Managed K8s cache root.
    pub k8s_schema_cache_dir: Option<PathBuf>,
    /// Bypass persistent K8s cache reads while refreshing cache writes.
    pub no_cache: bool,

    /// Whether provider cache misses may access upstream sources.
    pub allow_net: bool,
    /// Whether versioned `Kubernetes OpenAPI` providers are omitted.
    pub disable_k8s_schemas: bool,

    /// `crd_lookup_loose=true` activates the cross-version CRD cache
    /// scan + `CrdVersionAvailableAtOtherVersions` hint.
    pub crd_lookup_loose: bool,
    /// Additional CRD catalog mirror URLs.
    pub crd_catalog_mirrors: Vec<String>,
    /// Managed CRD cache root.
    pub crd_catalog_cache_dir: Option<PathBuf>,
    /// Hand-maintained CRD override root.
    pub crd_override_dir: Option<PathBuf>,
    /// Chart-local schema universe built from sources such as static `crds/`.
    pub local_schema_universe: LocalSchemaUniverse,
    /// Write `.meta` sidecars next to CRD cache entries.
    pub crd_cache_record_source: bool,

    /// Enable Feature D apiVersion inference.
    pub api_version_guess: bool,
}

#[instrument(skip_all)]
pub fn build_provider(opts: &ProviderOptions, diagnostic_sink: Option<&DiagnosticSink>) -> Chain {
    let mut providers: Vec<Box<dyn K8sSchemaProvider>> = Vec::new();
    let negative_cache = Arc::new(NegativeCache::new());
    let layout_checker = Arc::new(LayoutChecker::new());

    if let Some(dir) = &opts.crd_override_dir {
        providers.push(Box::new(
            LocalSchemaProvider::new(dir.clone()).with_api_version_guess(opts.api_version_guess),
        ));
    }

    let chart_local_provider = ChartLocalCrdSchemaProvider::new(opts.local_schema_universe.clone())
        .with_api_version_guess(opts.api_version_guess);
    if !chart_local_provider.is_empty() {
        providers.push(Box::new(chart_local_provider));
    }

    let mut crds_catalog = CrdsCatalogSchemaProvider::new()
        .with_allow_download(opts.allow_net)
        .with_mirrors(opts.crd_catalog_mirrors.clone())
        .with_loose(opts.crd_lookup_loose)
        .with_api_version_guess(opts.api_version_guess)
        .with_negative_cache(Arc::clone(&negative_cache))
        .with_layout_checker(Arc::clone(&layout_checker))
        .with_record_source(opts.crd_cache_record_source);
    if let Some(dir) = &opts.crd_catalog_cache_dir {
        crds_catalog = crds_catalog.with_cache_dir(dir.clone());
    }
    if let Some(sink) = diagnostic_sink {
        crds_catalog = crds_catalog.with_diagnostic_sink(sink.clone());
    }
    providers.push(Box::new(crds_catalog));

    if !opts.disable_k8s_schemas {
        let versions =
            K8sVersionChain::new(opts.k8s_versions.clone(), opts.k8s_version_fallback_window);
        let mut k8s = KubernetesJsonSchemaProvider::with_versions(versions)
            .with_allow_download(opts.allow_net)
            .with_use_cache(!opts.no_cache)
            .with_mirrors(opts.k8s_schema_mirrors.clone())
            .with_api_version_guess(opts.api_version_guess)
            .with_negative_cache(Arc::clone(&negative_cache))
            .with_layout_checker(Arc::clone(&layout_checker));
        if let Some(dir) = &opts.k8s_schema_cache_dir {
            k8s = k8s.with_cache_dir(dir.clone());
        }
        if let Some(sink) = diagnostic_sink {
            k8s = k8s.with_diagnostic_sink(sink.clone());
        }
        providers.push(Box::new(k8s));
    }

    let mut chain = Chain::new(providers).with_inference_enabled(opts.api_version_guess);
    if let Some(sink) = diagnostic_sink {
        chain = chain.with_diagnostic_sink(sink.clone());
    }
    chain
}
