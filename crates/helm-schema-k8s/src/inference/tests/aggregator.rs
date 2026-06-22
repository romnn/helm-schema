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
