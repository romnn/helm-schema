use std::path::{Path, PathBuf};
use std::sync::Arc;

use helm_schema_core::{ApiPresenceQuery, ResourceRef, YamlPath};
use serde_json::Value;

use crate::cache::{
    LayoutCheckOutcome, LayoutChecker, NegativeCache, SourceDocCache, cache_root_has_legacy_layout,
    default_cache_dir, k8s_cache_path, subdirs,
};
use crate::diagnostic::DiagnosticSink;
use crate::fetch::{HttpFetcher, UreqFetcher};
use crate::filename::candidate_filenames_for_resource;
use crate::inference::cache_scan::scan_k8s_cache;
use crate::inference::shortlist::canonical_api_version_for_kind;
use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::lookup::source_bundle::{SourceBundleNode, bundle_source_definition};
use crate::lookup::{
    K8sSchemaProvider, LookupTrace, ProviderLookupResult, ProviderOrigin, ProviderSchemaFragment,
    ProviderSchemaSource, SourceProbeTraceOutcome, TracedApiPresenceOutcome,
};
use crate::schema_doc::SchemaDoc;
use crate::source_cache::{
    CachedSchemaDocRequest, SourceDocOutcome, allow_download_from_env, load_source_schema_doc,
    probe_source_schema_doc, source_url,
};

use super::capability_probe::build_capability_probe;
use super::resolve_ctx::{ResolveCtx, descend_schema_path_expanding_leaf_with_location};
use super::version_chain::K8sVersionChain;
use crate::mirror_chain::{MirrorChain, SchemaSource};

const K8S_DEFAULT_BASE_URL: &str =
    "https://raw.githubusercontent.com/yannh/kubernetes-json-schema/master";

/// In-memory doc cache key: `(source_id, version_dir, filename)`.
type MemKey = (String, String, String);

fn mem_key(source_id: &str, version: &str, filename: &str) -> MemKey {
    (
        source_id.to_string(),
        version.to_string(),
        filename.to_string(),
    )
}

/// Composer of fetch + cache + lookup primitives for upstream K8s
/// OpenAPI schemas. Carries a [`K8sVersionChain`] and a
/// [`MirrorChain`] and walks the cross product version-first /
/// mirror-first: all configured sources are tried at one version
/// before falling back to the next version.
#[derive(Debug)]
pub struct KubernetesJsonSchemaProvider {
    pub versions: K8sVersionChain,
    pub(crate) mirrors: MirrorChain,
    pub cache_dir: PathBuf,
    pub allow_download: bool,
    pub use_cache: bool,
    pub allow_api_version_guess: bool,
    pub record_source: bool,

    fetcher: Arc<dyn HttpFetcher>,
    negative_cache: Arc<NegativeCache>,
    layout_checker: Arc<LayoutChecker>,
    diagnostic_sink: Option<DiagnosticSink>,

    mem: SourceDocCache<MemKey>,
}

struct LoadedK8sSchemaDoc {
    source: SchemaSource,
    version: String,
    filename: String,
    doc: SchemaDoc,
}

impl KubernetesJsonSchemaProvider {
    /// Convenience for the single-version case (today's most common
    /// shape): one explicit version, default mirror, no fallback.
    pub fn new(version_dir: impl Into<String>) -> Self {
        Self::with_versions(K8sVersionChain::new(vec![version_dir.into()], None))
    }

