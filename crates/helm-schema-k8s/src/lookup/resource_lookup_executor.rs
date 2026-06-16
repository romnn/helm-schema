use helm_schema_core::{ResourceRef, YamlPath};

use crate::filename::candidate_filenames_for_resource;

use super::chain_outcome::ChainLookupOutcome;
use super::provider_lookup_cache::ProviderLookupCache;
use super::provider_origin::ProviderOrigin;
use super::provider_result::ProviderLookupResult;
use super::trace::{LookupTrace, TracedLookupOutcome};
use super::trait_def::K8sSchemaProvider;

/// Executes one concrete `(apiVersion, kind, path)` lookup against the ordered
/// provider chain.
///
/// The executor owns provider precedence and stop/fallthrough semantics. It
/// does not emit diagnostics; callers project diagnostics from the returned
/// trace once they decide whether a miss is final.
pub(crate) struct ResourceLookupExecutor<'a> {
    providers: &'a [Box<dyn K8sSchemaProvider>],
    cache: &'a ProviderLookupCache,
}

impl<'a> ResourceLookupExecutor<'a> {
    pub(crate) fn new(
        providers: &'a [Box<dyn K8sSchemaProvider>],
        cache: &'a ProviderLookupCache,
    ) -> Self {
        Self { providers, cache }
    }

    pub(crate) fn execute(&self, resource: &ResourceRef, path: &YamlPath) -> TracedLookupOutcome {
        let mut trace = LookupTrace::new(resource, path);
        for (provider_index, provider) in self.providers.iter().enumerate() {
            let result = self
                .cache
                .lookup(provider_index, provider.as_ref(), resource, path);
            trace.record_provider(resource, provider.origin(), &result);

            match result {
                ProviderLookupResult::Found {
                    schema,
                    resolved_k8s_version,
                } => {
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

        TracedLookupOutcome {
            outcome: ChainLookupOutcome::MissingSchema {
                k8s_versions_tried: collect_tried_k8s_versions(self.providers),
                tried_filenames: candidate_filenames_for_resource(resource),
            },
            trace,
        }
    }
}

fn collect_tried_k8s_versions(providers: &[Box<dyn K8sSchemaProvider>]) -> Vec<String> {
    providers
        .iter()
        .filter_map(|provider| provider.k8s_version_chain())
        .flatten()
        .collect()
}
