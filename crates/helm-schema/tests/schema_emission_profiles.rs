//! Monotonicity and semantic-oracle harness for schema emission profiles.

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use serde::Serialize;
use serde_json::json;
use test_util::prelude::sim_assert_eq;

#[path = "common/emission_profile_harness.rs"]
mod harness;

#[path = "common/helm_adjudication.rs"]
mod helm_adjudication;

#[path = "common/kubernetes_version.rs"]
mod kubernetes_version;

#[path = "common/known_false_acceptances.rs"]
mod known_false_acceptances;

use helm_adjudication::{KubernetesVerdict, OfflineKubernetesValidator, PinnedHelmChart};
use known_false_acceptances::{
    Baseline, Family, KNOWN_FALSE_ACCEPTANCES, KNOWN_UNDECIDED_ACCEPTANCES, KnownFalseAcceptances,
    KnownUndecidedAcceptances, Probe, Rejection,
};

use harness::{
    ContractVerdict, ControlCategory, GuardSamplingStrategy, ProbeCoverage, ProbeInstance,
    ProfileSchemas, SemanticControl, Transport, generate_profile_outputs, generate_profile_schemas,
    read_chart_schema_fixture, read_coalesced_defaults, read_json_fixture,
    rejects_for_a_new_reason, round_robin_base_probes, sparse_override,
    sparse_override_for_composed, structural_probe_battery, structural_probe_battery_with_coverage,
};

#[derive(Debug, Serialize)]
struct ProbeCoverageReport {
    baseline_ref: String,
    charts: Vec<ProbeCoverage>,
    helm_adjudication: HelmAdjudicationCoverage,
}

// A non-zero allowance must land before the live run whose widening relies on it.
const PREREGISTERED_ACCEPTANCE_FLIP_ALLOWANCE: usize = 0;

#[derive(Debug, Default, Serialize)]
struct HelmAdjudicationCoverage {
    enabled: bool,
    /// Rust screening can miss flips that exact Helm coalescence would expose.
    screening_is_exact: bool,
    screened_flips: usize,
    screened_flips_collapsed: usize,
    flips_adjudicated: usize,
    tightenings_matched_helm_abort: usize,
    tightenings_matched_kubernetes_rejection: usize,
    loosenings_matched_kubernetes_validation: usize,
    loosenings_matched_defaults_violations: usize,
    /// Accepted cells Helm renders whose changed resources Kubernetes cannot
    /// decide.
    loosenings_with_uncertain_kubernetes: Vec<ObservedUndecidedAcceptance>,
    /// Labels of the charts whose flips were adjudicated live.
    charts_adjudicated: std::collections::BTreeSet<String>,
    /// Accepted cells Helm or Kubernetes reject.
    false_acceptances: Vec<ObservedFalseAcceptance>,
}

/// A false acceptance the battery observed, to be matched by the roster.
#[derive(Debug, Serialize)]
struct ObservedFalseAcceptance {
    case: String,
    rejection: Rejection,
    baseline: Baseline,
}

/// An accepted cell Kubernetes cannot decide, to be matched by the roster.
#[derive(Debug, Serialize)]
struct ObservedUndecidedAcceptance {
    case: String,
    uncertain: Vec<String>,
}

impl HelmAdjudicationCoverage {
    fn record_verdict(&mut self, verdict: HelmFlipVerdict, case: String) -> Option<bool> {
        let candidate_accepts = match verdict {
            HelmFlipVerdict::Collapsed => {
                self.screened_flips_collapsed += 1;
                return None;
            }
            HelmFlipVerdict::TighteningMatchedHelmAbort => {
                self.tightenings_matched_helm_abort += 1;
                false
            }
            HelmFlipVerdict::TighteningMatchedKubernetesRejection => {
                self.tightenings_matched_kubernetes_rejection += 1;
                false
            }
            HelmFlipVerdict::LooseningMatchedKubernetesValidation => {
                self.loosenings_matched_kubernetes_validation += 1;
                true
            }
            HelmFlipVerdict::LooseningMatchedDefaultsViolations => {
                self.loosenings_matched_defaults_violations += 1;
                true
            }
            HelmFlipVerdict::LooseningWithUncertainKubernetes(uncertain) => {
                self.loosenings_with_uncertain_kubernetes
                    .push(ObservedUndecidedAcceptance { case, uncertain });
                true
            }
            HelmFlipVerdict::CandidateAcceptsHelmAborts => {
                self.false_acceptances.push(ObservedFalseAcceptance {
                    case,
                    rejection: Rejection::HelmAborts,
                    baseline: Baseline::RejectsUnlikeItsDefaults,
                });
                true
            }
            HelmFlipVerdict::CandidateAcceptsKubernetesRejects => {
                self.false_acceptances.push(ObservedFalseAcceptance {
                    case,
                    rejection: Rejection::KubernetesRejects,
                    baseline: Baseline::RejectsUnlikeItsDefaults,
                });
                true
            }
            HelmFlipVerdict::UninformativeBaselineFalseAcceptance(rejection) => {
                self.false_acceptances.push(ObservedFalseAcceptance {
                    case,
                    rejection,
                    baseline: Baseline::RejectsItsDefaults,
                });
                true
            }
        };
        self.flips_adjudicated += 1;
        Some(candidate_accepts)
    }
}

/// Every problem with the adjudicated outcomes: unaccounted flips, and
/// accepted cells Kubernetes cannot decide or Helm or Kubernetes reject that
/// the rosters do not list, or that they list but that no longer fail alike
/// on a chart this run adjudicated.
fn validate_helm_adjudication_coverage(
    coverage: &HelmAdjudicationCoverage,
    roster: &[KnownFalseAcceptances],
    undecided_roster: &[KnownUndecidedAcceptances],
) -> eyre::Result<()> {
    let mut problems = Vec::new();
    if coverage.flips_adjudicated
        != coverage.tightenings_matched_helm_abort
            + coverage.tightenings_matched_kubernetes_rejection
            + coverage.loosenings_matched_kubernetes_validation
            + coverage.loosenings_matched_defaults_violations
            + coverage.loosenings_with_uncertain_kubernetes.len()
            + coverage.false_acceptances.len()
    {
        problems.push(format!(
            "adjudicated flip outcome accounting mismatch: {coverage:?}"
        ));
    }
    let mut unlisted_undecided = Vec::new();
    for observed in &coverage.loosenings_with_uncertain_kubernetes {
        let listed = undecided_roster
            .iter()
            .any(|group| group.lists(&observed.case, &observed.uncertain));
        if !listed {
            unlisted_undecided.push(format!("{} ({:?})", observed.case, observed.uncertain));
        }
    }
    if !unlisted_undecided.is_empty() {
        problems.push(format!(
            "accepted cells whose changed resources Kubernetes could not decide, missing \
             from KNOWN_UNDECIDED_ACCEPTANCES: {unlisted_undecided:?}"
        ));
    }
    let mut unlisted = Vec::new();
    for observed in &coverage.false_acceptances {
        let listed = roster
            .iter()
            .any(|group| group.lists(&observed.case, observed.rejection, observed.baseline));
        if !listed {
            unlisted.push(format!(
                "{} ({:?}, {:?})",
                observed.case, observed.rejection, observed.baseline
            ));
        }
    }
    if !unlisted.is_empty() {
        problems.push(format!(
            "false acceptances missing from KNOWN_FALSE_ACCEPTANCES: {unlisted:?}"
        ));
    }
    let mut fixed = Vec::new();
    for group in roster {
        if !coverage.charts_adjudicated.contains(group.chart) {
            continue;
        }
        for probe in group.probes {
            let observed = coverage.false_acceptances.iter().any(|observed| {
                observed.rejection == group.rejection
                    && observed.baseline == group.baseline
                    && known_false_acceptances::names(group.chart, probe, &observed.case)
            });
            if !observed {
                fixed.push(format!(
                    "{}: {} <- {} ({:?}, {:?}, {:?})",
                    group.chart,
                    probe.path,
                    probe.value,
                    group.rejection,
                    group.baseline,
                    group.family
                ));
            }
        }
    }
    for group in undecided_roster {
        if !coverage.charts_adjudicated.contains(group.chart) {
            continue;
        }
        for probe in group.probes {
            let observed = coverage
                .loosenings_with_uncertain_kubernetes
                .iter()
                .any(|observed| {
                    group.uncertain == observed.uncertain.as_slice()
                        && known_false_acceptances::names(group.chart, probe, &observed.case)
                });
            if !observed {
                fixed.push(format!(
                    "{}: {} <- {} (undecided: {:?})",
                    group.chart, probe.path, probe.value, group.uncertain
                ));
            }
        }
    }
    if !fixed.is_empty() {
        problems.push(format!(
            "roster entries that no longer fail alike; remove or re-adjudicate them: {fixed:?}"
        ));
    }
    eyre::ensure!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}

#[test]
fn matched_flip_validation_accepts_confirmed_outcomes() -> eyre::Result<()> {
    let mut coverage = HelmAdjudicationCoverage::default();
    for verdict in [
        HelmFlipVerdict::TighteningMatchedHelmAbort,
        HelmFlipVerdict::TighteningMatchedKubernetesRejection,
        HelmFlipVerdict::LooseningMatchedKubernetesValidation,
        HelmFlipVerdict::LooseningMatchedDefaultsViolations,
    ] {
        coverage.record_verdict(verdict, "confirmed".to_string());
        validate_helm_adjudication_coverage(&coverage, &[], &[])?;
    }
    Ok(())
}

/// A loosening is matched only by complete evidence: a changed resource
/// Kubernetes cannot decide leaves the acceptance unproved.
#[test]
fn uncertain_loosening_is_not_matched_by_the_render() {
    let mut coverage = HelmAdjudicationCoverage::default();
    coverage.record_verdict(
        HelmFlipVerdict::LooseningWithUncertainKubernetes(vec!["reason".to_string()]),
        "uncertain".to_string(),
    );
    assert!(validate_helm_adjudication_coverage(&coverage, &[], &[]).is_err());
}

/// The undecided roster lists an accepted cell only with the exact
/// uncertainty the adjudicator reported for it, and a listed cell of an
/// adjudicated chart that is no longer undecided alike must be removed.
#[test]
fn known_undecided_acceptances_are_matched_exactly() -> eyre::Result<()> {
    const PROBES: &[Probe] = &[Probe {
        path: "a",
        value: "true",
    }];
    const WITH_FIXED: &[Probe] = &[
        Probe {
            path: "a",
            value: "true",
        },
        Probe {
            path: "fixed",
            value: "true",
        },
    ];
    let group = |chart, uncertain, probes| KnownUndecidedAcceptances {
        chart,
        uncertain,
        probes,
    };
    let mut coverage = HelmAdjudicationCoverage::default();
    coverage.charts_adjudicated.insert("chart".to_string());
    coverage.record_verdict(
        HelmFlipVerdict::LooseningWithUncertainKubernetes(vec![
            "document 1: example.test/v1/Kind name: pinned CRD schema not found".to_string(),
        ]),
        "chart: a <- true".to_string(),
    );
    let reported: &[&str] = &["document 1: example.test/v1/Kind name: pinned CRD schema not found"];
    validate_helm_adjudication_coverage(&coverage, &[], &[group("chart", reported, PROBES)])?;
    // A chart this run did not adjudicate keeps its entries.
    validate_helm_adjudication_coverage(
        &coverage,
        &[],
        &[
            group("chart", reported, PROBES),
            group("other", reported, WITH_FIXED),
        ],
    )?;
    let other_reason: &[&str] =
        &["document 1: example.test/v1/Other name: pinned CRD schema not found"];
    let with_more: &[&str] = &[
        "document 1: example.test/v1/Kind name: pinned CRD schema not found",
        "document 2: example.test/v1/Other name: pinned CRD schema not found",
    ];
    for roster in [
        vec![group("chart", other_reason, PROBES)],
        vec![group("chart", with_more, PROBES)],
        vec![group("other", reported, PROBES)],
        vec![group("chart", reported, WITH_FIXED)],
    ] {
        assert!(validate_helm_adjudication_coverage(&coverage, &[], &roster).is_err());
    }
    // An undecided acceptance is no false acceptance: the false-acceptance
    // roster does not list it.
    let false_acceptances = [KnownFalseAcceptances {
        chart: "chart",
        rejection: Rejection::KubernetesRejects,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::Unfiled,
        probes: PROBES,
    }];
    assert!(validate_helm_adjudication_coverage(&coverage, &false_acceptances, &[]).is_err());
    Ok(())
}

