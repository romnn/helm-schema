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
    candidates.dedup_by(|a, b| {
        a.api_version == b.api_version && a.source == b.source && a.origin == b.origin
    });

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

    let mut distinct = candidates
        .iter()
        .map(|c| c.api_version.clone())
        .collect::<Vec<_>>();
    distinct.sort();
    distinct.dedup();
    if distinct.len() == 1 {
        // Pick the highest-priority source for the reported source field.
        let api_version = distinct.into_iter().next().unwrap_or_default();
        let chosen = best_source_candidate(&candidates);
        return ApiVersionInferenceOutcome::Resolved {
            api_version,
            source: chosen.source,
            origin: chosen.origin,
        };
    }

    ApiVersionInferenceOutcome::Ambiguous { candidates }
}

fn aggregate_authoritative_partition(
    origin: ProviderOrigin,
    authoritative: &[&ApiVersionCandidate],
    candidates: &[ApiVersionCandidate],
) -> ApiVersionInferenceOutcome {
    let mut distinct = authoritative
        .iter()
        .map(|c| c.api_version.clone())
        .collect::<Vec<_>>();
    distinct.sort();
    distinct.dedup();
    if distinct.len() == 1 {
        let chosen = authoritative[0].clone();
        return ApiVersionInferenceOutcome::Resolved {
            api_version: chosen.api_version,
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

fn best_source_candidate(candidates: &[ApiVersionCandidate]) -> ApiVersionCandidate {
    let mut best = candidates.first().cloned().unwrap_or(ApiVersionCandidate {
        api_version: String::new(),
        source: InferenceSource::Shortlist,
        origin: ProviderOrigin::DefaultCatalog,
    });
    for c in candidates.iter().skip(1) {
        if source_rank(c.source) < source_rank(best.source) {
            best = c.clone();
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_util::prelude::sim_assert_eq;

    fn candidate(
        api_version: &str,
        source: InferenceSource,
        origin: ProviderOrigin,
    ) -> ApiVersionCandidate {
        ApiVersionCandidate {
            api_version: api_version.to_string(),
            source,
            origin,
        }
    }

    #[test]
    fn chart_local_crd_candidate_beats_remote_catalog_candidate() {
        let outcome = aggregate(vec![
            candidate(
                "remote.example.com/v1",
                InferenceSource::Shortlist,
                ProviderOrigin::DefaultCatalog,
            ),
            candidate(
                "local.example.com/v1",
                InferenceSource::ChartLocalCrd,
                ProviderOrigin::ChartLocalCrd,
            ),
        ]);

        let ApiVersionInferenceOutcome::Resolved {
            api_version,
            source,
            origin,
        } = outcome
        else {
            panic!("expected resolved chart-local CRD candidate, got {outcome:?}");
        };
        sim_assert_eq!(
            have: (api_version.as_str(), source, origin),
            want: (
                "local.example.com/v1",
                InferenceSource::ChartLocalCrd,
                ProviderOrigin::ChartLocalCrd
            )
        );
    }

    #[test]
    fn explicit_override_candidate_beats_chart_local_crd_candidate() {
        let outcome = aggregate(vec![
            candidate(
                "local.example.com/v1",
                InferenceSource::ChartLocalCrd,
                ProviderOrigin::ChartLocalCrd,
            ),
            candidate(
                "override.example.com/v1",
                InferenceSource::Shortlist,
                ProviderOrigin::LocalOverride,
            ),
        ]);

        let ApiVersionInferenceOutcome::Resolved {
            api_version,
            source,
            origin,
        } = outcome
        else {
            panic!("expected resolved explicit override candidate, got {outcome:?}");
        };
        sim_assert_eq!(
            have: (api_version.as_str(), source, origin),
            want: (
                "override.example.com/v1",
                InferenceSource::Shortlist,
                ProviderOrigin::LocalOverride
            )
        );
    }
}
