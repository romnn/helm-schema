use helm_schema_core::{
    ApiPresenceQuery, CapabilityOracle, ProviderOrigin, ProviderSchemaUse, ResourceRef,
    ResourceSchemaOracle, YamlPath,
};

use crate::diagnostic::{Diagnostic, DiagnosticSink};
use crate::inference::ApiVersionInferenceOutcome;

use super::api_presence_executor::ApiPresenceLookupExecutor;
use super::api_version_inference_cache::ApiVersionInferenceCache;
use super::chain_outcome::ChainLookupOutcome;
use super::miss_diagnostics::MissingLookupDiagnostics;
use super::provider_lookup_cache::ProviderLookupCache;
use super::provider_schema_fragment::ProviderSchemaFragment;
use super::resource_lookup_executor::ResourceLookupExecutor;
use super::resource_lookup_plan::ResourceLookupPlan;
use super::trace::{LookupTrace, TracedApiPresenceOutcome, TracedLookupOutcome};
use super::trait_def::K8sSchemaProvider;

/// Composed provider chain with precedence
/// `LocalOverride > DefaultCatalog > KubernetesOpenApi`.
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
        self.resolve_against_chain_traced(resource, path)
            .into_outcome()
    }

    pub fn schema_fragment_for_resource_path(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        self.resolve_against_chain(resource, path)
            .into_schema_fragment()
    }

    /// Resolve a single concrete `(apiVersion, kind)` and keep the executed
    /// provider attempts. The current public schema APIs still consume only the
    /// outcome; diagnostics can later be projected from this trace.
    pub fn resolve_against_chain_traced(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> TracedLookupOutcome {
        self.resolve_concrete_resource(resource, path, true)
    }

    /// Answer a typed `.Capabilities.APIVersions.Has` query and retain the
    /// executed provider/source probes. The first provider that can answer wins,
    /// matching [`K8sSchemaProvider::capability_has_query_at_primary_version`].
    pub fn capability_has_query_at_primary_version_traced(
        &self,
        query: &ApiPresenceQuery,
    ) -> TracedApiPresenceOutcome {
        ApiPresenceLookupExecutor::new(self.providers.as_slice()).execute(query)
    }

    fn schema_fragment_for_resource_needing_inference(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        let inferred = if self.inference_enabled {
            self.inference_cache
                .infer(self.providers.as_slice(), &resource.kind)
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
                let inferred_ref = ResourceRef {
                    api_version,
                    kind: resource.kind.clone(),
                    api_version_candidates: Vec::new(),
                    api_version_branches: Vec::new(),
                };
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
        let plan = ResourceLookupPlan::for_resource(resource, self);
        for candidate in plan.candidates() {
            let outcome = self
                .resolve_concrete_resource(candidate, path, false)
                .into_outcome();
            match outcome {
                ChainLookupOutcome::Resolved {
                    schema: Some(schema),
                    ..
                } => return Some(schema),
                ChainLookupOutcome::Resolved { schema: None, .. } => {
                    any_resolved_owner = true;
                }
                ChainLookupOutcome::MissingSchema { .. } => {}
            }
        }

        if any_resolved_owner {
            return None;
        }

        let miss_trace = LookupTrace::new(resource, path);
        self.emit_missing_lookup_diagnostics(&miss_trace);
        None
    }

    #[tracing::instrument(skip_all, fields(kind = resource.kind.as_str(), api_version = resource.api_version.as_str(), path_len = path.0.len(), commit_miss_diagnostics))]
    fn resolve_concrete_resource(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
        commit_miss_diagnostics: bool,
    ) -> TracedLookupOutcome {
        let executor =
            ResourceLookupExecutor::new(self.providers.as_slice(), &self.provider_lookup_cache);
        let traced = executor.execute(resource, path);
        if let ChainLookupOutcome::Resolved {
            resolved_k8s_version,
            ..
        } = &traced.outcome
        {
            self.maybe_emit_fallback_version(resource, resolved_k8s_version.as_deref());
        }
        if commit_miss_diagnostics
            && matches!(traced.outcome, ChainLookupOutcome::MissingSchema { .. })
        {
            self.emit_missing_lookup_diagnostics(&traced.trace);
        }
        traced
    }

    fn emit_missing_lookup_diagnostics(&self, trace: &LookupTrace) {
        let Some(sink) = self.sink.as_ref() else {
            return;
        };
        let diagnostics = MissingLookupDiagnostics::new(self.providers.as_slice(), self);
        for diagnostic in diagnostics.project(trace) {
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
            .filter_map(|provider| provider.as_ref().primary_k8s_version())
            .next();
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
        self.capability_has_query_at_primary_version_traced(query)
            .into_answer()
    }

    fn kube_version(&self) -> Option<&str> {
        Chain::kube_version(self)
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
