use crate::lookup::ProviderOrigin;

use super::candidate::{ApiVersionCandidate, InferenceSource};
use super::inference_outcome::ApiVersionInferenceOutcome;

/// Cross-provider aggregation rule for Feature D.
///
/// The candidates here come from all providers' contributions for one
/// kind. The rule, in priority order:
///
/// 1. **`LocalOverride` candidates form a sub-partition that's evaluated
///    first.** Exactly one distinct `api_version` across override
///    candidates → `Resolved` with that candidate (other sources
///    ignored). Multiple distinct `api_version`s within the override
///    layer → `Ambiguous` — an internally-inconsistent override is no
///    longer authoritative.
/// 2. Otherwise, exactly one distinct `api_version` across non-override
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

    let overrides: Vec<&ApiVersionCandidate> = candidates
        .iter()
        .filter(|c| c.origin == ProviderOrigin::LocalOverride)
        .collect();

    if !overrides.is_empty() {
        let mut distinct = overrides
            .iter()
            .map(|c| c.api_version.clone())
            .collect::<Vec<_>>();
        distinct.sort();
        distinct.dedup();
        if distinct.len() == 1 {
            let chosen = overrides[0].clone();
            return ApiVersionInferenceOutcome::Resolved {
                api_version: chosen.api_version,
                source: chosen.source,
                origin: chosen.origin,
            };
        }
        // Internally-inconsistent override: ambiguous, override
        // candidates first, other candidates appended for context.
        let mut all = overrides.iter().map(|c| (*c).clone()).collect::<Vec<_>>();
        for c in &candidates {
            if c.origin != ProviderOrigin::LocalOverride {
                all.push(c.clone());
            }
        }
        return ApiVersionInferenceOutcome::Ambiguous { candidates: all };
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

fn sort_candidates(candidates: &mut [ApiVersionCandidate]) {
    candidates.sort_by(|a, b| {
        a.api_version
            .cmp(&b.api_version)
            .then_with(|| source_rank(a.source).cmp(&source_rank(b.source)))
            .then_with(|| format!("{:?}", a.origin).cmp(&format!("{:?}", b.origin)))
    });
}

fn source_rank(source: InferenceSource) -> u8 {
    match source {
        InferenceSource::Shortlist => 0,
        InferenceSource::LocalCacheScan => 1,
        InferenceSource::OnlineProbe => 2,
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
