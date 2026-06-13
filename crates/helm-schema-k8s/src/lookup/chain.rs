use helm_schema_ir::{HelperBranch, ResourceRef, ValueUse, YamlPath};
use serde_json::Value;

use crate::capability_eval::{self, CapabilityOracle};
use crate::diagnostic::{Diagnostic, DiagnosticSink};
use crate::filename::candidate_filenames_for_resource;
use crate::inference::ApiVersionInferenceOutcome;

use super::api_presence::ApiPresenceQuery;
use super::api_version_inference_cache::ApiVersionInferenceCache;
use super::chain_outcome::ChainLookupOutcome;
use super::miss_diagnostics::MissingLookupDiagnostics;
use super::provider_lookup_cache::ProviderLookupCache;
use super::provider_origin::ProviderOrigin;
use super::provider_result::ProviderLookupResult;
use super::resource_lookup_plan::ResourceLookupPlan;
use super::trace::{LookupTrace, TracedApiPresenceOutcome, TracedLookupOutcome};
use super::trait_def::K8sSchemaProvider;

/// Composed provider chain with precedence
/// `LocalOverride > DefaultCatalog > KubernetesOpenApi`.
///
/// This layer executes provider lookups and projects diagnostics from the
/// resulting traces. Providers report their local outcomes; the chain decides
/// when a miss is final.
#[derive(Debug)]
pub struct Chain {
    providers: Vec<Box<dyn K8sSchemaProvider>>,
    sink: Option<DiagnosticSink>,
    inference_enabled: bool,
    inference_cache: ApiVersionInferenceCache,
    provider_lookup_cache: ProviderLookupCache,
}

impl Chain {
    #[must_use]
    pub fn new(providers: Vec<Box<dyn K8sSchemaProvider>>) -> Self {
        Self {
            providers,
            sink: None,
            inference_enabled: false,
            inference_cache: ApiVersionInferenceCache::default(),
            provider_lookup_cache: ProviderLookupCache::default(),
        }
    }

    #[must_use]
    pub fn with_diagnostic_sink(mut self, sink: DiagnosticSink) -> Self {
        self.sink = Some(sink);
        self
    }

    /// Enable apiVersion inference (Feature D). Off by default.
    #[must_use]
    pub fn with_inference_enabled(mut self, enabled: bool) -> Self {
        self.inference_enabled = enabled;
        self
    }

    pub fn providers(&self) -> &[Box<dyn K8sSchemaProvider>] {
        &self.providers
    }

    fn infer_api_version_for_kind(&self, kind: &str) -> ApiVersionInferenceOutcome {
        self.inference_cache.infer(self.providers.as_slice(), kind)
    }

