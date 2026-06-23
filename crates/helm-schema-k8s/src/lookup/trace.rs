use helm_schema_core::{ApiPresenceQuery, ProviderOrigin, ResourceRef, YamlPath};

use super::chain_outcome::ChainLookupOutcome;
use super::provider_result::ProviderLookupResult;

/// Executed lookup trace for one concrete schema-knowledge query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LookupTrace {
    subject: LookupTraceSubject,
    entries: Vec<LookupTraceEntry>,
}

impl LookupTrace {
    #[must_use]
    pub fn new(resource: &ResourceRef, path: &YamlPath) -> Self {
        Self {
            subject: LookupTraceSubject::ResourcePath {
                resource: resource.clone(),
                path: path.clone(),
            },
            entries: Vec::new(),
        }
    }

    #[must_use]
    pub fn new_api_presence(query: &ApiPresenceQuery) -> Self {
        Self {
            subject: LookupTraceSubject::ApiPresence {
                query: query.clone(),
            },
            entries: Vec::new(),
        }
    }

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

    #[must_use]
    pub fn subject(&self) -> &LookupTraceSubject {
        &self.subject
    }

    #[must_use]
    pub fn resource(&self) -> Option<&ResourceRef> {
        match &self.subject {
            LookupTraceSubject::ResourcePath { resource, .. } => Some(resource),
            LookupTraceSubject::ApiPresence { .. } => None,
        }
    }

    #[must_use]
    pub fn path(&self) -> Option<&YamlPath> {
        match &self.subject {
            LookupTraceSubject::ResourcePath { path, .. } => Some(path),
            LookupTraceSubject::ApiPresence { .. } => None,
        }
    }

    #[must_use]
    pub fn entries(&self) -> &[LookupTraceEntry] {
        &self.entries
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookupTraceSubject {
    ResourcePath {
        resource: ResourceRef,
        path: YamlPath,
    },
    ApiPresence {
        query: ApiPresenceQuery,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookupTraceEntry {
    ResourceProvider {
        resource: ResourceRef,
        provider: ProviderOrigin,
        outcome: LookupTraceOutcome,
    },
    ApiPresenceProvider {
        provider: ProviderOrigin,
        answer: Option<bool>,
    },
    ApiPresenceSourceProbe {
        provider: ProviderOrigin,
        source_id: String,
        k8s_version: String,
        filename: String,
        outcome: SourceProbeTraceOutcome,
    },
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceProbeTraceOutcome {
    Found,
    AuthoritativelyAbsent,
    Uncertain,
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

#[derive(Debug)]
pub struct TracedApiPresenceOutcome {
    pub answer: Option<bool>,
    pub trace: LookupTrace,
}

impl TracedApiPresenceOutcome {
    pub(crate) fn into_answer(self) -> Option<bool> {
        self.answer
    }
}
