use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use helm_schema_core::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::cache::{
    LayoutCheckOutcome, LayoutChecker, NegativeCache, SourceDocCache, crd_cache_path,
};
use crate::diagnostic::{Diagnostic, DiagnosticSink};
use crate::doc_backed_schema::{
    LocalSchemaLeaf, debug_materialize_local_schema,
    descend_schema_path_expanding_leaf_with_root_metadata_source, fragment_for_source_leaf,
};
use crate::fetch::{HttpFetcher, UreqFetcher};
use crate::inference::cache_scan::scan_crd_cache;
use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::lookup::{
    K8sSchemaProvider, ProviderLookupResult, ProviderOrigin, ProviderSchemaFragment,
    ProviderSchemaSource,
};
use crate::schema_doc::SchemaDoc;
use crate::source_cache::{
    AuthoritativeAbsence, CachedSchemaDocRequest, load_source_schema_doc, source_url,
};

use super::cross_scan::collect_other_versions;
use super::mirror_chain::{CrdMirrorChain, CrdSource};
use super::relative_path::relative_path_for_resource;

/// In-memory cache key: `(source_id, relative_path)`.
type MemKey = (String, String);

fn mem_key(source_id: &str, relative_path: &str) -> MemKey {
    (source_id.to_string(), relative_path.to_string())
}

#[derive(Debug)]
pub struct CrdsCatalogSchemaProvider {
    pub mirrors: CrdMirrorChain,
    pub cache_dir: PathBuf,
    pub allow_download: bool,
    pub loose: bool,
    pub allow_api_version_guess: bool,
    pub record_source: bool,

    fetcher: Arc<dyn HttpFetcher>,
    negative_cache: Arc<NegativeCache>,
    layout_checker: Arc<LayoutChecker>,
    diagnostic_sink: Option<DiagnosticSink>,

    mem: SourceDocCache<MemKey>,
}

impl CrdsCatalogSchemaProvider {
    #[must_use]
    pub fn new() -> Self {
        Self {
            mirrors: CrdMirrorChain::default(),
            cache_dir: default_crd_schema_cache_dir(),
            allow_download: std::env::var("HELM_SCHEMA_ALLOW_NET")
                .ok()
                .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
            loose: false,
            allow_api_version_guess: false,
            record_source: false,
            fetcher: Arc::new(UreqFetcher::new()),
            negative_cache: Arc::new(NegativeCache::new()),
            layout_checker: Arc::new(LayoutChecker::new()),
            diagnostic_sink: None,
            mem: SourceDocCache::new(),
        }
    }

