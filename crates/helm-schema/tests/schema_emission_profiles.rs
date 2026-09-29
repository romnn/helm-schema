//! Monotonicity and semantic-oracle harness for schema emission profiles.

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use serde::Serialize;
use serde_json::json;
use std::path::PathBuf;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

#[path = "common/emission_profile_harness.rs"]
mod harness;

use helm_schema_test_support::helm::adjudication as helm_adjudication;
use helm_schema_test_support::helm::invocation as helm_invocation;
use helm_schema_test_support::{generate, registry};

#[path = "common/helm_pool.rs"]
mod helm_pool;

#[path = "common/known_false_acceptances.rs"]
mod known_false_acceptances;

#[path = "common/known_undecided_acceptances.rs"]
mod known_undecided_acceptances;

use helm_adjudication::{KubernetesVerdict, OfflineKubernetesValidator, PinnedHelmChart};
use helm_pool::{PoolLimits, PoolPeaks, run_ordered};
use known_false_acceptances::{
    Baseline, Family, KNOWN_FALSE_ACCEPTANCES, KnownFalseAcceptances, Probe, ROSTER_BASELINE,
    Rejection,
};
use known_undecided_acceptances::{KNOWN_UNDECIDED_ACCEPTANCES, KnownUndecidedAcceptances};

use harness::{
    ContractVerdict, ControlCategory, GuardSamplingStrategy, HelmChartDir, ProbeCoverage,
    ProbeInstance, ProbeValuesFile, ProfileSchemas, SemanticControl, Transport, UnreachableProbe,
    generate_profile_outputs, generate_profile_schemas, read_chart_schema_fixture,
    read_coalesced_defaults, rejects_for_a_new_reason, round_robin_base_probes, sparse_override,
    sparse_override_for_composed, structural_probe_battery, structural_probe_battery_with_coverage,
    violated_assertions,
};

// The roster's case matcher is this battery's own naming rule: the witness
// gate shares the roster data, not the battery's case spelling.

/// Whether the battery case `case` (`{chart}: {probe}`) is `probe` of
/// `chart`, plain or as the payload of a targeted guard probe.
pub(crate) fn names(chart: &str, probe: &Probe, case: &str) -> bool {
    let Some(name) = case
        .strip_prefix(chart)
        .and_then(|rest| rest.strip_prefix(": "))
    else {
        return false;
    };
    let assignment = format!("{} <- {}", probe.path, probe.value);
    name == assignment || name.ends_with(&format!("[targeted: {assignment}]"))
}

impl KnownFalseAcceptances {
    /// Whether this group lists `case`, failing by `rejection` behind a
    /// `baseline` rejection.
    pub(crate) fn lists(&self, case: &str, rejection: Rejection, baseline: Baseline) -> bool {
        self.rejection == rejection
            && self.baseline == baseline
            && self
                .probes
                .iter()
                .any(|probe| names(self.chart, probe, case))
    }
}

impl KnownUndecidedAcceptances {
    /// Whether this group lists `case`, left undecided for exactly
    /// `uncertain`.
    pub(crate) fn lists(&self, case: &str, uncertain: &[String]) -> bool {
        self.uncertain == uncertain
            && self
                .probes
                .iter()
                .any(|probe| names(self.chart, probe, case))
    }
}

#[derive(Debug, Serialize)]
struct ProbeCoverageReport {
    baseline_ref: String,
    charts: Vec<ProbeCoverage>,
    helm_adjudication: HelmAdjudicationCoverage,
}

// A non-zero allowance must land before the live run whose widening relies on it.
const PREREGISTERED_ACCEPTANCE_FLIP_ALLOWANCE: usize = 0;

/// Adjudicates screened flips live with the pinned Helm when set.
const ADJUDICATE_WITH_HELM_VAR: &str = "ADJUDICATE_WITH_HELM";
/// The commit whose fixtures are the baseline.
const BASELINE_REF_VAR: &str = "SCHEMA_ACCEPTANCE_BASELINE_REF";
/// The producer dump the candidate schemas are read from.
const CANDIDATE_DUMP_VAR: &str = "SCHEMA_ACCEPTANCE_CANDIDATE_DUMP";
/// Skips the pre-registered flip count when every flip is matched.
const ALLOW_MATCHED_FLIPS_VAR: &str = "SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS";
/// Restricts the battery to one fixture stem.
const CHART_VAR: &str = "SCHEMA_ACCEPTANCE_CHART";
/// Where the probe coverage report is written.
const COVERAGE_REPORT_VAR: &str = "SCHEMA_PROBE_COVERAGE_REPORT";
/// Where the per-chart Helm cost report is written, if anywhere.
const INVOCATION_REPORT_VAR: &str = "SCHEMA_HELM_INVOCATION_REPORT";
/// The chart directory of an external schema pair.
const EXTERNAL_CHART_VAR: &str = "SCHEMA_ACCEPTANCE_EXTERNAL_CHART";
/// The baseline schema file of an external schema pair.
const BASELINE_SCHEMA_VAR: &str = "SCHEMA_ACCEPTANCE_BASELINE_SCHEMA";
/// The candidate schema file of an external schema pair.
const CANDIDATE_SCHEMA_VAR: &str = "SCHEMA_ACCEPTANCE_CANDIDATE_SCHEMA";
/// The directory of legacy lean schemas the lean transition compares against.
const LEGACY_LEAN_SCHEMA_DIR_VAR: &str = "LEGACY_LEAN_SCHEMA_DIR";
/// Overrides the offline Kubernetes schema bundle the adjudication validates against.
const K8S_CACHE_VAR: &str = "SCHEMA_ACCEPTANCE_K8S_CACHE";
/// Restricts the lean-profile dump ([`test_util::SCHEMA_DUMP_VAR`]) to one chart.
const DUMP_CHART_VAR: &str = "SCHEMA_DUMP_CHART";

/// Whether screened flips are adjudicated live with the pinned Helm.
fn adjudicates_live() -> bool {
    std::env::var_os(ADJUDICATE_WITH_HELM_VAR).is_some()
}

fn baseline_ref() -> eyre::Result<String> {
    std::env::var(BASELINE_REF_VAR)
        .wrap_err_with(|| format!("{BASELINE_REF_VAR} must name the comparison commit"))
}

fn coverage_report_path() -> eyre::Result<std::path::PathBuf> {
    std::env::var_os(COVERAGE_REPORT_VAR)
        .map(std::path::PathBuf::from)
        .ok_or_else(|| eyre::eyre!("{COVERAGE_REPORT_VAR} must name the report file"))
}

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
    /// `chart: probe` of every rejected cell Helm renders whose rejection is
    /// exactly a declared default's type assertion on a path no template
    /// reads, which the `annotate` authoring policy accepts.
    tightenings_attributed_to_declared_types: Vec<String>,
    /// Accepted cells Helm renders whose changed resources Kubernetes cannot
    /// decide.
    loosenings_with_uncertain_kubernetes: Vec<ObservedUndecidedAcceptance>,
    /// Labels of the charts whose flips were adjudicated live.
    charts_adjudicated: std::collections::BTreeSet<String>,
    /// Accepted cells Helm or Kubernetes reject.
    false_acceptances: Vec<ObservedFalseAcceptance>,
    /// `chart: probe` of every composed probe no values file reaches. A
    /// roster row naming one cannot be observed and is kept, not "fixed".
    unreachable_cases: Vec<String>,
    /// The evidence directory of every adjudicated flip, preserved when a
    /// final gate fails.
    #[serde(skip)]
    flip_evidence: Vec<PathBuf>,
}

/// A false acceptance the battery observed, to be matched by the roster.
#[derive(Debug, Serialize)]
struct ObservedFalseAcceptance {
    case: String,
    rejection: Rejection,
    baseline: Baseline,
    #[serde(skip)]
    evidence: Option<PathBuf>,
}

/// An accepted cell Kubernetes cannot decide, to be matched by the roster.
#[derive(Debug, Serialize)]
struct ObservedUndecidedAcceptance {
    case: String,
    uncertain: Vec<String>,
    #[serde(skip)]
    evidence: Option<PathBuf>,
}

impl HelmAdjudicationCoverage {
    /// Counts `verdict` for `case`, keeping the flip's evidence directory
    /// (`None` for a verdict without one) for the final gates.
    fn record_verdict(
        &mut self,
        verdict: HelmFlipVerdict,
        case: String,
        evidence: Option<PathBuf>,
    ) -> Option<bool> {
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
            HelmFlipVerdict::TighteningMatchedDeclaredTypesPolicy => {
                self.tightenings_attributed_to_declared_types.push(case);
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
                    .push(ObservedUndecidedAcceptance {
                        case,
                        uncertain,
                        evidence: evidence.clone(),
                    });
                true
            }
            HelmFlipVerdict::CandidateAcceptsHelmAborts => {
                self.false_acceptances.push(ObservedFalseAcceptance {
                    case,
                    rejection: Rejection::HelmAborts,
                    baseline: Baseline::RejectsUnlikeItsDefaults,
                    evidence: evidence.clone(),
                });
                true
            }
            HelmFlipVerdict::CandidateAcceptsKubernetesRejects => {
                self.false_acceptances.push(ObservedFalseAcceptance {
                    case,
                    rejection: Rejection::KubernetesRejects,
                    baseline: Baseline::RejectsUnlikeItsDefaults,
                    evidence: evidence.clone(),
                });
                true
            }
            HelmFlipVerdict::UninformativeBaselineFalseAcceptance(rejection) => {
                self.false_acceptances.push(ObservedFalseAcceptance {
                    case,
                    rejection,
                    baseline: Baseline::RejectsItsDefaults,
                    evidence: evidence.clone(),
                });
                true
            }
        };
        self.flips_adjudicated += 1;
        self.flip_evidence.extend(evidence);
        Some(candidate_accepts)
    }
}

