use helm_schema_ir::{ResourceRef, YamlPath};

use super::chain_outcome::ChainLookupOutcome;
use super::provider_origin::ProviderOrigin;
use super::provider_result::ProviderLookupResult;

/// Executed lookup trace for one concrete resource/path query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LookupTrace {
    resource: ResourceRef,
    path: YamlPath,
    entries: Vec<LookupTraceEntry>,
}

impl LookupTrace {
    #[must_use]
    pub fn new(resource: &ResourceRef, path: &YamlPath) -> Self {
        Self {
            resource: resource.clone(),
            path: path.clone(),
            entries: Vec::new(),
        }
    }

    pub(crate) fn record_provider(
        &mut self,
        provider: ProviderOrigin,
        result: &ProviderLookupResult,
    ) {
        self.entries.push(LookupTraceEntry {
            provider,
            outcome: LookupTraceOutcome::from(result),
        });
    }

    #[must_use]
    pub fn resource(&self) -> &ResourceRef {
        &self.resource
    }

    #[must_use]
    pub fn path(&self) -> &YamlPath {
        &self.path
    }

    #[must_use]
    pub fn entries(&self) -> &[LookupTraceEntry] {
        &self.entries
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LookupTraceEntry {
    pub provider: ProviderOrigin,
    pub outcome: LookupTraceOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookupTraceOutcome {
    Found {
        resolved_k8s_version: Option<String>,
    },
    PathUnresolved,
    ResourceDocMissing {
        source_path: String,
        io_error: String,
    },
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

#[derive(Debug)]
pub struct TracedLookupOutcome {
    pub outcome: ChainLookupOutcome,
    pub trace: LookupTrace,
}

impl TracedLookupOutcome {
    pub(crate) fn into_outcome(self) -> ChainLookupOutcome {
        self.outcome
    }
}
