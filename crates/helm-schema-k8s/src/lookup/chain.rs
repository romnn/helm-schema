use helm_schema_core::{
    ApiPresenceQuery, CapabilityOracle, ProviderOrigin, ProviderSchemaUse, ResourceRef,
    ResourceSchemaOracle, YamlPath,
};

use crate::diagnostic::{Diagnostic, DiagnosticSink};
use crate::inference::{ApiVersionInferenceOutcome, infer_api_version};

use super::chain_outcome::ChainLookupOutcome;
use super::memo_cache::MemoCache;
use super::miss_diagnostics::MissingLookupDiagnostics;
use super::provider_result::ProviderLookupResult;
use super::provider_schema_fragment::ProviderSchemaFragment;
use super::resource_lookup_plan::resource_lookup_candidates;
use super::trait_def::K8sSchemaProvider;

/// Composed provider chain with precedence
/// `LocalOverride > DefaultCatalog > KubernetesOpenApi`.
#[derive(Debug)]
pub struct Chain {
    providers: Vec<Box<dyn K8sSchemaProvider>>,
    sink: Option<DiagnosticSink>,
    inference_enabled: bool,
    inference_cache: MemoCache<String, ApiVersionInferenceOutcome>,
    provider_lookup_cache: MemoCache<ProviderLookupCacheKey, ProviderLookupResult>,
}

impl Chain {
    /// Creates a chain in explicit provider-precedence order.
    #[must_use]
    pub fn new(providers: Vec<Box<dyn K8sSchemaProvider>>) -> Self {
        Self {
            providers,
            sink: None,
            inference_enabled: false,
            inference_cache: MemoCache::default(),
            provider_lookup_cache: MemoCache::default(),
        }
    }

    /// Emits committed final-outcome diagnostics into `sink`.
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

    /// Returns configured providers in lookup order.
    pub fn providers(&self) -> &[Box<dyn K8sSchemaProvider>] {
        &self.providers
    }

    /// Returns the primary Kubernetes release exposed by the provider policy.
    pub fn kube_version(&self) -> Option<&str> {
        self.providers
            .iter()
            .find_map(|provider| provider.primary_k8s_version())
    }

