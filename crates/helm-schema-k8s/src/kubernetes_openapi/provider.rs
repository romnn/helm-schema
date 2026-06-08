use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use helm_schema_ir::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::cache::{
    LayoutCheckOutcome, LayoutChecker, NegativeCache, default_source_id, k8s_cache_path,
    write_meta_sidecar,
};
use crate::diagnostic::DiagnosticSink;
use crate::fetch::{HttpFetcher, UreqFetcher};
use crate::filename::{candidate_filenames_for_resource, filename_for_resource};
use crate::inference::cache_scan::scan_k8s_cache;
use crate::inference::shortlist::canonical_api_version_for_kind;
use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::lookup::{K8sSchemaProvider, ProviderLookupResult, ProviderOrigin};

use super::mirror_chain::{K8sMirrorChain, K8sSource};
use super::resolve_ctx::{ResolveCtx, expand_schema_node};
use super::version_chain::K8sVersionChain;

/// In-memory doc cache key: `(source_id, version_dir, filename)`.
type MemKey = (String, String, String);

#[derive(Debug, Clone)]
struct MaterializedResourceDoc {
    resolved_k8s_version: String,
    root: Arc<Value>,
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
    pub allow_api_version_guess: bool,
    pub record_source: bool,

    fetcher: Arc<dyn HttpFetcher>,
    negative_cache: Arc<NegativeCache>,
    layout_checker: Arc<LayoutChecker>,
    diagnostic_sink: Option<DiagnosticSink>,

    mem: Mutex<HashMap<MemKey, Value>>,
    materialized: Mutex<HashMap<ResourceRef, MaterializedResourceDoc>>,
}

