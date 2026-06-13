use helm_schema_ir::ResourceRef;

use crate::capability_eval::{self, CapabilityOracle};
use crate::diagnostic::Diagnostic;
use crate::filename::candidate_filenames_for_resource;

use super::provider_origin::ProviderOrigin;
use super::trace::{LookupTrace, LookupTraceEntry, LookupTraceOutcome};
use super::trait_def::K8sSchemaProvider;

/// Projects user-facing diagnostics from a failed knowledge lookup trace.
pub(crate) struct MissingLookupDiagnostics<'a> {
    providers: &'a [Box<dyn K8sSchemaProvider>],
    capability_oracle: &'a dyn CapabilityOracle,
}

impl<'a> MissingLookupDiagnostics<'a> {
    pub(crate) fn new(
        providers: &'a [Box<dyn K8sSchemaProvider>],
        capability_oracle: &'a dyn CapabilityOracle,
    ) -> Self {
        Self {
            providers,
            capability_oracle,
        }
    }

    pub(crate) fn project(&self, trace: &LookupTrace) -> Vec<Diagnostic> {
        let Some(resource) = trace.resource() else {
            return Vec::new();
        };
        if let Some(diagnostic) = local_override_unreadable(trace, resource) {
            return vec![diagnostic];
        }
        let mut diagnostics = Vec::new();
        for attribution in self.attribution_resources(resource) {
            diagnostics.push(self.missing_schema_diagnostic(&attribution));
            for provider in self.providers {
                diagnostics.extend(provider.missing_schema_provider_diagnostics(&attribution));
            }
        }
        diagnostics
    }

    fn attribution_resources(&self, resource: &ResourceRef) -> Vec<ResourceRef> {
        self.attribution_api_versions(resource)
            .into_iter()
            .map(|api_version| ResourceRef {
                api_version,
                kind: resource.kind.clone(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            })
            .collect()
    }

    fn attribution_api_versions(&self, resource: &ResourceRef) -> Vec<String> {
        if !resource.api_version_branches.is_empty() {
            let live = capability_eval::live_literals(
                &resource.api_version_branches,
                self.capability_oracle,
            );
            match live.first().cloned() {
                Some(api_version) => vec![api_version],
                None if resource.api_version.is_empty()
                    && !resource.api_version_candidates.is_empty() =>
                {
                    resource.api_version_candidates.clone()
                }
                None => vec![resource.api_version.clone()],
            }
        } else if resource.api_version.is_empty() && !resource.api_version_candidates.is_empty() {
            resource.api_version_candidates.clone()
        } else {
            vec![resource.api_version.clone()]
        }
    }

    fn missing_schema_diagnostic(&self, resource: &ResourceRef) -> Diagnostic {
        let available_in_cache_versions = self.collect_available_cache_versions(resource);
        Diagnostic::MissingSchema {
            kind: resource.kind.clone(),
            api_version: resource.api_version.clone(),
            k8s_versions_tried: self.collect_tried_k8s_versions(),
            tried_filenames: candidate_filenames_for_resource(resource),
            suggested_k8s_version: available_in_cache_versions.first().cloned(),
            available_in_cache_versions,
            hint: crate::kubernetes_openapi::missing_schema_hint(resource),
        }
    }

    fn collect_available_cache_versions(&self, resource: &ResourceRef) -> Vec<String> {
        let mut out: Vec<String> = self
            .providers
            .iter()
            .flat_map(|provider| provider.cache_versions_holding(resource))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn collect_tried_k8s_versions(&self) -> Vec<String> {
        self.providers
            .iter()
            .filter_map(|provider| provider.k8s_version_chain())
            .flatten()
            .collect()
    }
}

fn local_override_unreadable(trace: &LookupTrace, resource: &ResourceRef) -> Option<Diagnostic> {
    trace.entries().iter().find_map(|entry| match entry {
        LookupTraceEntry::ResourceProvider {
            provider: ProviderOrigin::LocalOverride,
            outcome:
                LookupTraceOutcome::ResourceDocMissing {
                    source_path,
                    io_error,
                },
        } => Some(Diagnostic::LocalOverrideUnreadable {
            kind: resource.kind.clone(),
            api_version: resource.api_version.clone(),
            override_path: source_path.clone(),
            io_error: io_error.clone(),
        }),
        _ => None,
    })
}