/// The rows of `roster` on an adjudicated chart that this run did not
/// observe failing alike. A row whose probe is unreachable cannot be
/// observed and is kept.
fn false_acceptance_rows_not_observed(
    coverage: &HelmAdjudicationCoverage,
    roster: &[KnownFalseAcceptances],
) -> Vec<String> {
    let mut fixed = Vec::new();
    for group in roster {
        if !coverage.charts_adjudicated.contains(group.chart) {
            continue;
        }
        for probe in group.probes {
            let observed = coverage.false_acceptances.iter().any(|observed| {
                observed.rejection == group.rejection
                    && observed.baseline == group.baseline
                    && names(group.chart, probe, &observed.case)
            });
            let unreachable = coverage
                .unreachable_cases
                .iter()
                .any(|case| names(group.chart, probe, case));
            if unreachable {
                eprintln!(
                    "UNOBSERVABLE roster row (its probe is unreachable): {}: {} <- {}",
                    group.chart, probe.path, probe.value
                );
            } else if !observed {
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
    fixed
}

/// `; evidence=<dir>` naming the preserved copy of an observed cell's
/// evidence, which a failing gate reports; empty without evidence.
fn preserved_evidence(evidence: Option<&PathBuf>) -> String {
    match evidence {
        Some(evidence) => format!(
            "; evidence={}",
            helm_invocation::preserve_failure(evidence).display()
        ),
        None => String::new(),
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
            + coverage.tightenings_attributed_to_declared_types.len()
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
            unlisted_undecided.push(format!(
                "{} ({:?}){}",
                observed.case,
                observed.uncertain,
                preserved_evidence(observed.evidence.as_ref())
            ));
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
                "{} ({:?}, {:?}){}",
                observed.case,
                observed.rejection,
                observed.baseline,
                preserved_evidence(observed.evidence.as_ref())
            ));
        }
    }
    if !unlisted.is_empty() {
        problems.push(format!(
            "false acceptances missing from KNOWN_FALSE_ACCEPTANCES: {unlisted:?}"
        ));
    }
    let mut fixed = false_acceptance_rows_not_observed(coverage, roster);
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
                        && names(group.chart, probe, &observed.case)
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
        HelmFlipVerdict::TighteningMatchedDeclaredTypesPolicy,
        HelmFlipVerdict::LooseningMatchedKubernetesValidation,
        HelmFlipVerdict::LooseningMatchedDefaultsViolations,
    ] {
        coverage.record_verdict(verdict, "confirmed".to_string(), None);
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
        None,
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
        None,
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
    // Reachable coverage floor: at least half of a chart's composed probes
    // (rounded down) must be documents Helm composes. The known
    // unreachable case is the single "all declared keys deleted" probe of a
    // chart whose parent `global` reaches its dependencies.
    let reachable = chart
        .composed_probes
        .saturating_sub(chart.unreachable_probes.len());
    eyre::ensure!(
        chart.unreachable_probes.len() <= chart.composed_probes
            && reachable >= chart.composed_probes / 2,
        "too few composed probes are reachable through a values file: {chart:?}"
    );
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

/// Unreachable composed probes leave the battery, but at least half of a
/// chart's composed probes must stay reachable.
#[test]
fn probe_coverage_validation_holds_the_reachable_floor() {
    let coverage = |composed: usize, unreachable: usize| ProbeCoverage {
        label: "synthetic reachability".to_string(),
        base_candidates: 1,
        base_emitted: 1,
        total_emitted: 1 + composed,
        composed_probes: composed,
        unreachable_probes: (0..unreachable)
            .map(|index| UnreachableProbe {
                probe: format!("probe {index}"),
                reason: "synthetic".to_string(),
            })
            .collect(),
        ..ProbeCoverage::default()
    };
    assert!(validate_probe_coverage(&coverage(1, 1)).is_ok());
    assert!(validate_probe_coverage(&coverage(17, 9)).is_ok());
    assert!(validate_probe_coverage(&coverage(17, 10)).is_err());
    assert!(validate_probe_coverage(&coverage(0, 1)).is_err());
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
        None,
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
            HelmFlipVerdict::TighteningMatchedDeclaredTypesPolicy,
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
        sim_assert_eq!(have: coverage.record_verdict(verdict, "case".to_string(), None), want: accepted);
    }
    sim_assert_eq!(have: json!(coverage), want: json!({
        "enabled": false, "screening_is_exact": false, "screened_flips": 0,
        "screened_flips_collapsed": 1, "flips_adjudicated": 9,
        "tightenings_matched_helm_abort": 1, "tightenings_matched_kubernetes_rejection": 1,
        "loosenings_matched_kubernetes_validation": 1,
        "loosenings_matched_defaults_violations": 1,
        "tightenings_attributed_to_declared_types": ["case"],
        "loosenings_with_uncertain_kubernetes": [{ "case": "case", "uncertain": ["reason"] }],
        "charts_adjudicated": [],
        "unreachable_cases": [],
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

const ROSTER_LISTED: &[Probe] = &[
    Probe {
        path: "a.b",
        value: "non-coercible string",
    },
    Probe {
        path: "c",
        value: "non-coercible string",
    },
];
const ROSTER_WITH_FIXED: &[Probe] = &[
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
const ROSTER_INFORMED: &[Probe] = &[Probe {
    path: "d",
    value: "null deletion",
}];

fn roster_group(
    chart: &'static str,
    rejection: Rejection,
    baseline: Baseline,
    probes: &'static [Probe],
) -> KnownFalseAcceptances {
    KnownFalseAcceptances {
        chart,
        rejection,
        baseline,
        family: Family::Unfiled,
        probes,
    }
}

/// Helm aborts behind a baseline rejecting `chart`'s own defaults alike.
fn uninformed_group(probes: &'static [Probe]) -> KnownFalseAcceptances {
    roster_group(
        "chart",
        Rejection::HelmAborts,
        Baseline::RejectsItsDefaults,
        probes,
    )
}

/// Helm aborts behind a baseline rejecting unlike its defaults.
fn informed_group(probes: &'static [Probe]) -> KnownFalseAcceptances {
    roster_group(
        "chart",
        Rejection::HelmAborts,
        Baseline::RejectsUnlikeItsDefaults,
        probes,
    )
}

/// A roster row whose probe no values file reaches cannot be observed, so
/// it is kept rather than reported as fixed.
#[test]
fn an_unreachable_roster_row_is_not_reported_fixed() -> eyre::Result<()> {
    let mut coverage = HelmAdjudicationCoverage::default();
    coverage.charts_adjudicated.insert("chart".to_string());
    let roster = [uninformed_group(ROSTER_LISTED)];
    assert!(validate_helm_adjudication_coverage(&coverage, &roster, &[]).is_err());
    for probe in ROSTER_LISTED {
        coverage.unreachable_cases.push(format!(
            "chart: root guard 0 satisfied [targeted: {} <- {}]",
            probe.path, probe.value
        ));
    }
    validate_helm_adjudication_coverage(&coverage, &roster, &[])
}

/// The roster matches each false acceptance by chart, probe, rejection and
/// how the baseline rejected it, and a probe of an adjudicated chart that no
/// longer fails alike must be removed.
#[test]
fn known_false_acceptances_are_matched_by_the_roster() -> eyre::Result<()> {
    let mut coverage = HelmAdjudicationCoverage::default();
    coverage.charts_adjudicated.insert("chart".to_string());
    for case in [
        "chart: a.b <- non-coercible string",
        "chart: root guard 1 satisfied [targeted: c <- non-coercible string]",
    ] {
        coverage.record_verdict(
            HelmFlipVerdict::UninformativeBaselineFalseAcceptance(Rejection::HelmAborts),
            case.to_string(),
            None,
        );
    }
    coverage.record_verdict(
        HelmFlipVerdict::CandidateAcceptsHelmAborts,
        "chart: d <- null deletion".to_string(),
        None,
    );
    validate_helm_adjudication_coverage(
        &coverage,
        &[
            uninformed_group(ROSTER_LISTED),
            informed_group(ROSTER_INFORMED),
        ],
        &[],
    )?;
    // A chart this run did not adjudicate keeps its entries.
    validate_helm_adjudication_coverage(
        &coverage,
        &[
            uninformed_group(ROSTER_LISTED),
            informed_group(ROSTER_INFORMED),
            roster_group(
                "other",
                Rejection::HelmAborts,
                Baseline::RejectsItsDefaults,
                ROSTER_WITH_FIXED,
            ),
        ],
        &[],
    )?;
    for roster in [
        vec![uninformed_group(ROSTER_LISTED)],
        vec![
            uninformed_group(&ROSTER_LISTED[..1]),
            informed_group(ROSTER_INFORMED),
        ],
        vec![
            roster_group(
                "chart",
                Rejection::KubernetesRejects,
                Baseline::RejectsItsDefaults,
                ROSTER_LISTED,
            ),
            informed_group(ROSTER_INFORMED),
        ],
        // A cell behind a baseline with its own violations is not the
        // uninformed baseline's, and the reverse.
        vec![
            uninformed_group(ROSTER_LISTED),
            uninformed_group(ROSTER_INFORMED),
        ],
        vec![
            informed_group(ROSTER_LISTED),
            informed_group(ROSTER_INFORMED),
        ],
        vec![
            uninformed_group(ROSTER_WITH_FIXED),
            informed_group(ROSTER_INFORMED),
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
    sim_assert_eq!(
        have: test_util::helm_values::coalesce_tables(&patch, &defaults),
        want: composed
    );
    Ok(())
}

/// Screening composes a sparse override the way Helm's `CoalesceTables`
/// does: a null deletes the default it meets, and a null with no default
/// stays in the document the schema validates.
#[test]
fn a_null_override_without_a_default_stays_in_the_screened_document() -> eyre::Result<()> {
    let profiles = ProfileSchemas::compile(
        &json!({}),
        &json!({"properties": {"absent": {"type": "string"}, "present": {"type": "integer"}}}),
        json!({"present": 1}),
    )?;
    sim_assert_eq!(
        have: profiles.verdicts(&ProbeInstance::SparseOverride(json!({"present": null}))),
        want: (true, true)
    );
    sim_assert_eq!(
        have: profiles.verdicts(&ProbeInstance::SparseOverride(json!({"absent": null}))),
        want: (true, false)
    );
    Ok(())
}

/// A composed probe reaches Helm through the override that separates it
/// from the defaults; a document no override reaches is unreachable.
#[test]
fn a_composed_probe_helm_cannot_reach_is_unreachable() -> eyre::Result<()> {
    let chart = test_util::workspace_testdata().join("helm-values/nulls");
    let defaults = test_util::helm_values::coalesce_chart_values(&chart, json!({}))?;
    let mut deleted = defaults.clone();
    deleted
        .as_object_mut()
        .ok_or_eyre("defaults are a map")?
        .remove("owned");
    sim_assert_eq!(
        have: ProbeInstance::Coalesced(deleted).helm_values_file(&chart, &defaults)?,
        want: ProbeValuesFile::Reachable(json!({"owned": null}))
    );
    // Helm deletes a key a null override meets, so no values file holds it null.
    let mut nulled = defaults.clone();
    nulled
        .as_object_mut()
        .ok_or_eyre("defaults are a map")?
        .insert("owned".to_string(), serde_json::Value::Null);
    assert!(matches!(
        ProbeInstance::Coalesced(nulled).helm_values_file(&chart, &defaults)?,
        ProbeValuesFile::Unreachable(_)
    ));
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
    costs: Vec<ChartCost>,
    pool: Option<PoolReport>,
}

/// What one chart's comparison cost, for the Helm invocation report.
#[derive(Debug, Serialize)]
struct ChartCost {
    label: String,
    /// Reading both schemas, compiling them and generating the probe battery.
    context_ms: u64,
    screening_ms: u64,
    preparation_ms: u64,
    /// Helm executions and Kubernetes judgement of every screened flip.
    adjudication_ms: u64,
    /// Content identities of the prepared render and coalescence copies.
    prepared_trees: Option<(String, String)>,
    helm: std::collections::BTreeMap<String, helm_invocation::StageTotals>,
}

/// Writes the per-chart cost report `SCHEMA_HELM_INVOCATION_REPORT` names, if any.
fn write_invocation_report(costs: &[ChartCost], pool: Option<&PoolReport>) -> eyre::Result<()> {
    let Some(path) = std::env::var_os(INVOCATION_REPORT_VAR) else {
        return Ok(());
    };
    let path = std::path::PathBuf::from(path);
    let mut bytes = serde_json::to_vec_pretty(&json!({ "pool": pool, "charts": costs }))?;
    bytes.push(b'\n');
    std::fs::write(&path, bytes).wrap_err_with(|| format!("write {}", path.display()))
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
    let dump_chart = std::env::var(DUMP_CHART_VAR).ok();
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
        if std::env::var(test_util::SCHEMA_DUMP_VAR).is_ok() {
            std::fs::create_dir_all(test_util::scratch::target_dir().join("schema-dump"))
                .wrap_err("create schema dump root")?;
            let dump_path = test_util::scratch::target_dir()
                .join("schema-dump")
                .join(format!(
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
    let defaults = read_coalesced_defaults(chart)?;
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
    let defaults = read_coalesced_defaults(chart)?;
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
            definition_names: helm_schema::output::DefinitionNames::Source,
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
        costs: _,
        pool: _,
    } = corpus_acceptance_comparison()?;
    for chart in &coverage {
        validate_probe_coverage(chart)?;
    }
    let baseline_ref = baseline_ref()?;
    let report = ProbeCoverageReport {
        baseline_ref,
        charts: coverage,
        helm_adjudication,
    };
    let report_path = coverage_report_path()?;
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

/// Why a live battery against `baseline` cannot check the rosters: their
/// rows are flips against [`ROSTER_BASELINE`] and unobservable otherwise.
fn roster_baseline_problem(baseline: &str) -> Option<String> {
    (baseline != ROSTER_BASELINE).then(|| {
        format!(
            "the false-acceptance rosters are adjudicated against {ROSTER_BASELINE}; a live \
             battery against {baseline} observes none of their rows. Run it with \
             SCHEMA_ACCEPTANCE_BASELINE_REF={ROSTER_BASELINE}"
        )
    })
}

/// A same-code baseline screens no flips, so every roster row would look
/// fixed; the battery refuses that baseline up front instead.
#[test]
fn a_live_battery_needs_the_roster_baseline() {
    assert!(roster_baseline_problem(ROSTER_BASELINE).is_none());
    assert!(roster_baseline_problem("c02c01f8975fe799a0670720df87db3fed94eb7f").is_some());
}

#[test]
#[ignore = "maintenance: compares the Round 74 dump and records probe coverage"]
fn round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    if adjudicates_live() {
        // The shared runner checks the pinned Helm release once per process.
        helm_invocation::HelmRunner::shared()?;
        let baseline_ref = baseline_ref()?;
        let resolved = std::process::Command::new("git")
            .args([
                "rev-parse",
                "--verify",
                &format!("{baseline_ref}^{{commit}}"),
            ])
            .output()
            .wrap_err_with(|| format!("resolve {baseline_ref}"))?;
        eyre::ensure!(resolved.status.success(), "cannot resolve {baseline_ref}");
        if let Some(problem) = roster_baseline_problem(String::from_utf8(resolved.stdout)?.trim()) {
            eyre::bail!(problem);
        }
    }
    let AcceptanceComparison {
        charts_checked,
        probes_checked,
        flips,
        coverage,
        helm_adjudication_failures,
        helm_adjudication,
        costs,
        pool,
    } = corpus_acceptance_comparison()?;
    write_invocation_report(&costs, pool.as_ref())?;
    for chart in &coverage {
        validate_probe_coverage(chart)?;
    }
    let baseline_ref = baseline_ref()?;
    let report = ProbeCoverageReport {
        baseline_ref,
        charts: coverage,
        helm_adjudication,
    };
    let report_path = coverage_report_path()?;
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
    if std::env::var_os(ALLOW_MATCHED_FLIPS_VAR).is_none()
        && flips.len() != PREREGISTERED_ACCEPTANCE_FLIP_ALLOWANCE
    {
        let evidence: Vec<String> = report
            .helm_adjudication
            .flip_evidence
            .iter()
            .map(|evidence| {
                helm_invocation::preserve_failure(evidence)
                    .display()
                    .to_string()
            })
            .collect();
        eyre::bail!(
            "fixture acceptance flips differ from the pre-registered count:\n{}\nevidence:\n{}",
            flips.join("\n"),
            evidence.join("\n")
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
        adjudicates_live(),
        "external schema comparison requires {ADJUDICATE_WITH_HELM_VAR}"
    );
    let chart = std::path::PathBuf::from(
        std::env::var_os(EXTERNAL_CHART_VAR)
            .ok_or_else(|| eyre::eyre!("{EXTERNAL_CHART_VAR} must name the chart directory"))?,
    );
    let baseline: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::env::var_os(BASELINE_SCHEMA_VAR)
                .ok_or_else(|| eyre::eyre!("{BASELINE_SCHEMA_VAR} must name a schema"))?,
        )
        .wrap_err("read external baseline schema")?,
    )
    .wrap_err("parse external baseline schema")?;
    let candidate: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::env::var_os(CANDIDATE_SCHEMA_VAR)
                .ok_or_else(|| eyre::eyre!("{CANDIDATE_SCHEMA_VAR} must name a schema"))?,
        )
        .wrap_err("read external candidate schema")?,
    )
    .wrap_err("parse external candidate schema")?;
    let defaults = test_util::helm_values::coalesce_chart_values(&chart, json!({}))?;
    let chart = chart
        .to_str()
        .ok_or_eyre("external chart path must be UTF-8")?;
    let comparison = compare_charts(vec![ComparedChart {
        label: "external".to_string(),
        chart_relative_path: chart.to_string(),
        inputs: ChartInputs::Given {
            baseline,
            candidate,
            defaults,
        },
    }])?;
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
    let baseline_ref = baseline_ref()?;
    let candidate_dump = candidate_dump_dir(std::env::var_os(CANDIDATE_DUMP_VAR))?;
    let fixture_dir = test_util::workspace_testdata().join("chart-corpus-schemas");
    let mut fixture_paths = std::fs::read_dir(&fixture_dir)
        .wrap_err_with(|| format!("read {}", fixture_dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    fixture_paths.sort();

    let mut charts = Vec::new();
    let chart_filter = std::env::var(CHART_VAR).ok();
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
        charts.push(ComparedChart {
            label: chart.to_string(),
            chart_relative_path: chart.to_string(),
            inputs: ChartInputs::Fixture {
                baseline_ref: baseline_ref.clone(),
                relative_path: format!("testdata/chart-corpus-schemas/{filename}"),
                candidate_path: candidate_dump
                    .join(format!("helm-schema.cli.chart-corpus.{chart}.schema.json")),
            },
        });
    }
    for chart in LEAN_FIXTURE_CHARTS {
        if chart_filter.is_some() {
            continue;
        }
        let filename = format!("{chart}.schema.json");
        charts.push(ComparedChart {
            label: format!("lean/{chart}"),
            chart_relative_path: (*chart).to_string(),
            inputs: ChartInputs::Fixture {
                baseline_ref: baseline_ref.clone(),
                relative_path: format!("testdata/emission-profile-schemas/lean/{filename}"),
                candidate_path: candidate_dump.join(format!(
                    "helm-schema.emission-profile.lean.{chart}.schema.json"
                )),
            },
        });
    }
    let mut comparison = compare_charts(charts)?;
    comparison.charts_checked = comparison.coverage.len();
    Ok(comparison)
}

/// Reads a schema fixture as an earlier commit recorded it.
///
/// An earlier commit may carry Go/RE2 pattern spellings that today's strict
/// `u`-mode regex check rejects outright.
/// The schema passes through the same exact respelling that provider
/// ingestion now applies, so it accepts the same values and still compiles.
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
    let mut schema = serde_json::from_slice(&output.stdout)
        .wrap_err_with(|| format!("parse {reference}:{relative_path}"))?;
    helm_schema_core::normalize_schema_pattern_dialects(&mut schema);
    Ok(schema)
}

/// The producer dump the candidate schemas come from. Every mode requires it:
/// the committed fixtures are the candidate's own fallback, so comparing
/// against them screens a schema against itself and proves nothing.
fn candidate_dump_dir(value: Option<std::ffi::OsString>) -> eyre::Result<PathBuf> {
    value.map(PathBuf::from).ok_or_else(|| {
        eyre::eyre!(
            "{CANDIDATE_DUMP_VAR} must name the producer dump of the candidate build; the \
             committed fixtures are not a candidate"
        )
    })
}

#[test]
fn preservation_battery_requires_candidate_dump() {
    assert!(candidate_dump_dir(None).is_err());
    assert!(candidate_dump_dir(Some("/dump".into())).is_ok());
}

fn read_acceptance_candidate(candidate_path: &std::path::Path) -> eyre::Result<serde_json::Value> {
    serde_json::from_str(
        &std::fs::read_to_string(candidate_path)
            .wrap_err_with(|| format!("read {}", candidate_path.display()))?,
    )
    .wrap_err_with(|| format!("parse {}", candidate_path.display()))
}

/// One chart whose baseline and candidate schemas are compared.
struct ComparedChart {
    label: String,
    /// The chart directory, relative to the corpus charts or absolute.
    chart_relative_path: String,
    inputs: ChartInputs,
}

/// Where a compared chart's schemas and defaults come from.
enum ChartInputs {
    Fixture {
        baseline_ref: String,
        relative_path: String,
        candidate_path: PathBuf,
    },
    Given {
        baseline: serde_json::Value,
        candidate: serde_json::Value,
        defaults: serde_json::Value,
    },
}

/// The whole-chart recipe the registry generates `dump_name` from.
fn chart_recipe(dump_name: &str) -> Option<registry::ChartRecipe> {
    registry::registry()
        .into_iter()
        .find_map(|spec| match spec.recipe {
            registry::GenerationRecipe::Chart(recipe) if spec.dump_name == dump_name => {
                Some(recipe)
            }
            _ => None,
        })
}

impl ChartInputs {
    /// The candidate's registry recipe under `--declared-types=annotate`;
    /// `None` for a schema pair no recipe generates.
    fn declared_types_annotation(&self) -> Option<DeclaredTypesAnnotation> {
        let Self::Fixture { candidate_path, .. } = self else {
            return None;
        };
        let recipe = chart_recipe(candidate_path.file_name()?.to_str()?)?;
        Some(DeclaredTypesAnnotation::new(
            generate::chart_dir(recipe.chart),
            recipe,
        ))
    }

    /// The baseline schema, the candidate schema and the chart's defaults.
    fn load(
        self,
        chart: &str,
    ) -> eyre::Result<(serde_json::Value, serde_json::Value, serde_json::Value)> {
        match self {
            Self::Fixture {
                baseline_ref,
                relative_path,
                candidate_path,
            } => {
                let baseline = read_schema_at_ref(&baseline_ref, &relative_path)?;
                let candidate = read_acceptance_candidate(&candidate_path)?;
                let defaults = read_coalesced_defaults(chart)?;
                Ok((baseline, candidate, defaults))
            }
            Self::Given {
                baseline,
                candidate,
                defaults,
            } => Ok((baseline, candidate, defaults)),
        }
    }
}

/// A chart whose screened flips Helm adjudicates, shared by its probe jobs
/// and dropped with the last of them.
struct LiveChart {
    label: String,
    path: std::path::PathBuf,
    profiles: ProfileSchemas,
    /// Each screened probe's name and Helm values file, in probe order.
    flips: Vec<(String, serde_json::Value)>,
    helm: std::sync::Arc<ChartHelm>,
    /// The chart under `--declared-types=annotate`, when a registry recipe
    /// generates the candidate.
    annotation: Option<DeclaredTypesAnnotation>,
}

/// A chart's pinned Helm copies, prepared by the first probe job that needs
/// them, inline.
#[derive(Default)]
struct ChartHelm {
    chart: std::sync::OnceLock<Result<PinnedHelmChart, String>>,
    preparation_ms: std::sync::atomic::AtomicU64,
}

impl ChartHelm {
    fn chart(&self, path: &std::path::Path) -> Result<&PinnedHelmChart, &str> {
        self.chart
            .get_or_init(|| {
                let started = std::time::Instant::now();
                let chart = helm_invocation::HelmRunner::shared()
                    .and_then(|runner| PinnedHelmChart::prepare(runner, path))
                    .map_err(|error| error.to_string());
                self.preparation_ms
                    .store(elapsed_ms(started), std::sync::atomic::Ordering::Relaxed);
                chart
            })
            .as_ref()
            .map_err(String::as_str)
    }

    /// Memory a probe job of this chart reserves: 1.3 times the largest
    /// Helm child measured for it, or all of `budget` until one was, so that
    /// a chart's first probe runs alone.
    fn reservation(&self, budget: u64) -> u64 {
        match self.chart.get() {
            Some(Ok(chart)) if chart.peak_child_rss() > 0 => {
                (chart.peak_child_rss().saturating_mul(13) / 10).min(budget)
            }
            _ => budget,
        }
    }
}

enum AcceptanceJob {
    Chart(Box<ComparedChart>),
    Probe(std::sync::Arc<LiveChart>, usize),
}

enum AcceptanceResult {
    Chart(eyre::Result<Box<ScreenedChart>>),
    Probe(AdjudicatedProbe),
}

/// A chart's probe battery and its screening.
struct ScreenedChart {
    label: String,
    coverage: ProbeCoverage,
    probes_checked: usize,
    /// Flips judged by the schemas alone, when Helm does not adjudicate.
    flips: Vec<String>,
    /// Probes whose values file the chart could not compose at all.
    failures: Vec<String>,
    /// Flips handed to Helm.
    screened_flips: usize,
    helm: Option<std::sync::Arc<ChartHelm>>,
    cost: ChartCost,
}

struct AdjudicatedProbe {
    case: String,
    outcome: Result<(HelmFlipVerdict, std::path::PathBuf), String>,
    adjudication_ms: u64,
}

/// Screens every chart and adjudicates every screened flip on the pool,
/// then folds the outcomes in chart and probe order.
fn compare_charts(charts: Vec<ComparedChart>) -> eyre::Result<AcceptanceComparison> {
    let adjudicate_live = adjudicates_live();
    let limits = PoolLimits::from_env()?;
    // Custom resources are judged by the CRD schemas pinned beside the bundle.
    let kubernetes = if adjudicate_live {
        let cache = std::env::var_os(K8S_CACHE_VAR).map_or_else(
            || test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
            std::path::PathBuf::from,
        );
        Some(OfflineKubernetesValidator::with_crd_catalog(
            &cache,
            &test_util::workspace_testdata().join("provider-bundle/crds-catalog-cache"),
            helm_adjudication::KUBERNETES_RELEASE,
        )?)
    } else {
        None
    };
    let started = std::time::Instant::now();
    let jobs = charts
        .into_iter()
        .enumerate()
        .map(|(index, chart)| ((index, 0), AcceptanceJob::Chart(Box::new(chart))))
        .collect();
    let (results, peaks) = run_ordered(
        limits,
        jobs,
        |job| match job {
            AcceptanceJob::Chart(_) => 0,
            AcceptanceJob::Probe(live, _) => live.helm.reservation(limits.memory_bytes),
        },
        |(chart_index, _), job, submitter| match job {
            AcceptanceJob::Chart(chart) => {
                let (screened, live) = match screen_chart(*chart, adjudicate_live) {
                    Ok(screened) => screened,
                    Err(error) => return AcceptanceResult::Chart(Err(error)),
                };
                if let Some(live) = live {
                    let live = std::sync::Arc::new(live);
                    for flip in 0..live.flips.len() {
                        submitter.submit(
                            (chart_index, flip + 1),
                            AcceptanceJob::Probe(live.clone(), flip),
                        );
                    }
                }
                AcceptanceResult::Chart(Ok(Box::new(screened)))
            }
            AcceptanceJob::Probe(live, flip) => {
                AcceptanceResult::Probe(adjudicate_probe(&live, flip, kubernetes.as_ref()))
            }
        },
    );
    let mut comparison = fold_results(results, adjudicate_live)?;
    if let Some(kubernetes) = &kubernetes {
        kubernetes.verify_bundles_unchanged()?;
    }
    comparison.pool = Some(PoolReport {
        workers: limits.workers,
        memory_bytes: limits.memory_bytes,
        peaks,
        wall_ms: elapsed_ms(started),
    });
    Ok(comparison)
}

/// Folds chart and probe results, already in ordinal order, into one comparison.
fn fold_results(
    results: Vec<(helm_pool::Ordinal, AcceptanceResult)>,
    adjudicate_live: bool,
) -> eyre::Result<AcceptanceComparison> {
    let mut comparison = AcceptanceComparison::default();
    comparison.helm_adjudication.enabled = adjudicate_live;
    let mut helm_charts = Vec::new();
    for (_, result) in results {
        match result {
            AcceptanceResult::Chart(screened) => {
                let screened = screened?;
                if adjudicate_live {
                    comparison
                        .helm_adjudication
                        .charts_adjudicated
                        .insert(screened.label.clone());
                }
                comparison.probes_checked += screened.probes_checked;
                comparison.flips.extend(screened.flips);
                comparison
                    .helm_adjudication_failures
                    .extend(screened.failures);
                comparison.helm_adjudication.screened_flips += screened.screened_flips;
                for unreachable in &screened.coverage.unreachable_probes {
                    comparison
                        .helm_adjudication
                        .unreachable_cases
                        .push(format!("{}: {}", screened.label, unreachable.probe));
                }
                comparison.coverage.push(screened.coverage);
                comparison.costs.push(screened.cost);
                helm_charts.push(screened.helm);
            }
            AcceptanceResult::Probe(probe) => {
                if let Some(cost) = comparison.costs.last_mut() {
                    cost.adjudication_ms += probe.adjudication_ms;
                }
                match probe.outcome {
                    Err(failure) => comparison.helm_adjudication_failures.push(failure),
                    Ok((verdict, evidence)) => {
                        eprintln!("HELM_FLIP {verdict:?}: evidence={}", evidence.display());
                        if let Some(candidate_accepts) = comparison
                            .helm_adjudication
                            .record_verdict(verdict, probe.case.clone(), Some(evidence))
                        {
                            comparison.flips.push(format!(
                                "{}: candidate accepts={candidate_accepts}",
                                probe.case
                            ));
                        }
                    }
                }
            }
        }
    }
    record_helm_costs(&helm_charts, &mut comparison.costs);
    Ok(comparison)
}

/// Records each chart's prepared copies and Helm costs in its cost entry.
fn record_helm_costs(helm_charts: &[Option<std::sync::Arc<ChartHelm>>], costs: &mut [ChartCost]) {
    for (helm, cost) in helm_charts.iter().zip(costs) {
        let Some(helm) = helm else { continue };
        cost.preparation_ms = helm
            .preparation_ms
            .load(std::sync::atomic::Ordering::Relaxed);
        if let Some(Ok(chart)) = helm.chart.get() {
            cost.helm = helm_invocation::stage_totals(&chart.invocations());
            cost.prepared_trees = Some((
                chart.render_tree().sha256().to_string(),
                chart.coalesce_tree().sha256().to_string(),
            ));
        }
    }
}

/// The pool a comparison ran on and the most it ran at once.
#[derive(Debug, Serialize)]
struct PoolReport {
    workers: usize,
    memory_bytes: u64,
    peaks: PoolPeaks,
    wall_ms: u64,
}

/// Compiles both profiles, generates the chart's probe battery and screens
/// it, returning the screened flips for Helm when `adjudicate_live`.
fn screen_chart(
    chart: ComparedChart,
    adjudicate_live: bool,
) -> eyre::Result<(ScreenedChart, Option<LiveChart>)> {
    let started = std::time::Instant::now();
    let ComparedChart {
        label,
        chart_relative_path,
        inputs,
    } = chart;
    let annotation = inputs.declared_types_annotation();
    let (baseline, current, defaults) = inputs.load(&chart_relative_path)?;
    let profiles = ProfileSchemas::compile(&baseline, &current, defaults.clone())?;
    let (probes, mut coverage) = structural_probe_battery_with_coverage(
        &chart_relative_path,
        &defaults,
        &[&baseline, &current],
    )?;
    coverage.label = label.clone();
    let chart_dir = HelmChartDir::stage(&chart_relative_path)?;
    let mut cost = ChartCost {
        label: label.clone(),
        context_ms: elapsed_ms(started),
        screening_ms: 0,
        preparation_ms: 0,
        adjudication_ms: 0,
        prepared_trees: None,
        helm: std::collections::BTreeMap::new(),
    };
    let screening = std::time::Instant::now();
    let probes_checked = probes.len();
    let mut flips = Vec::new();
    let mut failures = Vec::new();
    let mut screened = Vec::new();
    for (probe_name, probe) in probes {
        if matches!(probe, ProbeInstance::Coalesced(_)) {
            coverage.composed_probes += 1;
        }
        let overlay = match probe.helm_values_file(chart_dir.path(), &defaults) {
            Ok(ProbeValuesFile::Reachable(overlay)) => overlay,
            Ok(ProbeValuesFile::Unreachable(reason)) => {
                coverage.unreachable_probes.push(UnreachableProbe {
                    probe: probe_name,
                    reason,
                });
                continue;
            }
            Err(error) => {
                failures.push(format!("{label}: {probe_name}: {error}"));
                continue;
            }
        };
        if !profiles.screens_a_flip(&probe) {
            continue;
        }
        if adjudicate_live {
            // Screening proposes an overlay; only Helm can establish its coalesced document.
            screened.push((probe_name, overlay));
        } else {
            let (before, after) = profiles.verdicts(&probe);
            flips.push(format!(
                "{label}: {probe_name}: before={before}, after={after}"
            ));
        }
    }
    cost.screening_ms = elapsed_ms(screening);
    let live = (!screened.is_empty()).then(|| LiveChart {
        label: label.clone(),
        path: test_util::workspace_testdata()
            .join("charts")
            .join(&chart_relative_path),
        profiles,
        flips: screened,
        helm: std::sync::Arc::default(),
        annotation,
    });
    let screened = ScreenedChart {
        label,
        coverage,
        probes_checked,
        flips,
        failures,
        screened_flips: live.as_ref().map_or(0, |live| live.flips.len()),
        helm: live.as_ref().map(|live| live.helm.clone()),
        cost,
    };
    Ok((screened, live))
}

/// Adjudicates screened flip `flip` of `live` with Helm and Kubernetes.
fn adjudicate_probe(
    live: &LiveChart,
    flip: usize,
    kubernetes: Option<&OfflineKubernetesValidator>,
) -> AdjudicatedProbe {
    let started = std::time::Instant::now();
    let Some((probe_name, overlay)) = live.flips.get(flip) else {
        return AdjudicatedProbe {
            case: format!("{}: flip {flip}", live.label),
            outcome: Err(format!("{}: no screened flip {flip}", live.label)),
            adjudication_ms: 0,
        };
    };
    let case = format!("{}: {probe_name}", live.label);
    let outcome = match (live.helm.chart(&live.path), kubernetes) {
        (Err(error), _) => Err(format!("{case}: {error}")),
        (Ok(_), None) => Err(format!("{case}: no Kubernetes validator")),
        (Ok(chart), Some(kubernetes)) => adjudicate_flip(
            chart,
            overlay,
            &live.profiles,
            kubernetes,
            live.annotation.as_ref(),
        )
        .map_err(|error| format!("{case}: {error}")),
    };
    AdjudicatedProbe {
        case,
        outcome,
        adjudication_ms: elapsed_ms(started),
    }
}

fn elapsed_ms(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum HelmFlipVerdict {
    Collapsed,
    TighteningMatchedHelmAbort,
    TighteningMatchedKubernetesRejection,
    /// Helm renders and Kubernetes proves no new violation, but the
    /// rejection is exactly a declared default's type assertion on a path no
    /// template reads, and the schema generated under
    /// `--declared-types=annotate` accepts the document: the rejection is the
    /// `assert` authoring policy, not a recovered constraint.
    TighteningMatchedDeclaredTypesPolicy,
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

/// Recorded, not yet decisive: how each profile judges every document
/// Helm's schema checks validate for `overlay`, lint's two included, or why
/// the port composed none.
fn acceptance_document_verdicts(
    chart: &PinnedHelmChart,
    overlay: &serde_json::Value,
    profiles: &ProfileSchemas,
) -> serde_json::Value {
    let mut documents = serde_json::Map::new();
    for (kind, document) in chart.acceptance_documents(overlay) {
        let judged = match document {
            Ok(document) => {
                let (baseline, candidate) = profiles.verdicts(&ProbeInstance::Coalesced(document));
                json!({"baseline_accepts": baseline, "candidate_accepts": candidate})
            }
            Err(error) => json!(error.to_string()),
        };
        documents.insert(format!("{kind:?}"), judged);
    }
    serde_json::Value::Object(documents)
}

fn adjudicate_round74_flip(
    chart: &PinnedHelmChart,
    overlay: &serde_json::Value,
    profiles: &ProfileSchemas,
    kubernetes: &OfflineKubernetesValidator,
) -> eyre::Result<HelmFlipVerdict> {
    Ok(adjudicate_flip(chart, overlay, profiles, kubernetes, None)?.0)
}

/// A chart regenerated by its candidate's recipe under both declared-types
/// policies, prepared by the first flip that needs it. The two generations
/// differ only in whether declared defaults assert their types, so an
/// assertion the `assert` one makes and the `annotate` one does not is a
/// declared default's.
struct DeclaredTypesAnnotation {
    chart_dir: PathBuf,
    recipe: registry::ChartRecipe,
    prepared: std::sync::OnceLock<Result<RegeneratedChart, String>>,
}

/// The chart's schemas under both policies, and the annotate session that
/// explains its paths.
struct RegeneratedChart {
    asserted: PolicySchema,
    annotated: PolicySchema,
    session: helm_schema::AnalysisSession,
}

/// One policy's generated schema and its compiled validator.
struct PolicySchema {
    schema: serde_json::Value,
    validator: jsonschema::Validator,
}

impl PolicySchema {
    fn generate(
        session: &helm_schema::AnalysisSession,
        recipe: &registry::ChartRecipe,
    ) -> Result<Self, String> {
        let policy = recipe.authoring.declared_types;
        let schema = generate::session_schema(session, recipe)
            .map_err(|error| format!("generate the {policy:?} schema: {error:#}"))?;
        let validator = jsonschema::validator_for(&schema)
            .map_err(|error| format!("compile the {policy:?} schema: {error}"))?;
        Ok(Self { schema, validator })
    }
}

impl DeclaredTypesAnnotation {
    /// The chart at `chart_dir`, generated by `recipe` under each
    /// declared-types policy.
    fn new(chart_dir: PathBuf, recipe: registry::ChartRecipe) -> Self {
        Self {
            chart_dir,
            recipe,
            prepared: std::sync::OnceLock::new(),
        }
    }

    fn prepared(&self) -> Result<&RegeneratedChart, String> {
        self.prepared
            .get_or_init(|| {
                let mut asserting = self.recipe;
                asserting.authoring.declared_types = helm_schema::generation::DeclaredTypes::Assert;
                let mut annotating = self.recipe;
                annotating.authoring.declared_types =
                    helm_schema::generation::DeclaredTypes::Annotate;
                let session = |recipe: &registry::ChartRecipe| {
                    helm_schema::AnalysisSession::new(generate::generate_options_at(
                        &self.chart_dir,
                        recipe,
                    ))
                };
                let asserted = PolicySchema::generate(&session(&asserting), &asserting)?;
                let annotate_session = session(&annotating);
                let annotated = PolicySchema::generate(&annotate_session, &annotating)?;
                Ok(RegeneratedChart {
                    asserted,
                    annotated,
                    session: annotate_session,
                })
            })
            .as_ref()
            .map_err(Clone::clone)
    }
}

/// [`declared_types_attribution`] of a Helm-rendered rejection, when the
/// chart has an annotate regeneration and Helm coalesced the document.
fn attribute_to_declared_types(
    annotation: Option<&DeclaredTypesAnnotation>,
    values: Option<&helm_adjudication::CoalescedValues>,
    profiles: &ProfileSchemas,
) -> Result<(), String> {
    match (annotation, values) {
        (None, _) => Err("the chart has no registry recipe to regenerate".to_string()),
        (Some(_), None) => Err("Helm produced no coalesced document".to_string()),
        (Some(annotation), Some(values)) => {
            declared_types_attribution(annotation, profiles, values.as_json())
        }
    }
}

/// Whether the candidate's rejection of the Helm-coalesced `document` is
/// exactly the `assert` authoring policy's declared-default type assertion
/// on paths no template reads: the annotate regeneration violates no
/// assertion on `document` the baseline does not (a chart whose own
/// defaults the baseline rejects, like nacos' `service.ports`, admits no
/// document), and every assertion the candidate violates that the baseline
/// does not is a `type` assertion at an object member that the assert
/// regeneration violates identically and the annotate regeneration does
/// not violate at all, on a path explain C1 reports unread (no use of it,
/// its descendants or an ancestor besides the seeded top-level claim, and
/// no generation decision). The two regenerations differ only in the
/// declared-types policy, so that difference is the declaration; the
/// Helm-coalesced defaults are not, since pruning drops declarations and
/// `import-values` adds values nothing declares. The error names the first
/// condition that fails.
fn declared_types_attribution(
    annotation: &DeclaredTypesAnnotation,
    profiles: &ProfileSchemas,
    document: &serde_json::Value,
) -> Result<(), String> {
    let regenerated = annotation.prepared()?;
    let annotated = &regenerated.annotated;
    if !profiles
        .assertions_unlike_the_baseline(&annotated.validator, &annotated.schema, document)
        .is_empty()
    {
        return Err(
            "the annotate schema rejects it for a reason the baseline does not".to_string(),
        );
    }
    let asserted_violations = violated_assertions(
        &regenerated.asserted.validator,
        &regenerated.asserted.schema,
        document,
    );
    let annotated_violations =
        violated_assertions(&annotated.validator, &annotated.schema, document);
    let violations = profiles.new_candidate_assertions(document);
    if violations.is_empty() {
        return Err("the candidate rejects it for no new reason".to_string());
    }
    for violation in violations {
        let path = violation.instance_path.as_str().to_string();
        if violation.keyword != "type" {
            return Err(format!("{path}: violates `{}`", violation.keyword));
        }
        let mut keys = Vec::new();
        let mut value = document;
        for segment in violation.instance_path.segments() {
            let key = match segment {
                jsonschema::paths::LocationSegment::Property(key) => key.into_owned(),
                jsonschema::paths::LocationSegment::Index(index) => index.to_string(),
            };
            value = match value {
                serde_json::Value::Object(members) => members
                    .get(&key)
                    .ok_or_else(|| format!("{path}: not in the document"))?,
                _ => return Err(format!("{path}: steps through a non-object")),
            };
            keys.push(key);
        }
        if !asserted_violations
            .iter()
            .any(|asserted| asserted.is_the_same_as(&violation))
        {
            return Err(format!(
                "{path}: the assert regeneration does not violate it"
            ));
        }
        if annotated_violations
            .iter()
            .any(|annotated| annotated.instance_path.as_str() == violation.instance_path.as_str())
        {
            return Err(format!("{path}: the annotate regeneration rejects it too"));
        }
        let mut reached = helm_schema_core::ValuesPath::parse("");
        let mut ancestry = vec![reached.clone()];
        for key in &keys {
            reached.push(key.clone());
            ancestry.push(reached.clone());
        }
        for (depth, values_path) in ancestry.iter().enumerate() {
            let values_path = values_path.encode();
            let explanation = regenerated
                .session
                .explain(&values_path)
                .map_err(|error| format!("explain `{values_path}`: {error}"))?;
            // Two claims are no template read: values seeding's claim on
            // every declared top-level key (a pathless scalar) and the
            // dependency-root claim `ContractIr::finalize` adds for every
            // dependency values root (a pathless fragment). Neither has a
            // YAML path or template provenance.
            let dependency_root = explanation
                .value_path_facts
                .as_ref()
                .is_some_and(|facts| facts.accepted_dependency_values_root_fragment);
            let template_reads = explanation.exact_uses.iter().any(|use_| {
                let claim = use_.path.0.is_empty()
                    && use_.provenance.is_empty()
                    && (use_.kind == helm_schema_core::ValueKind::Scalar
                        || (dependency_root && use_.kind == helm_schema_core::ValueKind::Fragment));
                !claim
            });
            if template_reads {
                return Err(format!("{path}: a template reads `{values_path}`"));
            }
            if depth < keys.len() {
                continue;
            }
            if !explanation.descendant_uses.is_empty() {
                return Err(format!("{path}: a template reads beneath `{values_path}`"));
            }
            if explanation.generation.is_some() {
                return Err(format!("{path}: generation decided `{values_path}`"));
            }
        }
    }
    Ok(())
}

/// Adjudicates `overlay` with Helm and Kubernetes, returning the verdict and
/// the probe's evidence directory.
#[expect(
    clippy::too_many_lines,
    reason = "the verdict chain reads as one decision over Helm, Kubernetes and the authoring policy"
)]
fn adjudicate_flip(
    chart: &PinnedHelmChart,
    overlay: &serde_json::Value,
    profiles: &ProfileSchemas,
    kubernetes: &OfflineKubernetesValidator,
    annotation: Option<&DeclaredTypesAnnotation>,
) -> eyre::Result<(HelmFlipVerdict, std::path::PathBuf)> {
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
        "helm_exit": probe.rendered.exit_code,
        "acceptance_documents": acceptance_document_verdicts(chart, overlay, profiles),
    });
    let verdict = if before == after && !new_rejection {
        Ok(HelmFlipVerdict::Collapsed)
    } else if !probe.rendered.success() {
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
        let comparison = kubernetes.compare_with_defaults(chart, &probe)?;
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
        } else if !after {
            let attribution =
                attribute_to_declared_types(annotation, probe.values.as_ref(), profiles);
            object.insert(
                "declared_types_attribution".to_string(),
                json!(attribution.as_ref().err()),
            );
            match (attribution, comparison.uncertain.is_empty()) {
                (Ok(()), _) => Ok(HelmFlipVerdict::TighteningMatchedDeclaredTypesPolicy),
                (Err(_), true) => Err(
                    "tightening rejects a document whose render adds no Kubernetes violation to the defaults render",
                ),
                (Err(_), false) => Err(
                    "tightening rejects a document Helm renders without a proved Kubernetes violation",
                ),
            }
        } else if !comparison.uncertain.is_empty() {
            Ok(HelmFlipVerdict::LooseningWithUncertainKubernetes(
                comparison.uncertain.clone(),
            ))
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
    let verdict = verdict.map_err(|reason| {
        let evidence = helm_invocation::preserve_failure(&probe.evidence_dir);
        eyre::eyre!("{reason}; evidence={}", evidence.display())
    })?;
    Ok((verdict, probe.evidence_dir))
}

/// A loosening is matched only when every new or changed resource is decided
/// and its violations add nothing, counted with multiplicity, to the defaults
/// render's.
#[test]
fn loosenings_need_complete_evidence_beyond_the_defaults_render() -> eyre::Result<()> {
    let source = ScratchDir::new("schema_emission_profiles")?;
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
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let kubernetes =
        OfflineKubernetesValidator::new(&cache, helm_adjudication::KUBERNETES_RELEASE)?;
    // The baseline accepts the defaults `{}`, so its rejections are evidence.
    let accept_all = ProfileSchemas::compile(&json!({"maxProperties": 0}), &json!({}), json!({}))?;
    let mut coverage = HelmAdjudicationCoverage::default();

    // A changed resource carrying only the defaults' violation is matched.
    let inherited =
        adjudicate_round74_flip(&chart, &json!({"tag": "b"}), &accept_all, &kubernetes)?;
    sim_assert_eq!(have: inherited, want: HelmFlipVerdict::LooseningMatchedDefaultsViolations);
    coverage.record_verdict(inherited, "inherited".to_string(), None);
    validate_helm_adjudication_coverage(&coverage, &[], &[])?;

    // An added resource without a schema leaves the acceptance unproved, even
    // though the render's proven violations are all the defaults' own.
    let render = chart.adjudicate(&json!({"extra": true}))?;
    assert!(matches!(
        kubernetes.validate(
            helm_invocation::HelmRunner::shared()?,
            &render.rendered.stdout
        )?,
        KubernetesVerdict::Invalid(_)
    ));
    let added = adjudicate_round74_flip(&chart, &json!({"extra": true}), &accept_all, &kubernetes)?;
    sim_assert_eq!(
        have: added,
        want: HelmFlipVerdict::LooseningWithUncertainKubernetes(vec![
            "document 2: example.test/v1/Unknown added: pinned resource schema not found"
                .to_string(),
        ]),
    );
    let mut added_coverage = HelmAdjudicationCoverage::default();
    added_coverage.record_verdict(added, "added".to_string(), None);
    assert!(validate_helm_adjudication_coverage(&added_coverage, &[], &[]).is_err());

    // Two changed copies of one identity inherit the defaults' single violation once.
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"copies": 2}), &accept_all, &kubernetes)?,
        want: HelmFlipVerdict::CandidateAcceptsKubernetesRejects,
    );
    Ok(())
}

/// A false acceptance missing from the roster fails the final gate with a
/// preserved bundle, and that bundle alone reproduces the case: with the
/// original scratch deleted, rendering its chart copy with its values yields
/// the recorded render.
#[test]
fn an_unlisted_false_acceptance_reports_a_bundle_that_reproduces_it() -> eyre::Result<()> {
    let source = ScratchDir::new("schema_emission_profiles")?;
    std::fs::create_dir(source.path().join("templates"))?;
    std::fs::write(
        source.path().join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: preserved-evidence
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
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let kubernetes =
        OfflineKubernetesValidator::new(&cache, helm_adjudication::KUBERNETES_RELEASE)?;
    let string_name = json!({"properties": {"name": {"type": "string"}}});
    let loosening = ProfileSchemas::compile(&string_name, &json!({}), json!({}))?;
    let (verdict, evidence) = adjudicate_flip(
        &chart,
        &json!({"name": true}),
        &loosening,
        &kubernetes,
        None,
    )?;
    sim_assert_eq!(have: verdict.clone(), want: HelmFlipVerdict::CandidateAcceptsKubernetesRejects);

    let mut coverage = HelmAdjudicationCoverage::default();
    coverage.record_verdict(
        verdict,
        "synthetic: name <- true".to_string(),
        Some(evidence.clone()),
    );
    let Err(error) = validate_helm_adjudication_coverage(&coverage, &[], &[]) else {
        eyre::bail!("an unlisted false acceptance passed the gate");
    };
    let scratch_root = test_util::scratch::root().canonicalize()?;
    let bundle = test_util::scratch::evidence_root()
        .join(evidence.canonicalize()?.strip_prefix(&scratch_root)?);
    eyre::ensure!(
        error
            .to_string()
            .contains(&format!("evidence={}", bundle.display())),
        "the gate does not report the preserved bundle {}: {error}",
        bundle.display()
    );

    // Delete the original scratch: the chart's evidence and its prepared trees.
    let original: serde_json::Value = serde_json::from_slice(&std::fs::read(
        evidence
            .parent()
            .ok_or_eyre("probe case has no parent")?
            .join("prepared.json"),
    )?)?;
    for name in ["render", "coalesce"] {
        let tree = original[name]
            .as_str()
            .ok_or_eyre("prepared.json names no tree")?;
        std::fs::remove_dir_all(tree)?;
    }
    std::fs::remove_dir_all(evidence.parent().ok_or_eyre("probe case has no parent")?)?;

    let prepared: serde_json::Value =
        serde_json::from_slice(&std::fs::read(bundle.join("prepared.json"))?)?;
    let render = prepared["render"]
        .as_str()
        .ok_or_eyre("bundle names no render chart")?;
    let kubernetes_version = prepared["kubernetes_version"]
        .as_str()
        .ok_or_eyre("bundle names no Kubernetes version")?;
    let runner = helm_invocation::HelmRunner::shared()?;
    let staged = runner.staging_dir()?;
    test_util::scratch::copy_tree(&bundle.join(render), &staged)?;
    let tree = runner.publish_tree(&staged)?;
    let rerun = ScratchDir::new("schema_emission_profiles")?;
    let values = std::fs::read(bundle.join("values.json"))?;
    runner.template(
        &helm_invocation::TemplateRequest {
            chart: &tree,
            values: &values,
            kubernetes_version,
        },
        rerun.path(),
        "render",
        &helm_invocation::Cacheability::bypass("reproduce a preserved bundle".to_string()),
    )?;
    sim_assert_eq!(
        have: std::fs::read(rerun.path().join("render.yaml"))?,
        want: std::fs::read(bundle.join("render.yaml"))?
    );
    Ok(())
}

#[test]
fn exact_flip_adjudication_uses_coalesced_values_and_kubernetes_evidence() -> eyre::Result<()> {
    let source = ScratchDir::new("schema_emission_profiles")?;
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
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let kubernetes =
        OfflineKubernetesValidator::new(&cache, helm_adjudication::KUBERNETES_RELEASE)?;
    let string_name = json!({"properties": {"name": {"type": "string"}}});
    let tightening = ProfileSchemas::compile(&json!({}), &string_name, json!({}))?;

    // Helm parses the manifest, but the rendered boolean violates Kubernetes metadata typing.
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": true}), &tightening, &kubernetes)?,
        want: HelmFlipVerdict::TighteningMatchedKubernetesRejection,
    );
    let loosening = ProfileSchemas::compile(&string_name, &json!({}), json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": true}), &loosening, &kubernetes)?,
        want: HelmFlipVerdict::CandidateAcceptsKubernetesRejects,
    );

    // Actual chart defaults fill this key even when the screening document did not include it.
    let screened =
        ProfileSchemas::compile(&json!({"required": ["filled"]}), &json!({}), json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({}), &screened, &kubernetes)?,
        want: HelmFlipVerdict::Collapsed,
    );

    // Neither successful rendering nor a missing provider schema proves a tightening valid.
    let unjustified = ProfileSchemas::compile(&json!({}), &json!(false), json!({}))?;
    assert!(adjudicate_round74_flip(&chart, &json!({}), &unjustified, &kubernetes).is_err());
    let empty_cache = ScratchDir::new("schema_emission_profiles")?;
    let missing =
        OfflineKubernetesValidator::new(empty_cache.path(), helm_adjudication::KUBERNETES_RELEASE)?;
    assert!(
        adjudicate_round74_flip(&chart, &json!({"name": true}), &tightening, &missing).is_err()
    );

    // Rendering alone and complete Kubernetes validation remain distinguishable evidence.
    // The baseline accepts the defaults `{}`, so its rejections are evidence.
    let accept_all = ProfileSchemas::compile(&json!({"maxProperties": 0}), &json!({}), json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({}), &accept_all, &kubernetes)?,
        want: HelmFlipVerdict::LooseningMatchedKubernetesValidation,
    );
    // A render identical to the defaults render needs no schema; a changed one does.
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({}), &accept_all, &missing)?,
        want: HelmFlipVerdict::LooseningMatchedKubernetesValidation,
    );
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": "changed"}), &accept_all, &missing)?,
        want: HelmFlipVerdict::LooseningWithUncertainKubernetes(vec![
            "document 0: v1/ConfigMap changed: pinned resource schema not found".to_string(),
        ]),
    );

    // The actual probe aborts even though this chart's unmodified defaults render.
    let invalid_yaml = json!({"name": "["});
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &invalid_yaml, &unjustified, &kubernetes)?,
        want: HelmFlipVerdict::TighteningMatchedHelmAbort,
    );
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &invalid_yaml, &accept_all, &kubernetes)?,
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
    let source = ScratchDir::new("schema_emission_profiles")?;
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
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let kubernetes =
        OfflineKubernetesValidator::new(&cache, helm_adjudication::KUBERNETES_RELEASE)?;
    let blanket = ProfileSchemas::compile(&json!({"required": ["unset"]}), &json!({}), json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": "["}), &blanket, &kubernetes)?,
        want: HelmFlipVerdict::UninformativeBaselineFalseAcceptance(Rejection::HelmAborts),
    );
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": true}), &blanket, &kubernetes)?,
        want: HelmFlipVerdict::UninformativeBaselineFalseAcceptance(Rejection::KubernetesRejects),
    );
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": "changed"}), &blanket, &kubernetes)?,
        want: HelmFlipVerdict::LooseningMatchedKubernetesValidation,
    );
    // A baseline error beyond its defaults' own is evidence again.
    let informative = ProfileSchemas::compile(
        &json!({"required": ["unset"], "properties": {"name": {"type": "string"}}}),
        &json!({}),
        json!({}),
    )?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"name": true}), &informative, &kubernetes)?,
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
    let source = ScratchDir::new("schema_emission_profiles")?;
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
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let kubernetes =
        OfflineKubernetesValidator::new(&cache, helm_adjudication::KUBERNETES_RELEASE)?;
    // The baseline rejects the defaults for missing `a`, and the probe for missing `b`.
    let requires_both = ProfileSchemas::compile(
        &json!({"required": ["a", "b"]}),
        &json!({}),
        json!({"b": "present"}),
    )?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"a": "set", "b": null}), &requires_both, &kubernetes)?,
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
    let source = ScratchDir::new("schema_emission_profiles")?;
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
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let kubernetes =
        OfflineKubernetesValidator::new(&cache, helm_adjudication::KUBERNETES_RELEASE)?;
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
        adjudicate_round74_flip(&chart, &json!({"a": 1, "b": 1}), &hidden, &kubernetes).is_err(),
        "the candidate's new rejection of a document Helm renders is unmatched"
    );
    let moved = ProfileSchemas::compile(
        &string_a,
        &json!({"allOf": [true, string_a]}),
        json!({"a": "x", "b": "y"}),
    )?;
    assert!(!moved.screens_a_flip(&probe));
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"a": 1, "b": 1}), &moved, &kubernetes)?,
        want: HelmFlipVerdict::Collapsed,
    );
    Ok(())
}