/// Tri-state outcome for the capability-oracle probe path. See
/// [`KubernetesJsonSchemaProvider::probe_at`] for the per-source
/// semantics and [`KubernetesJsonSchemaProvider::capability_has_at_primary_version`]
/// for how the per-source outcomes aggregate into the final
/// `Option<bool>` answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeOutcome {
    Found,
    AuthoritativelyAbsent,
    Uncertain,
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
            allow_api_version_guess: false,
            record_source: false,
            fetcher: Arc::new(UreqFetcher::new()),
            negative_cache: Arc::new(NegativeCache::new()),
            layout_checker: Arc::new(LayoutChecker::new()),
            diagnostic_sink: None,
            mem: Mutex::new(HashMap::new()),
            materialized: Mutex::new(HashMap::new()),
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
    fn load_resource_doc(&self, resource: &ResourceRef) -> Option<(String, String, String, Value)> {
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
                if let Some(v) = self.read_mem(&version, filename) {
                    return Some((
                        default_source_id().to_string(),
                        version.clone(),
                        filename.clone(),
                        v,
                    ));
                }
                for source in &self.mirrors.sources {
                    if let Some(doc) = self.try_load_from_source(source, &version, filename) {
                        return Some((
                            source.source_id.clone(),
                            version.clone(),
                            filename.clone(),
                            doc,
                        ));
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
    ) -> Option<Value> {
        if let Some(v) = self.read_mem_for(&source.source_id, version, filename) {
            return Some(v);
        }
        let local = k8s_cache_path(&self.cache_dir, &source.source_id, version, filename);
        if local.exists()
            && let Ok(bytes) = fs::read(&local)
            && let Ok(doc) = serde_json::from_slice::<Value>(&bytes)
        {
            self.write_mem(&source.source_id, version, filename, &doc);
            return Some(doc);
        }
        if !self.allow_download {
            return None;
        }
        if self
            .negative_cache
            .contains(&source.source_id, version, filename)
        {
            return None;
        }
        let url = format!(
            "{}/{version}/{filename}",
            source.base_url.trim_end_matches('/')
        );
        match self.fetcher.fetch(&url) {
            Ok(Some(bytes)) => {
                write_atomic(&local, &bytes).ok()?;
                if self.record_source {
                    write_meta_sidecar(&local, &url);
                }
                let doc = serde_json::from_slice::<Value>(&bytes).ok()?;
                self.write_mem(&source.source_id, version, filename, &doc);
                Some(doc)
            }
            Ok(None) => {
                self.negative_cache
                    .record(&source.source_id, version, filename);
                None
            }
            Err(_) => None,
        }
    }

    fn read_mem(&self, version: &str, filename: &str) -> Option<Value> {
        self.read_mem_for(default_source_id(), version, filename)
    }

    fn read_mem_for(&self, source_id: &str, version: &str, filename: &str) -> Option<Value> {
        self.mem.lock().ok().and_then(|g| {
            g.get(&(
                source_id.to_string(),
                version.to_string(),
                filename.to_string(),
            ))
            .cloned()
        })
    }

    fn write_mem(&self, source_id: &str, version: &str, filename: &str, doc: &Value) {
        if let Ok(mut guard) = self.mem.lock() {
            guard.insert(
                (
                    source_id.to_string(),
                    version.to_string(),
                    filename.to_string(),
                ),
                doc.clone(),
            );
        }
    }

    fn run_layout_check(&self) -> LayoutCheckOutcome {
        self.layout_checker.check_and_prepare(
            &self.cache_dir,
            self.diagnostic_sink.as_ref(),
            |root| k8s_root_has_legacy_layout(root),
        )
    }

    fn loader_for_source(
        &self,
        source_id: String,
        version: String,
    ) -> impl FnMut(&str) -> Option<PathBuf> + '_ {
        move |filename: &str| {
            let local = k8s_cache_path(&self.cache_dir, &source_id, &version, filename);
            if local.exists() {
                return Some(local);
            }
            // Fall through to fetch the neighbour file via the same
            // source. Look it up through `try_load_from_source` so the
            // negative-cache + meta-sidecar paths still run.
            let source = self
                .mirrors
                .sources
                .iter()
                .find(|s| s.source_id == source_id)
                .cloned()?;
            self.try_load_from_source(&source, &version, filename)?;
            if local.exists() { Some(local) } else { None }
        }
    }

    /// Materialise the entire schema for a resource (used by tests).
    #[must_use]
    pub fn materialize_schema_for_resource(&self, resource: &ResourceRef) -> Option<Value> {
        let materialized = self.materialized_schema_for_resource(resource)?;
        Some((*materialized.root).clone())
    }

    fn materialized_schema_for_resource(
        &self,
        resource: &ResourceRef,
    ) -> Option<MaterializedResourceDoc> {
        if let Ok(guard) = self.materialized.lock()
            && let Some(doc) = guard.get(resource)
        {
            return Some(doc.clone());
        }

        let (source_id, version, filename, root) = self.load_resource_doc(resource)?;
        let loader = self.loader_for_source(source_id, version.clone());
        let mut ctx = ResolveCtx::new(loader, filename.clone(), root);
        let root_doc = ctx.doc(&filename)?.clone();
        let (_, expanded) = expand_schema_node(&mut ctx, &filename, &root_doc, 0);
        let materialized = MaterializedResourceDoc {
            resolved_k8s_version: version,
            root: Arc::new(expanded),
        };
        if let Ok(mut guard) = self.materialized.lock() {
            guard.insert(resource.clone(), materialized.clone());
        }
        Some(materialized)
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
    pub fn capability_has_at_primary_version(&self, api: &str) -> Option<bool> {
        let primary = self.versions.primary()?;
        let probe = build_capability_probe(api)?;
        let candidates = candidate_filenames_for_resource(&probe);
        // Aggregate the outcome across (source × filename) pairs.
        // Found-ness short-circuits to `Some(true)`. Without that, the
        // worst case across all probes decides — any uncertain probe
        // beats an authoritative absent, since one source might have
        // it even if the other authoritatively doesn't.
        let mut worst: ProbeOutcome = ProbeOutcome::AuthoritativelyAbsent;
        for filename in &candidates {
            for source in &self.mirrors.sources {
                match self.probe_at(&source.source_id, primary, filename) {
                    ProbeOutcome::Found => return Some(true),
                    ProbeOutcome::Uncertain => worst = ProbeOutcome::Uncertain,
                    ProbeOutcome::AuthoritativelyAbsent => {
                        // worst stays at AuthoritativelyAbsent unless
                        // some other probe already set it to Uncertain.
                    }
                }
            }
        }
        match worst {
            ProbeOutcome::Found => unreachable!("Found short-circuits above"),
            ProbeOutcome::AuthoritativelyAbsent => Some(false),
            ProbeOutcome::Uncertain => None,
        }
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
        if self.read_mem_for(source_id, version, filename).is_some() {
            return ProbeOutcome::Found;
        }
        let local = k8s_cache_path(&self.cache_dir, source_id, version, filename);
        if local.exists() {
            return ProbeOutcome::Found;
        }
        // Negative cache is set ONLY when the fetcher returns a clean
        // "not found" — treat as authoritative even offline. A prior
        // online run already proved upstream doesn't have this file.
        if self.negative_cache.contains(source_id, version, filename) {
            return ProbeOutcome::AuthoritativelyAbsent;
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
                if write_atomic(&local, &bytes).is_err() {
                    // Couldn't persist — we still proved the schema
                    // exists upstream, but treat as Uncertain so a
                    // later run probes again rather than locking in
                    // a cache miss.
                    return ProbeOutcome::Uncertain;
                }
                if self.record_source {
                    write_meta_sidecar(&local, &url);
                }
                let Ok(doc) = serde_json::from_slice::<Value>(&bytes) else {
                    return ProbeOutcome::Uncertain;
                };
                self.write_mem(source_id, version, filename, &doc);
                ProbeOutcome::Found
            }
            Ok(None) => {
                self.negative_cache.record(source_id, version, filename);
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
            .map(|s| s.source_id.clone())
            .collect()
    }
}

impl K8sSchemaProvider for KubernetesJsonSchemaProvider {
    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        if self.run_layout_check() == LayoutCheckOutcome::ForwardIncompatible {
            return None;
        }
        let materialized = self.materialized_schema_for_resource(resource)?;
        crate::local_override::descend_schema_path(&materialized.root, &path.0)
    }

    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::KubernetesOpenApi
    }

    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        if self.run_layout_check() == LayoutCheckOutcome::ForwardIncompatible {
            return ProviderLookupResult::NotOwned;
        }
        let Some(materialized) = self.materialized_schema_for_resource(resource) else {
            return ProviderLookupResult::NotOwned;
        };
        let primary = self
            .versions
            .primary()
            .unwrap_or(&materialized.resolved_k8s_version)
            .to_string();
        match crate::local_override::descend_schema_path(&materialized.root, &path.0) {
            Some(schema) => ProviderLookupResult::Found {
                schema,
                resolved_k8s_version: if primary == materialized.resolved_k8s_version {
                    None
                } else {
                    Some(materialized.resolved_k8s_version)
                },
            },
            None => ProviderLookupResult::PathUnresolved,
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

    fn capability_has_at_primary_version(&self, api: &str) -> Option<bool> {
        KubernetesJsonSchemaProvider::capability_has_at_primary_version(self, api)
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

/// Build the `ResourceRef` we'll probe to decide
/// `.Capabilities.APIVersions.Has "api"`.
///
/// For `group/version/Kind`: probe that exact kind at that api
/// version. For `group/version` (no kind): probe a structurally-known
/// canonical kind at that api version — chosen so that if the api
/// version is supported AT ALL in this K8s version, the probe kind is
/// in it. Returns `None` for unknown api versions (custom CRD groups,
/// etc.) where there's no canonical probe target.
fn build_capability_probe(api: &str) -> Option<ResourceRef> {
    let parts: Vec<&str> = api.split('/').collect();
    let (api_version, kind) = match parts.as_slice() {
        // `group/version/Kind` — probe directly.
        [_, _, k] => (parts[..2].join("/"), (*k).to_string()),
        // `group/version` or core `version` — probe canonical kind.
        [_, _] | [_] => (api.to_string(), well_known_kind_at(api)?.to_string()),
        _ => return None,
    };
    Some(ResourceRef {
        api_version,
        kind,
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    })
}

/// Map a well-known Kubernetes `apiGroup/version` (or core `version`)
/// to a kind that has been the canonical resource at that api version
/// since the api version was first introduced. If the K8s schema
/// bundle holds this kind at `api_version`, the cluster supports
/// `api_version`.
///
/// This table reflects structural facts about the K8s API surface
/// (which kind anchors each api group/version), not user input.
/// Returns `None` for api versions outside the table — for custom
/// CRD groups or other exotic apis, the chain treats the guard as
/// opaque (potentially live) rather than guessing.
///
/// **Architectural note (round-9 follow-up):** the table is a
/// manually-maintained heuristic, here because the upstream schema
/// source is per-file (one JSON per kind, fetched on demand) with no
/// bundle manifest we could enumerate. To answer
/// `Has "group/version"` without a Kind suffix, we have to probe
/// *some* kind under that api version — the canonical one in the
/// table is the structurally-justified choice (present from the api
/// version's inception, so its existence proves the api version
/// exists). A future cleaner implementation could:
///   - depend on an upstream bundle that ships a `_index.json`-style
///     manifest of all kinds per api version, and enumerate from it
///     instead of probing; or
///   - eagerly pre-fetch the primary K8s version's full bundle on
///     startup and answer from the on-disk enumeration.
///
/// Until either of those lands, this table is the source of truth
/// for "what's the cheapest probe target for api version X".
fn well_known_kind_at(api_version: &str) -> Option<&'static str> {
    let map = [
        ("v1", "ConfigMap"),
        ("apps/v1", "Deployment"),
        ("apps/v1beta1", "Deployment"),
        ("apps/v1beta2", "Deployment"),
        ("batch/v1", "Job"),
        ("batch/v1beta1", "CronJob"),
        ("rbac.authorization.k8s.io/v1", "Role"),
        ("rbac.authorization.k8s.io/v1beta1", "Role"),
        ("rbac.authorization.k8s.io/v1alpha1", "Role"),
        ("networking.k8s.io/v1", "Ingress"),
        ("networking.k8s.io/v1beta1", "Ingress"),
        ("extensions/v1beta1", "Ingress"),
        ("policy/v1", "PodDisruptionBudget"),
        ("policy/v1beta1", "PodDisruptionBudget"),
        ("autoscaling/v1", "HorizontalPodAutoscaler"),
        ("autoscaling/v2", "HorizontalPodAutoscaler"),
        ("autoscaling/v2beta1", "HorizontalPodAutoscaler"),
        ("autoscaling/v2beta2", "HorizontalPodAutoscaler"),
        ("storage.k8s.io/v1", "StorageClass"),
        ("storage.k8s.io/v1beta1", "StorageClass"),
        ("apiextensions.k8s.io/v1", "CustomResourceDefinition"),
        ("apiextensions.k8s.io/v1beta1", "CustomResourceDefinition"),
        (
            "admissionregistration.k8s.io/v1",
            "MutatingWebhookConfiguration",
        ),
        (
            "admissionregistration.k8s.io/v1beta1",
            "MutatingWebhookConfiguration",
        ),
        ("scheduling.k8s.io/v1", "PriorityClass"),
        ("scheduling.k8s.io/v1beta1", "PriorityClass"),
        ("coordination.k8s.io/v1", "Lease"),
        ("coordination.k8s.io/v1beta1", "Lease"),
        ("node.k8s.io/v1", "RuntimeClass"),
        ("node.k8s.io/v1beta1", "RuntimeClass"),
        ("discovery.k8s.io/v1", "EndpointSlice"),
        ("discovery.k8s.io/v1beta1", "EndpointSlice"),
        ("events.k8s.io/v1", "Event"),
        ("events.k8s.io/v1beta1", "Event"),
        ("certificates.k8s.io/v1", "CertificateSigningRequest"),
        ("certificates.k8s.io/v1beta1", "CertificateSigningRequest"),
        ("authentication.k8s.io/v1", "TokenReview"),
        ("authorization.k8s.io/v1", "SubjectAccessReview"),
        ("flowcontrol.apiserver.k8s.io/v1", "FlowSchema"),
        ("flowcontrol.apiserver.k8s.io/v1beta3", "FlowSchema"),
        ("flowcontrol.apiserver.k8s.io/v1beta2", "FlowSchema"),
    ];
    map.iter().find(|(k, _)| *k == api_version).map(|(_, v)| *v)
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

/// True if the cache root contains a "legacy" K8s layout (i.e. version
/// dirs sitting directly under the root, no `<source_id>` layer).
fn k8s_root_has_legacy_layout(root: &Path) -> bool {
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
        // A directory named like `v1.35.0` (legacy) vs a per-source dir
        // (`default`, `<hash>`). The legacy layout puts version dirs at
        // the top.
        if looks_like_k8s_version_dir(&name) {
            return true;
        }
    }
    false
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
    if let Ok(p) = std::env::var("HELM_SCHEMA_K8S_SCHEMA_CACHE") {
        return PathBuf::from(p);
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg)
            .join("helm-schema")
            .join("kubernetes-json-schema");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("helm-schema")
            .join("kubernetes-json-schema");
    }
    PathBuf::from(".cache")
        .join("helm-schema")
        .join("kubernetes-json-schema")
}
