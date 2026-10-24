//! The policy-free schema acceptance comparison shared by the ordinary
//! battery and the value-algebra differential gate.
//!
//! [`compare_charts`] screens each chart's probe battery and dependency/context
//! documents against a baseline and a candidate schema, and, when a
//! Kubernetes validator is configured, adjudicates every screened flip with
//! the pinned Helm. It reports raw outcomes and completeness evidence; rosters,
//! baselines and landing policy belong to its callers. Every decision is an
//! explicit [`ComparisonConfig`] input, never the environment.

pub mod adjudicate;
/// Probe batteries, schema profiles and probe coverage.
pub mod discovery;
pub mod pool;
pub mod screen;

use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, OptionExt as _};
use serde::Serialize;
use serde_json::Value;

pub use adjudicate::{
    Baseline, DeclaredTypesAnnotation, FailureInput, FailureKind, HelmFlipVerdict, ObservedFailure,
    OracleFinding, Rejection,
};
use adjudicate::{ControlObservations, adjudicate_probe_group};
use discovery::ProbeCoverage;
use discovery::ProfileSchemas;
use pool::{Ordinal, PoolLimits, PoolPeaks, run_ordered};
use screen::{ImportedRoster, LintSchemas, ScreenedProbe, probe_groups, screen_chart};

use crate::helm::adjudication::{OfflineKubernetesValidator, PinnedHelmChart};

/// Everything a comparison decides besides its charts.
pub struct ComparisonConfig {
    /// Adjudicates screened flips live when present; without it the schemas
    /// alone judge.
    pub kubernetes: Option<OfflineKubernetesValidator>,
    /// The imported context roster, read for exactly the selected charts.
    pub contexts: Option<ImportedRoster>,
    /// Worker and memory bounds.
    pub pool: PoolLimits,
}

/// An adjudication that reached no verdict.
#[derive(Debug)]
pub struct AdjudicationFailure {
    /// What it says about the document.
    pub kind: FailureKind,
    /// `{case}: {reason}`, as the ordinary battery reports it.
    pub message: String,
}

impl std::fmt::Display for AdjudicationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl AdjudicationFailure {
    /// A failure with no oracle answer.
    #[must_use]
    pub fn infrastructure(message: String) -> Self {
        Self {
            kind: FailureKind::Infrastructure,
            message,
        }
    }
}

#[derive(Debug, Default, Serialize)]
/// Accounting of every screened flip's live adjudication.
pub struct HelmAdjudicationCoverage {
    /// Whether Helm adjudicated the screened flips.
    pub enabled: bool,
    /// Adjudications that reached no verdict.
    pub adjudication_failures: Vec<ObservedFailure>,
    /// Rust screening can miss flips that exact Helm coalescence would expose.
    pub screening_is_exact: bool,
    /// Flips handed to Helm.
    pub screened_flips: usize,
    /// Screened flips Helm's own coalesced document shows unchanged.
    pub screened_flips_collapsed: usize,
    /// Flips with a verdict.
    pub flips_adjudicated: usize,
    /// Tightenings matched by a Helm abort.
    pub tightenings_matched_helm_abort: usize,
    /// Tightenings matched by a schema-free lint rejection.
    pub tightenings_matched_lint_rejection: usize,
    /// Loosenings matched by a clean schema-free lint.
    pub loosenings_matched_lint_validation: usize,
    /// Tightenings matched by a new Kubernetes violation.
    pub tightenings_matched_kubernetes_rejection: usize,
    /// Loosenings matched by a Kubernetes-valid render.
    pub loosenings_matched_kubernetes_validation: usize,
    /// Loosenings whose every Kubernetes violation the defaults render carries.
    pub loosenings_matched_defaults_violations: usize,
    /// `chart: probe` of every rejected cell Helm renders whose rejection is
    /// exactly a declared default's type assertion on a path no template
    /// reads, which the `annotate` authoring policy accepts.
    pub tightenings_attributed_to_declared_types: Vec<String>,
    /// Accepted cells Helm renders whose changed resources Kubernetes cannot
    /// decide.
    pub loosenings_with_uncertain_kubernetes: Vec<ObservedUndecidedAcceptance>,
    /// Exact contextual identities actually examined by live adjudication.
    #[serde(skip)]
    pub examined_contexts: std::collections::BTreeSet<String>,
    /// Labels of the charts whose flips were adjudicated live.
    pub charts_adjudicated: std::collections::BTreeSet<String>,
    /// Accepted cells Helm or Kubernetes reject.
    pub false_acceptances: Vec<ObservedFalseAcceptance>,
    /// `chart: probe` of every composed probe no values file reaches. A
    /// roster row naming one cannot be observed and is kept, not "fixed".
    pub unreachable_cases: Vec<String>,
    /// The evidence directory of every adjudicated flip, preserved when a
    /// final gate fails.
    #[serde(skip)]
    pub flip_evidence: Vec<PathBuf>,
}