#[test]
fn scalar_overrides_of_subchart_tables_are_adjudicated_by_helm_exit() -> eyre::Result<()> {
    let source = ScratchDir::new("schema_emission_profiles")?;
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
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let kubernetes =
        OfflineKubernetesValidator::new(&cache, helm_adjudication::KUBERNETES_RELEASE)?;

    // A scalar subchart scope aborts coalescence: "type mismatch on child".
    let child_scope = json!({"properties": {"child": {"type": "object"}}});
    let scope_tightening = ProfileSchemas::compile(&json!({}), &child_scope, json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"child": false}), &scope_tightening, &kubernetes)?,
        want: HelmFlipVerdict::TighteningMatchedHelmAbort,
    );
    let scope_loosening = ProfileSchemas::compile(&child_scope, &json!({}), json!({}))?;
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"child": false}), &scope_loosening, &kubernetes)?,
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
            &kubernetes,
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
            &kubernetes,
        )?,
        want: HelmFlipVerdict::TighteningMatchedHelmAbort,
    );
    Ok(())
}

#[test]
fn kubernetes_verdicts_are_relative_to_the_defaults_render() -> eyre::Result<()> {
    let source = ScratchDir::new("schema_emission_profiles")?;
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
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, source.path())?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let kubernetes =
        OfflineKubernetesValidator::new(&cache, helm_adjudication::KUBERNETES_RELEASE)?;
    let base_label = json!({"properties": {"label": {"const": "base"}}});
    let loosening = ProfileSchemas::compile(&base_label, &json!({}), json!({}))?;

    // The probe renders only the defaults' own violation, wherever it lands.
    for overlay in [json!({"label": "x"}), json!({"label": "x", "extra": true})] {
        sim_assert_eq!(
            have: adjudicate_round74_flip(&chart, &overlay, &loosening, &kubernetes)?,
            want: HelmFlipVerdict::LooseningMatchedDefaultsViolations,
        );
    }
    // A probe adding its own violation at /data/size stays unmatched.
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"label": "x", "size": 3}), &loosening, &kubernetes)?,
        want: HelmFlipVerdict::CandidateAcceptsKubernetesRejects,
    );
    // A tightening needs a violation the defaults do not already render.
    let tightening = ProfileSchemas::compile(&json!({}), &base_label, json!({}))?;
    assert!(
        adjudicate_round74_flip(&chart, &json!({"label": "x"}), &tightening, &kubernetes).is_err()
    );
    sim_assert_eq!(
        have: adjudicate_round74_flip(&chart, &json!({"label": "x", "size": 3}), &tightening, &kubernetes)?,
        want: HelmFlipVerdict::TighteningMatchedKubernetesRejection,
    );
    Ok(())
}