fn validate_probe_coverage(chart: &ProbeCoverage) -> eyre::Result<()> {
    eyre::ensure!(
        chart.base_emitted > 0,
        "targeted probes consumed the base-probe budget: {chart:?}"
    );
    eyre::ensure!(
        chart.base_dropped == 0,
        "base probes were truncated: {chart:?}"
    );
    eyre::ensure!(
        chart.third_level_dropped == 0,
        "depth-three probes were truncated: {chart:?}"
    );
    eyre::ensure!(
        chart.base_candidates == chart.base_emitted,
        "base probe accounting mismatch: {chart:?}"
    );
    eyre::ensure!(
        chart.third_level_candidates == chart.third_level_emitted,
        "depth-three probe accounting mismatch: {chart:?}"
    );
    eyre::ensure!(
        chart.guards_discovered == chart.guards_attempted + chart.guards_skipped_by_cap,
        "guard accounting mismatch: {chart:?}"
    );
    eyre::ensure!(
        chart.guards_attempted == chart.guard_pairs_emitted + chart.guards_without_witness_pair,
        "guard witness accounting mismatch: {chart:?}"
    );
    eyre::ensure!(
        chart.composite_targets
            == chart.composite_pairs_emitted
                + chart.composite_targets_without_payload
                + chart.composite_pairs_dropped_by_cap,
        "composite probe accounting mismatch: {chart:?}"
    );
    eyre::ensure!(
        chart.guard_sampling_strategy == GuardSamplingStrategy::SchemaOrderPrefix,
        "undisclosed guard sampling strategy: {chart:?}"
    );
    eyre::ensure!(chart.total_emitted > 0, "empty probe battery: {chart:?}");
    Ok(())
}

#[test]
fn probe_coverage_validation_rejects_synthetic_truncation() {
    let coverage = ProbeCoverage {
        label: "synthetic truncation".to_string(),
        base_candidates: 2,
        base_emitted: 1,
        base_dropped: 1,
        third_level_candidates: 2,
        third_level_emitted: 1,
        third_level_dropped: 1,
        total_emitted: 2,
        ..ProbeCoverage::default()
    };

    assert!(validate_probe_coverage(&coverage).is_err());
}

#[test]
fn probe_coverage_validation_rejects_target_only_battery() {
    let coverage = ProbeCoverage {
        label: "synthetic target-only battery".to_string(),
        total_emitted: 1,
        ..ProbeCoverage::default()
    };

    assert!(validate_probe_coverage(&coverage).is_err());
}

#[test]
fn capped_base_probes_visit_every_bucket_before_repeating() {
    let mut buckets = std::collections::BTreeMap::new();
    buckets.insert(
        ("alpha".to_string(), 0),
        vec![
            ("alpha-0-first".to_string(), ProbeInstance::Defaults),
            ("alpha-0-second".to_string(), ProbeInstance::Defaults),
        ],
    );
    buckets.insert(
        ("alpha".to_string(), 1),
        vec![("alpha-1-first".to_string(), ProbeInstance::Defaults)],
    );
    buckets.insert(
        ("beta".to_string(), 0),
        vec![
            ("beta-0-first".to_string(), ProbeInstance::Defaults),
            ("beta-0-second".to_string(), ProbeInstance::Defaults),
        ],
    );

    let labels = round_robin_base_probes(Vec::new(), buckets)
        .into_iter()
        .map(|(label, _)| label)
        .collect::<Vec<_>>();

    sim_assert_eq!(
        have: labels,
        want: vec![
            "alpha-0-first",
            "alpha-1-first",
            "beta-0-first",
            "alpha-0-second",
            "beta-0-second",
        ]
    );
}

#[test]
fn helm_adjudication_validation_rejects_unregistered_accepted_abort() {
    let mut coverage = HelmAdjudicationCoverage::default();
    coverage.record_verdict(
        HelmFlipVerdict::CandidateAcceptsHelmAborts,
        "chart: probe".to_string(),
    );

    assert!(validate_helm_adjudication_coverage(&coverage, &[], &[]).is_err());
}

#[test]
fn helm_adjudication_records_each_outcome_once() {
    let mut coverage = HelmAdjudicationCoverage::default();
    for (verdict, accepted) in [
        (HelmFlipVerdict::Collapsed, None),
        (HelmFlipVerdict::TighteningMatchedHelmAbort, Some(false)),
        (
            HelmFlipVerdict::TighteningMatchedKubernetesRejection,
            Some(false),
        ),
        (
            HelmFlipVerdict::LooseningMatchedKubernetesValidation,
            Some(true),
        ),
        (
            HelmFlipVerdict::LooseningMatchedDefaultsViolations,
            Some(true),
        ),
        (
            HelmFlipVerdict::LooseningWithUncertainKubernetes(vec!["reason".to_string()]),
            Some(true),
        ),
        (HelmFlipVerdict::CandidateAcceptsHelmAborts, Some(true)),
        (
            HelmFlipVerdict::CandidateAcceptsKubernetesRejects,
            Some(true),
        ),
        (
            HelmFlipVerdict::UninformativeBaselineFalseAcceptance(Rejection::HelmAborts),
            Some(true),
        ),
    ] {
        sim_assert_eq!(have: coverage.record_verdict(verdict, "case".to_string()), want: accepted);
    }
    sim_assert_eq!(have: json!(coverage), want: json!({
        "enabled": false, "screening_is_exact": false, "screened_flips": 0,
        "screened_flips_collapsed": 1, "flips_adjudicated": 8,
        "tightenings_matched_helm_abort": 1, "tightenings_matched_kubernetes_rejection": 1,
        "loosenings_matched_kubernetes_validation": 1,
        "loosenings_matched_defaults_violations": 1,
        "loosenings_with_uncertain_kubernetes": [{ "case": "case", "uncertain": ["reason"] }],
        "charts_adjudicated": [],
        "false_acceptances": [
            {
                "case": "case", "rejection": "HelmAborts",
                "baseline": "RejectsUnlikeItsDefaults",
            },
            {
                "case": "case", "rejection": "KubernetesRejects",
                "baseline": "RejectsUnlikeItsDefaults",
            },
            { "case": "case", "rejection": "HelmAborts", "baseline": "RejectsItsDefaults" },
        ]
    }));
}

/// The roster matches each false acceptance by chart, probe, rejection and
/// how the baseline rejected it, and a probe of an adjudicated chart that no
/// longer fails alike must be removed.
#[test]
fn known_false_acceptances_are_matched_by_the_roster() -> eyre::Result<()> {
    const LISTED: &[Probe] = &[
        Probe {
            path: "a.b",
            value: "non-coercible string",
        },
        Probe {
            path: "c",
            value: "non-coercible string",
        },
    ];
    const WITH_FIXED: &[Probe] = &[
        Probe {
            path: "a.b",
            value: "non-coercible string",
        },
        Probe {
            path: "c",
            value: "non-coercible string",
        },
        Probe {
            path: "fixed",
            value: "non-coercible string",
        },
    ];
    const INFORMED: &[Probe] = &[Probe {
        path: "d",
        value: "null deletion",
    }];
    let group = |chart, rejection, baseline, probes| KnownFalseAcceptances {
        chart,
        rejection,
        baseline,
        family: Family::Unfiled,
        probes,
    };
    let uninformed = |probes| {
        group(
            "chart",
            Rejection::HelmAborts,
            Baseline::RejectsItsDefaults,
            probes,
        )
    };
    let informed = group(
        "chart",
        Rejection::HelmAborts,
        Baseline::RejectsUnlikeItsDefaults,
        INFORMED,
    );
    let mut coverage = HelmAdjudicationCoverage::default();
    coverage.charts_adjudicated.insert("chart".to_string());
    for case in [
        "chart: a.b <- non-coercible string",
        "chart: root guard 1 satisfied [targeted: c <- non-coercible string]",
    ] {
        coverage.record_verdict(
            HelmFlipVerdict::UninformativeBaselineFalseAcceptance(Rejection::HelmAborts),
            case.to_string(),
        );
    }
    coverage.record_verdict(
        HelmFlipVerdict::CandidateAcceptsHelmAborts,
        "chart: d <- null deletion".to_string(),
    );
    validate_helm_adjudication_coverage(&coverage, &[uninformed(LISTED), informed], &[])?;
    // A chart this run did not adjudicate keeps its entries.
    validate_helm_adjudication_coverage(
        &coverage,
        &[
            uninformed(LISTED),
            group(
                "chart",
                Rejection::HelmAborts,
                Baseline::RejectsUnlikeItsDefaults,
                INFORMED,
            ),
            group(
                "other",
                Rejection::HelmAborts,
                Baseline::RejectsItsDefaults,
                WITH_FIXED,
            ),
        ],
        &[],
    )?;
    for roster in [
        vec![uninformed(LISTED)],
        vec![
            uninformed(&LISTED[..1]),
            group(
                "chart",
                Rejection::HelmAborts,
                Baseline::RejectsUnlikeItsDefaults,
                INFORMED,
            ),
        ],
        vec![
            group(
                "chart",
                Rejection::KubernetesRejects,
                Baseline::RejectsItsDefaults,
                LISTED,
            ),
            group(
                "chart",
                Rejection::HelmAborts,
                Baseline::RejectsUnlikeItsDefaults,
                INFORMED,
            ),
        ],
        // A cell behind a baseline with its own violations is not the
        // uninformed baseline's, and the reverse.
        vec![
            uninformed(LISTED),
            group(
                "chart",
                Rejection::HelmAborts,
                Baseline::RejectsItsDefaults,
                INFORMED,
            ),
        ],
        vec![
            group(
                "chart",
                Rejection::HelmAborts,
                Baseline::RejectsUnlikeItsDefaults,
                LISTED,
            ),
            group(
                "chart",
                Rejection::HelmAborts,
                Baseline::RejectsUnlikeItsDefaults,
                INFORMED,
            ),
        ],
        vec![
            uninformed(WITH_FIXED),
            group(
                "chart",
                Rejection::HelmAborts,
                Baseline::RejectsUnlikeItsDefaults,
                INFORMED,
            ),
        ],
    ] {
        assert!(validate_helm_adjudication_coverage(&coverage, &roster, &[]).is_err());
    }
    Ok(())
}

#[test]
fn composed_probe_sparse_override_round_trips_null_deletion_and_replacement() -> eyre::Result<()> {
    let defaults = serde_json::json!({
        "guard": true,
        "nested": { "deleted": "x", "kept": 1 },
        "scalar": "old",
    });
    let composed = serde_json::json!({
        "guard": false,
        "nested": { "kept": 1, "added": 2 },
        "scalar": { "replacement": true },
    });
    let patch = sparse_override_for_composed(&defaults, &composed)
        .ok_or_eyre("different documents must produce an override")?;
    let mut round_trip = defaults;

    harness::merge_override(&mut round_trip, patch);

    sim_assert_eq!(have: round_trip, want: composed);
    Ok(())
}

#[derive(Default)]
struct AcceptanceComparison {
    charts_checked: usize,
    probes_checked: usize,
    flips: Vec<String>,
    coverage: Vec<ProbeCoverage>,
    helm_adjudication_failures: Vec<String>,
    helm_adjudication: HelmAdjudicationCoverage,
}

