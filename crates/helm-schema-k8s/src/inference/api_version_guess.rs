use crate::builtin_groups::is_k8s_builtin_group;
use crate::lookup::{K8sSchemaProvider, ProviderOrigin};

use super::aggregator::aggregate;
use super::candidate::{ApiVersionCandidate, InferenceSource};
use super::inference_outcome::ApiVersionInferenceOutcome;
use super::shortlist::canonical_api_version_for_kind;

/// Top-level entry point: gather candidates across all providers and
/// apply the [`aggregate`] rule.
///
/// Providers contribute source-specific candidates. This owner adds the
/// shared shortlist candidate once, then performs cross-provider aggregation.
#[must_use]
pub(crate) fn infer_api_version(
    providers: &[Box<dyn K8sSchemaProvider>],
    kind: &str,
) -> ApiVersionInferenceOutcome {
    let mut all: Vec<ApiVersionCandidate> = Vec::new();
    for provider in providers {
        all.extend(provider.infer_api_version_candidates(kind));
    }
    if let Some(api_version) = canonical_api_version_for_kind(kind) {
        let group = api_version.split_once('/').map_or("", |(group, _)| group);
        let origin = if is_k8s_builtin_group(group) {
            ProviderOrigin::KubernetesOpenApi
        } else {
            ProviderOrigin::DefaultCatalog
        };
        if providers.iter().any(|provider| provider.origin() == origin) {
            all.push(ApiVersionCandidate {
                api_version: api_version.to_string(),
                source: InferenceSource::Shortlist,
                origin,
            });
        }
    }
    aggregate(all)
}