#[test]
fn existing_configmap_name_tightenings_match_kubernetes_rejections() -> eyre::Result<()> {
    let chart_path = test_util::workspace_testdata().join("charts/oauth2-proxy");
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, &chart_path)?;
    let candidate = read_chart_schema_fixture("oauth2-proxy")?;
    let profiles = ProfileSchemas::compile(&json!({}), &candidate, json!({}))?;
    let cache =
        test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache");
    let kubernetes =
        OfflineKubernetesValidator::new(&cache, helm_adjudication::KUBERNETES_RELEASE)?;

    // Each source value becomes a non-string ConfigMap volume name after Helm's YAML conversion.
    for name in [json!(true), json!(1.5), json!("3")] {
        sim_assert_eq!(
            have: adjudicate_round74_flip(
                &chart,
                &json!({"config": {"existingConfig": name}}),
                &profiles,
                &kubernetes,
            )?,
            want: HelmFlipVerdict::TighteningMatchedKubernetesRejection,
        );
    }
    sim_assert_eq!(
        have: adjudicate_round74_flip(
            &chart,
            &json!({"config": {"existingConfig": "valid-config"}}),
            &profiles,
            &kubernetes,
        )?,
        want: HelmFlipVerdict::Collapsed,
    );
    Ok(())
}