const LEAN_FIXTURE_CHARTS: &[&str] = &[
    "schema-emission-controls",
    "schema-emission-local-kind",
    "schema-emission-temporal-wrapper",
    "schema-emission-unconditional-fail",
];

#[test]
fn lean_profile_schemas_match_their_separate_fixture_lane() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let dump_chart = std::env::var("SCHEMA_DUMP_CHART").ok();
    for chart in LEAN_FIXTURE_CHARTS {
        if dump_chart
            .as_deref()
            .is_some_and(|selected| selected != *chart)
        {
            continue;
        }
        let (_, lean) = generate_profile_schemas(chart)?;
        let fixture_path = test_util::workspace_testdata()
            .join("emission-profile-schemas/lean")
            .join(format!("{chart}.schema.json"));
        if std::env::var("SCHEMA_DUMP").is_ok() {
            let dump_path = std::env::temp_dir().join(format!(
                "helm-schema.emission-profile.lean.{chart}.schema.json"
            ));
            let mut bytes = serde_json::to_vec_pretty(&lean).wrap_err("serialize lean schema")?;
            bytes.push(b'\n');
            std::fs::write(&dump_path, bytes)
                .wrap_err_with(|| format!("write {}", dump_path.display()))?;
            continue;
        }
        let expected: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&fixture_path)
                .wrap_err_with(|| format!("read {}", fixture_path.display()))?,
        )
        .wrap_err_with(|| format!("parse {}", fixture_path.display()))?;
        sim_assert_eq!(
            have: lean,
            want: expected,
            "{chart}: lean profile fixture mismatch"
        );
    }
    Ok(())
}

#[test]
fn current_profiles_obey_monotonicity_and_semantic_controls() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let defaults = read_coalesced_defaults("schema-emission-controls")?;
    let (full, lean) = generate_profile_schemas("schema-emission-controls")?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults.clone())?;

    let controls = semantic_controls();

    profiles.assert_controls(&controls)?;
    let mut probes =
        structural_probe_battery("schema-emission-controls", &defaults, &[&full, &lean])?;
    probes.extend(controls.iter().map(|control| {
        (
            format!("semantic control: {}", control.name),
            control.instance.clone(),
        )
    }));
    profiles.assert_monotone(
        probes
            .iter()
            .map(|(name, instance)| (name.as_str(), instance)),
    )?;

    Ok(())
}

#[test]
fn structural_helper_widening_abstains_in_all_adjacent_input_states() -> eyre::Result<()> {
    let (full, _) = generate_profile_schemas("structural-helper-widening")?;
    let defaults = read_coalesced_defaults("structural-helper-widening")?;
    let schemas = ProfileSchemas::compile(&full, &full, defaults)?;
    let verdicts = [
        ProbeInstance::Defaults,
        ProbeInstance::SparseOverride(json!({ "focus": 7 })),
        ProbeInstance::SparseOverride(json!({ "focus": "selected" })),
        ProbeInstance::SparseOverride(json!({
            "focus": "selected",
            "guard": { "deep": { "flag": null } }
        })),
        ProbeInstance::SparseOverride(json!({
            "focus": "selected",
            "guard": { "deep": { "flag": "wrong-default-type" } }
        })),
        ProbeInstance::SparseOverride(json!({
            "focus": "selected",
            "guard": { "deep": { "flag": 1 } }
        })),
    ]
    .iter()
    .map(|probe| schemas.verdicts(probe).0)
    .collect::<Vec<_>>();

    sim_assert_eq!(have: verdicts, want: vec![true, true, true, true, true, true]);
    Ok(())
}

#[test]
fn lean_profile_obeys_the_middle_point_fact_floor() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    for chart in [
        "schema-emission-controls",
        "schema-emission-local-kind",
        "schema-emission-temporal-wrapper",
    ] {
        let (full, lean) = generate_profile_outputs(chart)?;
        eyre::ensure!(
            full.emission_report.facts.selected == full.emission_report.facts.lowered,
            "full drops a fact for {chart}"
        );
        for class in [
            helm_schema::generation::EmissionClassKind::OrdinaryRoot,
            helm_schema::generation::EmissionClassKind::KindPartitionRoot,
            helm_schema::generation::EmissionClassKind::KindPartitionLocal,
            helm_schema::generation::EmissionClassKind::TerminalAlways,
            helm_schema::generation::EmissionClassKind::TerminalGuarded,
        ] {
            eyre::ensure!(
                lean.emission_report.counts_for_class(class).selected == 0,
                "lean selects {class:?} facts for {chart}"
            );
        }
        let mandatory = lean
            .emission_report
            .counts_for_class(helm_schema::generation::EmissionClassKind::Mandatory);
        eyre::ensure!(
            mandatory.dropped == 0,
            "lean drops mandatory facts for {chart}"
        );
        let local = lean
            .emission_report
            .counts_for_class(helm_schema::generation::EmissionClassKind::OrdinaryLocal);
        eyre::ensure!(
            local.selected == local.lowered,
            "lean drops local conditional facts for {chart}"
        );
    }
    Ok(())
}

#[test]
fn unconditional_fail_is_an_independent_terminal_tooth() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let chart = "schema-emission-unconditional-fail";
    let defaults = read_coalesced_defaults(chart)?;
    let (full, lean) = generate_profile_outputs(chart)?;
    let fixture = read_chart_schema_fixture(chart)?;

    sim_assert_eq!(have: &full.schema, want: &fixture);
    let full_validator = jsonschema::validator_for(&full.schema)?;
    for instance in [
        serde_json::Value::Null,
        json!(false),
        json!(0),
        json!("value"),
        json!([]),
        json!({}),
        json!({ "enabled": true }),
    ] {
        eyre::ensure!(
            !full_validator.is_valid(&instance),
            "full accepted {instance} despite the unconditional terminal"
        );
    }
    eyre::ensure!(
        full.emission_report.facts.lowered == 1 && full.emission_report.facts.selected == 1,
        "full must report one unconditional terminal fact"
    );
    eyre::ensure!(
        lean.emission_report.facts.lowered == 1 && lean.emission_report.facts.selected == 0,
        "legacy lean must drop the unconditional terminal fact"
    );

    let profiles = ProfileSchemas::compile(&full.schema, &lean.schema, defaults)?;
    let controls = [SemanticControl {
        name: "unconditional fail",
        category: ControlCategory::RemovedTooth,
        instance: ProbeInstance::Defaults,
        transport: Transport::ValuesFileJson,
        contract: ContractVerdict::Reject("the chart always aborts rendering"),
        lean_accepts: true,
        rationale: "terminal-clauses off soundly removes an always-false constraint",
    }];
    profiles.assert_controls(&controls)?;
    profiles.assert_monotone(
        controls
            .iter()
            .map(|control| (control.name, &control.instance)),
    )?;
    Ok(())
}

fn semantic_controls() -> Vec<SemanticControl> {
    let mut controls = provider_controls();
    controls.extend(conditional_controls());
    controls.extend(partition_controls());
    controls
}

fn provider_controls() -> Vec<SemanticControl> {
    use ContractVerdict::{Accept, Reject};
    use ControlCategory::{PositiveControl, RetainedTooth};
    use Transport::{Set, SetString, ValuesFileJson};

    vec![
        control(
            "valid composed defaults",
            PositiveControl,
            ProbeInstance::Defaults,
            ValuesFileJson,
            Accept,
            true,
            "renderable defaults remain inside the lint floor",
        ),
        control(
            "provider-backed replica integer",
            RetainedTooth,
            sparse_override(&["replicas"], json!(3)),
            Set,
            Accept,
            true,
            "an integer reaches the Deployment replica sink",
        ),
        control(
            "coercible replica spelling",
            RetainedTooth,
            sparse_override(&["replicas"], json!("3")),
            SetString,
            Accept,
            true,
            "an unquoted numeric string renders as an integer token",
        ),
        control(
            "non-coercible provider replica",
            RetainedTooth,
            sparse_override(&["replicas"], json!("three")),
            ValuesFileJson,
            Reject("rendered Deployment replicas is not an integer"),
            false,
            "unconditional provider typing survives every profile",
        ),
    ]
}

fn conditional_controls() -> Vec<SemanticControl> {
    use ContractVerdict::{Accept, Reject};
    use ControlCategory::{PositiveControl, RemovedTooth, RetainedTooth};
    use Transport::ValuesFileJson;

    vec![
        control(
            "required value deletion",
            RetainedTooth,
            sparse_override(&["requiredText"], json!(null)),
            ValuesFileJson,
            Reject("required aborts template rendering"),
            true,
            "today's lean profile intentionally drops terminal clauses",
        ),
        control(
            "version pattern near miss",
            RemovedTooth,
            sparse_override(&["version"], json!("v1")),
            ValuesFileJson,
            Reject("the chart rejects a non-matching version"),
            false,
            "the unguarded pattern is mandatory and survives every profile",
        ),
        control(
            "nil-safe object host deletion",
            PositiveControl,
            sparse_override(&["host"], json!(null)),
            ValuesFileJson,
            Accept,
            true,
            "a dropped conditional must not retain a stricter object host",
        ),
        control(
            "disabled dependency ignores a wrong replica kind",
            PositiveControl,
            ProbeInstance::SparseOverride(
                json!({ "worker": { "enabled": false, "replicas": "three" } }),
            ),
            ValuesFileJson,
            Accept,
            true,
            "the dependency does not render while its condition is false",
        ),
        control(
            "enabled dependency rejects a wrong replica kind",
            RetainedTooth,
            ProbeInstance::SparseOverride(
                json!({ "worker": { "enabled": true, "replicas": "three" } }),
            ),
            ValuesFileJson,
            Reject("the enabled worker renders invalid replicas"),
            false,
            "the middle-point lean contract retains dependency-local provider typing",
        ),
    ]
}

fn partition_controls() -> Vec<SemanticControl> {
    use ContractVerdict::{Accept, Reject};
    use ControlCategory::{PositiveControl, RemovedTooth};
    use Transport::ValuesFileJson;

    vec![
        control(
            "ConfigMap branch rejects a Service spelling",
            RemovedTooth,
            ProbeInstance::SparseOverride(
                json!({ "local": { "kind": "ConfigMap", "setting": "ClusterIP" } }),
            ),
            ValuesFileJson,
            Reject("ConfigMap immutable must be a boolean"),
            true,
            "lean deletes local kind-partition refinements",
        ),
        control(
            "Service branch accepts its own spelling",
            PositiveControl,
            ProbeInstance::SparseOverride(
                json!({ "local": { "kind": "Service", "setting": "ClusterIP" } }),
            ),
            ValuesFileJson,
            Accept,
            true,
            "the adjacent kind branch remains renderable",
        ),
        control(
            "unknown dynamic provider kind",
            PositiveControl,
            sparse_override(&["dynamic", "kind"], json!("FutureKind")),
            ValuesFileJson,
            Accept,
            true,
            "provider uncertainty must not become a rejection",
        ),
    ]
}

fn control(
    name: &'static str,
    category: ControlCategory,
    instance: ProbeInstance,
    transport: Transport,
    contract: ContractVerdict,
    lean_accepts: bool,
    rationale: &'static str,
) -> SemanticControl {
    SemanticControl {
        name,
        category,
        instance,
        transport,
        contract,
        lean_accepts,
        rationale,
    }
}

#[test]
fn lean_profile_keeps_nil_safe_host_relaxation() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let defaults = read_coalesced_defaults("schema-emission-controls")?;
    let (full, lean) = generate_profile_schemas("schema-emission-controls")?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults)?;
    let probe = sparse_override(&["host"], json!(null));

    let (full_accepts, lean_accepts) = profiles.verdicts(&probe);
    eyre::ensure!(full_accepts, "full must accept the nil-safe host deletion");
    eyre::ensure!(
        lean_accepts,
        "lean retained a strict object host after dropping its conditional arm"
    );
    Ok(())
}

