use crate::lookup::ProviderOrigin;

use super::candidate::{ApiVersionCandidate, InferenceSource};
use super::inference_outcome::ApiVersionInferenceOutcome;

/// Cross-provider aggregation rule for Feature D.
///
/// The candidates here come from all providers' contributions for one
/// kind. The rule, in priority order:
///
/// 1. **Authoritative local sources form sub-partitions evaluated in
///    precedence order.** `LocalOverride` is first; chart-bundled CRDs are
///    second. Exactly one distinct `api_version` inside the first present
///    authoritative partition resolves with that candidate and ignores remote
///    sources. Multiple distinct `api_version`s inside that partition stay
///    ambiguous — an internally-inconsistent local source is not collapsed.
/// 2. Otherwise, exactly one distinct `api_version` across non-authoritative
///    sources → `Resolved`. Reported `source` uses the priority
///    `Shortlist > LocalCacheScan > OnlineProbe` when multiple tiers
///    contribute the same `api_version`.
/// 3. Otherwise `Ambiguous`.
#[must_use]
pub fn aggregate(mut candidates: Vec<ApiVersionCandidate>) -> ApiVersionInferenceOutcome {
    if candidates.is_empty() {
        return ApiVersionInferenceOutcome::NoMatch;
    }

    // Stable canonicalisation: same input order = same output.
    sort_candidates(&mut candidates);
    candidates.dedup();

    for authoritative_origin in [ProviderOrigin::LocalOverride, ProviderOrigin::ChartLocalCrd] {
        let authoritative: Vec<&ApiVersionCandidate> = candidates
            .iter()
            .filter(|c| c.origin == authoritative_origin)
            .collect();

        if !authoritative.is_empty() {
            return aggregate_authoritative_partition(
                authoritative_origin,
                &authoritative,
                &candidates,
            );
        }
    }

    // Exactly one distinct api_version ⇒ resolved. The list is sorted
    // with `api_version` as the primary key, so first == last decides,
    // and the first candidate reports the highest-priority
    // (source, origin) pair for it.
    let single_api_version = candidates
        .first()
        .zip(candidates.last())
        .is_some_and(|(first, last)| first.api_version == last.api_version);
    if !single_api_version {
        return ApiVersionInferenceOutcome::Ambiguous { candidates };
    }
    let Some(chosen) = candidates.into_iter().next() else {
        return ApiVersionInferenceOutcome::NoMatch;
    };
    ApiVersionInferenceOutcome::Resolved {
        api_version: chosen.api_version,
        source: chosen.source,
        origin: chosen.origin,
    }
}

fn aggregate_authoritative_partition(
    origin: ProviderOrigin,
    authoritative: &[&ApiVersionCandidate],
    candidates: &[ApiVersionCandidate],
) -> ApiVersionInferenceOutcome {
    // Same first == last shortcut as in `aggregate`: the partition is
    // a filtered projection of the sorted candidate list, so it stays
    // sorted with `api_version` as the primary key.
    let single_api_version = authoritative
        .first()
        .zip(authoritative.last())
        .is_some_and(|(first, last)| first.api_version == last.api_version);
    if single_api_version && let Some(chosen) = authoritative.first() {
        return ApiVersionInferenceOutcome::Resolved {
            api_version: chosen.api_version.clone(),
            source: chosen.source,
            origin: chosen.origin,
        };
    }

    let mut all = authoritative
        .iter()
        .map(|candidate| (*candidate).clone())
        .collect::<Vec<_>>();
    for candidate in candidates {
        if candidate.origin != origin {
            all.push(candidate.clone());
        }
    }
    ApiVersionInferenceOutcome::Ambiguous { candidates: all }
}

fn sort_candidates(candidates: &mut [ApiVersionCandidate]) {
    candidates.sort_by(|a, b| {
        a.api_version
            .cmp(&b.api_version)
            .then_with(|| source_rank(a.source).cmp(&source_rank(b.source)))
            .then_with(|| origin_rank(a.origin).cmp(&origin_rank(b.origin)))
    });
}

fn source_rank(source: InferenceSource) -> u8 {
    match source {
        InferenceSource::ChartLocalCrd => 0,
        InferenceSource::Shortlist => 1,
        InferenceSource::LocalCacheScan => 2,
        InferenceSource::OnlineProbe => 3,
    }
}

fn origin_rank(origin: ProviderOrigin) -> u8 {
    match origin {
        ProviderOrigin::LocalOverride => 0,
        ProviderOrigin::ChartLocalCrd => 1,
        ProviderOrigin::DefaultCatalog => 2,
        ProviderOrigin::KubernetesOpenApi => 3,
    }
}

#[cfg(test)]
#[path = "tests/aggregator.rs"]
mod tests;