#[test]
#[ignore = "maintenance: requires LEGACY_LEAN_SCHEMA_DIR"]
fn middle_lean_transition_has_only_preregistered_tightenings() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let baseline_dir =
        std::path::PathBuf::from(std::env::var(LEGACY_LEAN_SCHEMA_DIR_VAR).wrap_err_with(
            || format!("{LEGACY_LEAN_SCHEMA_DIR_VAR} must contain legacy lean schemas"),
        )?);
    let mut probes_checked = 0;
    let mut tightenings = Vec::new();
    let mut inverse = Vec::new();
    let adjudicate_live = adjudicates_live();
    if adjudicate_live {
        let output = std::process::Command::new("helm")
            .envs(test_util::scratch::temp_env()?)
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
        let mut baseline: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&baseline_path)
                .wrap_err_with(|| format!("read {}", baseline_path.display()))?,
        )
        .wrap_err_with(|| format!("parse {}", baseline_path.display()))?;
        // Earlier lean fixtures may carry patterns the strict regex check rejects.
        helm_schema_core::normalize_schema_pattern_dialects(&mut baseline);
        let current = generate_profile_schemas(chart)?.1;
        let defaults = read_coalesced_defaults(chart)?;
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
    let values = match probe.helm_values_file(HelmChartDir::stage(chart)?.path(), defaults)? {
        ProbeValuesFile::Reachable(values) => values,
        ProbeValuesFile::Unreachable(reason) => {
            eprintln!("UNREACHABLE {chart}: {probe_name}: {reason}");
            return Ok(());
        }
    };
    let chart_path = test_util::workspace_testdata().join("charts").join(chart);
    let tempdir = ScratchDir::new("schema_emission_profiles")
        .wrap_err("create live adjudication directory")?;
    let values_path = tempdir.path().join("values.json");
    std::fs::write(&values_path, serde_json::to_vec(&values)?)
        .wrap_err("write live adjudication values")?;
    let rendered = std::process::Command::new("helm")
        .envs(test_util::scratch::temp_env()?)
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