#[test]
fn temporal_wrapper_pairwise_matrix_is_monotone() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let chart = "schema-emission-temporal-wrapper";
    let defaults = read_json_fixture(chart, "coalesced-defaults.json")?;
    let (full, lean) = generate_profile_schemas(chart)?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults)?;

    let replica_values = [
        json!(null),
        json!(0),
        json!(1),
        json!(1.5),
        json!("3"),
        json!("three"),
        json!([]),
        json!({}),
    ];
    let label_values = [
        json!(null),
        json!({}),
        json!({ "app": "temporal" }),
        json!({ "app": 3 }),
        json!([]),
    ];
    let mut probes = vec![("defaults".to_string(), ProbeInstance::Defaults)];
    for replica in replica_values {
        for labels in &label_values {
            probes.push((
                format!("replica={replica}, podLabels={labels}"),
                ProbeInstance::SparseOverride(json!({
                    "temporal": {
                        "server": {
                            "replicaCount": replica,
                            "podLabels": labels,
                        }
                    }
                })),
            ));
        }
    }

    profiles.assert_monotone(
        probes
            .iter()
            .map(|(name, instance)| (name.as_str(), instance)),
    )?;
    profiles.assert_controls(&[
        SemanticControl {
            name: "temporal defaults",
            category: ControlCategory::PositiveControl,
            instance: ProbeInstance::Defaults,
            transport: Transport::ValuesFileJson,
            contract: ContractVerdict::Accept,
            lean_accepts: true,
            rationale: "the pinned wrapper defaults render and must satisfy both profiles",
        },
        SemanticControl {
            name: "temporal replicaCount non-coercible spelling",
            category: ControlCategory::RetainedTooth,
            instance: sparse_override(&["temporal", "server", "replicaCount"], json!("three")),
            transport: Transport::ValuesFileJson,
            contract: ContractVerdict::Reject("Deployment replicas is not an integer"),
            lean_accepts: false,
            rationale: "the middle-point lean contract retains dependency-local provider typing",
        },
    ])?;
    Ok(())
}

#[test]
fn structural_battery_preserves_helm_v4_dependency_roots() -> eyre::Result<()> {
    let chart = "schema-emission-temporal-wrapper";
    let defaults = read_json_fixture(chart, "coalesced-defaults.json")?;
    let probes = structural_probe_battery(chart, &defaults, &[])?;

    let retained = probes
        .iter()
        .find_map(|(name, probe)| (name == "all declared keys deleted").then_some(probe));
    let Some(ProbeInstance::Coalesced(retained)) = retained else {
        return Err(eyre::eyre!("all-declared-keys probe missing"));
    };
    assert!(
        retained.get("temporal").is_some(),
        "Helm v4 refills the dependency root after a parent null deletion"
    );
    assert!(probes.iter().all(|(_, probe)| {
        !matches!(
            probe,
            ProbeInstance::SparseOverride(value)
                if value.get("temporal").is_some_and(serde_json::Value::is_null)
        )
    }));
    Ok(())
}

#[test]
fn structural_battery_preserves_unlisted_vendored_dependency_roots() -> eyre::Result<()> {
    let chart = "schema-emission-unlisted-dependency";
    let defaults = json!({ "vendored": { "enabled": true } });
    let probes = structural_probe_battery(chart, &defaults, &[])?;

    let retained = probes
        .iter()
        .find_map(|(name, probe)| (name == "all declared keys deleted").then_some(probe));
    let Some(ProbeInstance::Coalesced(retained)) = retained else {
        return Err(eyre::eyre!("all-declared-keys probe missing"));
    };
    sim_assert_eq!(
        have: retained.get("vendored"),
        want: Some(&json!({ "enabled": true }))
    );
    sim_assert_eq!(
        have: probes.iter().any(|(_, probe)| matches!(
            probe,
            ProbeInstance::SparseOverride(value)
                if value.get("vendored").is_some_and(serde_json::Value::is_null)
        )),
        want: false
    );
    Ok(())
}

#[test]
fn structural_battery_samples_depth_three_and_guard_states() -> eyre::Result<()> {
    let chart = "schema-emission-controls";
    let defaults = read_coalesced_defaults(chart)?;
    let (full, lean) = generate_profile_schemas(chart)?;
    let (probes, coverage) =
        structural_probe_battery_with_coverage(chart, &defaults, &[&full, &lean])?;

    eyre::ensure!(
        probes
            .iter()
            .any(|(name, _)| name == "host.nested.value <- null deletion [depth 3]"),
        "the depth-three deletion lane did not reach host.nested.value"
    );
    eyre::ensure!(
        probes
            .iter()
            .any(|(name, _)| name.starts_with("root guard ")
                && name.contains(" satisfied [targeted:")),
        "the guard-state lane produced no satisfying witness"
    );
    eyre::ensure!(
        probes
            .iter()
            .any(|(name, _)| name.starts_with("root guard ")
                && name.contains(" violated [targeted:")),
        "the guard-state lane produced no violating witness"
    );
    eyre::ensure!(
        coverage.guard_pairs_emitted > 0
            && coverage.guards_discovered
                == coverage.guards_attempted + coverage.guards_skipped_by_cap,
        "guard coverage accounting is incomplete: {coverage:?}"
    );
    Ok(())
}

#[test]
fn guard_battery_synthesizes_composite_guard_and_payload_states() -> eyre::Result<()> {
    let defaults = json!({ "enabled": false, "payload": "valid" });
    let schema = json!({
        "allOf": [{
            "if": {
                "properties": { "enabled": { "const": true } },
                "required": ["enabled"],
                "type": "object",
            },
            "then": {
                "properties": { "payload": { "type": "string" } },
                "required": ["payload"],
                "type": "object",
            },
        }],
    });
    let mut coverage = harness::ProbeCoverage::default();

    let probes = harness::guard_state_probes(&defaults, &[&schema], &mut coverage)?;

    sim_assert_eq!(have: coverage.guard_pairs_emitted, want: 1);
    sim_assert_eq!(have: coverage.composite_pairs_emitted, want: 1);
    eyre::ensure!(
        probes
            .iter()
            .filter(|(name, _)| name.contains("[composite "))
            .count()
            == 2,
        "composite witnesses were not emitted as a guard-state pair: {probes:?}"
    );
    Ok(())
}

#[test]
fn archive_dependency_depth_ignores_a_leading_current_directory_component() {
    sim_assert_eq!(
        have: harness::archive_entry_depth(std::path::Path::new("./vendored/Chart.yaml")),
        want: 2
    );
    sim_assert_eq!(
        have: harness::archive_entry_depth(std::path::Path::new("vendored/Chart.yaml")),
        want: 2
    );
}

#[test]
fn ordinary_kind_partition_evidence_keeps_the_complete_range_domain() -> eyre::Result<()> {
    let chart = "schema-emission-kind-range";
    let defaults = read_coalesced_defaults(chart)?;
    let (full, lean) = generate_profile_schemas(chart)?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults)?;

    for (name, probe) in [
        (
            "integer range",
            sparse_override(&["entries"], serde_json::json!(2)),
        ),
        (
            "map range",
            sparse_override(&["entries"], serde_json::json!({ "configured": "value" })),
        ),
    ] {
        let (full_accepts, lean_accepts) = profiles.verdicts(&probe);
        eyre::ensure!(
            full_accepts && lean_accepts,
            "{name} must survive both the selected and ordinary projections: full={full_accepts}, lean={lean_accepts}"
        );
    }
    Ok(())
}

#[test]
#[ignore = "maintenance: records Step 2 Temporal policy measurements"]
fn temporal_middle_policy_measurements() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let full_session = harness::profile_session(
        "schema-emission-temporal-wrapper",
        helm_schema::generation::SchemaProfile::Full,
        false,
    );
    let lean_session = harness::profile_session(
        "schema-emission-temporal-wrapper",
        helm_schema::generation::SchemaProfile::Lean,
        false,
    );
    let full = full_session.generated_schema()?;
    let lean = lean_session.generated_schema()?;
    let emit_request = helm_schema::output::EmitRequest {
        reference_policy: helm_schema::output::ReferencePolicy::SelfContained,
        output: helm_schema::output::OutputPipelineOptions {
            strip_descriptions: false,
            minimize: true,
        },
    };
    let full_schema = full_session.emit(emit_request)?;
    let lean_schema = lean_session.emit(emit_request)?;
    let mut full_bytes = Vec::new();
    let full_metrics = helm_schema::output::write_schema_json(
        &mut full_bytes,
        &full_schema,
        helm_schema::output::JsonOutputFormat::Compact,
    )?;
    let mut lean_bytes = Vec::new();
    let lean_metrics = helm_schema::output::write_schema_json(
        &mut lean_bytes,
        &lean_schema,
        helm_schema::output::JsonOutputFormat::Compact,
    )?;
    let lean_budget_limit = 9 * 1024 * 1024 / 2;
    eyre::ensure!(
        lean_metrics.serialized_bytes < lean_budget_limit,
        "Temporal lean output is {} bytes, over the {lean_budget_limit}-byte budget",
        lean_metrics.serialized_bytes,
    );
    eprintln!(
        "full={full_metrics:?} lean={lean_metrics:?} lean_budget_bytes={} lean_budget_limit={}",
        lean_metrics.serialized_bytes, lean_budget_limit,
    );
    for class in [
        helm_schema::generation::EmissionClassKind::Mandatory,
        helm_schema::generation::EmissionClassKind::OrdinaryRoot,
        helm_schema::generation::EmissionClassKind::OrdinaryLocal,
        helm_schema::generation::EmissionClassKind::KindPartitionRoot,
        helm_schema::generation::EmissionClassKind::KindPartitionLocal,
        helm_schema::generation::EmissionClassKind::TerminalAlways,
        helm_schema::generation::EmissionClassKind::TerminalGuarded,
    ] {
        let counts = full.emission_report.counts_for_class(class);
        let lean_counts = lean.emission_report.counts_for_class(class);
        eprintln!(
            "class={class:?} lowered={} full_selected={} lean_selected={} delta={}",
            counts.lowered,
            counts.selected,
            lean_counts.selected,
            counts.selected - lean_counts.selected
        );
    }
    eprintln!(
        "full_canonical={:?} full_mandatory={:?} lean_canonical={:?} lean_mandatory={:?}",
        full.emission_report.canonicalization,
        full.emission_report.mandatory_outcomes,
        lean.emission_report.canonicalization,
        lean.emission_report.mandatory_outcomes,
    );
    Ok(())
}

#[test]
fn local_kind_partition_is_a_local_policy_fact() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let chart = "schema-emission-local-kind";
    let defaults = read_coalesced_defaults(chart)?;
    let (full, lean) = generate_profile_schemas(chart)?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults)?;
    let controls = [
        SemanticControl {
            name: "Deployment accepts Deployment strategy",
            category: ControlCategory::PositiveControl,
            instance: ProbeInstance::Defaults,
            transport: Transport::ValuesFileJson,
            contract: ContractVerdict::Accept,
            lean_accepts: true,
            rationale: "the selected provider arm owns the Deployment strategy shape",
        },
        SemanticControl {
            name: "Deployment rejects StatefulSet strategy",
            category: ControlCategory::RemovedTooth,
            instance: ProbeInstance::SparseOverride(json!({
                "workload": {
                    "kind": "Deployment",
                    "strategy": { "rollingUpdate": { "partition": 1 } },
                }
            })),
            transport: Transport::ValuesFileJson,
            contract: ContractVerdict::Reject(
                "Deployment strategy has no rollingUpdate.partition member",
            ),
            lean_accepts: true,
            rationale: "today's lean profile drops every conditional partition",
        },
        SemanticControl {
            name: "StatefulSet accepts StatefulSet strategy",
            category: ControlCategory::PositiveControl,
            instance: ProbeInstance::SparseOverride(json!({
                "workload": {
                    "kind": "StatefulSet",
                    "strategy": { "rollingUpdate": { "partition": 1 } },
                }
            })),
            transport: Transport::ValuesFileJson,
            contract: ContractVerdict::Accept,
            lean_accepts: true,
            rationale: "the adjacent local partition remains renderable",
        },
    ];

    profiles.assert_controls(&controls)?;
    profiles.assert_monotone(
        controls
            .iter()
            .map(|control| (control.name, &control.instance)),
    )?;
    Ok(())
}

