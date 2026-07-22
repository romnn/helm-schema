use helm_schema_core::{ProviderOrigin, ResourceRef};

use super::chain_outcome::ChainLookupOutcome;
use super::provider_result::ProviderLookupResult;

/// Executed lookup trace for one concrete schema-knowledge query.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LookupTrace {
    entries: Vec<LookupTraceEntry>,
}

impl LookupTrace {
    pub(crate) fn record_provider(
        &mut self,
        resource: &ResourceRef,
        provider: ProviderOrigin,
        result: &ProviderLookupResult,
    ) {
        self.entries.push(LookupTraceEntry::ResourceProvider {
            resource: resource.clone(),
            provider,
            outcome: LookupTraceOutcome::from(result),
        });
    }

    pub(crate) fn record_api_presence_provider(
        &mut self,
        provider: ProviderOrigin,
        answer: Option<bool>,
    ) {
        self.entries
            .push(LookupTraceEntry::ApiPresenceProvider { provider, answer });
    }

    pub(crate) fn record_api_presence_source_probe(
        &mut self,
        provider: ProviderOrigin,
        source_id: &str,
        k8s_version: &str,
        filename: &str,
        outcome: SourceProbeTraceOutcome,
    ) {
        self.entries.push(LookupTraceEntry::ApiPresenceSourceProbe {
            provider,
            source_id: source_id.to_string(),
            k8s_version: k8s_version.to_string(),
            filename: filename.to_string(),
            outcome,
        });
    }

    pub(crate) fn extend_entries(&mut self, entries: impl IntoIterator<Item = LookupTraceEntry>) {
        self.entries.extend(entries);
    }

    pub(crate) fn into_entries(self) -> Vec<LookupTraceEntry> {
        self.entries
    }

    /// Returns executed provider and source probes in evaluation order.
    #[must_use]
    pub fn entries(&self) -> &[LookupTraceEntry] {
        &self.entries
    }
}

/// One executed step in a schema or API-presence lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookupTraceEntry {
    /// A provider attempted to resolve a concrete resource schema.
    ResourceProvider {
        /// Resource requested from the provider.
        resource: ResourceRef,
        /// Provider that handled the step.
        provider: ProviderOrigin,
        /// Provider's normalized lookup outcome.
        outcome: LookupTraceOutcome,
    },
    /// A provider answered whether an API version is present.
    ApiPresenceProvider {
        /// Provider that handled the query.
        provider: ProviderOrigin,
        /// Authoritative answer, or `None` when the provider cannot decide.
        answer: Option<bool>,
    },
    /// A concrete upstream or cache source was probed for API presence.
    ApiPresenceSourceProbe {
        /// Provider responsible for the source.
        provider: ProviderOrigin,
        /// Stable namespace of the probed source.
        source_id: String,
        /// Kubernetes release queried at the source.
        k8s_version: String,
        /// Resource filename used as the capability witness.
        filename: String,
        /// Authority level and result of the source probe.
        outcome: SourceProbeTraceOutcome,
    },
}

/// Normalized outcome of one concrete resource-provider lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookupTraceOutcome {
    /// The provider returned a schema fragment.
    Found {
        /// Kubernetes version that supplied the schema, when versioned.
        resolved_k8s_version: Option<String>,
    },
    /// The resource exists, but the requested YAML path has no schema node.
    PathUnresolved,
    /// The provider owned the resource but could not load its document.
    ResourceDocMissing {
        /// Source path or URL that failed.
        source_path: String,
        /// Human-readable I/O or transport failure.
        io_error: String,
    },
    /// The provider does not own the requested resource.
    NotOwned,
}

impl From<&ProviderLookupResult> for LookupTraceOutcome {
    fn from(result: &ProviderLookupResult) -> Self {
        match result {
            ProviderLookupResult::Found {
                resolved_k8s_version,
                ..
            } => Self::Found {
                resolved_k8s_version: resolved_k8s_version.clone(),
            },
            ProviderLookupResult::PathUnresolved => Self::PathUnresolved,
            ProviderLookupResult::ResourceDocMissing {
                source_path,
                io_error,
            } => Self::ResourceDocMissing {
                source_path: source_path.clone(),
                io_error: io_error.clone(),
            },
            ProviderLookupResult::NotOwned => Self::NotOwned,
        }
    }
}

/// Authority-aware outcome of probing one schema source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceProbeTraceOutcome {
    /// The source contains the capability witness.
    Found,
    /// An authoritative response proves the witness is absent.
    AuthoritativelyAbsent,
    /// Available local state cannot establish presence or absence.
    Uncertain,
}

/// Schema-chain result paired with its executed lookup trace.
#[derive(Debug)]
pub struct TracedLookupOutcome {
    /// Result returned by the provider chain.
    pub outcome: ChainLookupOutcome,
    /// Provider and source steps executed to obtain the result.
    pub trace: LookupTrace,
}

/// API-presence answer paired with its executed lookup trace.
#[derive(Debug)]
pub struct TracedApiPresenceOutcome {
    /// Authoritative answer, or `None` when the chain remains uncertain.
    pub answer: Option<bool>,
    /// Provider and source steps executed to obtain the answer.
    pub trace: LookupTrace,
}