    /// Schema for a [`ValueUse`] — iterates the ordered api-version
    /// candidates silently and, on total exhaustion, commits ONE
    /// `MissingSchema` attributed to the user-written primary
    /// `api_version` (not to any speculative candidate). Speculative
    /// per-candidate misses never reach the sink.
    #[tracing::instrument(
        skip_all,
        fields(
            kind = use_
                .resource
                .as_ref()
                .map(|resource| resource.kind.as_str())
                .unwrap_or(""),
            api_version = use_
                .resource
                .as_ref()
                .map(|resource| resource.api_version.as_str())
                .unwrap_or(""),
            path_len = use_.path.0.len(),
        )
    )]
    pub fn schema_for_use(&self, use_: &ValueUse) -> Option<Value> {
        let resource = use_.resource.as_ref()?;

        if needs_inference(resource) {
            let inferred = if self.inference_enabled {
                self.infer_api_version_for_kind(&resource.kind)
            } else {
                ApiVersionInferenceOutcome::NoMatch
            };
            match inferred {
                ApiVersionInferenceOutcome::Resolved {
                    api_version,
                    source,
                    origin,
                } => {
                    // InferredApiVersion is diagnostic-only: resolution still
                    // uses the inferred apiVersion below. Built-in K8s kinds
                    // are obvious enough that reporting the guess would add
                    // noise without helping the chart author.
                    let inferred_group = api_version.split_once('/').map_or("", |(g, _)| g);
                    let is_builtin = crate::is_k8s_builtin_group(inferred_group);
                    if !is_builtin && let Some(sink) = &self.sink {
                        sink.push(Diagnostic::InferredApiVersion {
                            kind: resource.kind.clone(),
                            inferred_api_version: api_version.clone(),
                            source,
                            origin,
                        });
                    }
                    let inferred_ref = ResourceRef {
                        api_version,
                        kind: resource.kind.clone(),
                        api_version_candidates: Vec::new(),
                        api_version_branches: Vec::new(),
                    };
                    return self
                        .resolve_against_chain(&inferred_ref, &use_.path)
                        .into_schema();
                }
                ApiVersionInferenceOutcome::Ambiguous { candidates } => {
                    if let Some(sink) = &self.sink {
                        sink.push(Diagnostic::AmbiguousApiVersion {
                            kind: resource.kind.clone(),
                            candidates,
                        });
                    }
                    return None;
                }
                ApiVersionInferenceOutcome::NoMatch => {
                    return self
                        .resolve_against_chain(resource, &use_.path)
                        .into_schema();
                }
            }
        }

        // Track whether ANY candidate's resolution reached a
        // `Resolved` outcome — including `Resolved { schema: None }`,
        // which is the intentional PathUnresolved silence (provider
        // owns the resource, but this YAML path simply has no schema
        // defined). MissingSchema must NOT fire in that case; it would
        // turn legitimate path-coverage gaps (e.g. ConfigMap.data.X
        // where the spec doesn't constrain free-form data) into
        // diagnostic noise.
        let mut any_resolved_owner = false;
        let plan = ResourceLookupPlan::for_resource(resource, self);
        for candidate in plan.candidates() {
            let outcome = self.resolve_against_chain_internal(candidate, &use_.path, false);
            match outcome {
                ChainLookupOutcome::Resolved {
                    schema: Some(v), ..
                } => return Some(v),
                ChainLookupOutcome::Resolved { schema: None, .. } => {
                    any_resolved_owner = true;
                }
                ChainLookupOutcome::MissingSchema { .. } => {}
            }
        }
        if any_resolved_owner {
            // A provider claimed ownership for one of the candidates
            // but the YAML path is intentionally silent — propagate
            // that silence; no MissingSchema.
            return None;
        }
        // All candidates exhausted AND no provider claimed ownership: emit the
        // final miss against the user-written primary apiVersion. Provider-side
        // miss diagnostics (CrdVersionNotFound, etc.) ride along.
        let miss_trace = LookupTrace::new(resource, &use_.path);
        self.emit_missing_lookup_diagnostics(&miss_trace);
        None
    }

    /// Resolve a single concrete `(apiVersion, kind)` against the
    /// chain and return the typed outcome. Emits miss-side diagnostics.
    pub fn resolve_against_chain(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> ChainLookupOutcome {
        self.resolve_against_chain_internal(resource, path, true)
    }

    /// Resolve a single concrete `(apiVersion, kind)` and keep the executed
    /// provider attempts. The current public schema APIs still consume only the
    /// outcome; diagnostics can later be projected from this trace.
    pub fn resolve_against_chain_traced(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> TracedLookupOutcome {
        self.resolve_against_chain_traced_internal(resource, path, true)
    }

    /// Answer a typed `.Capabilities.APIVersions.Has` query and retain the
    /// executed provider/source probes. The first provider that can answer wins,
    /// matching [`K8sSchemaProvider::capability_has_query_at_primary_version`].
    pub fn capability_has_query_at_primary_version_traced(
        &self,
        query: &ApiPresenceQuery,
    ) -> TracedApiPresenceOutcome {
        let mut trace = LookupTrace::new_api_presence(query);
        for provider in &self.providers {
            let provider_outcome = provider.capability_has_query_at_primary_version_traced(query);
            let answer = provider_outcome.answer;
            trace.extend_entries(provider_outcome.trace.into_entries());
            if answer.is_some() {
                return TracedApiPresenceOutcome { answer, trace };
            }
        }
        TracedApiPresenceOutcome {
            answer: None,
            trace,
        }
    }

    /// `commit_miss_diagnostics = false` is silent mode used by
    /// [`Self::schema_for_use`] during multi-candidate iteration. In
    /// silent mode the chain still returns the typed outcome and still
    /// emits success-side diagnostics (`ResolvedFromFallbackVersion`)
    /// because those reflect a *resolution* that happened. Only the
    /// miss-side (MissingSchema, LocalOverrideUnreadable, provider
    /// CrdVersionNotFound) is suppressed.
    #[tracing::instrument(skip_all, fields(kind = resource.kind.as_str(), api_version = resource.api_version.as_str(), path_len = path.0.len()))]
    fn resolve_against_chain_internal(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
        commit_miss_diagnostics: bool,
    ) -> ChainLookupOutcome {
        self.resolve_against_chain_traced_internal(resource, path, commit_miss_diagnostics)
            .into_outcome()
    }

    fn resolve_against_chain_traced_internal(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
        commit_miss_diagnostics: bool,
    ) -> TracedLookupOutcome {
        let mut trace = LookupTrace::new(resource, path);
        for (provider_index, provider) in self.providers.iter().enumerate() {
            let result = self.provider_lookup_cache.lookup(
                provider_index,
                provider.as_ref(),
                resource,
                path,
            );
            trace.record_provider(provider.origin(), &result);
            match result {
                ProviderLookupResult::Found {
                    schema,
                    resolved_k8s_version,
                } => {
                    self.maybe_emit_fallback_version(
                        resource,
                        provider.origin(),
                        resolved_k8s_version.as_deref(),
                    );
                    return TracedLookupOutcome {
                        outcome: ChainLookupOutcome::Resolved {
                            schema: Some(schema),
                            resolving_provider: provider.origin(),
                            resolved_k8s_version,
                        },
                        trace,
                    };
                }
                ProviderLookupResult::PathUnresolved => {
                    return TracedLookupOutcome {
                        outcome: ChainLookupOutcome::Resolved {
                            schema: None,
                            resolving_provider: provider.origin(),
                            resolved_k8s_version: None,
                        },
                        trace,
                    };
                }
                ProviderLookupResult::ResourceDocMissing { .. } => {
                    if provider.origin() == ProviderOrigin::LocalOverride {
                        if commit_miss_diagnostics {
                            self.emit_missing_lookup_diagnostics(&trace);
                        }
                        return TracedLookupOutcome {
                            outcome: ChainLookupOutcome::MissingSchema {
                                k8s_versions_tried: Vec::new(),
                                tried_filenames: candidate_filenames_for_resource(resource),
                            },
                            trace,
                        };
                    }
                }
                ProviderLookupResult::NotOwned => {}
            }
        }
        let outcome = ChainLookupOutcome::MissingSchema {
            k8s_versions_tried: self.collect_tried_k8s_versions(),
            tried_filenames: candidate_filenames_for_resource(resource),
        };
        if commit_miss_diagnostics {
            self.emit_missing_lookup_diagnostics(&trace);
        }
        TracedLookupOutcome { outcome, trace }
    }

    fn emit_missing_lookup_diagnostics(&self, trace: &LookupTrace) {
        let Some(sink) = &self.sink else {
            return;
        };
        let diagnostics = MissingLookupDiagnostics::new(self.providers.as_slice(), self);
        for diagnostic in diagnostics.project(trace) {
            sink.push(diagnostic);
        }
    }

    /// Pick the branch the chart would emit at runtime for the
    /// configured primary K8s version. Delegates to the standalone
    /// `capability_eval::select_live_branch` walker, parameterising
    /// it with `self` as the [`CapabilityOracle`] (which queries the
    /// provider chain's primary K8s bundle authoritatively;
    /// upstream-first, cache is just a speed optimisation).
    pub fn select_live_branch<'a>(&self, branches: &'a [HelperBranch]) -> Option<&'a HelperBranch> {
        capability_eval::select_live_branch(branches, self)
    }

    fn maybe_emit_fallback_version(
        &self,
        resource: &ResourceRef,
        _origin: ProviderOrigin,
        resolved_k8s_version: Option<&str>,
    ) {
        let Some(sink) = &self.sink else {
            return;
        };
        let Some(resolved_version) = resolved_k8s_version else {
            return;
        };
        let primary_version = self
            .providers
            .iter()
            .filter_map(|p| p.as_ref().primary_k8s_version())
            .next();
        let Some(primary) = primary_version else {
            return;
        };
        if primary == resolved_version {
            return;
        }
        sink.push(Diagnostic::ResolvedFromFallbackVersion {
            kind: resource.kind.clone(),
            api_version: resource.api_version.clone(),
            primary_version: primary.to_string(),
            resolved_version: resolved_version.to_string(),
        });
    }

    fn collect_tried_k8s_versions(&self) -> Vec<String> {
        self.providers
            .iter()
            .filter_map(|p| p.k8s_version_chain())
            .flatten()
            .collect()
    }
}