#[test]
#[ignore = "maintenance: compares current full and lean fixtures with a baseline ref"]
fn early_provider_definition_pruning_is_acceptance_equivalent() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let (charts_checked, probes_checked, flips, _) = corpus_acceptance_flips()?;
    eyre::ensure!(
        flips.is_empty(),
        "step 3 changed fixture acceptance:\n{}",
        flips.join("\n")
    );
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=0");
    Ok(())
}

#[test]
#[ignore = "maintenance: compares the Round 68 dump with its baseline ref"]
fn round68_fixture_flips_match_the_helm_adjudicated_list() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let (charts_checked, probes_checked, flips, _) = corpus_acceptance_flips()?;
    let expected = vec![
        "airflow: fullnameOverride <- false: before=false, after=true",
        "airflow: fullnameOverride <- true: before=false, after=true",
        "airflow: fullnameOverride <- integer: before=false, after=true",
        "airflow: fullnameOverride <- number: before=false, after=true",
        "airflow: fullnameOverride <- empty array: before=false, after=true",
        "airflow: fullnameOverride <- empty object item: before=false, after=true",
        "airflow: fullnameOverride <- empty object: before=false, after=true",
        "airflow: fullnameOverride <- unknown object member: before=false, after=true",
        "metallb: fullnameOverride <- false: before=false, after=true",
        "metallb: fullnameOverride <- integer: before=false, after=true",
        "metallb: fullnameOverride <- empty array: before=false, after=true",
        "metallb: fullnameOverride <- empty object: before=false, after=true",
        "traefik: namespaceOverride <- false: before=false, after=true",
        "traefik: namespaceOverride <- integer: before=false, after=true",
        "traefik: namespaceOverride <- empty array: before=false, after=true",
        "traefik: namespaceOverride <- empty object: before=false, after=true",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    sim_assert_eq!(have: flips, want: expected);
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=16");
    Ok(())
}

#[test]
#[ignore = "maintenance: compares the Round 69 dump with its baseline ref"]
fn round69_override_bundling_is_corpus_acceptance_equivalent() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let (charts_checked, probes_checked, flips, _) = corpus_acceptance_flips()?;
    eyre::ensure!(
        flips.is_empty(),
        "round 69 changed fixture acceptance:\n{}",
        flips.join("\n")
    );
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=0");
    Ok(())
}

#[test]
#[ignore = "maintenance: compares the Round 70 dump with its baseline ref"]
fn round70_partition_and_canonicalization_changes_are_acceptance_equivalent() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let (charts_checked, probes_checked, flips, _) = corpus_acceptance_flips()?;
    eyre::ensure!(
        flips.is_empty(),
        "round 70 changed fixture acceptance:\n{}",
        flips.join("\n")
    );
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=0");
    Ok(())
}

#[test]
#[ignore = "maintenance: retro-adjudicates the Round 70 oauth2-proxy schema change"]
fn round70_oauth2_proxy_tpl_change_kept_the_eager_string_tooth() -> eyre::Result<()> {
    let baseline = read_schema_at_ref(
        "34e58cc",
        "testdata/chart-corpus-schemas/oauth2-proxy.schema.json",
    )?;
    let current = read_chart_schema_fixture("oauth2-proxy")?;
    let defaults = read_coalesced_defaults("oauth2-proxy")?;
    let transition = ProfileSchemas::compile(&baseline, &current, defaults)?;

    for (name, probe) in [
        (
            "selected non-string tpl fallback",
            sparse_override(&["global", "imageRegistry"], serde_json::Value::from(7)),
        ),
        (
            "eager non-string tpl fallback behind a live primary",
            ProbeInstance::SparseOverride(json!({
                "image": { "registry": "quay.io" },
                "global": { "imageRegistry": 7 },
            })),
        ),
    ] {
        sim_assert_eq!(
            have: transition.verdicts(&probe),
            want: (false, false),
            "{name} must remain rejected by the unconditional tpl program contract"
        );
    }
    Ok(())
}

#[test]
#[ignore = "maintenance: compares the Round 72 dump with its baseline ref"]
fn round72_pipeline_changes_are_acceptance_equivalent() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let (charts_checked, probes_checked, flips, _) = corpus_acceptance_flips()?;
    eyre::ensure!(
        flips.is_empty(),
        "round 72 changed fixture acceptance:\n{}",
        flips.join("\n")
    );
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=0");
    Ok(())
}

#[test]
#[ignore = "maintenance: compares the Round 73 dump and records probe coverage"]
fn round73_fixture_flips_are_adjudicated_and_probe_caps_are_disclosed() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let AcceptanceComparison {
        charts_checked,
        probes_checked,
        flips,
        coverage,
        helm_adjudication_failures,
        helm_adjudication,
    } = corpus_acceptance_comparison()?;
    for chart in &coverage {
        validate_probe_coverage(chart)?;
    }
    let baseline_ref = std::env::var("SCHEMA_ACCEPTANCE_BASELINE_REF")
        .wrap_err("SCHEMA_ACCEPTANCE_BASELINE_REF must name the comparison commit")?;
    let report = ProbeCoverageReport {
        baseline_ref,
        charts: coverage,
        helm_adjudication,
    };
    let report_path = std::path::PathBuf::from(
        std::env::var_os("SCHEMA_PROBE_COVERAGE_REPORT")
            .ok_or_eyre("SCHEMA_PROBE_COVERAGE_REPORT must name the report file")?,
    );
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent).wrap_err_with(|| format!("create {}", parent.display()))?;
    }
    let mut bytes = serde_json::to_vec_pretty(&report).wrap_err("serialize probe coverage")?;
    bytes.push(b'\n');
    std::fs::write(&report_path, bytes)
        .wrap_err_with(|| format!("write {}", report_path.display()))?;
    eyre::ensure!(
        helm_adjudication_failures.is_empty(),
        "Round 73 Helm adjudication failures:\n{}",
        helm_adjudication_failures.join("\n")
    );
    validate_helm_adjudication_coverage(
        &report.helm_adjudication,
        KNOWN_FALSE_ACCEPTANCES,
        KNOWN_UNDECIDED_ACCEPTANCES,
    )?;
    eyre::ensure!(
        flips.is_empty(),
        "round 73 changed fixture acceptance:\n{}",
        flips.join("\n")
    );
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=0");
    Ok(())
}

#[test]
#[ignore = "maintenance: compares the Round 74 dump and records probe coverage"]
fn round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    if std::env::var("ADJUDICATE_WITH_HELM").is_ok() {
        let version = std::process::Command::new("helm")
            .args(["version", "--template", "{{.Version}}"])
            .output()
            .wrap_err("read Helm version for Round 74 adjudication")?;
        eyre::ensure!(version.status.success(), "helm version failed");
        eyre::ensure!(
            String::from_utf8(version.stdout)?.trim() == "v4.2.3",
            "Round 74 adjudication requires Helm v4.2.3"
        );
    }
    let AcceptanceComparison {
        charts_checked,
        probes_checked,
        flips,
        coverage,
        helm_adjudication_failures,
        helm_adjudication,
    } = corpus_acceptance_comparison()?;
    for chart in &coverage {
        validate_probe_coverage(chart)?;
    }
    let baseline_ref = std::env::var("SCHEMA_ACCEPTANCE_BASELINE_REF")
        .wrap_err("SCHEMA_ACCEPTANCE_BASELINE_REF must name the comparison commit")?;
    let report = ProbeCoverageReport {
        baseline_ref,
        charts: coverage,
        helm_adjudication,
    };
    let report_path = std::path::PathBuf::from(
        std::env::var_os("SCHEMA_PROBE_COVERAGE_REPORT")
            .ok_or_eyre("SCHEMA_PROBE_COVERAGE_REPORT must name the report file")?,
    );
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent).wrap_err_with(|| format!("create {}", parent.display()))?;
    }
    let mut bytes = serde_json::to_vec_pretty(&report).wrap_err("serialize probe coverage")?;
    bytes.push(b'\n');
    std::fs::write(&report_path, bytes)
        .wrap_err_with(|| format!("write {}", report_path.display()))?;
    eyre::ensure!(
        helm_adjudication_failures.is_empty(),
        "Round 74 Helm adjudication failures:\n{}",
        helm_adjudication_failures.join("\n")
    );
    eyre::ensure!(
        report.helm_adjudication.flips_adjudicated == flips.len(),
        "fixture flips were not all live-adjudicated: {report:?}"
    );
    // Correctness campaigns permit oracle-matched changes; preservation runs keep their count gate.
    validate_helm_adjudication_coverage(
        &report.helm_adjudication,
        KNOWN_FALSE_ACCEPTANCES,
        KNOWN_UNDECIDED_ACCEPTANCES,
    )?;
    if std::env::var_os("SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS").is_none() {
        eyre::ensure!(
            flips.len() == PREREGISTERED_ACCEPTANCE_FLIP_ALLOWANCE,
            "fixture acceptance flips differ from the pre-registered count:\n{}",
            flips.join("\n")
        );
    }
    eprintln!(
        "charts_checked={charts_checked} probes_checked={probes_checked} flips={} \
         listed_false_acceptances={} listed_undecided_acceptances={}",
        flips.len(),
        report.helm_adjudication.false_acceptances.len(),
        report
            .helm_adjudication
            .loosenings_with_uncertain_kubernetes
            .len()
    );
    Ok(())
}

#[test]
#[ignore = "maintenance: compares an external chart schema pair at full probe depth"]
fn external_schema_pair_flips_are_helm_adjudicated() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    eyre::ensure!(
        std::env::var("ADJUDICATE_WITH_HELM").is_ok(),
        "external schema comparison requires ADJUDICATE_WITH_HELM"
    );
    let chart = std::path::PathBuf::from(
        std::env::var_os("SCHEMA_ACCEPTANCE_EXTERNAL_CHART")
            .ok_or_eyre("SCHEMA_ACCEPTANCE_EXTERNAL_CHART must name the chart directory")?,
    );
    let baseline: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::env::var_os("SCHEMA_ACCEPTANCE_BASELINE_SCHEMA")
                .ok_or_eyre("SCHEMA_ACCEPTANCE_BASELINE_SCHEMA must name a schema")?,
        )
        .wrap_err("read external baseline schema")?,
    )
    .wrap_err("parse external baseline schema")?;
    let candidate: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::env::var_os("SCHEMA_ACCEPTANCE_CANDIDATE_SCHEMA")
                .ok_or_eyre("SCHEMA_ACCEPTANCE_CANDIDATE_SCHEMA must name a schema")?,
        )
        .wrap_err("read external candidate schema")?,
    )
    .wrap_err("parse external candidate schema")?;
    let defaults: serde_json::Value = serde_yaml::from_str(
        &std::fs::read_to_string(chart.join("values.yaml"))
            .wrap_err("read external chart defaults")?,
    )
    .wrap_err("parse external chart defaults")?;
    let chart = chart
        .to_str()
        .ok_or_eyre("external chart path must be UTF-8")?;
    let mut comparison = AcceptanceComparison::default();
    collect_acceptance_flips(
        "external",
        chart,
        &baseline,
        &candidate,
        &defaults,
        &mut comparison,
    )?;
    for coverage in &comparison.coverage {
        validate_probe_coverage(coverage)?;
    }
    eyre::ensure!(
        comparison.helm_adjudication_failures.is_empty(),
        "external Helm adjudication failures:\n{}",
        comparison.helm_adjudication_failures.join("\n")
    );
    validate_helm_adjudication_coverage(
        &comparison.helm_adjudication,
        KNOWN_FALSE_ACCEPTANCES,
        KNOWN_UNDECIDED_ACCEPTANCES,
    )?;
    eprintln!(
        "probes_checked={} flips={}",
        comparison.probes_checked,
        comparison.flips.len()
    );
    for flip in comparison.flips {
        eprintln!("ADJUDICATED {flip}");
    }
    Ok(())
}