/// A false acceptance the battery observed, to be matched by the roster.
#[derive(Debug, Serialize)]
pub struct ObservedFalseAcceptance {
    /// `chart: probe` of the cell.
    pub case: String,
    /// How the render fails.
    pub rejection: Rejection,
    /// How the baseline rejected the cell.
    pub baseline: Baseline,
    #[serde(skip)]
    /// The cell's evidence directory.
    pub evidence: Option<PathBuf>,
}

/// An accepted cell Kubernetes cannot decide, to be matched by the roster.
#[derive(Debug, Serialize)]
pub struct ObservedUndecidedAcceptance {
    /// `chart: probe` of the cell.
    pub case: String,
    /// Why Kubernetes cannot decide, as reported.
    pub uncertain: Vec<String>,
    #[serde(skip)]
    /// The cell's evidence directory.
    pub evidence: Option<PathBuf>,
}

impl HelmAdjudicationCoverage {
    /// Counts `verdict` for `case`, keeping the flip's evidence directory
    /// (`None` for a verdict without one) for the final gates.
    pub fn record_verdict(
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
            HelmFlipVerdict::TighteningMatchedLintRejection => {
                self.tightenings_matched_lint_rejection += 1;
                false
            }
            HelmFlipVerdict::LooseningMatchedLintValidation => {
                self.loosenings_matched_lint_validation += 1;
                true
            }
            HelmFlipVerdict::CandidateAcceptsLintRejects => {
                self.false_acceptances.push(ObservedFalseAcceptance {
                    case,
                    rejection: Rejection::LintRejects,
                    baseline: Baseline::RejectsUnlikeItsDefaults,
                    evidence: evidence.clone(),
                });
                true
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

#[derive(Default)]
/// Everything one comparison observed, in chart and probe order.
pub struct AcceptanceComparison {
    /// Charts screened.
    pub charts_checked: usize,
    /// Probes generated over those charts, unreachable ones included.
    pub probes_checked: usize,
    /// Flips judged by the schemas alone, or live-adjudicated non-collapsed flips.
    pub flips: Vec<String>,
    /// Each chart's probe accounting.
    pub coverage: Vec<ProbeCoverage>,
    /// Probes whose values file a chart could not compose.
    pub screening_failures: Vec<String>,
    /// Live adjudication accounting.
    pub helm_adjudication: HelmAdjudicationCoverage,
    /// Each chart's cost.
    pub costs: Vec<ChartCost>,
    /// The pool the comparison ran on.
    pub pool: Option<PoolReport>,
}

impl AcceptanceComparison {
    /// Checks every chart's lint lane for completeness.
    ///
    /// # Errors
    ///
    /// Returns an error when a lane is incomplete.
    pub fn validate_lint_completeness(&self, live: bool) -> eyre::Result<()> {
        Self::validate_lint_coverages(&self.coverage, live)
    }

    /// Checks `charts`' lint lanes for completeness, including complete live adjudication when `live`.
    ///
    /// # Errors
    ///
    /// Returns an error when no chart is given or a lane is incomplete.
    pub fn validate_lint_coverages(charts: &[ProbeCoverage], live: bool) -> eyre::Result<()> {
        eyre::ensure!(
            !charts.is_empty(),
            "no charts checked for lint completeness"
        );
        for chart in charts {
            let lint = chart.lint.as_ref().ok_or_eyre("lint lane was not run")?;
            if live {
                lint.validate_live()?;
            } else {
                lint.validate()?;
            }
        }
        Ok(())
    }
}

/// What one chart's comparison cost, for the Helm invocation report.
#[derive(Debug, Serialize)]
pub struct ChartCost {
    /// The chart's label.
    pub label: String,
    /// Reading both schemas, compiling them and generating the probe battery.
    pub context_ms: u64,
    /// Screening both lanes.
    pub screening_ms: u64,
    /// Screening the lint lane.
    pub lint_screening_ms: u64,
    /// Compiling the lint lane's schemas.
    pub lint_compilation_ms: u64,
    /// Preparing the pinned Helm chart copies.
    pub preparation_ms: u64,
    /// Helm executions and Kubernetes judgement of every screened flip.
    pub adjudication_ms: u64,
    /// Content identities of the prepared render and coalescence copies.
    pub prepared_trees: Option<(String, String)>,
    /// Helm invocation totals per stage.
    pub helm: std::collections::BTreeMap<String, crate::helm::invocation::StageTotals>,
}

/// One chart whose baseline and candidate schemas are compared.
pub struct ComparedChart {
    /// The chart's label in reports.
    pub label: String,
    /// The chart directory, relative to the corpus charts or absolute.
    pub chart_relative_path: String,
    /// Where its schemas come from.
    pub inputs: ChartInputs,
    /// The candidate's annotate regeneration, when the caller can provide
    /// one generated by the candidate's own build; without it, no tightening
    /// is attributed to the declared-types policy.
    pub annotation: Option<DeclaredTypesAnnotation>,
}

/// Where a compared chart's schemas and defaults come from.
pub enum ChartInputs {
    /// A candidate producer artifact whose manifest entry and Helm-ready
    /// companion the lint lane verifies; `load` reads both schemas and the
    /// defaults on the worker that screens the chart.
    Artifact {
        /// Reads the chart's inputs.
        load: InputLoader,
        /// The candidate artifact inside its producer dump.
        candidate_path: PathBuf,
    },
    /// Schemas and defaults already in memory.
    Given {
        /// The baseline schema as validated.
        baseline: Value,
        /// The baseline schema's exact bytes.
        baseline_bytes: Vec<u8>,
        /// The candidate schema.
        candidate: Value,
        /// The candidate schema's exact bytes.
        candidate_bytes: Vec<u8>,
        /// The chart's coalesced defaults.
        defaults: Value,
    },
}

/// Reads one chart's [`LoadedInputs`].
pub type InputLoader = Box<dyn FnOnce() -> eyre::Result<LoadedInputs> + Send>;

/// A chart's schemas and defaults, read.
pub struct LoadedInputs {
    /// The baseline schema as validated.
    pub baseline: Value,
    /// The baseline artifact's exact bytes.
    pub baseline_bytes: Vec<u8>,
    /// The candidate schema.
    pub candidate: Value,
    /// The candidate artifact's exact bytes, not a Helm-ready spelling.
    pub candidate_bytes: Vec<u8>,
    /// The chart's coalesced defaults.
    pub defaults: Value,
}

impl ChartInputs {
    /// The candidate's producer artifact, if it comes from a dump.
    #[must_use]
    pub fn candidate_path(&self) -> Option<&Path> {
        match self {
            Self::Artifact { candidate_path, .. } => Some(candidate_path),
            Self::Given { .. } => None,
        }
    }

    /// The baseline schema, the candidate schema and the chart's defaults.
    ///
    /// # Errors
    ///
    /// Returns the loader's error.
    pub fn load(self) -> eyre::Result<LoadedInputs> {
        match self {
            Self::Artifact { load, .. } => load(),
            Self::Given {
                baseline,
                baseline_bytes,
                candidate,
                candidate_bytes,
                defaults,
            } => Ok(LoadedInputs {
                baseline,
                baseline_bytes,
                candidate,
                candidate_bytes,
                defaults,
            }),
        }
    }
}

/// A chart whose screened flips Helm adjudicates, shared by its probe jobs
/// and dropped with the last of them.
pub struct LiveChart {
    /// The chart's label.
    pub label: String,
    /// The chart directory Helm renders.
    pub path: std::path::PathBuf,
    /// Both schemas as compiled for screening.
    pub profiles: ProfileSchemas,
    /// Each screened probe's name and Helm values file, in probe order.
    pub flips: Vec<ScreenedProbe>,
    /// Both schemas as installed for lint.
    pub lint_schemas: LintSchemas,
    /// The chart's pinned Helm copies.
    pub helm: std::sync::Arc<ChartHelm>,
    /// The chart under `--declared-types=annotate`, when a registry recipe
    /// generates the candidate.
    pub annotation: Option<DeclaredTypesAnnotation>,
}

/// A chart's pinned Helm copies, prepared by the first probe job that needs
/// them, inline.
#[derive(Default)]
pub struct ChartHelm {
    /// The pinned copies, or why they could not be prepared.
    pub chart: std::sync::OnceLock<Result<PinnedHelmChart, String>>,
    /// How long preparing them took.
    pub preparation_ms: std::sync::atomic::AtomicU64,
    /// Replayable defaults and context control observations.
    pub controls: ControlObservations,
}

impl ChartHelm {
    /// The pinned copies of the chart at `path`, prepared on first use.
    ///
    /// # Errors
    ///
    /// Returns why the copies could not be prepared.
    pub fn chart(&self, path: &std::path::Path) -> Result<&PinnedHelmChart, &str> {
        self.chart
            .get_or_init(|| {
                let started = std::time::Instant::now();
                let chart = crate::helm::invocation::HelmRunner::shared()
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
    pub fn reservation(&self, budget: u64) -> u64 {
        match self.chart.get() {
            Some(Ok(chart)) if chart.peak_child_rss() > 0 => {
                (chart.peak_child_rss().saturating_mul(13) / 10).min(budget)
            }
            _ => budget,
        }
    }
}

/// One unit of pool work.
pub enum AcceptanceJob {
    /// Screens one chart.
    Chart(Box<ComparedChart>),
    /// Adjudicates one group of a chart's screened flips.
    Probe(std::sync::Arc<LiveChart>, Vec<usize>),
}

/// The result of one [`AcceptanceJob`].
pub enum AcceptanceResult {
    /// A chart's screening.
    Chart(eyre::Result<Box<ScreenedChart>>),
    /// One adjudicated flip.
    Probe(Box<AdjudicatedProbe>),
}

/// A chart's probe battery and its screening.
pub struct ScreenedChart {
    /// The chart's label.
    pub label: String,
    /// Its probe accounting.
    pub coverage: ProbeCoverage,
    /// Probes generated, unreachable ones included.
    pub probes_checked: usize,
    /// Flips judged by the schemas alone, when Helm does not adjudicate.
    pub flips: Vec<String>,
    /// Probes whose values file the chart could not compose at all.
    pub failures: Vec<String>,
    /// Flips handed to Helm.
    pub screened_flips: usize,
    /// The chart's pinned Helm copies, when any flip needs Helm.
    pub helm: Option<std::sync::Arc<ChartHelm>>,
    /// Its cost.
    pub cost: ChartCost,
}

/// The live adjudication of one screened flip.
pub struct AdjudicatedProbe {
    /// The flip's index among its chart's screened flips.
    pub flip: usize,
    /// `chart: probe` of the flip.
    pub case: String,
    /// The document a document-lane flip judges; `None` for the exact-template lane.
    pub document_kind: Option<test_util::helm_values::AcceptanceDocument>,
    /// The Kubernetes version Helm rendered for.
    pub kubernetes_version: String,
    /// The exact input identity, when it could be computed.
    pub input: Option<FailureInput>,
    /// The verdict and its evidence directory, or why there is none.
    pub outcome: Result<(HelmFlipVerdict, PathBuf), AdjudicationFailure>,
    /// Time spent adjudicating it.
    pub adjudication_ms: u64,
}

/// Screens every chart and adjudicates every screened flip on the pool,
/// then folds the outcomes in chart and probe order.
#[tracing::instrument(target = "helm_schema::acceptance", skip_all, fields(charts = charts.len()))]
///
/// # Errors
///
/// Returns an error when no chart is selected, a chart cannot be screened or the Kubernetes bundles changed.
pub fn compare_charts(
    charts: Vec<ComparedChart>,
    config: &ComparisonConfig,
) -> eyre::Result<AcceptanceComparison> {
    eyre::ensure!(
        !charts.is_empty(),
        "no charts selected for acceptance screening"
    );
    let ComparisonConfig {
        kubernetes,
        contexts: roster,
        pool: limits,
    } = config;
    let limits = *limits;
    let adjudicate_live = kubernetes.is_some();
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
                let (screened, live) = match screen_chart(*chart, adjudicate_live, roster.as_ref())
                {
                    Ok(screened) => screened,
                    Err(error) => {
                        return vec![((chart_index, 0), AcceptanceResult::Chart(Err(error)))];
                    }
                };
                if let Some(live) = live {
                    let live = std::sync::Arc::new(live);
                    for flips in probe_groups(&live) {
                        let Some(first) = flips.first() else { continue };
                        submitter.submit(
                            (chart_index, first + 1),
                            AcceptanceJob::Probe(live.clone(), flips),
                        );
                    }
                }
                vec![(
                    (chart_index, 0),
                    AcceptanceResult::Chart(Ok(Box::new(screened))),
                )]
            }
            AcceptanceJob::Probe(live, flips) => {
                adjudicate_probe_group(&live, &flips, kubernetes.as_ref())
                    .into_iter()
                    .map(|probe| {
                        (
                            (chart_index, probe.flip + 1),
                            AcceptanceResult::Probe(Box::new(probe)),
                        )
                    })
                    .collect()
            }
        },
    );
    let results = results
        .into_iter()
        .flat_map(|(_, results)| results)
        .collect();
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

/// Folds chart and probe results at their original ordinals into one comparison.
///
/// # Errors
///
/// Returns a chart's screening error.
pub fn fold_results(
    mut results: Vec<(Ordinal, AcceptanceResult)>,
    adjudicate_live: bool,
) -> eyre::Result<AcceptanceComparison> {
    // One input group can contain nonadjacent documents; execution grouping never reorders coverage.
    results.sort_by_key(|(ordinal, _)| *ordinal);
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
                comparison.screening_failures.extend(screened.failures);
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
                if probe.document_kind.is_some() {
                    comparison
                        .helm_adjudication
                        .examined_contexts
                        .insert(probe.case.clone());
                }
                if let Some(kind) = probe.document_kind
                    && let Some(lint) = comparison
                        .coverage
                        .last_mut()
                        .and_then(|chart| chart.lint.as_mut())
                    && let Some(counts) = lint.documents.get_mut(&format!("{kind:?}"))
                {
                    if probe.outcome.is_ok() {
                        counts.adjudicated += 1;
                    } else {
                        counts.adjudication_failures += 1;
                    }
                }
                if let Some(cost) = comparison.costs.last_mut() {
                    cost.adjudication_ms += probe.adjudication_ms;
                }
                match probe.outcome {
                    Err(failure) => comparison.helm_adjudication.adjudication_failures.push(
                        ObservedFailure::from_adjudication(
                            probe.case,
                            probe.kubernetes_version,
                            &failure.message,
                            probe.input,
                        ),
                    ),
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
pub fn record_helm_costs(
    helm_charts: &[Option<std::sync::Arc<ChartHelm>>],
    costs: &mut [ChartCost],
) {
    for (helm, cost) in helm_charts.iter().zip(costs) {
        let Some(helm) = helm else { continue };
        cost.preparation_ms = helm
            .preparation_ms
            .load(std::sync::atomic::Ordering::Relaxed);
        if let Some(Ok(chart)) = helm.chart.get() {
            cost.helm = crate::helm::invocation::stage_totals(&chart.invocations());
            cost.prepared_trees = Some((
                chart.render_tree().sha256().to_string(),
                chart.coalesce_tree().sha256().to_string(),
            ));
        }
    }
}

/// The pool a comparison ran on and the most it ran at once.
#[derive(Debug, Serialize)]
pub struct PoolReport {
    /// Worker threads.
    pub workers: usize,
    /// The memory budget in bytes.
    pub memory_bytes: u64,
    /// The most the pool ran at once.
    pub peaks: PoolPeaks,
    /// Wall time of the comparison.
    pub wall_ms: u64,
}

/// Milliseconds since `started`, saturating.
#[must_use]
pub fn elapsed_ms(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
