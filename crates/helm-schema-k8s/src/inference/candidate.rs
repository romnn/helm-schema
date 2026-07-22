use serde::{Deserialize, Serialize};

use crate::lookup::ProviderOrigin;

/// Which tier of `infer_api_version` produced a
/// candidate. Higher up the list = higher priority when multiple
/// tiers report the same apiVersion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InferenceSource {
    /// CRD document bundled directly in the chart's static `crds/` directory.
    ChartLocalCrd,
    /// Hardcoded canonical `kind → apiVersion` table.
    Shortlist,
    /// Scan across all configured K8s + CRD cache namespaces.
    LocalCacheScan,
    /// Kind-scoped HTTP probe against the upstream catalog/mirrors.
    OnlineProbe,
}

/// A single apiVersion candidate contributed by a provider during
/// Feature D inference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ApiVersionCandidate {
    /// API version proposed for the resource kind.
    pub api_version: String,
    /// Evidence tier that produced the candidate.
    pub source: InferenceSource,
    /// Provider family that supplied the evidence.
    pub origin: ProviderOrigin,
}
