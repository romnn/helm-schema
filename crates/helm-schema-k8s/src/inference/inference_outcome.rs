use crate::lookup::ProviderOrigin;

use super::candidate::{ApiVersionCandidate, InferenceSource};

/// Outcome of inferring apiVersion for a kind whose `api_version`
/// (and `api_version_candidates`) were empty. Unlike resource
/// lookup, this is an aggregate-then-decide contract: providers
/// contribute candidates and ambiguity is meaningful here.
#[derive(Debug)]
pub enum ApiVersionInferenceOutcome {
    Resolved {
        api_version: String,
        source: InferenceSource,
        origin: ProviderOrigin,
    },
    Ambiguous {
        candidates: Vec<ApiVersionCandidate>,
    },
    NoMatch,
}