fn corpus_acceptance_flips() -> eyre::Result<(usize, usize, Vec<String>, Vec<ProbeCoverage>)> {
    let comparison = corpus_acceptance_comparison()?;
    Ok((
        comparison.charts_checked,
        comparison.probes_checked,
        comparison.flips,
        comparison.coverage,
    ))
}

fn corpus_acceptance_comparison() -> eyre::Result<AcceptanceComparison> {
    let baseline_ref = std::env::var("SCHEMA_ACCEPTANCE_BASELINE_REF")
        .wrap_err("SCHEMA_ACCEPTANCE_BASELINE_REF must name the comparison commit")?;
    let fixture_dir = test_util::workspace_testdata().join("chart-corpus-schemas");
    let mut fixture_paths = std::fs::read_dir(&fixture_dir)
        .wrap_err_with(|| format!("read {}", fixture_dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    fixture_paths.sort();

    let mut comparison = AcceptanceComparison::default();
    let chart_filter = std::env::var("SCHEMA_ACCEPTANCE_CHART").ok();
    for fixture_path in fixture_paths {
        let Some(filename) = fixture_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(chart) = filename.strip_suffix(".schema.json") else {
            continue;
        };
        if chart_filter
            .as_deref()
            .is_some_and(|filter| filter != chart)
        {
            continue;
        }
        let relative_path = format!("testdata/chart-corpus-schemas/{filename}");
        let baseline = read_schema_at_ref(&baseline_ref, &relative_path)?;
        let dump_filename = format!("helm-schema.cli.chart-corpus.{chart}.schema.json");
        let current = read_acceptance_candidate(&fixture_path, &dump_filename)?;
        let defaults = read_coalesced_defaults(chart)?;
        collect_acceptance_flips(
            chart,
            chart,
            &baseline,
            &current,
            &defaults,
            &mut comparison,
        )?;
        comparison.charts_checked += 1;
    }
    let lean_fixture_dir = test_util::workspace_testdata().join("emission-profile-schemas/lean");
    for chart in LEAN_FIXTURE_CHARTS {
        if chart_filter.is_some() {
            continue;
        }
        let filename = format!("{chart}.schema.json");
        let fixture_path = lean_fixture_dir.join(&filename);
        let relative_path = format!("testdata/emission-profile-schemas/lean/{filename}");
        let baseline = read_schema_at_ref(&baseline_ref, &relative_path)?;
        let dump_filename = format!("helm-schema.emission-profile.lean.{chart}.schema.json");
        let current = read_acceptance_candidate(&fixture_path, &dump_filename)?;
        let defaults = if *chart == "schema-emission-temporal-wrapper" {
            read_json_fixture(chart, "coalesced-defaults.json")?
        } else {
            read_coalesced_defaults(chart)?
        };
        collect_acceptance_flips(
            &format!("lean/{chart}"),
            chart,
            &baseline,
            &current,
            &defaults,
            &mut comparison,
        )?;
        comparison.charts_checked += 1;
    }

    Ok(comparison)
}

fn read_schema_at_ref(reference: &str, relative_path: &str) -> eyre::Result<serde_json::Value> {
    let output = std::process::Command::new("git")
        .args(["show", &format!("{reference}:{relative_path}")])
        .output()
        .wrap_err_with(|| format!("read {relative_path} from {reference}"))?;
    eyre::ensure!(
        output.status.success(),
        "git show failed for {reference}:{relative_path}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
        .wrap_err_with(|| format!("parse {reference}:{relative_path}"))
}

fn read_acceptance_candidate(
    fixture_path: &std::path::Path,
    dump_filename: &str,
) -> eyre::Result<serde_json::Value> {
    let candidate_path = if let Some(dir) = std::env::var_os("SCHEMA_ACCEPTANCE_CANDIDATE_DUMP") {
        std::path::PathBuf::from(dir).join(dump_filename)
    } else {
        eyre::ensure!(
            std::env::var_os("ADJUDICATE_WITH_HELM").is_none(),
            "live adjudication requires SCHEMA_ACCEPTANCE_CANDIDATE_DUMP"
        );
        fixture_path.to_path_buf()
    };
    serde_json::from_str(
        &std::fs::read_to_string(&candidate_path)
            .wrap_err_with(|| format!("read {}", candidate_path.display()))?,
    )
    .wrap_err_with(|| format!("parse {}", candidate_path.display()))
}

fn collect_acceptance_flips(
    label: &str,
    chart_relative_path: &str,
    baseline: &serde_json::Value,
    current: &serde_json::Value,
    defaults: &serde_json::Value,
    comparison: &mut AcceptanceComparison,
) -> eyre::Result<()> {
    let adjudicate_live = std::env::var("ADJUDICATE_WITH_HELM").is_ok();
    comparison.helm_adjudication.enabled |= adjudicate_live;
    if adjudicate_live {
        comparison
            .helm_adjudication
            .charts_adjudicated
            .insert(label.to_string());
    }
    let profiles = ProfileSchemas::compile(baseline, current, defaults.clone())?;
    let (probes, mut chart_coverage) = structural_probe_battery_with_coverage(
        chart_relative_path,
        defaults,
        &[baseline, current],
    )?;
    chart_coverage.label = label.to_string();
    let helm_chart = std::cell::OnceCell::new();
    let cache = std::env::var_os("SCHEMA_ACCEPTANCE_K8S_CACHE").map_or_else(
        || test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        std::path::PathBuf::from,
    );
    // Custom resources are judged by the CRD schemas pinned beside the bundle.
    let mut kubernetes = OfflineKubernetesValidator::new(&cache).with_crd_catalog(
        &test_util::workspace_testdata().join("provider-bundle/crds-catalog-cache"),
    );
    for (probe_name, probe) in probes {
        comparison.probes_checked += 1;
        if profiles.screens_a_flip(&probe) {
            if adjudicate_live {
                comparison.helm_adjudication.screened_flips += 1;
                // Screening proposes an overlay; only Helm can establish its coalesced document.
                let chart = match helm_chart.get_or_init(|| {
                    let path = test_util::workspace_testdata()
                        .join("charts")
                        .join(chart_relative_path);
                    PinnedHelmChart::prepare(&path)
                }) {
                    Ok(chart) => chart,
                    Err(error) => {
                        comparison
                            .helm_adjudication_failures
                            .push(format!("{label}: {probe_name}: {error}"));
                        continue;
                    }
                };
                let verdict = match adjudicate_round74_flip(
                    chart,
                    &probe.helm_values_file(defaults),
                    &profiles,
                    &mut kubernetes,
                ) {
                    Ok(verdict) => verdict,
                    Err(error) => {
                        comparison
                            .helm_adjudication_failures
                            .push(format!("{label}: {probe_name}: {error}"));
                        continue;
                    }
                };
                let Some(candidate_accepts) = comparison
                    .helm_adjudication
                    .record_verdict(verdict, format!("{label}: {probe_name}"))
                else {
                    continue;
                };
                comparison.flips.push(format!(
                    "{label}: {probe_name}: candidate accepts={candidate_accepts}"
                ));
            } else {
                let (before, after) = profiles.verdicts(&probe);
                comparison.flips.push(format!(
                    "{label}: {probe_name}: before={before}, after={after}"
                ));
            }
        }
    }
    comparison.coverage.push(chart_coverage);
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum HelmFlipVerdict {
    Collapsed,
    TighteningMatchedHelmAbort,
    TighteningMatchedKubernetesRejection,
    LooseningMatchedKubernetesValidation,
    /// Helm renders, and every Kubernetes violation is one the defaults render already carries.
    LooseningMatchedDefaultsViolations,
    /// Helm renders, but a new or changed resource has no decidable schema,
    /// for the reasons listed.
    LooseningWithUncertainKubernetes(Vec<String>),
    CandidateAcceptsHelmAborts,
    CandidateAcceptsKubernetesRejects,
    /// The candidate accepts a cell Helm or Kubernetes rejects, and the
    /// baseline rejected it exactly as it rejects the chart's own defaults:
    /// a false acceptance judged on the candidate alone.
    UninformativeBaselineFalseAcceptance(Rejection),
}

fn adjudicate_round74_flip(
    chart: &PinnedHelmChart,
    overlay: &serde_json::Value,
    profiles: &ProfileSchemas,
    kubernetes: &mut OfflineKubernetesValidator,
) -> eyre::Result<HelmFlipVerdict> {
    let probe = chart.adjudicate(overlay)?;
    // Without a Helm-coalesced document, the screened composition is the judged instance.
    let (baseline_errors, candidate_errors) = match &probe.values {
        Some(values) => profiles.coalesced_errors(values),
        None => profiles.override_errors(overlay),
    };
    let before = baseline_errors.is_empty();
    let after = candidate_errors.is_empty();
    // A candidate rejecting for an assertion the baseline does not violate
    // is judged as a tightening even when the baseline rejects too.
    let new_rejection = !after && rejects_for_a_new_reason(&baseline_errors, &candidate_errors);
    // A baseline rejecting the probe only as it rejects the chart's own
    // defaults constrains nothing the probe changed.
    let uninformative =
        !before && profiles.baseline_rejects_it_as_its_defaults(probe.values.as_ref(), overlay);
    let baseline_errors: Vec<String> = baseline_errors.iter().map(ToString::to_string).collect();
    let candidate_errors: Vec<String> = candidate_errors.iter().map(ToString::to_string).collect();
    let mut evidence = json!({
        "baseline_rejects_it_as_its_defaults": uninformative,
        "baseline_accepts": before,
        "candidate_accepts": after,
        "candidate_rejects_for_a_new_reason": new_rejection,
        "baseline_errors": baseline_errors,
        "candidate_errors": candidate_errors,
        "helm_coalesced": probe.values.is_some(),
        "helm_exit": probe.rendered.status.code(),
    });
    let verdict = if before == after && !new_rejection {
        Ok(HelmFlipVerdict::Collapsed)
    } else if !probe.rendered.status.success() {
        if after && uninformative {
            Ok(HelmFlipVerdict::UninformativeBaselineFalseAcceptance(
                Rejection::HelmAborts,
            ))
        } else if after {
            Ok(HelmFlipVerdict::CandidateAcceptsHelmAborts)
        } else {
            Ok(HelmFlipVerdict::TighteningMatchedHelmAbort)
        }
    } else {
        let comparison = kubernetes.compare_with_defaults(chart, &probe.rendered.stdout)?;
        let object = evidence
            .as_object_mut()
            .ok_or_eyre("schema evidence is not an object")?;
        let new_errors: Vec<String> = comparison
            .new_violations
            .iter()
            .map(ToString::to_string)
            .collect();
        let inherited_errors: Vec<String> = comparison
            .inherited_violations
            .iter()
            .map(ToString::to_string)
            .collect();
        object.insert("new_kubernetes_errors".to_string(), json!(new_errors));
        object.insert(
            "inherited_kubernetes_errors".to_string(),
            json!(inherited_errors),
        );
        object.insert(
            "kubernetes_uncertainty".to_string(),
            json!(comparison.uncertain),
        );
        // A new violation is decisive; otherwise every changed resource must be decided.
        if !comparison.new_violations.is_empty() {
            if after && uninformative {
                Ok(HelmFlipVerdict::UninformativeBaselineFalseAcceptance(
                    Rejection::KubernetesRejects,
                ))
            } else if after {
                Ok(HelmFlipVerdict::CandidateAcceptsKubernetesRejects)
            } else {
                Ok(HelmFlipVerdict::TighteningMatchedKubernetesRejection)
            }
        } else if !comparison.uncertain.is_empty() {
            if after {
                Ok(HelmFlipVerdict::LooseningWithUncertainKubernetes(
                    comparison.uncertain.clone(),
                ))
            } else {
                Err(
                    "tightening rejects a document Helm renders without a proved Kubernetes violation",
                )
            }
        } else if !after {
            Err(
                "tightening rejects a document whose render adds no Kubernetes violation to the defaults render",
            )
        } else if comparison.inherited_violations.is_empty() {
            Ok(HelmFlipVerdict::LooseningMatchedKubernetesValidation)
        } else {
            Ok(HelmFlipVerdict::LooseningMatchedDefaultsViolations)
        }
    };
    std::fs::write(
        probe.evidence_dir.join("schema-verdicts.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )
    .wrap_err("write exact schema verdict evidence")?;
    let verdict = verdict
        .map_err(|reason| eyre::eyre!("{reason}; evidence={}", probe.evidence_dir.display()))?;
    eprintln!(
        "HELM_FLIP {verdict:?}: evidence={}",
        probe.evidence_dir.display()
    );
    Ok(verdict)
}

/// A loosening is matched only when every new or changed resource is decided
/// and its violations add nothing, counted with multiplicity, to the defaults
/// render's.
#[test]
fn loosenings_need_complete_evidence_beyond_the_defaults_render() -> eyre::Result<()> {
    let source = tempfile::tempdir()?;
    std::fs::create_dir(source.path().join("templates"))?;
    std::fs::write(
        source.path().join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: oracle-evidence
        version: 0.1.0
    "},
    )?;
    std::fs::write(
        source.path().join("values.yaml"),
        indoc::indoc! {"
        extra: false
        copies: 1
        tag: a
    "},
    )?;
    std::fs::write(
        source.path().join("templates/resources.yaml"),
        indoc::indoc! {r#"
        {{- if .Values.extra }}
        apiVersion: example.test/v1
        kind: Unknown
        metadata:
          name: added
        ---
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: invalid
        data:
          token: true
        {{- range until (int .Values.copies) }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: copy
          labels:
            copies: "{{ $.Values.copies }}"
            tag: "{{ $.Values.tag }}"
        data:
          token: true
        {{- end }}
    "#},
    )?;
    let chart = PinnedHelmChart::prepare(source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let mut kubernetes = OfflineKubernetesValidator::new(&cache);
    // The baseline accepts the defaults `{}`, so its rejections are evidence.
    let accept_all = ProfileSchemas::compile(&json!({"maxProperties": 0}), &json!({}), json!({}))?;
    let mut coverage = HelmAdjudicationCoverage::default();

    // A changed resource carrying only the defaults' violation is matched.
    let inherited =
        adjudicate_round74_flip(&chart, &json!({"tag": "b"}), &accept_all, &mut kubernetes)?;
    sim_assert_eq!(have: inherited, want: HelmFlipVerdict::LooseningMatchedDefaultsViolations);
    coverage.record_verdict(inherited, "inherited".to_string());
    validate_helm_adjudication_coverage(&coverage, &[], &[])?;

    // An added resource without a schema leaves the acceptance unproved, even
    // though the render's proven violations are all the defaults' own.
    let render = chart.adjudicate(&json!({"extra": true}))?;
    assert!(matches!(
        kubernetes.validate(&render.rendered.stdout)?,
        KubernetesVerdict::Invalid(_)
    ));
    let added = adjudicate_round74_flip(
        &chart,
        &json!({"extra": true}),
        &accept_all,
        &mut kubernetes,
    )?;
    sim_assert_eq!(
        have: added,
        want: HelmFlipVerdict::LooseningWithUncertainKubernetes(vec![
            "document 2: example.test/v1/Unknown added: pinned resource schema not found"
                .to_string(),
        ]),
    );
    let mut added_coverage = HelmAdjudicationCoverage::default();
    added_coverage.record_verdict(added, "added".to_string());
    assert!(validate_helm_adjudication_coverage(&added_coverage, &[], &[]).is_err());

    // Two changed copies of one identity inherit the defaults' single violation once.
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"copies": 2}), &accept_all, &mut kubernetes)?,
        want: HelmFlipVerdict::CandidateAcceptsKubernetesRejects,
    );
    Ok(())
}

#[test]
fn exact_flip_adjudication_uses_coalesced_values_and_kubernetes_evidence() -> eyre::Result<()> {
    let source = tempfile::tempdir()?;
    std::fs::create_dir(source.path().join("templates"))?;
    std::fs::write(
        source.path().join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: oracle-control
        version: 0.1.0
    "},
    )?;
    std::fs::write(
        source.path().join("values.yaml"),
        indoc::indoc! {"
        name: valid
        filled: true
    "},
    )?;
    std::fs::write(
        source.path().join("templates/configmap.yaml"),
        indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ .Values.name }}
    "},
    )?;
    let chart = PinnedHelmChart::prepare(source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let mut kubernetes = OfflineKubernetesValidator::new(&cache);
    let string_name = json!({"properties": {"name": {"type": "string"}}});
    let tightening = ProfileSchemas::compile(&json!({}), &string_name, json!({}))?;

    // Helm parses the manifest, but the rendered boolean violates Kubernetes metadata typing.
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": true}), &tightening, &mut kubernetes)?,
        want: HelmFlipVerdict::TighteningMatchedKubernetesRejection,
    );
    let loosening = ProfileSchemas::compile(&string_name, &json!({}), json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": true}), &loosening, &mut kubernetes)?,
        want: HelmFlipVerdict::CandidateAcceptsKubernetesRejects,
    );

    // Actual chart defaults fill this key even when the screening document did not include it.
    let screened =
        ProfileSchemas::compile(&json!({"required": ["filled"]}), &json!({}), json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({}), &screened, &mut kubernetes)?,
        want: HelmFlipVerdict::Collapsed,
    );

    // Neither successful rendering nor a missing provider schema proves a tightening valid.
    let unjustified = ProfileSchemas::compile(&json!({}), &json!(false), json!({}))?;
    assert!(adjudicate_round74_flip(&chart, &json!({}), &unjustified, &mut kubernetes).is_err());
    let empty_cache = tempfile::tempdir()?;
    let mut missing = OfflineKubernetesValidator::new(empty_cache.path());
    assert!(
        adjudicate_round74_flip(&chart, &json!({"name": true}), &tightening, &mut missing).is_err()
    );

    // Rendering alone and complete Kubernetes validation remain distinguishable evidence.
    // The baseline accepts the defaults `{}`, so its rejections are evidence.
    let accept_all = ProfileSchemas::compile(&json!({"maxProperties": 0}), &json!({}), json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({}), &accept_all, &mut kubernetes)?,
        want: HelmFlipVerdict::LooseningMatchedKubernetesValidation,
    );
    // A render identical to the defaults render needs no schema; a changed one does.
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({}), &accept_all, &mut missing)?,
        want: HelmFlipVerdict::LooseningMatchedKubernetesValidation,
    );
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": "changed"}), &accept_all, &mut missing)?,
        want: HelmFlipVerdict::LooseningWithUncertainKubernetes(vec![
            "document 0: v1/ConfigMap changed: pinned resource schema not found".to_string(),
        ]),
    );

    // The actual probe aborts even though this chart's unmodified defaults render.
    let invalid_yaml = json!({"name": "["});
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &invalid_yaml, &unjustified, &mut kubernetes)?,
        want: HelmFlipVerdict::TighteningMatchedHelmAbort,
    );
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &invalid_yaml, &accept_all, &mut kubernetes)?,
        want: HelmFlipVerdict::CandidateAcceptsHelmAborts,
    );
    Ok(())
}

