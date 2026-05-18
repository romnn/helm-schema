use serde::{Deserialize, Serialize};

use crate::lookup::ProviderOrigin;

/// Which tier of [`crate::inference::infer_api_version`] produced a
/// candidate. Higher up the list = higher priority when multiple
/// tiers report the same apiVersion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InferenceSource {
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
    pub api_version: String,
    pub source: InferenceSource,
    pub origin: ProviderOrigin,
}
