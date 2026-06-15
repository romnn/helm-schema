use helm_schema_ir::{CapabilityOracle, ResourceRef};

use crate::diagnostic::Diagnostic;
use crate::filename::candidate_filenames_for_resource;

use super::provider_origin::ProviderOrigin;
use super::resource_lookup_plan::MissingSchemaAttributionPlan;
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
        if let Some(diagnostic) = local_override_unreadable(trace) {
            return vec![diagnostic];
        }
        let attribution_plan =
            MissingSchemaAttributionPlan::for_resource(resource, self.capability_oracle);
        let mut diagnostics = Vec::new();
        for attribution in attribution_plan.candidates() {
            diagnostics.push(self.missing_schema_diagnostic(attribution));
            for provider in self.providers {
                diagnostics.extend(provider.missing_schema_provider_diagnostics(attribution));
            }
        }
        diagnostics
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

fn local_override_unreadable(trace: &LookupTrace) -> Option<Diagnostic> {
    trace.entries().iter().find_map(|entry| match entry {
        LookupTraceEntry::ResourceProvider {
            resource: attempted_resource,
            provider: ProviderOrigin::LocalOverride,
            outcome:
                LookupTraceOutcome::ResourceDocMissing {
                    source_path,
                    io_error,
                },
        } => Some(Diagnostic::LocalOverrideUnreadable {
            kind: attempted_resource.kind.clone(),
            api_version: attempted_resource.api_version.clone(),
            override_path: source_path.clone(),
            io_error: io_error.clone(),
        }),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use helm_schema_ir::YamlPath;

    use super::*;
    use crate::lookup::ProviderLookupResult;

    fn resource(api_version: &str) -> ResourceRef {
        ResourceRef {
            api_version: api_version.to_string(),
            kind: "Widget".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        }
    }

    #[test]
    fn local_override_unreadable_uses_attempted_resource_from_trace_entry() {
        let subject = resource("example.com/v1beta1");
        let attempted = resource("example.com/v1");
        let mut trace = LookupTrace::new(&subject, &YamlPath(Vec::new()));
        trace.record_provider(
            &attempted,
            ProviderOrigin::LocalOverride,
            &ProviderLookupResult::ResourceDocMissing {
                source_path: "/tmp/widget.schema.json".to_string(),
                io_error: "permission denied".to_string(),
            },
        );

        let diagnostic = local_override_unreadable(&trace).expect("diagnostic");

        assert_eq!(
            diagnostic,
            Diagnostic::LocalOverrideUnreadable {
                kind: "Widget".to_string(),
                api_version: "example.com/v1".to_string(),
                override_path: "/tmp/widget.schema.json".to_string(),
                io_error: "permission denied".to_string(),
            }
        );
    }
}
