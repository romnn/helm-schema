use crate::lookup::ProviderOrigin;

use super::candidate::{ApiVersionCandidate, InferenceSource};

/// Outcome of inferring apiVersion for a kind whose `api_version`
/// (and `api_version_candidates`) were empty. Unlike resource
/// lookup, this is an aggregate-then-decide contract: providers
/// contribute candidates and ambiguity is meaningful here.
#[derive(Debug, Clone)]
pub enum ApiVersionInferenceOutcome {
    /// One candidate outranked all alternatives.
    Resolved {
        /// API version selected for the resource kind.
        api_version: String,
        /// Evidence tier that supplied the winning candidate.
        source: InferenceSource,
        /// Provider family that supplied the winning candidate.
        origin: ProviderOrigin,
    },
    /// Several equally ranked candidates remain viable.
    Ambiguous {
        /// Stable set of candidates and their provenance.
        candidates: Vec<ApiVersionCandidate>,
    },
    /// No configured provider supplied a candidate.
    NoMatch,
}