/// The nacos `ingress.apiVersion` witness of the declared-types policy
/// attribution. Helm's define precedence selects nacos' own
/// `common.capabilities.ingress.apiVersion`, a literal, so no template reads
/// the key; the vendored mysql copy of that define is the one Helm discards.
/// The witness chart deletes exactly that discarded define, which leaves
/// every Helm render unchanged and lets this tree resolve the winning
/// helper, so the key is typed `string` only by its declared `""` default:
/// each probe renders, the candidate rejects it, and the annotate schema
/// accepts it. Without the annotate regeneration the same cells stay false
/// rejections.
#[test]
fn nacos_ingress_api_version_rejection_is_attributed_to_declared_types() -> eyre::Result<()> {
    let scratch = ScratchDir::new("schema_emission_profiles")?;
    let chart_dir = scratch.path().join("nacos");
    test_util::scratch::copy_tree(
        &test_util::workspace_testdata().join("charts/nacos"),
        &chart_dir,
    )?;
    let helpers = chart_dir.join("charts/mysql/charts/common/templates/_capabilities.tpl");
    let discarded = indoc::indoc! {r#"
        {{- define "common.capabilities.ingress.apiVersion" -}}
        {{- if .Values.ingress -}}
        {{- if .Values.ingress.apiVersion -}}
        {{- .Values.ingress.apiVersion -}}
        {{- else if semverCompare "<1.14-0" (include "common.capabilities.kubeVersion" .) -}}
        {{- print "extensions/v1beta1" -}}
        {{- else if semverCompare "<1.19-0" (include "common.capabilities.kubeVersion" .) -}}
        {{- print "networking.k8s.io/v1beta1" -}}
        {{- else -}}
        {{- print "networking.k8s.io/v1" -}}
        {{- end }}
        {{- else if semverCompare "<1.14-0" (include "common.capabilities.kubeVersion" .) -}}
        {{- print "extensions/v1beta1" -}}
        {{- else if semverCompare "<1.19-0" (include "common.capabilities.kubeVersion" .) -}}
        {{- print "networking.k8s.io/v1beta1" -}}
        {{- else -}}
        {{- print "networking.k8s.io/v1" -}}
        {{- end -}}
        {{- end -}}
    "#};
    let source = std::fs::read_to_string(&helpers)?;
    sim_assert_eq!(have: source.matches(discarded).count(), want: 1);
    std::fs::write(&helpers, source.replace(discarded, ""))?;

    let recipe = chart_recipe("helm-schema.cli.chart-corpus.nacos.schema.json")
        .ok_or_eyre("the registry has no nacos corpus recipe")?;
    let candidate = generate::session_schema(
        &helm_schema::AnalysisSession::new(generate::generate_options_at(&chart_dir, &recipe)),
        &recipe,
    )?;
    let defaults = test_util::helm_values::coalesce_chart_values(&chart_dir, json!({}))?;
    let profiles =
        ProfileSchemas::compile(&read_chart_schema_fixture("nacos")?, &candidate, defaults)?;
    let annotation = DeclaredTypesAnnotation::new(chart_dir.clone(), recipe);
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, &chart_dir)?;
    let kubernetes = OfflineKubernetesValidator::with_crd_catalog(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        &test_util::workspace_testdata().join("provider-bundle/crds-catalog-cache"),
        helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let probes = [
        json!(false),
        json!(true),
        json!(7),
        json!(1.5),
        json!([]),
        json!([{}]),
        json!({}),
        json!({"unknown": "member"}),
    ];
    let mut verdicts = Vec::new();
    let mut unattributed = Vec::new();
    for value in probes {
        let overlay = json!({"ingress": {"apiVersion": value}});
        verdicts
            .push(adjudicate_flip(&chart, &overlay, &profiles, &kubernetes, Some(&annotation))?.0);
        unattributed.push(
            adjudicate_flip(&chart, &overlay, &profiles, &kubernetes, None)
                .map(|(verdict, _)| verdict)
                .map_err(|error| {
                    error
                        .to_string()
                        .contains("tightening rejects a document whose render adds no")
                }),
        );
    }
    sim_assert_eq!(
        have: verdicts,
        want: vec![HelmFlipVerdict::TighteningMatchedDeclaredTypesPolicy; 8]
    );
    sim_assert_eq!(have: unattributed, want: vec![Err(true); 8]);
    Ok(())
}

/// Controls of the declared-types attribution: a rejection is the policy's
/// only when the path is unread, every new violation is the default's
/// `type`, and the annotate schema accepts the document. A violation counts
/// as shared with the baseline only when the baseline violates the same
/// assertion: a baseline `boolean` and a candidate `integer` at one value
/// are two assertions, so the changed one is not hidden behind the policy.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn declared_types_attribution_requires_an_unread_type_only_rejection() -> eyre::Result<()> {
    let source = ScratchDir::new("schema_emission_profiles")?;
    std::fs::create_dir(source.path().join("templates"))?;
    std::fs::write(
        source.path().join("Chart.yaml"),
        indoc::indoc! {"
            apiVersion: v2
            name: declared-types
            version: 0.1.0
        "},
    )?;
    std::fs::write(
        source.path().join("values.yaml"),
        indoc::indoc! {r#"
            settings:
              enabled: false
              unread: ""
              size: 3
        "#},
    )?;
    std::fs::write(
        source.path().join("templates/configmap.yaml"),
        indoc::indoc! {"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: declared-types
            data:
              enabled: {{ .Values.settings.enabled | quote }}
            ---
            apiVersion: example.com/v1
            kind: Widget
            metadata:
              name: declared-types
            spec:
              size: {{ .Values.settings.size }}
        "},
    )?;
    let recipe = registry::ChartRecipe::corpus(
        "declared-types",
        helm_schema::generation::SchemaProfile::Full,
    );
    let assert_schema = generate::session_schema(
        &helm_schema::AnalysisSession::new(generate::generate_options_at(source.path(), &recipe)),
        &recipe,
    )?;
    let defaults = test_util::helm_values::coalesce_chart_values(source.path(), json!({}))?;
    let annotation = DeclaredTypesAnnotation::new(source.path().to_path_buf(), recipe);
    let chart = PinnedHelmChart::prepare(helm_invocation::HelmRunner::shared()?, source.path())?;
    let kubernetes = OfflineKubernetesValidator::new(
        &test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache"),
        helm_adjudication::KUBERNETES_RELEASE,
    )?;
    let profiles = ProfileSchemas::compile(&json!({}), &assert_schema, defaults.clone())?;
    let excluding_seven = json!({"allOf": [
        assert_schema,
        {"properties": {"settings": {"properties": {"unread": {"not": {"const": 7}}}}}},
    ]});
    let not_only_type = ProfileSchemas::compile(&json!({}), &excluding_seven, defaults.clone())?;
    let boolean_size =
        json!({"properties": {"settings": {"properties": {"size": {"type": "boolean"}}}}});
    let changed_assertion =
        ProfileSchemas::compile(&boolean_size, &assert_schema, defaults.clone())?;
    // The baseline's closed root admits only `undeclared`: it rejects
    // `settings`, while the candidate and annotate schemas reject
    // `undeclared`. One keyword and argument, two failures.
    let other_closure = json!({"properties": {"undeclared": {}}, "additionalProperties": false});
    let changed_closure = ProfileSchemas::compile(&other_closure, &assert_schema, defaults)?;
    let rendered = [
        json!({"settings": {"unread": 7}}),
        json!({"settings": {"size": "x"}}),
    ]
    .iter()
    .map(|overlay| {
        adjudicate_flip(&chart, overlay, &profiles, &kubernetes, Some(&annotation))
            .map(|(verdict, _)| verdict)
            .map_err(|error| {
                error
                    .to_string()
                    .starts_with("tightening rejects a document")
            })
    })
    .collect::<Vec<_>>();
    sim_assert_eq!(
        have: rendered,
        want: vec![Ok(HelmFlipVerdict::TighteningMatchedDeclaredTypesPolicy), Err(true)]
    );
    // Helm renders it; the baseline rejects `size` as non-boolean, the
    // candidate as non-integer beside the unread `unread` default type.
    let mixed = json!({"settings": {"enabled": false, "size": "x", "unread": 7}});
    sim_assert_eq!(
        have: adjudicate_flip(&chart, &mixed, &changed_assertion, &kubernetes, Some(&annotation))
            .map(|(verdict, _)| verdict)
            .map_err(|error| error.to_string().starts_with("tightening rejects a document")),
        want: Err(true)
    );
    // Helm renders it: no template reads `undeclared`.
    let closed = json!({"settings": {"enabled": false, "size": 3, "unread": 7}, "undeclared": 1});
    sim_assert_eq!(
        have: adjudicate_flip(&chart, &closed, &changed_closure, &kubernetes, Some(&annotation))
            .map(|(verdict, _)| verdict)
            .map_err(|error| error.to_string().starts_with("tightening rejects a document")),
        want: Err(true)
    );
    let settings = |unread: serde_json::Value, size: serde_json::Value| json!({"settings": {"enabled": false, "unread": unread, "size": size}});
    let mut undeclared = settings(json!(7), json!(3));
    undeclared["undeclared"] = json!(1);
    let cells = [
        (settings(json!(7), json!(3)), &profiles),
        (settings(json!(""), json!("x")), &profiles),
        (undeclared, &profiles),
        (settings(json!(7), json!(3)), &not_only_type),
        (mixed, &changed_assertion),
        (closed, &changed_closure),
    ];
    let attributions = cells
        .iter()
        .map(|(document, profiles)| declared_types_attribution(&annotation, profiles, document))
        .collect::<Vec<_>>();
    sim_assert_eq!(
        have: attributions,
        want: vec![
            Ok(()),
            Err("/settings/size: a template reads `settings.size`".to_string()),
            Err("the annotate schema rejects it for a reason the baseline does not".to_string()),
            Err("/settings/unread: violates `not`".to_string()),
            Err("/settings/size: a template reads `settings.size`".to_string()),
            Err("the annotate schema rejects it for a reason the baseline does not".to_string()),
        ]
    );
    Ok(())
}

/// A violated assertion is one failure: the keyword with its argument, what
/// the validator reports (for `additionalProperties`, the keys it rejects,
/// beside the properties it admits), and literal data kept as data. Equal
/// `additionalProperties: false` arguments that reject different keys, and
/// `const` values that differ only in which `$ref`-shaped data they hold,
/// are different failures.
#[test]
fn violated_assertions_keep_the_failure_detail_and_literal_data() -> eyre::Result<()> {
    let only_a = json!({"properties": {"a": {}}, "additionalProperties": false});
    let only_b = json!({"properties": {"b": {}}, "additionalProperties": false});
    let closures = ProfileSchemas::compile(&only_a, &only_b, json!({}))?;
    let constant = |name: &str| {
        json!({
            "$defs": {"a": {"type": "string"}, "b": {"type": "string"}},
            "properties": {"x": {"const": {"$ref": format!("#/$defs/{name}")}}},
        })
    };
    let constants = ProfileSchemas::compile(&constant("a"), &constant("b"), json!({}))?;
    let identical = ProfileSchemas::compile(&only_a, &only_a, json!({}))?;
    let new_failures = |profiles: &ProfileSchemas, document: serde_json::Value| {
        profiles
            .new_candidate_assertions(&document)
            .into_iter()
            .map(|violation| format!("{} {}", violation.instance_path.as_str(), violation.keyword))
            .collect::<Vec<_>>()
    };
    sim_assert_eq!(
        have: (
            new_failures(&closures, json!({"a": 1, "b": 1})),
            new_failures(&constants, json!({"x": 1})),
            new_failures(&identical, json!({"a": 1, "b": 1})),
        ),
        want: (
            vec![" additionalProperties".to_string()],
            vec!["/x const".to_string()],
            Vec::new(),
        )
    );
    Ok(())
}

/// Whether a violated `type` assertion is a declared default's is read from
/// the generation, not from the Helm-coalesced defaults. The attribution
/// regenerates the chart under both policies: the assert regeneration must
/// violate the same assertion and the annotate regeneration nothing at that
/// value. A value `import-values` supplies (`settings.unread: 7`, the parent
/// declares `settings: {enabled: false}`) is no declaration, so a candidate
/// typing it `integer` is not the policy's; a declaration below a pruned
/// dependency (`kid.unread: ""`, absent from the coalesced defaults) is.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn declared_types_attribution_reads_declarations_from_the_generation() -> eyre::Result<()> {
    let write = |path: std::path::PathBuf, contents: &str| -> eyre::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, contents)?;
        Ok(())
    };
    let config_map = |name: &str, data: &str| {
        indoc::formatdoc! {"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: {name}
            data:
              {data}
        "}
    };
    let scratch = ScratchDir::new("schema_emission_profiles")?;
    let imported = scratch.path().join("imported");
    write(
        imported.join("Chart.yaml"),
        indoc::indoc! {"
            apiVersion: v2
            name: parent
            version: 0.1.0
            dependencies:
              - name: child
                version: 0.1.0
                import-values:
                  - child: exported
                    parent: settings
        "},
    )?;
    write(
        imported.join("values.yaml"),
        indoc::indoc! {"
            settings:
              enabled: false
        "},
    )?;
    write(
        imported.join("templates/cm.yaml"),
        &config_map("parent", "enabled: {{ .Values.settings.enabled | quote }}"),
    )?;
    write(
        imported.join("charts/child/Chart.yaml"),
        indoc::indoc! {"
            apiVersion: v2
            name: child
            version: 0.1.0
        "},
    )?;
    write(
        imported.join("charts/child/values.yaml"),
        indoc::indoc! {"
            exported:
              unread: 7
        "},
    )?;
    write(
        imported.join("charts/child/templates/cm.yaml"),
        &config_map("child", "k: v"),
    )?;
    let pruned = scratch.path().join("pruned");
    write(
        pruned.join("Chart.yaml"),
        indoc::indoc! {"
            apiVersion: v2
            name: parent
            version: 0.1.0
            dependencies:
              - name: child
                version: 0.1.0
                alias: kid
                condition: kidEnabled
        "},
    )?;
    write(pruned.join("values.yaml"), "kidEnabled: false\n")?;
    write(
        pruned.join("templates/cm.yaml"),
        &config_map("parent", "on: {{ .Values.kidEnabled | quote }}"),
    )?;
    write(
        pruned.join("charts/child/Chart.yaml"),
        indoc::indoc! {"
            apiVersion: v2
            name: child
            version: 0.1.0
        "},
    )?;
    write(
        pruned.join("charts/child/values.yaml"),
        indoc::indoc! {r#"
            enabled: false
            unread: ""
        "#},
    )?;
    write(
        pruned.join("charts/child/templates/cm.yaml"),
        &config_map("child", "enabled: {{ .Values.enabled | quote }}"),
    )?;
    let recipe = |chart: &'static str| {
        registry::ChartRecipe::corpus(chart, helm_schema::generation::SchemaProfile::Full)
    };

    // Helm coalesces `import-values` into the parent's settings and prunes
    // the disabled `kid` together with its declared defaults.
    let imported_defaults = test_util::helm_values::coalesce_chart_values(&imported, json!({}))?;
    let pruned_defaults = test_util::helm_values::coalesce_chart_values(&pruned, json!({}))?;
    sim_assert_eq!(
        have: (&imported_defaults["settings"], pruned_defaults.get("kid")),
        want: (&json!({"enabled": false, "unread": 7}), None)
    );

    // A candidate typing the imported value as its coalesced type.
    let integer_unread =
        json!({"properties": {"settings": {"properties": {"unread": {"type": "integer"}}}}});
    let imported_profiles =
        ProfileSchemas::compile(&json!({}), &integer_unread, imported_defaults)?;
    let imported_annotation = DeclaredTypesAnnotation::new(imported.clone(), recipe("imported"));
    // Helm renders both documents (rework4/helm-r14.log).
    let imported_document = json!({
        "child": {"exported": {"unread": 7}, "global": {}},
        "settings": {"enabled": false, "unread": "x"},
    });

    let pruned_candidate = generate::session_schema(
        &helm_schema::AnalysisSession::new(generate::generate_options_at(
            &pruned,
            &recipe("pruned"),
        )),
        &recipe("pruned"),
    )?;
    let pruned_profiles = ProfileSchemas::compile(&json!({}), &pruned_candidate, pruned_defaults)?;
    let pruned_annotation = DeclaredTypesAnnotation::new(pruned.clone(), recipe("pruned"));
    let pruned_document = json!({"kid": {"unread": 7}, "kidEnabled": false});

    sim_assert_eq!(
        have: vec![
            declared_types_attribution(&imported_annotation, &imported_profiles, &imported_document),
            declared_types_attribution(&pruned_annotation, &pruned_profiles, &pruned_document),
        ],
        want: vec![
            Err("/settings/unread: the assert regeneration does not violate it".to_string()),
            Ok(()),
        ]
    );
    Ok(())
}
