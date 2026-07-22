use serde::{Deserialize, Serialize};

use crate::inference::candidate::{ApiVersionCandidate, InferenceSource};
use crate::lookup::ProviderOrigin;

use super::canonicalise::{canonicalise_candidates, canonicalise_strings};

/// Identity key used to dedupe diagnostics. Diagnostics with the same
/// key are considered "the same logical event"; only the first one
/// inserted into a [`crate::diagnostic::DiagnosticSink`] is kept.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiagnosticKey {
    /// No provider could supply the requested resource schema.
    MissingSchema {
        /// Kubernetes resource kind.
        kind: String,
        /// API version requested by the chart.
        api_version: String,
    },
    /// A compatible Kubernetes release supplied the resource schema.
    ResolvedFromFallbackVersion {
        /// Kubernetes resource kind.
        kind: String,
        /// API version requested by the chart.
        api_version: String,
        /// Kubernetes release that supplied the schema.
        resolved_version: String,
    },
    /// Static or bounded fallback analysis inferred a missing API version.
    InferredApiVersion {
        /// Kubernetes resource kind.
        kind: String,
        /// API version selected by inference.
        inferred_api_version: String,
        /// Evidence tier that produced the candidate.
        source: InferenceSource,
    },
    /// More than one API version remains viable for a resource kind.
    AmbiguousApiVersion {
        /// Kubernetes resource kind with ambiguous candidates.
        kind: String,
    },
    /// A CRD catalog owns the group and kind but lacks the requested version.
    CrdVersionNotFound {
        /// CRD API group.
        group: String,
        /// CRD resource kind.
        kind: String,
        /// Version requested by the chart.
        requested_version: String,
    },
    /// A CRD exists at versions other than the one requested.
    CrdVersionAvailableAtOtherVersions {
        /// CRD API group.
        group: String,
        /// CRD resource kind.
        kind: String,
        /// Version requested by the chart.
        requested_version: String,
    },
    /// A configured local override exists but cannot be read.
    LocalOverrideUnreadable {
        /// Kubernetes resource kind.
        kind: String,
        /// API version requested by the chart.
        api_version: String,
        /// Filesystem path of the unreadable override.
        override_path: String,
    },
    /// An older cache layout was invalidated before use.
    CacheLayoutInvalidated {
        /// Root directory containing the cache.
        cache_root: String,
        /// Layout marker previously found on disk.
        previous_marker: Option<u32>,
    },
    /// The on-disk cache was written by a newer incompatible binary.
    CacheLayoutForwardIncompatible {
        /// Root directory containing the cache.
        cache_root: String,
        /// Newer layout marker found on disk.
        on_disk_marker: u32,
    },
    /// Input channels give the same JSON number different Helm range semantics.
    InputChannelNumericRangeAmbiguity {
        /// Values path affected by the ambiguity.
        value_path: String,
    },
}

/// User-facing diagnostic. Every event helm-schema emits at runtime is
/// one of these. `Diagnostic::key` produces the deduplication key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Diagnostic {
    /// No configured provider could supply a resource schema.
    MissingSchema {
        /// Kubernetes resource kind.
        kind: String,
        /// API version requested by the chart.
        api_version: String,
        /// Kubernetes releases consulted in order.
        k8s_versions_tried: Vec<String>,
        /// Candidate schema filenames consulted.
        tried_filenames: Vec<String>,
        /// Releases whose local caches contain the resource.
        available_in_cache_versions: Vec<String>,
        /// Older release likely to contain the removed API.
        suggested_k8s_version: Option<String>,
        /// Actionable context for resolving the lookup failure.
        hint: Option<String>,
    },
    /// A fallback Kubernetes release supplied the requested API schema.
    ResolvedFromFallbackVersion {
        /// Kubernetes resource kind.
        kind: String,
        /// API version requested by the chart.
        api_version: String,
        /// Primary Kubernetes release selected by policy.
        primary_version: String,
        /// Fallback Kubernetes release that supplied the schema.
        resolved_version: String,
    },
    /// Analysis inferred an API version omitted from the resource.
    InferredApiVersion {
        /// Kubernetes resource kind.
        kind: String,
        /// API version selected by inference.
        inferred_api_version: String,
        /// Evidence tier that produced the candidate.
        source: InferenceSource,
        /// Provider family that supplied the evidence.
        origin: ProviderOrigin,
    },
    /// Several inferred API versions remain equally viable.
    AmbiguousApiVersion {
        /// Kubernetes resource kind.
        kind: String,
        /// Stable set of candidates and their provenance.
        candidates: Vec<ApiVersionCandidate>,
    },
    /// A CRD catalog lacks the requested version.
    CrdVersionNotFound {
        /// CRD API group.
        group: String,
        /// CRD resource kind.
        kind: String,
        /// Version requested by the chart.
        requested_version: String,
        /// Cache paths and upstream locations consulted.
        locations_tried: Vec<String>,
    },
    /// A CRD exists, but only at other versions.
    CrdVersionAvailableAtOtherVersions {
        /// CRD API group.
        group: String,
        /// CRD resource kind.
        kind: String,
        /// Version requested by the chart.
        requested_version: String,
        /// Versions found for the same group and kind.
        available_versions: Vec<String>,
    },
    /// A configured local schema override could not be read.
    LocalOverrideUnreadable {
        /// Kubernetes resource kind.
        kind: String,
        /// API version requested by the chart.
        api_version: String,
        /// Filesystem path of the override.
        override_path: String,
        /// Human-readable I/O failure.
        io_error: String,
    },
    /// An obsolete cache layout was discarded.
    CacheLayoutInvalidated {
        /// Root directory containing the cache.
        cache_root: String,
        /// Layout marker previously found on disk.
        previous_marker: Option<u32>,
        /// Layout marker required by this binary.
        current_marker: u32,
    },
    /// A newer on-disk cache layout cannot be read safely.
    CacheLayoutForwardIncompatible {
        /// Root directory containing the cache.
        cache_root: String,
        /// Newer layout marker found on disk.
        on_disk_marker: u32,
        /// Layout marker understood by this binary.
        compiled_marker: u32,
    },
    /// A one-variable Helm range can iterate an integer supplied through
    /// `--set`, while the same JSON number supplied by a values file or
    /// `--set-json` has a non-rangeable runtime kind. JSON Schema cannot
    /// distinguish those input channels.
    InputChannelNumericRangeAmbiguity {
        /// Values path affected by channel-dependent numeric semantics.
        value_path: String,
    },
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
