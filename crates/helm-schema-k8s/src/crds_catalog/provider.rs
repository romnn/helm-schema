use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use helm_schema_core::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::cache::{
    LayoutCheckOutcome, LayoutChecker, NegativeCache, SourceDocCache, crd_cache_path,
    read_cached_json_doc, write_meta_sidecar,
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

use super::cross_scan::collect_other_versions;
use super::mirror_chain::{CrdMirrorChain, CrdSource};
use super::relative_path::relative_path_for_resource;

/// In-memory cache key: `(source_id, relative_path)`.
type MemKey = (String, String);

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
            .map(|source| {
                format!(
                    "{}/{}",
                    source.base_url.trim_end_matches('/'),
                    relative_path
                )
            })
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
            .map(|s| s.source_id.clone())
            .collect()
    }

    fn try_load_from_source(&self, source: &CrdSource, relative_path: &str) -> Option<SchemaDoc> {
        if let Some(v) = self.read_mem(&source.source_id, relative_path) {
            return Some(v);
        }
        let local = crd_cache_path(&self.cache_dir, &source.source_id, relative_path);
        if local.exists()
            && let Some(doc) = read_cached_json_doc(&local)
        {
            self.write_mem(&source.source_id, relative_path, doc.clone());
            return Some(doc);
        }
        if !self.allow_download {
            return None;
        }
        if self
            .negative_cache
            .contains(&source.source_id, "", relative_path)
        {
            return None;
        }
        let url = format!(
            "{}/{}",
            source.base_url.trim_end_matches('/'),
            relative_path
        );
        match self.fetcher.fetch(&url) {
            Ok(Some(bytes)) => {
                write_atomic(&local, &bytes).ok()?;
                if self.record_source {
                    write_meta_sidecar(&local, &url);
                }
                let doc = SchemaDoc::new(serde_json::from_slice::<Value>(&bytes).ok()?);
                self.write_mem(&source.source_id, relative_path, doc.clone());
                Some(doc)
            }
            Ok(None) => {
                self.negative_cache
                    .record(&source.source_id, "", relative_path);
                None
            }
            Err(_) => None,
        }
    }

    fn read_mem(&self, source_id: &str, relative_path: &str) -> Option<SchemaDoc> {
        self.mem
            .read(&(source_id.to_string(), relative_path.to_string()))
    }

    fn write_mem(&self, source_id: &str, relative_path: &str, doc: SchemaDoc) {
        self.mem
            .write((source_id.to_string(), relative_path.to_string()), doc);
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
    fn schema_fragment_for_resource_path(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        if self.run_layout_check() == LayoutCheckOutcome::ForwardIncompatible {
            return None;
        }
        let loaded = self.load_schema_doc(resource)?;
        self.schema_leaf_for_resource_path_from_doc(&loaded.doc, path)
            .map(|leaf| self.fragment_for_leaf(&loaded, leaf))
    }

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

impl helm_schema_core::ResourceSchemaOracle for CrdsCatalogSchemaProvider {
    fn schema_fragment_for_use(
        &self,
        use_: &helm_schema_core::ProviderSchemaUse,
    ) -> Option<helm_schema_core::ProviderSchemaFragment> {
        <Self as K8sSchemaProvider>::schema_fragment_for_use(self, use_)
    }

    fn schema_fragment_for_resource_path(
        &self,
        resource: &helm_schema_core::ResourceRef,
        path: &helm_schema_core::YamlPath,
    ) -> Option<helm_schema_core::ProviderSchemaFragment> {
        <Self as K8sSchemaProvider>::schema_fragment_for_resource_path(self, resource, path)
    }

    fn origin(&self) -> helm_schema_core::ProviderOrigin {
        <Self as K8sSchemaProvider>::origin(self)
    }

    fn has_resource(&self, resource: &helm_schema_core::ResourceRef) -> bool {
        <Self as K8sSchemaProvider>::has_resource(self, resource)
    }
}

fn write_atomic(local: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = local.parent() {
        fs::create_dir_all(parent)?;
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = local.with_extension(format!("json.tmp.{}.{}", std::process::id(), unique));
    {
        let mut f = fs::File::create(&tmp)?;
        std::io::Write::write_all(&mut f, bytes)?;
    }
    match fs::rename(&tmp, local) {
        Ok(()) => Ok(()),
        Err(err) => {
            if local.exists() {
                let _ = fs::remove_file(&tmp);
                Ok(())
            } else {
                Err(err)
            }
        }
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
mod tests {
    use helm_schema_core::{ResourceRef, YamlPath};
    use serde_json::json;

    use super::*;
    use crate::cache::default_source_id;

    fn widget_resource() -> ResourceRef {
        ResourceRef {
            api_version: "example.com/v1".to_string(),
            kind: "Widget".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        }
    }

    #[test]
    fn catalog_lookup_attaches_provider_source() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let cache_dir = std::env::temp_dir().join(format!("helm-schema-crd-source-{unique}"));
        let relative_path = "example.com/widget_v1.json";
        let schema_path = crd_cache_path(&cache_dir, default_source_id(), relative_path);
        std::fs::create_dir_all(
            schema_path
                .parent()
                .expect("schema cache path should have parent"),
        )
        .expect("create crd cache test directory");
        std::fs::write(
            &schema_path,
            serde_json::to_vec(&json!({
                "type": "object",
                "properties": {
                    "spec": {
                        "$ref": "#/definitions/Spec"
                    }
                },
                "definitions": {
                    "Spec": {
                        "type": "object",
                        "properties": {
                            "size": { "type": "integer" }
                        }
                    }
                }
            }))
            .expect("serialize crd cache schema"),
        )
        .expect("write crd cache schema");

        let provider = CrdsCatalogSchemaProvider::new().with_cache_dir(cache_dir);
        let result = provider.lookup(
            &widget_resource(),
            &YamlPath(vec!["spec".to_string(), "size".to_string()]),
        );
        let ProviderLookupResult::Found { schema, .. } = result else {
            panic!("catalog lookup should resolve spec.size");
        };
        let source = schema.source().expect("catalog source should attach");

        assert_eq!(source.origin(), ProviderOrigin::DefaultCatalog);
        assert_eq!(source.source_id(), default_source_id());
        assert_eq!(source.filename(), relative_path);
        assert_eq!(source.pointer(), "/definitions/Spec/properties/size");
    }
}