    /// Resolve a single concrete `(apiVersion, kind)` against the
    /// chain and return the typed outcome. Emits miss-side diagnostics.
    pub fn resolve_against_chain(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> ChainLookupOutcome {
        self.resolve_concrete_resource(resource, path, true)
    }

    /// Resolves and returns only the materialized schema fragment.
    pub fn schema_fragment_for_resource_path(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        self.resolve_against_chain(resource, path)
            .into_schema_fragment()
    }

    /// Answer a typed `.Capabilities.APIVersions.Has` query. The first provider that can answer wins,
    /// matching [`K8sSchemaProvider::capability_has_query_at_primary_version`].
    fn capability_has_query_at_primary_version(&self, query: &ApiPresenceQuery) -> Option<bool> {
        for provider in &self.providers {
            let answer = provider.capability_has_query_at_primary_version(query);
            if answer.is_some() {
                return answer;
            }
        }
        None
    }

    fn schema_fragment_for_resource_needing_inference(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        let inferred = if self.inference_enabled {
            self.inference_cache
                .get_or_compute(resource.kind.clone(), || {
                    infer_api_version(self.providers.as_slice(), &resource.kind)
                })
        } else {
            ApiVersionInferenceOutcome::NoMatch
        };

        match inferred {
            ApiVersionInferenceOutcome::Resolved {
                api_version,
                source,
                origin,
            } => {
                self.maybe_emit_inferred_api_version(resource, &api_version, source, origin);
                let inferred_ref = ResourceRef::concrete(api_version, resource.kind.clone());
                self.resolve_against_chain(&inferred_ref, path)
                    .into_schema_fragment()
            }
            ApiVersionInferenceOutcome::Ambiguous { candidates } => {
                self.push_diagnostic(Diagnostic::AmbiguousApiVersion {
                    kind: resource.kind.clone(),
                    candidates,
                });
                None
            }
            ApiVersionInferenceOutcome::NoMatch => self
                .resolve_against_chain(resource, path)
                .into_schema_fragment(),
        }
    }

    fn schema_fragment_for_planned_candidates(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        let mut any_resolved_owner = false;
        let mut local_override_unreadable = None;
        for candidate in resource_lookup_candidates(resource, self) {
            let outcome = self.resolve_concrete_resource(&candidate, path, false);
            match outcome {
                ChainLookupOutcome::Resolved(Some(schema)) => return Some(schema),
                ChainLookupOutcome::Resolved(None) => any_resolved_owner = true,
                ChainLookupOutcome::MissingSchema => {}
                ChainLookupOutcome::LocalOverrideUnreadable {
                    override_path,
                    io_error,
                } => {
                    local_override_unreadable.get_or_insert_with(|| {
                        Diagnostic::LocalOverrideUnreadable {
                            kind: candidate.kind.clone(),
                            api_version: candidate.api_version.clone(),
                            override_path,
                            io_error,
                        }
                    });
                }
            }
        }

        if any_resolved_owner {
            return None;
        }

        self.emit_missing_lookup_diagnostics(resource, local_override_unreadable);
        None
    }

    #[tracing::instrument(skip_all, fields(kind = resource.kind.as_str(), api_version = resource.api_version.as_str(), path_len = path.0.len(), commit_miss_diagnostics))]
    fn resolve_concrete_resource(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
        commit_miss_diagnostics: bool,
    ) -> ChainLookupOutcome {
        for (provider_index, provider) in self.providers.iter().enumerate() {
            let result = self.provider_lookup_cache.get_or_compute(
                ProviderLookupCacheKey::new(provider_index, resource, path),
                || provider.lookup(resource, path),
            );
            let outcome = match result {
                ProviderLookupResult::Found {
                    schema,
                    resolved_k8s_version,
                } => {
                    self.maybe_emit_fallback_version(resource, resolved_k8s_version.as_deref());
                    Some(ChainLookupOutcome::Resolved(Some(schema)))
                }
                ProviderLookupResult::PathUnresolved => Some(ChainLookupOutcome::Resolved(None)),
                ProviderLookupResult::ResourceDocMissing {
                    source_path,
                    io_error,
                } if provider.origin() == ProviderOrigin::LocalOverride => {
                    Some(ChainLookupOutcome::LocalOverrideUnreadable {
                        override_path: source_path,
                        io_error,
                    })
                }
                ProviderLookupResult::ResourceDocMissing { .. }
                | ProviderLookupResult::NotOwned => None,
            };

            if let Some(outcome) = outcome {
                return self.finish_concrete_resource_lookup(
                    resource,
                    outcome,
                    commit_miss_diagnostics,
                );
            }
        }

        self.finish_concrete_resource_lookup(
            resource,
            ChainLookupOutcome::MissingSchema,
            commit_miss_diagnostics,
        )
    }

    fn finish_concrete_resource_lookup(
        &self,
        resource: &ResourceRef,
        outcome: ChainLookupOutcome,
        commit_miss_diagnostics: bool,
    ) -> ChainLookupOutcome {
        if commit_miss_diagnostics {
            let local_override_unreadable = match &outcome {
                ChainLookupOutcome::LocalOverrideUnreadable {
                    override_path,
                    io_error,
                } => Some(Diagnostic::LocalOverrideUnreadable {
                    kind: resource.kind.clone(),
                    api_version: resource.api_version.clone(),
                    override_path: override_path.clone(),
                    io_error: io_error.clone(),
                }),
                ChainLookupOutcome::MissingSchema => None,
                ChainLookupOutcome::Resolved(_) => return outcome,
            };
            self.emit_missing_lookup_diagnostics(resource, local_override_unreadable);
        }
        outcome
    }

    fn emit_missing_lookup_diagnostics(
        &self,
        resource: &ResourceRef,
        local_override_unreadable: Option<Diagnostic>,
    ) {
        let Some(sink) = self.sink.as_ref() else {
            return;
        };
        let diagnostics = MissingLookupDiagnostics::new(self.providers.as_slice(), self);
        for diagnostic in diagnostics.project(resource, local_override_unreadable) {
            sink.push(diagnostic);
        }
    }

    fn maybe_emit_inferred_api_version(
        &self,
        resource: &ResourceRef,
        api_version: &str,
        source: crate::inference::InferenceSource,
        origin: ProviderOrigin,
    ) {
        let inferred_group = api_version.split_once('/').map_or("", |(group, _)| group);
        if crate::is_k8s_builtin_group(inferred_group) {
            return;
        }
        self.push_diagnostic(Diagnostic::InferredApiVersion {
            kind: resource.kind.clone(),
            inferred_api_version: api_version.to_string(),
            source,
            origin,
        });
    }

    fn maybe_emit_fallback_version(
        &self,
        resource: &ResourceRef,
        resolved_k8s_version: Option<&str>,
    ) {
        let Some(resolved_version) = resolved_k8s_version else {
            return;
        };
        let primary_version = self
            .providers
            .iter()
            .find_map(|provider| provider.as_ref().primary_k8s_version());
        let Some(primary) = primary_version else {
            return;
        };
        if primary == resolved_version {
            return;
        }
        self.push_diagnostic(Diagnostic::ResolvedFromFallbackVersion {
            kind: resource.kind.clone(),
            api_version: resource.api_version.clone(),
            primary_version: primary.to_string(),
            resolved_version: resolved_version.to_string(),
        });
    }

    fn push_diagnostic(&self, diagnostic: Diagnostic) {
        if let Some(sink) = self.sink.as_ref() {
            sink.push(diagnostic);
        }
    }
}

impl ResourceSchemaOracle for Chain {
    #[tracing::instrument(
        skip_all,
        fields(
            kind = use_
                .resource
                .kind
                .as_str(),
            api_version = use_
                .resource
                .api_version
                .as_str(),
            path_len = use_.path.0.len(),
        )
    )]
    fn schema_fragment_for_use(&self, use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        let resource = &use_.resource;

        if needs_inference(resource) {
            return self.schema_fragment_for_resource_needing_inference(resource, &use_.path);
        }

        self.schema_fragment_for_planned_candidates(resource, &use_.path)
    }
}

impl CapabilityOracle for Chain {
    fn capability_has_query(&self, query: &ApiPresenceQuery) -> Option<bool> {
        self.capability_has_query_at_primary_version(query)
    }
}

fn needs_inference(resource: &ResourceRef) -> bool {
    if !resource.api_version.trim().is_empty() {
        return false;
    }
    !resource
        .api_version_candidates
        .iter()
        .any(|version| !version.trim().is_empty())
}

/// Cache key for one provider's `(resource, path)` lookup result.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ProviderLookupCacheKey {
    provider_index: usize,
    api_version: String,
    kind: String,
    path: Vec<String>,
}

impl ProviderLookupCacheKey {
    fn new(provider_index: usize, resource: &ResourceRef, path: &YamlPath) -> Self {
        Self {
            provider_index,
            api_version: resource.api_version.clone(),
            kind: resource.kind.clone(),
            path: path.0.clone(),
        }
    }
}

#[cfg(test)]
#[path = "tests/chain.rs"]
mod tests;
