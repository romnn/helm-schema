use helm_schema_ir::{
    ApiPresenceQuery, CapabilityOracle, ProviderSchemaUse, ResourceRef, YamlPath,
};

use crate::diagnostic::DiagnosticSink;

use super::api_version_inference_cache::ApiVersionInferenceCache;
use super::chain_outcome::ChainLookupOutcome;
use super::orchestrator::{LookupOrchestrator, LookupOrchestratorConfig};
use super::provider_lookup_cache::ProviderLookupCache;
use super::provider_origin::ProviderOrigin;
use super::provider_schema_fragment::ProviderSchemaFragment;
use super::trace::{TracedApiPresenceOutcome, TracedLookupOutcome};
use super::trait_def::K8sSchemaProvider;

/// Composed provider chain with precedence
/// `LocalOverride > DefaultCatalog > KubernetesOpenApi`.
///
/// This is the public compatibility facade over the lookup orchestrator. The
/// orchestrator plans candidates, delegates concrete provider execution, and
/// projects diagnostics from final-miss traces.
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

    fn lookup_orchestrator(&self) -> LookupOrchestrator<'_> {
        LookupOrchestrator::new(LookupOrchestratorConfig {
            providers: self.providers.as_slice(),
            sink: self.sink.as_ref(),
            inference_enabled: self.inference_enabled,
            inference_cache: &self.inference_cache,
            provider_lookup_cache: &self.provider_lookup_cache,
            capability_oracle: self,
        })
    }

    /// Resolve a single concrete `(apiVersion, kind)` against the
    /// chain and return the typed outcome. Emits miss-side diagnostics.
    pub fn resolve_against_chain(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> ChainLookupOutcome {
        self.lookup_orchestrator()
            .resolve_against_chain(resource, path)
    }

    /// Resolve a single concrete `(apiVersion, kind)` and keep the executed
    /// provider attempts. The current public schema APIs still consume only the
    /// outcome; diagnostics can later be projected from this trace.
    pub fn resolve_against_chain_traced(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> TracedLookupOutcome {
        self.lookup_orchestrator()
            .resolve_against_chain_traced(resource, path)
    }

    /// Answer a typed `.Capabilities.APIVersions.Has` query and retain the
    /// executed provider/source probes. The first provider that can answer wins,
    /// matching [`K8sSchemaProvider::capability_has_query_at_primary_version`].
    pub fn capability_has_query_at_primary_version_traced(
        &self,
        query: &ApiPresenceQuery,
    ) -> TracedApiPresenceOutcome {
        self.lookup_orchestrator()
            .capability_has_query_at_primary_version_traced(query)
    }
}

impl K8sSchemaProvider for Chain {
    fn schema_fragment_for_use(&self, use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        self.lookup_orchestrator().schema_fragment_for_use(use_)
    }

    fn schema_fragment_for_resource_path(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        self.resolve_against_chain(resource, path)
            .into_schema_fragment()
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
    fn capability_has_query(&self, query: &ApiPresenceQuery) -> Option<bool> {
        <Self as K8sSchemaProvider>::capability_has_query_at_primary_version(self, query)
    }

    fn kube_version(&self) -> Option<&str> {
        <Self as K8sSchemaProvider>::kube_version(self)
    }
}