/// A baseline that rejects its own defaults rejects every probe the same
/// way, so its rejection is no evidence: the flip is judged on the candidate
/// alone, and a candidate acceptance Helm or Kubernetes rejects is a false
/// acceptance of the candidate rather than a loosening.
#[test]
fn a_baseline_rejecting_its_own_defaults_is_no_evidence() -> eyre::Result<()> {
    let source = tempfile::tempdir()?;
    std::fs::create_dir(source.path().join("templates"))?;
    std::fs::write(
        source.path().join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: oracle-uninformative
        version: 0.1.0
    "},
    )?;
    std::fs::write(source.path().join("values.yaml"), "name: valid\n")?;
    std::fs::write(
        source.path().join("templates/configmap.yaml"),
        indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ .Values.name }}
    "},
    )?;
    let chart = PinnedHelmChart::prepare(source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let mut kubernetes = OfflineKubernetesValidator::new(&cache);
    let blanket = ProfileSchemas::compile(&json!({"required": ["unset"]}), &json!({}), json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": "["}), &blanket, &mut kubernetes)?,
        want: HelmFlipVerdict::UninformativeBaselineFalseAcceptance(Rejection::HelmAborts),
    );
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": true}), &blanket, &mut kubernetes)?,
        want: HelmFlipVerdict::UninformativeBaselineFalseAcceptance(Rejection::KubernetesRejects),
    );
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": "changed"}), &blanket, &mut kubernetes)?,
        want: HelmFlipVerdict::LooseningMatchedKubernetesValidation,
    );
    // A baseline error beyond its defaults' own is evidence again.
    let informative = ProfileSchemas::compile(
        &json!({"required": ["unset"], "properties": {"name": {"type": "string"}}}),
        &json!({}),
        json!({}),
    )?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": true}), &informative, &mut kubernetes)?,
        want: HelmFlipVerdict::CandidateAcceptsKubernetesRejects,
    );
    Ok(())
}

/// A baseline rejecting its defaults for one missing property is no
/// evidence about a probe missing another: the missing property is part of
/// the violated assertion.
#[test]
fn a_different_missing_property_is_evidence_against_a_baseline_rejecting_its_defaults()
-> eyre::Result<()> {
    let source = tempfile::tempdir()?;
    std::fs::create_dir(source.path().join("templates"))?;
    std::fs::write(
        source.path().join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: oracle-missing-property
        version: 0.1.0
    "},
    )?;
    std::fs::write(source.path().join("values.yaml"), "b: present\n")?;
    std::fs::write(
        source.path().join("templates/configmap.yaml"),
        indoc::indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: missing-property
        data:
          b: {{ required "b" .Values.b | quote }}
    "#},
    )?;
    let chart = PinnedHelmChart::prepare(source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let mut kubernetes = OfflineKubernetesValidator::new(&cache);
    // The baseline rejects the defaults for missing `a`, and the probe for missing `b`.
    let requires_both = ProfileSchemas::compile(
        &json!({"required": ["a", "b"]}),
        &json!({}),
        json!({"b": "present"}),
    )?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"a": "set", "b": null}), &requires_both, &mut kubernetes)?,
        want: HelmFlipVerdict::CandidateAcceptsHelmAborts,
    );
    Ok(())
}

/// A candidate rejecting a probe the baseline rejects too is a flip when it
/// rejects for a reason the baseline does not: a new false rejection must
/// not hide behind an old one. The same assertion placed elsewhere in the
/// candidate schema is no new reason.
#[test]
fn a_new_rejection_behind_a_baseline_rejection_is_adjudicated() -> eyre::Result<()> {
    let source = tempfile::tempdir()?;
    std::fs::create_dir(source.path().join("templates"))?;
    std::fs::write(
        source.path().join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: oracle-hidden-rejection
        version: 0.1.0
    "},
    )?;
    std::fs::write(
        source.path().join("values.yaml"),
        indoc::indoc! {"
        a: x
        b: y
    "},
    )?;
    std::fs::write(
        source.path().join("templates/configmap.yaml"),
        indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: hidden-rejection
        data:
          a: {{ .Values.a | quote }}
          b: {{ .Values.b | quote }}
    "},
    )?;
    let chart = PinnedHelmChart::prepare(source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let mut kubernetes = OfflineKubernetesValidator::new(&cache);
    let string_a = json!({"properties": {"a": {"type": "string"}}});
    // Helm renders `{a: 1, b: 1}`; both schemas reject it, the candidate also for `b`.
    let hidden = ProfileSchemas::compile(
        &string_a,
        &json!({"properties": {"a": {"type": "string"}, "b": {"type": "string"}}}),
        json!({"a": "x", "b": "y"}),
    )?;
    let probe = ProbeInstance::SparseOverride(json!({"a": 1, "b": 1}));
    assert!(hidden.screens_a_flip(&probe));
    assert!(
        adjudicate_round74_flip(&chart, &json!({"a": 1, "b": 1}), &hidden, &mut kubernetes)
            .is_err(),
        "the candidate's new rejection of a document Helm renders is unmatched"
    );
    let moved = ProfileSchemas::compile(
        &string_a,
        &json!({"allOf": [true, string_a]}),
        json!({"a": "x", "b": "y"}),
    )?;
    assert!(!moved.screens_a_flip(&probe));
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"a": 1, "b": 1}), &moved, &mut kubernetes)?,
        want: HelmFlipVerdict::Collapsed,
    );
    Ok(())
}

