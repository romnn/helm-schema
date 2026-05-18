use helm_schema_ir::{ResourceRef, ValueUse, YamlPath};
use serde_json::Value;

use crate::diagnostic::Diagnostic;
use crate::filename::ordered_api_versions_for_resource;
use crate::inference::candidate::ApiVersionCandidate;

use super::provider_origin::ProviderOrigin;
use super::provider_result::ProviderLookupResult;

/// Provides JSON Schema fragments for Kubernetes resource fields.
///
/// Implementations typically own one source of schemas (local files,
/// upstream HTTP catalog, etc.) and delegate to shared `fetch`, `cache`,
/// and `lookup` primitives.
pub trait K8sSchemaProvider: Send + Sync + std::fmt::Debug {
    /// Schema for a specific value use (resource + YAML path).
    ///
    /// Default impl: iterate the resource's ordered apiVersion
    /// candidates and ask `schema_for_resource_path` for each. The
    /// `Chain` overrides this to layer fallback / inference / typed
    /// diagnostics on top.
    fn schema_for_use(&self, use_: &ValueUse) -> Option<Value> {
        let resource = use_.resource.as_ref()?;
        for v in ordered_api_versions_for_resource(resource) {
            let candidate = ResourceRef {
                api_version: v.to_string(),
                kind: resource.kind.clone(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            };
            if let Some(schema) = self.schema_for_resource_path(&candidate, &use_.path) {
                return Some(schema);
            }
        }
        None
    }

    /// Schema for a specific resource type + YAML path.
    ///
    /// Returns `None` for any provider-local failure (no ownership, no
    /// resource doc, no path within doc). Use [`Self::lookup`] when the
    /// chain needs to distinguish those cases for diagnostic
    /// attribution.
    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value>;

    /// Identifier of the layer this provider implements. Drives chain
    /// precedence and origin-specific diagnostic rules.
    fn origin(&self) -> ProviderOrigin;

    /// Typed lookup: distinguish ownership, doc presence, and path
    /// presence so the chain can attribute diagnostics correctly.
    ///
    /// Default impl synthesises one of `Found` / `NotOwned` from the
    /// scalar `schema_for_resource_path` so older providers keep
    /// working; new code should override.
    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        if !self.has_resource(resource) {
            return ProviderLookupResult::NotOwned;
        }
        match self.schema_for_resource_path(resource, path) {
            Some(schema) => ProviderLookupResult::Found {
                schema,
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
    /// so speculative candidate probing in [`Chain::schema_for_use`]
    /// never leaks per-candidate misses.
    ///
    /// Default returns an empty list (the openapi and local-override
    /// providers don't contribute miss-side diagnostics beyond what the
    /// chain itself emits).
    fn missing_schema_provider_diagnostics(&self, _resource: &ResourceRef) -> Vec<Diagnostic> {
        Vec::new()
    }

    /// Authoritative answer to `.Capabilities.APIVersions.Has "api"`
    /// against this provider's primary K8s version.
    ///
    /// `api` is the literal Helm argument: either `group/version`
    /// (e.g. `"policy/v1"` — true if the K8s version supports the api
    /// group at that version) or `group/version/Kind`
    /// (e.g. `"policy/v1/PodSecurityPolicy"` — true if the kind exists
    /// at that api version in this K8s version). The core API uses
    /// `version` (e.g. `"v1"`).
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
    fn capability_has_at_primary_version(&self, _api: &str) -> Option<bool> {
        None
    }
}
