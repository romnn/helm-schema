use crate::lookup::K8sSchemaProvider;

use super::aggregator::aggregate;
use super::candidate::ApiVersionCandidate;
use super::inference_outcome::ApiVersionInferenceOutcome;

/// Top-level entry point: gather candidates across all providers and
/// apply the [`aggregate`] rule.
///
/// Each provider's `infer_api_version_candidates` is expected to do
/// all three tiers internally where applicable (shortlist + local
/// cache scan + online probe). This entry point owns the
/// cross-provider aggregation only.
#[must_use]
pub(crate) fn infer_api_version(
    providers: &[Box<dyn K8sSchemaProvider>],
    kind: &str,
) -> ApiVersionInferenceOutcome {
    let mut all: Vec<ApiVersionCandidate> = Vec::new();
    for provider in providers {
        all.extend(provider.infer_api_version_candidates(kind));
    }
    aggregate(all)
}