#[test]
fn scalar_overrides_of_subchart_tables_are_adjudicated_by_helm_exit() -> eyre::Result<()> {
    let source = tempfile::tempdir()?;
    let child = source.path().join("charts/child");
    std::fs::create_dir_all(source.path().join("templates"))?;
    std::fs::create_dir_all(child.join("templates"))?;
    std::fs::write(
        source.path().join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: parent
        version: 0.1.0
        dependencies:
          - name: child
            version: 0.1.0
    "},
    )?;
    std::fs::write(source.path().join("values.yaml"), "{}\n")?;
    std::fs::write(
        child.join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: child
        version: 0.1.0
    "},
    )?;
    std::fs::write(
        child.join("values.yaml"),
        indoc::indoc! {"
        settings:
          mode: a
        strict:
          mode: a
    "},
    )?;
    std::fs::write(
        child.join("templates/configmap.yaml"),
        indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: child
        data:
          settings: {{ .Values.settings | toJson | quote }}
          strict: {{ .Values.strict.mode | quote }}
    "},
    )?;
    let chart = PinnedHelmChart::prepare(source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let mut kubernetes = OfflineKubernetesValidator::new(&cache);

    // A scalar subchart scope aborts coalescence: "type mismatch on child".
    let child_scope = json!({"properties": {"child": {"type": "object"}}});
    let scope_tightening = ProfileSchemas::compile(&json!({}), &child_scope, json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"child": false}), &scope_tightening, &mut kubernetes)?,
        want: HelmFlipVerdict::TighteningMatchedHelmAbort,
    );
    let scope_loosening = ProfileSchemas::compile(&child_scope, &json!({}), json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"child": false}), &scope_loosening, &mut kubernetes)?,
        want: HelmFlipVerdict::CandidateAcceptsHelmAborts,
    );

    // A scalar over a table inside the subchart only warns: Helm keeps the scalar and renders.
    let settings_table =
        json!({"properties": {"child": {"properties": {"settings": {"type": "object"}}}}});
    let settings_loosening = ProfileSchemas::compile(&settings_table, &json!({}), json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(
            &chart,
            &json!({"child": {"settings": false}}),
            &settings_loosening,
            &mut kubernetes,
        )?,
        want: HelmFlipVerdict::LooseningMatchedKubernetesValidation,
    );

    // After the same warning, the template itself aborts on field access through the scalar.
    let strict_table =
        json!({"properties": {"child": {"properties": {"strict": {"type": "object"}}}}});
    let strict_tightening = ProfileSchemas::compile(&json!({}), &strict_table, json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(
            &chart,
            &json!({"child": {"strict": false}}),
            &strict_tightening,
            &mut kubernetes,
        )?,
        want: HelmFlipVerdict::TighteningMatchedHelmAbort,
    );
    Ok(())
}

#[test]
fn kubernetes_verdicts_are_relative_to_the_defaults_render() -> eyre::Result<()> {
    let source = tempfile::tempdir()?;
    std::fs::create_dir(source.path().join("templates"))?;
    std::fs::write(
        source.path().join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: defaults-violate
        version: 0.1.0
    "},
    )?;
    std::fs::write(
        source.path().join("values.yaml"),
        indoc::indoc! {"
        count: 1
        label: base
        size: small
        extra: false
    "},
    )?;
    // The defaults already render an integer ConfigMap value at /data/count.
    std::fs::write(
        source.path().join("templates/configmap.yaml"),
        indoc::indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: defaults-violate
        data:
          count: {{ .Values.count }}
          label: {{ .Values.label | quote }}
          size: {{ .Values.size }}
    "},
    )?;
    // Sorted first, so enabling it moves the violating ConfigMap to another document.
    std::fs::write(
        source.path().join("templates/a-extra.yaml"),
        indoc::indoc! {"
        {{- if .Values.extra }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: a-extra
        {{- end }}
    "},
    )?;
    let chart = PinnedHelmChart::prepare(source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let mut kubernetes = OfflineKubernetesValidator::new(&cache);
    let base_label = json!({"properties": {"label": {"const": "base"}}});
    let loosening = ProfileSchemas::compile(&base_label, &json!({}), json!({}))?;

    // The probe renders only the defaults' own violation, wherever it lands.
    for overlay in [json!({"label": "x"}), json!({"label": "x", "extra": true})] {
        sim_assert_eq!(
            have: adjudicate_round74_flip(&chart, &overlay, &loosening, &mut kubernetes)?,
            want: HelmFlipVerdict::LooseningMatchedDefaultsViolations,
        );
    }
    // A probe adding its own violation at /data/size stays unmatched.
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"label": "x", "size": 3}), &loosening, &mut kubernetes)?,
        want: HelmFlipVerdict::CandidateAcceptsKubernetesRejects,
    );
    // A tightening needs a violation the defaults do not already render.
    let tightening = ProfileSchemas::compile(&json!({}), &base_label, json!({}))?;
    assert!(
        adjudicate_round74_flip(&chart, &json!({"label": "x"}), &tightening, &mut kubernetes)
            .is_err()
    );
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"label": "x", "size": 3}), &tightening, &mut kubernetes)?,
        want: HelmFlipVerdict::TighteningMatchedKubernetesRejection,
    );
    Ok(())
}

#[test]
fn existing_configmap_name_tightenings_match_kubernetes_rejections() -> eyre::Result<()> {
    let chart_path = test_util::workspace_testdata().join("charts/oauth2-proxy");
    let chart = PinnedHelmChart::prepare(&chart_path)?;
    let candidate = read_chart_schema_fixture("oauth2-proxy")?;
    let profiles = ProfileSchemas::compile(&json!({}), &candidate, json!({}))?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let mut kubernetes = OfflineKubernetesValidator::new(&cache);

    // Each source value becomes a non-string ConfigMap volume name after Helm's YAML conversion.
    for name in [json!(true), json!(1.5), json!("3")] {
        sim_assert_eq!(
            have: adjudicate_round74_flip(
                &chart,
                &json!({"config": {"existingConfig": name}}),
                &profiles,
                &mut kubernetes,
            )?,
            want: HelmFlipVerdict::TighteningMatchedKubernetesRejection,
        );
    }
    sim_assert_eq!(
        have: adjudicate_round74_flip(
            &chart,
            &json!({"config": {"existingConfig": "valid-config"}}),
            &profiles,
            &mut kubernetes,
        )?,
        want: HelmFlipVerdict::Collapsed,
    );
    Ok(())
}

#[test]
#[ignore = "maintenance: requires LEGACY_LEAN_SCHEMA_DIR"]
fn middle_lean_transition_has_only_preregistered_tightenings() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let baseline_dir = std::path::PathBuf::from(
        std::env::var("LEGACY_LEAN_SCHEMA_DIR")
            .wrap_err("LEGACY_LEAN_SCHEMA_DIR must contain legacy lean schemas")?,
    );
    let mut probes_checked = 0;
    let mut tightenings = Vec::new();
    let mut inverse = Vec::new();
    let adjudicate_live = std::env::var("ADJUDICATE_WITH_HELM").is_ok();
    if adjudicate_live {
        let output = std::process::Command::new("helm")
            .args(["version", "--template", "{{.Version}}"])
            .output()
            .wrap_err("read Helm version for transition adjudication")?;
        eyre::ensure!(output.status.success(), "helm version failed");
        let version = String::from_utf8(output.stdout).wrap_err("decode Helm version")?;
        eyre::ensure!(
            version.trim() == "v4.2.3",
            "transition adjudication requires Helm v4.2.3, found {}",
            version.trim()
        );
    }

    for chart in LEAN_FIXTURE_CHARTS {
        let baseline_path = baseline_dir.join(format!("{chart}.schema.json"));
        let baseline: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&baseline_path)
                .wrap_err_with(|| format!("read {}", baseline_path.display()))?,
        )
        .wrap_err_with(|| format!("parse {}", baseline_path.display()))?;
        let current = generate_profile_schemas(chart)?.1;
        let defaults = if *chart == "schema-emission-temporal-wrapper" {
            read_json_fixture(chart, "coalesced-defaults.json")?
        } else {
            read_coalesced_defaults(chart)?
        };
        let transition = ProfileSchemas::compile(&baseline, &current, defaults.clone())?;
        for (probe_name, probe) in
            structural_probe_battery(chart, &defaults, &[&baseline, &current])?
        {
            probes_checked += 1;
            let (legacy, middle) = transition.verdicts(&probe);
            match (legacy, middle) {
                (true, false) => {
                    if adjudicate_live {
                        adjudicate_transition_tightening(chart, &probe_name, &probe, &defaults)?;
                    }
                    tightenings.push(format!("{chart}: {probe_name}"));
                }
                (false, true) => inverse.push(format!("{chart}: {probe_name}")),
                (false, false) | (true, true) => {}
            }
        }
    }

    eyre::ensure!(
        inverse.is_empty(),
        "middle lean unexpectedly loosens legacy lean:\n{}",
        inverse.join("\n")
    );
    eyre::ensure!(
        !tightenings.is_empty(),
        "middle lean produced no preregistered transition tightenings"
    );
    eprintln!(
        "probes_checked={probes_checked} tightenings={} inverse=0",
        tightenings.len()
    );
    for tightening in tightenings {
        eprintln!("TIGHTEN {tightening}");
    }
    Ok(())
}

fn adjudicate_transition_tightening(
    chart: &str,
    probe_name: &str,
    probe: &ProbeInstance,
    defaults: &serde_json::Value,
) -> eyre::Result<()> {
    let values = probe.helm_values_file(defaults);
    let tempdir = tempfile::tempdir().wrap_err("create live adjudication directory")?;
    let values_path = tempdir.path().join("values.json");
    std::fs::write(&values_path, serde_json::to_vec(&values)?)
        .wrap_err("write live adjudication values")?;
    let chart_path = test_util::workspace_testdata().join("charts").join(chart);
    let rendered = std::process::Command::new("helm")
        .args(["template", "step2-lean-transition"])
        .arg(chart_path)
        .arg("--skip-schema-validation")
        .arg("-f")
        .arg(values_path)
        .output()
        .wrap_err_with(|| format!("render {chart}: {probe_name}"))?;
    if !rendered.status.success() {
        eprintln!("HELM_REJECT {chart}: {probe_name}");
        return Ok(());
    }

    let manifest_path = tempdir.path().join("rendered.yaml");
    std::fs::write(&manifest_path, rendered.stdout).wrap_err("write rendered manifest")?;
    let provider = std::process::Command::new("kubeconform")
        .args(["-strict", "-kubernetes-version", "1.29.0"])
        .arg(manifest_path)
        .output()
        .wrap_err_with(|| format!("validate {chart}: {probe_name}"))?;
    eyre::ensure!(
        !provider.status.success(),
        "{chart}: {probe_name} renders and passes the provider; the middle-lean tightening is a false rejection"
    );
    eprintln!("PROVIDER_REJECT {chart}: {probe_name}");
    Ok(())
}
