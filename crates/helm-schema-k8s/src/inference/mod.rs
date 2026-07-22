mod aggregator;
mod api_version_guess;
/// Deterministic scans of configured local schema caches.
pub mod cache_scan;
/// API-version candidates and their evidence tier.
pub mod candidate;
mod inference_outcome;
/// Kind-scoped upstream probing for bounded fallback inference.
pub mod online_probe;
/// Canonical API versions for well-known resource kinds.
pub mod shortlist;

pub use aggregator::aggregate;
pub(crate) use api_version_guess::infer_api_version;
pub use candidate::{ApiVersionCandidate, InferenceSource};
pub use inference_outcome::ApiVersionInferenceOutcome;