    #[must_use]
    pub fn with_versions(versions: K8sVersionChain) -> Self {
        Self {
            versions,
            mirrors: MirrorChain::with_mirrors(K8S_DEFAULT_BASE_URL, Vec::new()),
            cache_dir: default_k8s_schema_cache_dir(),
            allow_download: allow_download_from_env(),
            use_cache: true,
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
    pub fn with_use_cache(mut self, use_cache: bool) -> Self {
        self.use_cache = use_cache;
        self
    }

    #[must_use]
    pub fn with_mirrors(mut self, mirrors: Vec<String>) -> Self {
        self.mirrors = MirrorChain::with_mirrors(K8S_DEFAULT_BASE_URL, mirrors);
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

    /// Provider-facing entry point: walk `(version, mirror)` and
    /// return the first source that owns the resource.
    #[tracing::instrument(skip_all, fields(kind = resource.kind.as_str(), api_version = resource.api_version.as_str()))]
    fn load_resource_doc(&self, resource: &ResourceRef) -> Option<LoadedK8sSchemaDoc> {
        if resource.api_version.trim().is_empty() {
            return None;
        }

        // `candidate_filenames_for_resource` always yields at least one
        // filename for a non-empty apiVersion.
        let candidates = candidate_filenames_for_resource(resource);

        let layout = self.run_layout_check();
        if layout == LayoutCheckOutcome::ForwardIncompatible {
            return None;
        }

        for version in self.versions.ordered() {
            for filename in &candidates {
                for source in &self.mirrors.sources {
                    if let Some(doc) = self.try_load_from_source(source, &version, filename) {
                        return Some(LoadedK8sSchemaDoc {
                            source: source.clone(),
                            version: version.clone(),
                            filename: filename.clone(),
                            doc,
                        });
                    }
                }
            }
        }
        None
    }

    fn doc_request<'a>(
        &'a self,
        source: &'a SchemaSource,
        version: &'a str,
        filename: &'a str,
    ) -> CachedSchemaDocRequest<'a> {
        CachedSchemaDocRequest {
            local: k8s_cache_path(&self.cache_dir, &source.source_id, version, filename),
            url: source_url(&source.base_url, &format!("{version}/{filename}")),
            source_id: &source.source_id,
            cache_namespace: version,
            cache_key: filename,
            allow_download: self.allow_download,
            use_cache: self.use_cache,
            record_source: self.record_source,
            use_not_found_marker: true,
            fetcher: self.fetcher.as_ref(),
            negative_cache: &self.negative_cache,
        }
    }

    fn try_load_from_source(
        &self,
        source: &SchemaSource,
        version: &str,
        filename: &str,
    ) -> Option<SchemaDoc> {
        load_source_schema_doc(
            self.doc_request(source, version, filename),
            &self.mem,
            mem_key(&source.source_id, version, filename),
        )
    }

    fn run_layout_check(&self) -> LayoutCheckOutcome {
        self.layout_checker.check_and_prepare(
            &self.cache_dir,
            self.diagnostic_sink.as_ref(),
            k8s_root_has_legacy_layout,
        )
    }

    #[tracing::instrument(skip_all, fields(kind = resource.kind.as_str(), api_version = resource.api_version.as_str(), path_len = path.0.len()))]
    fn schema_fragment_for_resource_path_uncached(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<(String, Option<ProviderSchemaFragment>)> {
        let LoadedK8sSchemaDoc {
            source,
            version,
            filename,
            doc,
        } = self.load_resource_doc(resource)?;
        let mut ctx = ResolveCtx::new(
            |next_filename| self.try_load_from_source(&source, &version, next_filename),
            filename.clone(),
            doc,
        );
        let root_doc = ctx.doc(&filename)?.clone();
        let schema_node = descend_schema_path_expanding_leaf_with_location(
            &mut ctx, &filename, &root_doc, &path.0,
        );
        let fragment = schema_node.map(|schema_node| {
            let source = ProviderSchemaSource::kubernetes_openapi(
                source.source_id.clone(),
                version.clone(),
                schema_node.location().filename(),
                schema_node.location().pointer(),
            );
            let source_schema = schema_node.source_schema().clone();
            let definition_schema =
                bundled_definition_schema_for_source_leaf(&mut ctx, &schema_node);
            ProviderSchemaFragment::new(schema_node.schema().clone())
                .with_required_in_parent(schema_node.required_in_parent())
                .with_source_definition_schema(source, source_schema, definition_schema)
        });
        Some((version, fragment))
    }

    /// Single-probe upstream-first lookup with tri-state outcome. The
    /// per-outcome semantics — the heart of the capability oracle's
    /// offline-safety contract — live on
    /// [`crate::source_cache::SourceDocOutcome`], which this probe
    /// shares with resource lookup.
    fn probe_at(
        &self,
        source: &SchemaSource,
        version: &str,
        filename: &str,
    ) -> SourceProbeTraceOutcome {
        match probe_source_schema_doc(
            self.doc_request(source, version, filename),
            &self.mem,
            mem_key(&source.source_id, version, filename),
        ) {
            SourceDocOutcome::Found(_) => SourceProbeTraceOutcome::Found,
            SourceDocOutcome::AuthoritativelyAbsent => {
                SourceProbeTraceOutcome::AuthoritativelyAbsent
            }
            SourceDocOutcome::Uncertain => SourceProbeTraceOutcome::Uncertain,
        }
    }
}

fn bundled_definition_schema_for_source_leaf<F: FnMut(&str) -> Option<SchemaDoc>>(
    ctx: &mut ResolveCtx<F>,
    schema_leaf: &super::resolve_ctx::ResolvedSchemaLeaf,
) -> Value {
    let source_schema = schema_leaf.source_schema();
    bundle_source_definition(
        schema_leaf.location().filename(),
        schema_leaf.location().pointer(),
        source_schema,
        |current_location, reference| {
            ctx.resolve_ref(&current_location.document, reference)
                .map(|target| {
                    let filename = target.location().filename().to_string();
                    let pointer = target.location().pointer().to_string();
                    SourceBundleNode::new(filename, pointer, target.into_schema())
                })
        },
    )
}

impl K8sSchemaProvider for KubernetesJsonSchemaProvider {
    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::KubernetesOpenApi
    }

    #[tracing::instrument(skip_all, fields(kind = resource.kind.as_str(), api_version = resource.api_version.as_str(), path_len = path.0.len()))]
    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        if self.run_layout_check() == LayoutCheckOutcome::ForwardIncompatible {
            return ProviderLookupResult::NotOwned;
        }
        let Some((resolved_k8s_version, schema)) =
            self.schema_fragment_for_resource_path_uncached(resource, path)
        else {
            return ProviderLookupResult::NotOwned;
        };
        let Some(schema) = schema else {
            return ProviderLookupResult::PathUnresolved;
        };
        let primary = self
            .versions
            .primary()
            .unwrap_or(&resolved_k8s_version)
            .to_string();
        ProviderLookupResult::Found {
            schema,
            resolved_k8s_version: if primary == resolved_k8s_version {
                None
            } else {
                Some(resolved_k8s_version)
            },
        }
    }

