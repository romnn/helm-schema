use serde::{Deserialize, Serialize};

use crate::inference::candidate::{ApiVersionCandidate, InferenceSource};
use crate::lookup::ProviderOrigin;

use super::canonicalise::{canonicalise_candidates, canonicalise_strings};

/// Identity key used to dedupe diagnostics. Diagnostics with the same
/// key are considered "the same logical event"; only the first one
/// inserted into a [`crate::diagnostic::DiagnosticSink`] is kept.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiagnosticKey {
    MissingSchema {
        kind: String,
        api_version: String,
    },
    ResolvedFromFallbackVersion {
        kind: String,
        api_version: String,
        resolved_version: String,
    },
    InferredApiVersion {
        kind: String,
        inferred_api_version: String,
        source: InferenceSource,
    },
    AmbiguousApiVersion {
        kind: String,
    },
    CrdVersionNotFound {
        group: String,
        kind: String,
        requested_version: String,
    },
    CrdVersionAvailableAtOtherVersions {
        group: String,
        kind: String,
        requested_version: String,
    },
    LocalOverrideUnreadable {
        kind: String,
        api_version: String,
        override_path: String,
    },
    CacheLayoutInvalidated {
        cache_root: String,
        previous_marker: Option<u32>,
    },
    CacheLayoutForwardIncompatible {
        cache_root: String,
        on_disk_marker: u32,
    },
    InputChannelNumericRangeAmbiguity {
        value_path: String,
    },
}

/// User-facing diagnostic. Every event helm-schema emits at runtime is
/// one of these. `Diagnostic::key` produces the deduplication key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Diagnostic {
    MissingSchema {
        kind: String,
        api_version: String,
        k8s_versions_tried: Vec<String>,
        tried_filenames: Vec<String>,
        available_in_cache_versions: Vec<String>,
        suggested_k8s_version: Option<String>,
        hint: Option<String>,
    },
    ResolvedFromFallbackVersion {
        kind: String,
        api_version: String,
        primary_version: String,
        resolved_version: String,
    },
    InferredApiVersion {
        kind: String,
        inferred_api_version: String,
        source: InferenceSource,
        origin: ProviderOrigin,
    },
    AmbiguousApiVersion {
        kind: String,
        candidates: Vec<ApiVersionCandidate>,
    },
    CrdVersionNotFound {
        group: String,
        kind: String,
        requested_version: String,
        locations_tried: Vec<String>,
    },
    CrdVersionAvailableAtOtherVersions {
        group: String,
        kind: String,
        requested_version: String,
        available_versions: Vec<String>,
    },
    LocalOverrideUnreadable {
        kind: String,
        api_version: String,
        override_path: String,
        io_error: String,
    },
    CacheLayoutInvalidated {
        cache_root: String,
        previous_marker: Option<u32>,
        current_marker: u32,
    },
    CacheLayoutForwardIncompatible {
        cache_root: String,
        on_disk_marker: u32,
        compiled_marker: u32,
    },
    /// A one-variable Helm range can iterate an integer supplied through
    /// `--set`, while the same JSON number supplied by a values file or
    /// `--set-json` has a non-rangeable runtime kind. JSON Schema cannot
    /// distinguish those input channels.
    InputChannelNumericRangeAmbiguity { value_path: String },
}

impl Diagnostic {
    /// The deduplication key for this diagnostic.
    #[must_use]
    pub fn key(&self) -> DiagnosticKey {
        match self {
            Diagnostic::MissingSchema {
                kind, api_version, ..
            } => DiagnosticKey::MissingSchema {
                kind: kind.clone(),
                api_version: api_version.clone(),
            },
            Diagnostic::ResolvedFromFallbackVersion {
                kind,
                api_version,
                resolved_version,
                ..
            } => DiagnosticKey::ResolvedFromFallbackVersion {
                kind: kind.clone(),
                api_version: api_version.clone(),
                resolved_version: resolved_version.clone(),
            },
            Diagnostic::InferredApiVersion {
                kind,
                inferred_api_version,
                source,
                ..
            } => DiagnosticKey::InferredApiVersion {
                kind: kind.clone(),
                inferred_api_version: inferred_api_version.clone(),
                source: *source,
            },
            Diagnostic::AmbiguousApiVersion { kind, .. } => {
                DiagnosticKey::AmbiguousApiVersion { kind: kind.clone() }
            }
            Diagnostic::CrdVersionNotFound {
                group,
                kind,
                requested_version,
                ..
            } => DiagnosticKey::CrdVersionNotFound {
                group: group.clone(),
                kind: kind.clone(),
                requested_version: requested_version.clone(),
            },
            Diagnostic::CrdVersionAvailableAtOtherVersions {
                group,
                kind,
                requested_version,
                ..
            } => DiagnosticKey::CrdVersionAvailableAtOtherVersions {
                group: group.clone(),
                kind: kind.clone(),
                requested_version: requested_version.clone(),
            },
            Diagnostic::LocalOverrideUnreadable {
                kind,
                api_version,
                override_path,
                ..
            } => DiagnosticKey::LocalOverrideUnreadable {
                kind: kind.clone(),
                api_version: api_version.clone(),
                override_path: override_path.clone(),
            },
            Diagnostic::CacheLayoutInvalidated {
                cache_root,
                previous_marker,
                ..
            } => DiagnosticKey::CacheLayoutInvalidated {
                cache_root: cache_root.clone(),
                previous_marker: *previous_marker,
            },
            Diagnostic::CacheLayoutForwardIncompatible {
                cache_root,
                on_disk_marker,
                ..
            } => DiagnosticKey::CacheLayoutForwardIncompatible {
                cache_root: cache_root.clone(),
                on_disk_marker: *on_disk_marker,
            },
            Diagnostic::InputChannelNumericRangeAmbiguity { value_path } => {
                DiagnosticKey::InputChannelNumericRangeAmbiguity {
                    value_path: value_path.clone(),
                }
            }
        }
    }

    /// Canonicalise mutable list fields so two emissions of the same
    /// logical event produce identical payloads regardless of probe
    /// order.
    pub(crate) fn canonicalise(&mut self) {
        match self {
            Diagnostic::MissingSchema {
                k8s_versions_tried,
                tried_filenames,
                available_in_cache_versions,
                ..
            } => {
                canonicalise_strings(k8s_versions_tried);
                canonicalise_strings(tried_filenames);
                canonicalise_strings(available_in_cache_versions);
            }
            Diagnostic::AmbiguousApiVersion { candidates, .. } => {
                canonicalise_candidates(candidates);
            }
            Diagnostic::CrdVersionNotFound {
                locations_tried, ..
            } => canonicalise_strings(locations_tried),
            Diagnostic::CrdVersionAvailableAtOtherVersions {
                available_versions, ..
            } => canonicalise_strings(available_versions),
            Diagnostic::ResolvedFromFallbackVersion { .. }
            | Diagnostic::InferredApiVersion { .. }
            | Diagnostic::LocalOverrideUnreadable { .. }
            | Diagnostic::CacheLayoutInvalidated { .. }
            | Diagnostic::CacheLayoutForwardIncompatible { .. }
            | Diagnostic::InputChannelNumericRangeAmbiguity { .. } => {}
        }
    }
}
