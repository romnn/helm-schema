use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use helm_schema_core::{ApiPresenceQuery, ResourceRef, YamlPath};
use serde_json::Value;

use crate::cache::{
    LayoutCheckOutcome, LayoutChecker, NegativeCache, SourceDocCache, cache_root_has_legacy_layout,
    default_cache_dir, k8s_cache_path, not_found_marker_exists,
};
use crate::diagnostic::DiagnosticSink;
use crate::fetch::{HttpFetcher, UreqFetcher};
use crate::filename::{candidate_filenames_for_resource, filename_for_resource};
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
    AuthoritativeAbsence, CachedSchemaDocRequest, load_source_schema_doc, source_url,
};

use super::capability_probe::DEFAULT_CAPABILITY_PROBE_TABLE;
use super::mirror_chain::{K8sMirrorChain, K8sSource};
use super::resolve_ctx::{ResolveCtx, descend_schema_path_expanding_leaf_with_location};
use super::version_chain::K8sVersionChain;

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
/// OpenAPI schemas. Carries a [`K8sVersionChain`] (Feature B) and a
/// [`K8sMirrorChain`] (Feature B+) and walks the cross product
/// version-first / mirror-first per the design in
/// `plan/helm-schema/multi-version-k8s-and-apiversion-guessing.md`.
#[derive(Debug)]
pub struct KubernetesJsonSchemaProvider {
    pub versions: K8sVersionChain,
    pub mirrors: K8sMirrorChain,
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

/// Tri-state outcome for the capability-oracle probe path. See
/// [`KubernetesJsonSchemaProvider::probe_at`] for the per-source
/// semantics and [`KubernetesJsonSchemaProvider::capability_has_query_at_primary_version`]
/// for how the per-source outcomes aggregate into the final
/// `Option<bool>` answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeOutcome {
    Found,
    AuthoritativelyAbsent,
    Uncertain,
}

struct LoadedK8sSchemaDoc {
    source: K8sSource,
    version: String,
    filename: String,
    doc: SchemaDoc,
}

impl From<ProbeOutcome> for SourceProbeTraceOutcome {
    fn from(outcome: ProbeOutcome) -> Self {
        match outcome {
            ProbeOutcome::Found => Self::Found,
            ProbeOutcome::AuthoritativelyAbsent => Self::AuthoritativelyAbsent,
            ProbeOutcome::Uncertain => Self::Uncertain,
        }
    }
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
            mirrors: K8sMirrorChain::default(),
            cache_dir: default_k8s_schema_cache_dir(),
            allow_download: std::env::var("HELM_SCHEMA_ALLOW_NET")
                .ok()
                .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
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
        self.mirrors = K8sMirrorChain::with_mirrors(mirrors);
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

        let mut candidates = candidate_filenames_for_resource(resource);
        if candidates.is_empty() {
            candidates.push(filename_for_resource(resource));
        }

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

