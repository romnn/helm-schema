use helm_schema_core::{
    ApiPresenceQuery, ProviderOrigin, ProviderSchemaFragment, ProviderSchemaUse, ResourceRef,
    YamlPath, schema_fragment_for_use_across_ordered_versions,
};

use crate::diagnostic::Diagnostic;
use crate::inference::candidate::ApiVersionCandidate;

use super::provider_result::ProviderLookupResult;
use super::trace::{LookupTrace, TracedApiPresenceOutcome};

/// Provides JSON Schema fragments for Kubernetes resource fields.
///
/// Implementations typically own one source of schemas (local files,
/// upstream HTTP catalog, etc.) and delegate to shared `fetch`, `cache`,
/// and `lookup` primitives.
pub trait K8sSchemaProvider: Send + Sync + std::fmt::Debug {
    /// Schema for a specific provider-schema lookup request.
    ///
    /// Default impl: iterate the resource's ordered apiVersion candidates and
    /// ask `schema_fragment_for_resource_path` for each. The `Chain` overrides
    /// this to layer fallback / inference / typed diagnostics on top.
    fn schema_fragment_for_use(&self, use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        schema_fragment_for_use_across_ordered_versions(use_, |resource, path| {
            self.schema_fragment_for_resource_path(resource, path)
        })
    }

    /// Provider-owned schema fragment for a specific resource type + YAML path.
    ///
    /// Implementations return a fragment so source identity and future
    /// ref-shaped metadata survive provider lookup. Use [`Self::lookup`] when
    /// the chain needs to distinguish no ownership, missing docs, and
    /// unresolved paths for diagnostic attribution.
    fn schema_fragment_for_resource_path(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment>;

    /// Identifier of the layer this provider implements. Drives chain
    /// precedence and origin-specific diagnostic rules.
    fn origin(&self) -> ProviderOrigin;

    /// Typed lookup: distinguish ownership, doc presence, and path
    /// presence so the chain can attribute diagnostics correctly.
    ///
    /// Default impl synthesises one of `Found` / `NotOwned` from the fragment
    /// fragment lookup adapter so simple providers only need to implement one
    /// structural schema-fragment boundary; richer providers should override.
    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        if !self.has_resource(resource) {
            return ProviderLookupResult::NotOwned;
        }
        match self.schema_fragment_for_resource_path(resource, path) {
            Some(fragment) => ProviderLookupResult::Found {
                schema: fragment,
                resolved_k8s_version: None,
            },
            None => ProviderLookupResult::PathUnresolved,
        }
    }

    /// Cheap check for "does this provider own this resource type?".
    /// MUST NOT issue network requests — providers answer from local
    /// cache + per-process negative cache only.
    fn has_resource(&self, resource: &ResourceRef) -> bool;

    /// Contribute apiVersion candidates for a kind whose apiVersion
    /// the caller couldn't pin AFTER `api_version_candidates` has been
    /// exhausted. Returns ALL candidates the provider knows about; the
    /// caller aggregates across providers and decides Resolved vs
    /// Ambiguous. Default returns an empty list.
    fn infer_api_version_candidates(&self, _kind: &str) -> Vec<ApiVersionCandidate> {
        Vec::new()
    }

    /// Primary K8s version this provider holds (for
    /// `ResolvedFromFallbackVersion`). Non-K8s providers leave the
    /// default `None`.
    fn primary_k8s_version(&self) -> Option<&str> {
        None
    }

    /// Kubernetes version targeted by capability evaluation. This is the
    /// public adapter name used by higher layers; provider implementations can
    /// keep `primary_k8s_version` as their source of truth during migration.
    fn kube_version(&self) -> Option<&str> {
        self.primary_k8s_version()
    }

    /// Full K8s version chain (for `MissingSchema` payload). Non-K8s
    /// providers leave the default `None`.
    fn k8s_version_chain(&self) -> Option<Vec<String>> {
        None
    }

    /// K8s versions *outside* the configured chain that happen to have
    /// the resource's file cached. Used by the chain layer to populate
    /// `Diagnostic::MissingSchema.available_in_cache_versions`. Default
    /// returns empty (CRD / local-override providers don't carry a K8s
    /// version concept).
    fn cache_versions_holding(&self, _resource: &ResourceRef) -> Vec<String> {
        Vec::new()
    }

    /// Provider-side diagnostics the chain should commit on a final
    /// miss. Providers MUST NOT emit these themselves; the chain calls
    /// this method only after exhausting all candidates and providers,
    /// so speculative candidate probing in provider-schema lookup
    /// never leaks per-candidate misses.
    ///
    /// Default returns an empty list (the openapi and local-override
    /// providers don't contribute miss-side diagnostics beyond what the
    /// chain itself emits).
    fn missing_schema_provider_diagnostics(&self, _resource: &ResourceRef) -> Vec<Diagnostic> {
        Vec::new()
    }

    /// Authoritative answer to a typed `.Capabilities.APIVersions.Has ...`
    /// query against this provider's primary K8s version.
    ///
    /// [`ApiPresenceQuery::Resource`] probes an exact kind;
    /// [`ApiPresenceQuery::GroupVersion`] asks whether the API group/version is
    /// present.
    ///
    /// Returns:
    ///   - `Some(true)` when the api (and kind, if specified) exists
    ///     in the primary K8s version's schema bundle,
    ///   - `Some(false)` when the bundle exists but does not contain
    ///     the api/kind,
    ///   - `None` when this provider can't answer (e.g. no primary
    ///     version configured, unknown probe target, fetch failure).
    ///
    /// Cache is fetch-on-miss: when the file isn't in the local
    /// cache, the provider MAY fetch from upstream to give an
    /// authoritative answer. This is the upstream-first contract; the
    /// cache is a speed optimisation, never the sole oracle.
    ///
    /// Default returns `None` (providers that don't carry a K8s
    /// version concept — `LocalOverride`, `DefaultCatalog` — abstain).
    fn capability_has_query_at_primary_version(&self, _query: &ApiPresenceQuery) -> Option<bool> {
        None
    }

    /// Same answer as [`Self::capability_has_query_at_primary_version`], plus
    /// the executed provider-side knowledge probes.
    fn capability_has_query_at_primary_version_traced(
        &self,
        query: &ApiPresenceQuery,
    ) -> TracedApiPresenceOutcome {
        let answer = self.capability_has_query_at_primary_version(query);
        let mut trace = LookupTrace::new_api_presence(query);
        trace.record_api_presence_provider(self.origin(), answer);
        TracedApiPresenceOutcome { answer, trace }
    }
}
