use helm_schema_ir::{HelperBranch, ResourceRef, ValueUse, YamlPath};
use serde_json::Value;

use crate::capability_eval::{self, CapabilityOracle};
use crate::diagnostic::{Diagnostic, DiagnosticSink};
use crate::filename::candidate_filenames_for_resource;
use crate::inference::{self, ApiVersionInferenceOutcome};

use super::chain_outcome::ChainLookupOutcome;
use super::provider_origin::ProviderOrigin;
use super::provider_result::ProviderLookupResult;
use super::trait_def::K8sSchemaProvider;

/// Composed provider chain with precedence
/// `LocalOverride > DefaultCatalog > KubernetesOpenApi`.
///
/// This is the only layer that emits
/// [`Diagnostic::MissingSchema`]. Providers report their local
/// outcomes; the chain decides what to do with them.
#[derive(Debug)]
pub struct Chain {
    providers: Vec<Box<dyn K8sSchemaProvider>>,
    sink: Option<DiagnosticSink>,
    inference_enabled: bool,
}

impl Chain {
    #[must_use]
    pub fn new(providers: Vec<Box<dyn K8sSchemaProvider>>) -> Self {
        Self {
            providers,
            sink: None,
            inference_enabled: false,
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

    /// Schema for a [`ValueUse`] — iterates the ordered api-version
    /// candidates silently and, on total exhaustion, commits ONE
    /// `MissingSchema` attributed to the user-written primary
    /// `api_version` (not to any speculative candidate). Speculative
    /// per-candidate misses never reach the sink (Finding 4).
    pub fn schema_for_use(&self, use_: &ValueUse) -> Option<Value> {
        let resource = use_.resource.as_ref()?;

        // Round-5 Finding 3: `kind: List` is the standard K8s envelope
        // for emitting multiple resources from a single template (used
        // by `alertmanager/templates/ingressperreplica.yaml`,
        // `serviceperreplica.yaml`, etc.). The envelope itself isn't
        // a validated resource type — its only schema-relevant field
        // is `items[*]` which contains real resource manifests. The
        // detector currently attributes uses inside `items[*]` to
        // the wrapper, but emitting `MissingSchema(kind=List, …)` is
        // noise the user can't act on. Skip validation for the
        // envelope; the inner resources keep their own attribution
        // when the detector recognises them at higher indent.
        if resource.kind == "List" {
            return None;
        }

        if needs_inference(resource) {
            let inferred = if self.inference_enabled {
                inference::infer_api_version(self.providers.as_slice(), &resource.kind)
            } else {
                ApiVersionInferenceOutcome::NoMatch
            };
            match inferred {
                ApiVersionInferenceOutcome::Resolved {
                    api_version,
                    source,
                    origin,
                } => {
                    // Round-3 Finding 3, Option B: InferredApiVersion is
                    // an informational diagnostic that helps users
                    // notice when an apiVersion was guessed. For built-in
                    // K8s kinds (`ConfigMap → v1`, `Deployment → apps/v1`,
                    // `ClusterRole → rbac.authorization.k8s.io/v1`, …)
                    // the inference is trivially correct and adds noise.
                    // We still RESOLVE the inferred apiVersion below;
                    // we just don't tell the user about it for built-ins.
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
                        .into_value();
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
                        .into_value();
                }
            }
        }

        // Round-8 Finding 1 + round-12 Finding 1: when typed branches
        // are present, evaluate their guards against the primary K8s
        // version's bundle (the authoritative capability oracle) and
        // try ONLY the live branch's literals. `live_literals`
        // recurses through `HelperBranchBody::Nested` so guard
        // structure composes through delegation depth — e.g. outer
        // `if Has A then (include branched_inner) else Y` correctly
        // picks the inner if-branch when both A and B are live.
        // This makes resource identity match what the chart would
        // actually emit at runtime — both for resolution and for
        // MissingSchema attribution. Without typed branches, fall
        // back to the existing flat-candidate iteration.
        let iteration_versions: Vec<String> = if !resource.api_version_branches.is_empty() {
            let live = capability_eval::live_literals(&resource.api_version_branches, self);
            if live.is_empty() {
                crate::ordered_api_versions_for_resource(resource)
                    .into_iter()
                    .map(str::to_string)
                    .collect()
            } else {
                live
            }
        } else {
            crate::ordered_api_versions_for_resource(resource)
                .into_iter()
                .map(str::to_string)
                .collect()
        };

        // Track whether ANY candidate's resolution reached a
        // `Resolved` outcome — including `Resolved { schema: None }`,
        // which is the intentional PathUnresolved silence (provider
        // owns the resource, but this YAML path simply has no schema
        // defined). MissingSchema must NOT fire in that case; it would
        // turn legitimate path-coverage gaps (e.g. ConfigMap.data.X
        // where the spec doesn't constrain free-form data) into
        // diagnostic noise.
        let mut any_resolved_owner = false;
        for api_version in iteration_versions {
            let candidate = ResourceRef {
                api_version,
                kind: resource.kind.clone(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            };
            let outcome = self.resolve_against_chain_internal(&candidate, &use_.path, false);
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
        // All candidates exhausted AND no provider claimed ownership —
        // commit one MissingSchema attributed to the user-written
        // primary apiVersion. Provider-side miss diagnostics
        // (CrdVersionNotFound, etc.) ride along.
        self.commit_missing_schema(resource);
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

    /// `commit_miss_diagnostics = false` is silent mode used by
    /// [`Self::schema_for_use`] during multi-candidate iteration. In
    /// silent mode the chain still returns the typed outcome and still
    /// emits success-side diagnostics (`ResolvedFromFallbackVersion`)
    /// because those reflect a *resolution* that happened. Only the
    /// miss-side (MissingSchema, LocalOverrideUnreadable, provider
    /// CrdVersionNotFound) is suppressed.
    fn resolve_against_chain_internal(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
        commit_miss_diagnostics: bool,
    ) -> ChainLookupOutcome {
        for provider in &self.providers {
            match provider.lookup(resource, path) {
                ProviderLookupResult::Found {
                    schema,
                    resolved_k8s_version,
                } => {
                    self.maybe_emit_fallback_version(
                        resource,
                        provider.origin(),
                        resolved_k8s_version.as_deref(),
                    );
                    return ChainLookupOutcome::Resolved {
                        schema: Some(schema),
                        resolving_provider: provider.origin(),
                        resolved_k8s_version,
                    };
                }
                ProviderLookupResult::PathUnresolved => {
                    return ChainLookupOutcome::Resolved {
                        schema: None,
                        resolving_provider: provider.origin(),
                        resolved_k8s_version: None,
                    };
                }
                ProviderLookupResult::ResourceDocMissing {
                    io_error,
                    source_path,
                } => {
                    if provider.origin() == ProviderOrigin::LocalOverride {
                        if commit_miss_diagnostics && let Some(sink) = &self.sink {
                            sink.push(Diagnostic::LocalOverrideUnreadable {
                                kind: resource.kind.clone(),
                                api_version: resource.api_version.clone(),
                                override_path: source_path,
                                io_error,
                            });
                        }
                        return ChainLookupOutcome::MissingSchema {
                            k8s_versions_tried: Vec::new(),
                            tried_filenames: candidate_filenames_for_resource(resource),
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
            self.commit_missing_schema(resource);
        }
        outcome
    }

    /// Emit one `MissingSchema` for `resource` and all provider-side
    /// miss-companion diagnostics
    /// ([`K8sSchemaProvider::missing_schema_provider_diagnostics`]).
    /// Attribution uses `resource.api_version` verbatim — callers in
    /// multi-candidate iteration pass the user-written primary so we
    /// don't attribute the miss to a speculative candidate.
    fn commit_missing_schema(&self, resource: &ResourceRef) {
        let Some(sink) = &self.sink else {
            return;
        };
        // Round-6 Finding 3 (extended): `kind: List` is a transparent
        // K8s envelope (used by alertmanager's ingressperreplica.yaml,
        // serviceperreplica.yaml, …). The envelope itself isn't a
        // validatable resource — emitting MissingSchema for it is noise.
        // schema_for_use already short-circuits on List, but every
        // direct caller of resolve_against_chain (tests, other
        // providers, …) also routes through this commit, so the
        // suppression has to live here too.
        if resource.kind == "List" {
            return;
        }

        // Round-8 Finding 1: when typed branches are present, emit
        // ONE MissingSchema attributing to the LIVE branch (the one
        // the chart would emit at runtime for the configured primary
        // K8s version). This catches real chart bugs — e.g. the
        // elasticsearch PSP template's `if Has "policy/v1"` evaluates
        // to true in K8s 1.35 (PDB is at policy/v1), so the chart
        // emits `apiVersion: policy/v1`, but PSP doesn't exist there;
        // the diagnostic correctly attributes to policy/v1, not the
        // else-branch's policy/v1beta1.
        //
        // Mutually-exclusive branches aren't peer misses: at runtime
        // exactly one branch is taken, so emitting one diagnostic per
        // branch would misrepresent what the chart emits. Without
        // typed branches, fall back to per-candidate attribution (the
        // Round-6 behaviour) so the user still sees the full set of
        // attempted apiVersions instead of the uninformative
        // empty-string attribution.
        let attributions: Vec<String> = if !resource.api_version_branches.is_empty() {
            // live_literals recurses through nested branch bodies, so
            // the picked literal correctly reflects composed guards.
            let live = capability_eval::live_literals(&resource.api_version_branches, self);
            match live.first().cloned() {
                Some(lit) => vec![lit],
                // All branches dead / empty — fall through to the
                // candidate / api_version fallback below.
                None if resource.api_version.is_empty()
                    && !resource.api_version_candidates.is_empty() =>
                {
                    resource.api_version_candidates.clone()
                }
                None => vec![resource.api_version.clone()],
            }
        } else if resource.api_version.is_empty() && !resource.api_version_candidates.is_empty() {
            // Fallback: untyped multi-candidate (Round-6 contract).
            resource.api_version_candidates.clone()
        } else {
            vec![resource.api_version.clone()]
        };

        for api_version in attributions {
            let attribution = ResourceRef {
                api_version,
                kind: resource.kind.clone(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            };
            let tried_filenames = candidate_filenames_for_resource(&attribution);
            let k8s_versions_tried = self.collect_tried_k8s_versions();
            let available_in_cache_versions = self.collect_available_cache_versions(&attribution);
            let suggested_k8s_version = available_in_cache_versions.first().cloned();
            sink.push(Diagnostic::MissingSchema {
                kind: attribution.kind.clone(),
                api_version: attribution.api_version.clone(),
                k8s_versions_tried,
                tried_filenames,
                available_in_cache_versions,
                suggested_k8s_version,
                hint: crate::kubernetes_openapi::missing_schema_hint(&attribution),
            });
            for provider in &self.providers {
                for diag in provider.missing_schema_provider_diagnostics(&attribution) {
                    sink.push(diag);
                }
            }
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

    fn collect_available_cache_versions(&self, resource: &ResourceRef) -> Vec<String> {
        let mut out: Vec<String> = self
            .providers
            .iter()
            .flat_map(|p| p.cache_versions_holding(resource))
            .collect();
        out.sort();
        out.dedup();
        out
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

impl ChainLookupOutcome {
    /// Convenience for legacy `Option<Value>`-shaped callers — discards
    /// the typed outcome and returns the inner schema if any.
    #[must_use]
    pub fn into_value(self) -> Option<Value> {
        match self {
            ChainLookupOutcome::Resolved { schema, .. } => schema,
            ChainLookupOutcome::MissingSchema { .. } => None,
        }
    }
}

impl K8sSchemaProvider for Chain {
    fn schema_for_use(&self, use_: &ValueUse) -> Option<Value> {
        self.schema_for_use(use_)
    }

    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        self.resolve_against_chain(resource, path).into_value()
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

    fn capability_has_at_primary_version(&self, api: &str) -> Option<bool> {
        // First non-`None` answer wins — typically the K8s OpenAPI
        // provider for built-in apis. CRD / local-override providers
        // abstain (default `None`).
        self.providers
            .iter()
            .find_map(|p| p.capability_has_at_primary_version(api))
    }
}

impl CapabilityOracle for Chain {
    fn capability_has(&self, api: &str) -> Option<bool> {
        <Self as K8sSchemaProvider>::capability_has_at_primary_version(self, api)
    }
}
