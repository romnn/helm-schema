use helm_schema_ir::ApiPresenceQuery;

use super::trace::{LookupTrace, TracedApiPresenceOutcome};
use super::trait_def::K8sSchemaProvider;

/// Executes one `.Capabilities.APIVersions.Has` query against the ordered
/// provider chain.
///
/// The first provider that can answer wins. Providers that cannot answer still
/// contribute trace entries so uncertainty remains diagnosable.
pub(crate) struct ApiPresenceLookupExecutor<'a> {
    providers: &'a [Box<dyn K8sSchemaProvider>],
}

impl<'a> ApiPresenceLookupExecutor<'a> {
    pub(crate) fn new(providers: &'a [Box<dyn K8sSchemaProvider>]) -> Self {
        Self { providers }
    }

    pub(crate) fn execute(&self, query: &ApiPresenceQuery) -> TracedApiPresenceOutcome {
        let mut trace = LookupTrace::new_api_presence(query);
        for provider in self.providers {
            let provider_outcome = provider.capability_has_query_at_primary_version_traced(query);
            let answer = provider_outcome.answer;
            trace.extend_entries(provider_outcome.trace.into_entries());
            if answer.is_some() {
                return TracedApiPresenceOutcome { answer, trace };
            }
        }

        TracedApiPresenceOutcome {
            answer: None,
            trace,
        }
    }
}