fn needs_inference(resource: &ResourceRef) -> bool {
    if !resource.api_version.trim().is_empty() {
        return false;
    }
    !resource
        .api_version_candidates
        .iter()
        .any(|v| !v.trim().is_empty())
}

impl K8sSchemaProvider for Chain {
    fn schema_for_use(&self, use_: &ValueUse) -> Option<Value> {
        self.schema_for_use(use_)
    }

    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        self.resolve_against_chain(resource, path).into_schema()
    }

    fn origin(&self) -> ProviderOrigin {
        // Chains are not addressable as a single origin; report the
        // first provider's origin for the rare caller that asks.
        self.providers
            .first()
            .map(|p| p.origin())
            .unwrap_or(ProviderOrigin::KubernetesOpenApi)
    }

    fn has_resource(&self, resource: &ResourceRef) -> bool {
        self.providers.iter().any(|p| p.has_resource(resource))
    }

    fn kube_version(&self) -> Option<&str> {
        self.providers.iter().find_map(|p| p.kube_version())
    }

    fn capability_has_query_at_primary_version(&self, query: &ApiPresenceQuery) -> Option<bool> {
        self.capability_has_query_at_primary_version_traced(query)
            .into_answer()
    }

    fn capability_has_query_at_primary_version_traced(
        &self,
        query: &ApiPresenceQuery,
    ) -> TracedApiPresenceOutcome {
        Chain::capability_has_query_at_primary_version_traced(self, query)
    }
}

impl CapabilityOracle for Chain {
    fn capability_has(&self, api: &str) -> Option<bool> {
        <Self as K8sSchemaProvider>::capability_has_at_primary_version(self, api)
    }
}