    /// Schema for a resource is owned by us if any (version, source)
    /// has its file already cached or recorded as negative-cache; no
    /// fetches issued during ownership probe (PR 0e contract).
    fn has_resource(&self, resource: &ResourceRef) -> bool {
        if !self.use_cache {
            return false;
        }
        if resource.api_version.trim().is_empty() {
            return false;
        }
        let candidates = candidate_filenames_for_resource(resource);
        for version in self.versions.ordered() {
            for filename in &candidates {
                for source in &self.mirrors.sources {
                    let local =
                        k8s_cache_path(&self.cache_dir, &source.source_id, &version, filename);
                    if local.exists() {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn primary_k8s_version(&self) -> Option<&str> {
        self.versions.primary()
    }

    fn k8s_version_chain(&self) -> Option<Vec<String>> {
        Some(self.versions.ordered())
    }

    /// Scan the cache for K8s versions that hold this resource's file
    /// in any CONFIGURED source namespace. Used by the chain layer to
    /// populate `Diagnostic::MissingSchema.available_in_cache_versions`
    /// when the configured chain didn't resolve the resource.
    ///
    /// Stale `<source_id>` dirs left on disk from removed mirrors are
    /// skipped: only currently-configured sources may surface
    /// inference / hint signals.
    ///
    /// Returns a sorted+deduped list of `version_dir` strings.
    fn cache_versions_holding(&self, resource: &ResourceRef) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if resource.api_version.trim().is_empty() {
            return out;
        }
        let candidates = candidate_filenames_for_resource(resource);
        let configured_versions: std::collections::HashSet<String> =
            self.versions.ordered().into_iter().collect();
        let configured_source_ids = self.mirrors.source_ids();
        for (source_id, source_path) in subdirs(&self.cache_dir) {
            if !configured_source_ids.contains(&source_id) {
                continue;
            }
            for (version_name, version_path) in subdirs(&source_path) {
                if configured_versions.contains(&version_name) {
                    // Configured versions were already probed — skip them.
                    continue;
                }
                for filename in &candidates {
                    if version_path.join(filename).exists() {
                        out.push(version_name.clone());
                        break;
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// Authoritative answer to `.Capabilities.APIVersions.Has "api"`
    /// against the primary K8s version. Upstream-first: probes the
    /// local cache, falling back to a real fetch if the file is
    /// absent and downloads are enabled.
    ///
    /// `api` is the literal Helm argument: `group/version` or
    /// `group/version/Kind` or `version` (core API).
    ///
    /// Returns:
    ///   - `Some(true)` — probe was positively found (in-mem cache hit,
    ///     disk cache hit, or successful upstream fetch).
    ///   - `Some(false)` — probe is authoritatively absent (the
    ///     upstream fetcher reported "not found", which recorded a
    ///     negative-cache entry). Includes negative-cache hits from a
    ///     prior online run even when downloads are now disabled —
    ///     those represent a confirmed past 404.
    ///   - `None` — uncertain. We can't reach a conclusion either way:
    ///     no primary version configured, unknown probe target (no
    ///     canonical kind for this 2-part api version), downloads
    ///     disabled with the probe absent from both the local cache
    ///     AND the negative cache (offline + the probe was never
    ///     previously attempted), or a network error during fetch.
    ///
    /// The branch selector treats `None` as "potentially live" so
    /// uncertainty never silently drops a branch — the cache
    /// completeness of a partial offline run doesn't get to vote on
    /// what the chart would emit.
    fn capability_has_query_at_primary_version(&self, query: &ApiPresenceQuery) -> Option<bool> {
        self.capability_has_query_at_primary_version_traced(query)
            .answer
    }

    /// Traced form of
    /// [`K8sSchemaProvider::capability_has_query_at_primary_version`].
    fn capability_has_query_at_primary_version_traced(
        &self,
        query: &ApiPresenceQuery,
    ) -> TracedApiPresenceOutcome {
        let mut trace = LookupTrace::default();
        let Some(primary) = self.versions.primary() else {
            trace.record_api_presence_provider(ProviderOrigin::KubernetesOpenApi, None);
            return TracedApiPresenceOutcome {
                answer: None,
                trace,
            };
        };
        let Some(probe) = build_capability_probe(query) else {
            trace.record_api_presence_provider(ProviderOrigin::KubernetesOpenApi, None);
            return TracedApiPresenceOutcome {
                answer: None,
                trace,
            };
        };
        let candidates = candidate_filenames_for_resource(&probe);
        // Aggregate the outcome across (source × filename) pairs.
        // Found-ness short-circuits to `Some(true)`. Without that, the
        // worst case across all probes decides — any uncertain probe
        // beats an authoritative absent, since one source might have
        // it even if the other authoritatively doesn't.
        let mut worst = SourceProbeTraceOutcome::AuthoritativelyAbsent;
        for filename in &candidates {
            for source in &self.mirrors.sources {
                let outcome = self.probe_at(source, primary, filename);
                trace.record_api_presence_source_probe(
                    ProviderOrigin::KubernetesOpenApi,
                    &source.source_id,
                    primary,
                    filename,
                    outcome,
                );
                match outcome {
                    SourceProbeTraceOutcome::Found => {
                        trace.record_api_presence_provider(
                            ProviderOrigin::KubernetesOpenApi,
                            Some(true),
                        );
                        return TracedApiPresenceOutcome {
                            answer: Some(true),
                            trace,
                        };
                    }
                    SourceProbeTraceOutcome::Uncertain => {
                        worst = SourceProbeTraceOutcome::Uncertain;
                    }
                    SourceProbeTraceOutcome::AuthoritativelyAbsent => {
                        // worst stays at AuthoritativelyAbsent unless
                        // some other probe already set it to Uncertain.
                    }
                }
            }
        }
        let answer = match worst {
            SourceProbeTraceOutcome::Found => unreachable!("Found short-circuits above"),
            SourceProbeTraceOutcome::AuthoritativelyAbsent => Some(false),
            SourceProbeTraceOutcome::Uncertain => None,
        };
        trace.record_api_presence_provider(ProviderOrigin::KubernetesOpenApi, answer);
        TracedApiPresenceOutcome { answer, trace }
    }

    fn infer_api_version_candidates(&self, kind: &str) -> Vec<ApiVersionCandidate> {
        if !self.allow_api_version_guess {
            return Vec::new();
        }
        let mut out: Vec<ApiVersionCandidate> = Vec::new();
        if let Some(api_version) = canonical_api_version_for_kind(kind) {
            out.push(ApiVersionCandidate {
                api_version: api_version.to_string(),
                source: InferenceSource::Shortlist,
                origin: ProviderOrigin::KubernetesOpenApi,
            });
        }
        let inference_versions: std::collections::HashSet<String> = self
            .versions
            .inference_scan_versions()
            .into_iter()
            .collect();
        out.extend(scan_k8s_cache(
            &self.cache_dir,
            kind,
            &self.mirrors.source_ids(),
            &inference_versions,
        ));
        out
    }
}

/// True if the cache root contains a "legacy" K8s layout (i.e. version
/// dirs sitting directly under the root, no `<source_id>` layer).
fn k8s_root_has_legacy_layout(root: &Path) -> bool {
    // A directory named like `v1.35.0` (legacy) vs a per-source dir
    // (`default`, `<hash>`). The legacy layout puts version dirs at
    // the top.
    cache_root_has_legacy_layout(root, looks_like_k8s_version_dir)
}

fn looks_like_k8s_version_dir(name: &str) -> bool {
    let s = name.trim_start_matches('v');
    let mut parts = s.split('.');
    let Some(major) = parts.next() else {
        return false;
    };
    let Some(minor) = parts.next() else {
        return false;
    };
    major.chars().all(|c| c.is_ascii_digit()) && minor.chars().all(|c| c.is_ascii_digit())
}

fn default_k8s_schema_cache_dir() -> PathBuf {
    default_cache_dir("HELM_SCHEMA_K8S_SCHEMA_CACHE", "kubernetes-json-schema")
}