    fn try_load_from_source(
        &self,
        source: &K8sSource,
        version: &str,
        filename: &str,
    ) -> Option<SchemaDoc> {
        let local = k8s_cache_path(&self.cache_dir, &source.source_id, version, filename);
        let url = source_url(&source.base_url, &format!("{version}/{filename}"));
        load_source_schema_doc(
            CachedSchemaDocRequest {
                local: &local,
                url: &url,
                source_id: &source.source_id,
                cache_namespace: version,
                cache_key: filename,
                allow_download: self.allow_download,
                use_cache: self.use_cache,
                record_source: self.record_source,
                fetcher: self.fetcher.as_ref(),
                negative_cache: &self.negative_cache,
            },
            &self.mem,
            mem_key(&source.source_id, version, filename),
            AuthoritativeAbsence::MarkerPath(&local),
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
        let fragment =
            schema_node.map(|schema_node| {
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
                    .with_source_definition_schema(source, source_schema, definition_schema)
            });
        Some((version, fragment))
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
    #[must_use]
    pub fn capability_has_query_at_primary_version(
        &self,
        query: &ApiPresenceQuery,
    ) -> Option<bool> {
        self.capability_has_query_at_primary_version_traced(query)
            .answer
    }

    /// Traced form of [`Self::capability_has_query_at_primary_version`].
    #[must_use]
    pub fn capability_has_query_at_primary_version_traced(
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
        let Some(probe) = DEFAULT_CAPABILITY_PROBE_TABLE.build_probe(query) else {
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
        let mut worst: ProbeOutcome = ProbeOutcome::AuthoritativelyAbsent;
        for filename in &candidates {
            for source in &self.mirrors.sources {
                let outcome = self.probe_at(&source.source_id, primary, filename);
                trace.record_api_presence_source_probe(
                    ProviderOrigin::KubernetesOpenApi,
                    &source.source_id,
                    primary,
                    filename,
                    SourceProbeTraceOutcome::from(outcome),
                );
                match outcome {
                    ProbeOutcome::Found => {
                        trace.record_api_presence_provider(
                            ProviderOrigin::KubernetesOpenApi,
                            Some(true),
                        );
                        return TracedApiPresenceOutcome {
                            answer: Some(true),
                            trace,
                        };
                    }
                    ProbeOutcome::Uncertain => worst = ProbeOutcome::Uncertain,
                    ProbeOutcome::AuthoritativelyAbsent => {
                        // worst stays at AuthoritativelyAbsent unless
                        // some other probe already set it to Uncertain.
                    }
                }
            }
        }
        let answer = match worst {
            ProbeOutcome::Found => unreachable!("Found short-circuits above"),
            ProbeOutcome::AuthoritativelyAbsent => Some(false),
            ProbeOutcome::Uncertain => None,
        };
        trace.record_api_presence_provider(ProviderOrigin::KubernetesOpenApi, answer);
        TracedApiPresenceOutcome { answer, trace }
    }

    /// Single-probe upstream-first lookup with tri-state outcome. The
    /// authoritative-vs-uncertain distinction is the heart of the
    /// capability oracle's offline-safety contract:
    ///   - `Found`: schema is loadable (mem cache, disk cache, or
    ///     successful fetch).
    ///   - `AuthoritativelyAbsent`: the fetcher confirmed the schema
    ///     does not exist upstream (recorded in negative cache).
    ///     Includes negative-cache hits from a prior online run, since
    ///     those represent a confirmed past 404 — still authoritative.
    ///   - `Uncertain`: no cache hit, no fetch attempted (offline AND
    ///     no negative-cache record), or fetch failed with a network
    ///     error. The probe gives no information either way.
    fn probe_at(&self, source_id: &str, version: &str, filename: &str) -> ProbeOutcome {
        let local = k8s_cache_path(&self.cache_dir, source_id, version, filename);
        if self.use_cache {
            if self
                .mem
                .read(&mem_key(source_id, version, filename))
                .is_some()
            {
                return ProbeOutcome::Found;
            }
            if local.exists() {
                return ProbeOutcome::Found;
            }
            // Negative cache is set ONLY when the fetcher returns a clean
            // "not found" — treat as authoritative even offline. A prior
            // online run already proved upstream doesn't have this file.
            if self.negative_cache.contains(source_id, version, filename)
                || not_found_marker_exists(&local)
            {
                return ProbeOutcome::AuthoritativelyAbsent;
            }
        }
        if !self.allow_download {
            // Offline + no cache + no negative-cache record: nothing
            // to base an answer on.
            return ProbeOutcome::Uncertain;
        }
        // Online: try the fetcher and let it disambiguate.
        let source = self
            .mirrors
            .sources
            .iter()
            .find(|s| s.source_id == source_id);
        let Some(source) = source else {
            return ProbeOutcome::Uncertain;
        };
        let url = format!(
            "{}/{version}/{filename}",
            source.base_url.trim_end_matches('/')
        );
        match self.fetcher.fetch(&url) {
            Ok(Some(bytes)) => {
                let Some(doc) = crate::cache_write::write_fetched_schema_doc(
                    &local,
                    &url,
                    &bytes,
                    self.record_source,
                ) else {
                    // Couldn't persist or parse — we still proved the
                    // schema exists upstream, but treat as Uncertain so
                    // a later run probes again rather than locking in a
                    // cache miss.
                    return ProbeOutcome::Uncertain;
                };
                AuthoritativeAbsence::MarkerPath(&local).clear();
                self.mem.write(mem_key(source_id, version, filename), doc);
                ProbeOutcome::Found
            }
            Ok(None) => {
                self.negative_cache.record(source_id, version, filename);
                AuthoritativeAbsence::MarkerPath(&local).record();
                ProbeOutcome::AuthoritativelyAbsent
            }
            // Network error: uncertain. Don't pollute the negative
            // cache, since the failure isn't proof of absence.
            Err(_) => ProbeOutcome::Uncertain,
        }
    }

    /// Schema for a resource is owned by us if any (version, source)
    /// has its file already cached or recorded as negative-cache; no
    /// fetches issued during ownership probe (PR 0e contract).
    fn local_owns_resource(&self, resource: &ResourceRef) -> bool {
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

    /// Scan the cache for K8s versions that hold this resource's file
    /// in any CONFIGURED source namespace. Used by the chain layer to
    /// populate `Diagnostic::MissingSchema.available_in_cache_versions`
    /// when the configured chain didn't resolve the resource.
    ///
    /// Stale `<source_id>` dirs left on disk from removed mirrors are
    /// skipped (Finding 2 — only currently-configured sources may
    /// surface inference / hint signals).
    ///
    /// Returns a sorted+deduped list of `version_dir` strings.
    #[must_use]
    pub fn cache_versions_holding(&self, resource: &ResourceRef) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if resource.api_version.trim().is_empty() {
            return out;
        }
        let candidates = candidate_filenames_for_resource(resource);
        let Ok(source_entries) = fs::read_dir(&self.cache_dir) else {
            return out;
        };
        let configured_versions: std::collections::HashSet<String> =
            self.versions.ordered().into_iter().collect();
        let configured_source_ids = self.configured_source_ids();
        for source_entry in source_entries.flatten() {
            let source_path = source_entry.path();
            if !source_path.is_dir() {
                continue;
            }
            let Some(source_id) = source_entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if !configured_source_ids.contains(&source_id) {
                continue;
            }
            let Ok(version_entries) = fs::read_dir(&source_path) else {
                continue;
            };
            for version_entry in version_entries.flatten() {
                let version_path = version_entry.path();
                if !version_path.is_dir() {
                    continue;
                }
                let Some(version_name) = version_entry.file_name().to_str().map(str::to_string)
                else {
                    continue;
                };
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

    /// Source-id directory names currently configured (`default` plus
    /// any `--k8s-schema-mirror` mirrors). Cache scans consult only
    /// these — stale dirs from removed mirrors do not influence live
    /// inference or hints (Finding 2).
    fn configured_source_ids(&self) -> std::collections::HashSet<String> {
        self.mirrors
            .sources
            .iter()
            .map(|source| source.source_id.clone())
            .collect()
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
            ctx.resolve_schema_reference(&current_location.document, reference)
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

    fn has_resource(&self, resource: &ResourceRef) -> bool {
        self.local_owns_resource(resource)
    }

    fn primary_k8s_version(&self) -> Option<&str> {
        self.versions.primary()
    }

    fn k8s_version_chain(&self) -> Option<Vec<String>> {
        Some(self.versions.ordered())
    }

    fn cache_versions_holding(&self, resource: &ResourceRef) -> Vec<String> {
        KubernetesJsonSchemaProvider::cache_versions_holding(self, resource)
    }

    fn capability_has_query_at_primary_version(&self, query: &ApiPresenceQuery) -> Option<bool> {
        KubernetesJsonSchemaProvider::capability_has_query_at_primary_version(self, query)
    }

    fn capability_has_query_at_primary_version_traced(
        &self,
        query: &ApiPresenceQuery,
    ) -> TracedApiPresenceOutcome {
        KubernetesJsonSchemaProvider::capability_has_query_at_primary_version_traced(self, query)
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
            &self.configured_source_ids(),
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
