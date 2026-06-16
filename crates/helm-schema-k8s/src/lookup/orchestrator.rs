use helm_schema_core::{
    ApiPresenceQuery, CapabilityOracle, ProviderSchemaUse, ResourceRef, YamlPath,
};

use crate::diagnostic::{Diagnostic, DiagnosticSink};
use crate::inference::ApiVersionInferenceOutcome;

use super::api_presence_executor::ApiPresenceLookupExecutor;
use super::api_version_inference_cache::ApiVersionInferenceCache;
use super::chain_outcome::ChainLookupOutcome;
use super::miss_diagnostics::MissingLookupDiagnostics;
use super::provider_lookup_cache::ProviderLookupCache;
use super::provider_origin::ProviderOrigin;
use super::provider_schema_fragment::ProviderSchemaFragment;
use super::resource_lookup_executor::ResourceLookupExecutor;
use super::resource_lookup_plan::ResourceLookupPlan;
use super::trace::{LookupTrace, TracedApiPresenceOutcome, TracedLookupOutcome};
use super::trait_def::K8sSchemaProvider;

/// High-level lookup orchestration for the compatibility chain facade.
///
/// This owns apiVersion inference, candidate iteration, success diagnostics,
/// and final-miss diagnostic projection. Concrete provider precedence stays in
/// `ResourceLookupExecutor`.
pub(crate) struct LookupOrchestrator<'a> {
    providers: &'a [Box<dyn K8sSchemaProvider>],
    sink: Option<&'a DiagnosticSink>,
    inference_enabled: bool,
    inference_cache: &'a ApiVersionInferenceCache,
    provider_lookup_cache: &'a ProviderLookupCache,
    capability_oracle: &'a dyn CapabilityOracle,
}

impl<'a> LookupOrchestrator<'a> {
    pub(crate) fn new(config: LookupOrchestratorConfig<'a>) -> Self {
        Self {
            providers: config.providers,
            sink: config.sink,
            inference_enabled: config.inference_enabled,
            inference_cache: config.inference_cache,
            provider_lookup_cache: config.provider_lookup_cache,
            capability_oracle: config.capability_oracle,
        }
    }

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
    pub(crate) fn schema_fragment_for_use(
        &self,
        use_: &ProviderSchemaUse,
    ) -> Option<ProviderSchemaFragment> {
        let resource = &use_.resource;

        if needs_inference(resource) {
            return self.schema_fragment_for_resource_needing_inference(resource, &use_.path);
        }

        self.schema_fragment_for_planned_candidates(resource, &use_.path)
    }

    pub(crate) fn resolve_against_chain(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> ChainLookupOutcome {
        self.resolve_against_chain_traced(resource, path)
            .into_outcome()
    }

    pub(crate) fn resolve_against_chain_traced(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> TracedLookupOutcome {
        self.resolve_concrete_resource(resource, path, true)
    }

    pub(crate) fn capability_has_query_at_primary_version_traced(
        &self,
        query: &ApiPresenceQuery,
    ) -> TracedApiPresenceOutcome {
        ApiPresenceLookupExecutor::new(self.providers).execute(query)
    }

    fn schema_fragment_for_resource_needing_inference(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        let inferred = if self.inference_enabled {
            self.inference_cache.infer(self.providers, &resource.kind)
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
        let plan = ResourceLookupPlan::for_resource(resource, self.capability_oracle);
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
        let executor = ResourceLookupExecutor::new(self.providers, self.provider_lookup_cache);
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
        let Some(sink) = self.sink else {
            return;
        };
        let diagnostics = MissingLookupDiagnostics::new(self.providers, self.capability_oracle);
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
        if let Some(sink) = self.sink {
            sink.push(diagnostic);
        }
    }
}

pub(crate) struct LookupOrchestratorConfig<'a> {
    pub(crate) providers: &'a [Box<dyn K8sSchemaProvider>],
    pub(crate) sink: Option<&'a DiagnosticSink>,
    pub(crate) inference_enabled: bool,
    pub(crate) inference_cache: &'a ApiVersionInferenceCache,
    pub(crate) provider_lookup_cache: &'a ProviderLookupCache,
    pub(crate) capability_oracle: &'a dyn CapabilityOracle,
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