    #[must_use]
    pub fn with_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = dir.into();
        self
    }

    #[must_use]
    pub fn with_allow_download(mut self, allow: bool) -> Self {
        self.allow_download = allow;
        self
    }

    #[must_use]
    pub fn with_mirrors(mut self, mirrors: Vec<String>) -> Self {
        self.mirrors = CrdMirrorChain::with_mirrors(mirrors);
        self
    }

    #[must_use]
    pub fn with_loose(mut self, loose: bool) -> Self {
        self.loose = loose;
        self
    }

    #[must_use]
    pub fn with_fetcher(mut self, fetcher: Arc<dyn HttpFetcher>) -> Self {
        self.fetcher = fetcher;
        self
    }

    #[must_use]
    pub fn with_negative_cache(mut self, negative_cache: Arc<NegativeCache>) -> Self {
        self.negative_cache = negative_cache;
        self
    }

    #[must_use]
    pub fn with_layout_checker(mut self, checker: Arc<LayoutChecker>) -> Self {
        self.layout_checker = checker;
        self
    }

    #[must_use]
    pub fn with_diagnostic_sink(mut self, sink: DiagnosticSink) -> Self {
        self.diagnostic_sink = Some(sink);
        self
    }

    #[must_use]
    pub fn with_api_version_guess(mut self, enabled: bool) -> Self {
        self.allow_api_version_guess = enabled;
        self
    }

    #[must_use]
    pub fn with_record_source(mut self, record: bool) -> Self {
        self.record_source = record;
        self
    }

    fn run_layout_check(&self) -> LayoutCheckOutcome {
        self.layout_checker.check_and_prepare(
            &self.cache_dir,
            self.diagnostic_sink.as_ref(),
            crd_root_has_legacy_layout,
        )
    }

    fn load_schema_doc(&self, resource: &ResourceRef) -> Option<LoadedCrdSchemaDoc> {
        let relative_path = relative_path_for_resource(resource)?;
        let layout = self.run_layout_check();
        if layout == LayoutCheckOutcome::ForwardIncompatible {
            return None;
        }
        for source in &self.mirrors.sources {
            if let Some(doc) = self.try_load_from_source(source, &relative_path) {
                return Some(LoadedCrdSchemaDoc {
                    source_id: source.source_id.clone(),
                    relative_path,
                    doc,
                });
            }
        }
        None
    }

    /// URLs the provider would have probed for `resource`'s
    /// `relative_path`. Reported as the `locations_tried` field of
    /// [`Diagnostic::CrdVersionNotFound`] on final-miss commit by the
    /// chain layer.
    fn locations_tried_for(&self, relative_path: &str) -> Vec<String> {
        self.mirrors
            .sources
            .iter()
            .map(|source| source_url(&source.base_url, relative_path))
            .collect()
    }

    /// Source-id directory names currently configured (`default` plus
    /// any `--crd-catalog-mirror` mirrors). Cache scans MUST consult
    /// only these; on-disk dirs from previously-removed mirrors are
    /// stale and must not influence live inference or cross-version
    /// hints (Finding 2).
    fn configured_source_ids(&self) -> std::collections::HashSet<String> {
        self.mirrors
            .sources
            .iter()
            .map(|source| source.source_id.clone())
            .collect()
    }

    fn try_load_from_source(&self, source: &CrdSource, relative_path: &str) -> Option<SchemaDoc> {
        let local = crd_cache_path(&self.cache_dir, &source.source_id, relative_path);
        let url = source_url(&source.base_url, relative_path);
        load_source_schema_doc(
            CachedSchemaDocRequest {
                local: &local,
                url: &url,
                source_id: &source.source_id,
                cache_namespace: "",
                cache_key: relative_path,
                allow_download: self.allow_download,
                use_cache: true,
                record_source: self.record_source,
                fetcher: self.fetcher.as_ref(),
                negative_cache: &self.negative_cache,
            },
            &self.mem,
            mem_key(&source.source_id, relative_path),
            AuthoritativeAbsence::None,
        )
    }

    fn local_owns_resource(&self, resource: &ResourceRef) -> bool {
        let Some(relative_path) = relative_path_for_resource(resource) else {
            return false;
        };
        for source in &self.mirrors.sources {
            let local = crd_cache_path(&self.cache_dir, &source.source_id, &relative_path);
            if local.exists() {
                return true;
            }
        }
        false
    }

    fn schema_leaf_for_resource_path_from_doc(
        &self,
        root: &SchemaDoc,
        path: &YamlPath,
    ) -> Option<LocalSchemaLeaf> {
        descend_schema_path_expanding_leaf_with_root_metadata_source(root.root(), &path.0)
    }

    fn source_for_leaf(
        &self,
        loaded: &LoadedCrdSchemaDoc,
        leaf: &LocalSchemaLeaf,
    ) -> Option<ProviderSchemaSource> {
        let pointer = leaf.pointer()?;
        Some(ProviderSchemaSource::new(
            ProviderOrigin::DefaultCatalog,
            loaded.source_id.clone(),
            None,
            loaded.relative_path.clone(),
            pointer.to_string(),
        ))
    }

    fn fragment_for_leaf(
        &self,
        loaded: &LoadedCrdSchemaDoc,
        leaf: LocalSchemaLeaf,
    ) -> ProviderSchemaFragment {
        fragment_for_source_leaf(&loaded.doc, self.source_for_leaf(loaded, &leaf), leaf)
    }
}

struct LoadedCrdSchemaDoc {
    source_id: String,
    relative_path: String,
    doc: SchemaDoc,
}

impl Default for CrdsCatalogSchemaProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Expand the full catalog document for regression tests and debugging.
///
/// Production provider lookup stays on the fragment-first path.
#[must_use]
pub fn debug_materialize_schema_for_resource(
    provider: &CrdsCatalogSchemaProvider,
    resource: &ResourceRef,
) -> Option<Value> {
    let loaded = provider.load_schema_doc(resource)?;
    Some(debug_materialize_local_schema(loaded.doc.root()))
}

impl K8sSchemaProvider for CrdsCatalogSchemaProvider {
    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::DefaultCatalog
    }

    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        if self.run_layout_check() == LayoutCheckOutcome::ForwardIncompatible {
            return ProviderLookupResult::NotOwned;
        }
        let Some(loaded) = self.load_schema_doc(resource) else {
            return ProviderLookupResult::NotOwned;
        };
        match self.schema_leaf_for_resource_path_from_doc(&loaded.doc, path) {
            Some(leaf) => ProviderLookupResult::Found {
                schema: self.fragment_for_leaf(&loaded, leaf),
                resolved_k8s_version: None,
            },
            None => ProviderLookupResult::PathUnresolved,
        }
    }

    fn has_resource(&self, resource: &ResourceRef) -> bool {
        self.local_owns_resource(resource)
    }

    fn missing_schema_provider_diagnostics(&self, resource: &ResourceRef) -> Vec<Diagnostic> {
        let Some(relative_path) = relative_path_for_resource(resource) else {
            return Vec::new();
        };
        let Some((group, requested_version)) = resource.api_version.split_once('/') else {
            return Vec::new();
        };
        let mut out: Vec<Diagnostic> = Vec::new();
        out.push(Diagnostic::CrdVersionNotFound {
            group: group.to_string(),
            kind: resource.kind.clone(),
            requested_version: requested_version.to_string(),
            locations_tried: self.locations_tried_for(&relative_path),
        });
        if self.loose {
            let other_versions = collect_other_versions(
                &self.cache_dir,
                resource,
                requested_version,
                &self.configured_source_ids(),
            );
            if !other_versions.is_empty() {
                out.push(Diagnostic::CrdVersionAvailableAtOtherVersions {
                    group: group.to_string(),
                    kind: resource.kind.clone(),
                    requested_version: requested_version.to_string(),
                    available_versions: other_versions,
                });
            }
        }
        out
    }

    fn infer_api_version_candidates(&self, kind: &str) -> Vec<ApiVersionCandidate> {
        if !self.allow_api_version_guess {
            return Vec::new();
        }
        let mut out = scan_crd_cache(
            &self.cache_dir,
            kind,
            ProviderOrigin::DefaultCatalog,
            &self.configured_source_ids(),
        );
        // Tier 3 online probe.
        if self.allow_download {
            for source in &self.mirrors.sources {
                out.extend(crate::inference::online_probe::probe_crd_catalog(
                    &self.fetcher,
                    &source.base_url,
                    kind,
                ));
            }
        }
        // Stamp source as Shortlist if the shortlist owns the kind so
        // the aggregator's `Shortlist > Cache > Online` priority is
        // applied uniformly.
        if let Some(api_version) = crate::inference::shortlist::canonical_api_version_for_kind(kind)
        {
            out.push(ApiVersionCandidate {
                api_version: api_version.to_string(),
                source: InferenceSource::Shortlist,
                origin: ProviderOrigin::DefaultCatalog,
            });
        }
        out
    }
}

fn crd_root_has_legacy_layout(root: &Path) -> bool {
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == crate::cache::LAYOUT_MARKER_FILENAME {
            continue;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Legacy layout: group dirs (e.g. `monitoring.coreos.com`) sit
        // directly under the root. The new layout always has a
        // `<source_id>` directory in between.
        if name.contains('.') {
            return true;
        }
    }
    false
}

fn default_crd_schema_cache_dir() -> PathBuf {
    if let Ok(p) = std::env::var("HELM_SCHEMA_CRD_SCHEMA_CACHE") {
        return PathBuf::from(p);
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("helm-schema").join("crds-catalog");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("helm-schema")
            .join("crds-catalog");
    }
    PathBuf::from(".cache")
        .join("helm-schema")
        .join("crds-catalog")
}

#[cfg(test)]
#[path = "tests/provider.rs"]
mod tests;
