//! Pinned Helm and Kubernetes evidence for screened flips: the exact-template
//! lane, the document lane with its controls, and the declared-types policy
//! attribution.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use test_util::helm_values::AcceptanceDocument;

use super::discovery::{ProbeInstance, ProfileSchemas, rejects_for_a_new_reason};
use super::screen::{LintSchemas, ScreenedDocument, ScreenedProbe};
use super::{AdjudicatedProbe, AdjudicationFailure, LiveChart, elapsed_ms};
use crate::helm::adjudication::{
    CoalescedValues, DefaultsComparison, HelmLintProbe, HelmProbe, LintProbeRequest,
    OfflineKubernetesValidator, PinnedHelmChart, UnusableControl,
};
use crate::helm::invocation::{HelmExecution, Replay, preserve_failure};
use crate::helm::lint::{LintClassification, LintErrorKind, classify_lint};
use crate::source_digest::sha256_hex;
use crate::{generate, registry};

/// How the render of an accepted document fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Rejection {
    /// Schema-free `helm lint` rejects the input semantically.
    LintRejects,
    /// `helm template` aborts.
    HelmAborts,
    /// Helm renders, and the pinned Kubernetes bundle rejects the render.
    KubernetesRejects,
}

/// How the baseline schema rejected a false acceptance's cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Baseline {
    /// With exactly the violations of the chart's own defaults.
    RejectsItsDefaults,
    /// With violations that differ from those of the chart's own defaults.
    RejectsUnlikeItsDefaults,
}

/// What an adjudication that reached no verdict says about its document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum FailureKind {
    /// The candidate rejects a document Helm renders, and nothing attributes
    /// the rejection: the pinned oracle accepts the document.
    UnattributedRejection,
    /// The pinned oracle cannot decide the document: unusable controls,
    /// inherited lint errors or undecidable Kubernetes resources.
    Undecided,
    /// A tool, process or input failed; the document has no oracle answer.
    Infrastructure,
}

/// An oracle finding carried by an adjudication error; its display is the
/// reason the ordinary battery has always reported.
#[derive(Debug, thiserror::Error)]
#[error("{reason}")]
pub struct OracleFinding {
    /// What the finding says about the document.
    pub kind: FailureKind,
    /// The reported reason.
    pub reason: String,
}

/// An error carrying `kind` with display `reason`.
fn finding(kind: FailureKind, reason: impl Into<String>) -> eyre::Report {
    eyre::Report::new(OracleFinding {
        kind,
        reason: reason.into(),
    })
}

impl FailureKind {
    /// The kind `error` carries: an unusable Kubernetes control is
    /// undecided, and an error without a kind is infrastructure.
    #[must_use]
    pub fn of(error: &eyre::Report) -> Self {
        if error.downcast_ref::<UnusableControl>().is_some() {
            return Self::Undecided;
        }
        error
            .downcast_ref::<OracleFinding>()
            .map_or(Self::Infrastructure, |finding| finding.kind)
    }
}

/// Fresh payload observations and the owning chart experiment's shared controls.
pub struct GroupObservations<'a> {
    chart: &'a PinnedHelmChart,
    kubernetes_version: &'a str,
    values_file: &'a [u8],
    lint: Arc<HelmLintProbe>,
    payload: Arc<Mutex<InputObservations>>,
    controls: &'a ControlObservations,
    fresh_controls: ControlObservations,
}

/// Evidence paths for replayable controls of one immutable chart, engine, and environment.
#[derive(Default)]
pub struct ControlObservations {
    inputs: Mutex<BTreeMap<ControlInput, Arc<Mutex<InputObservations>>>>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct ControlInput {
    values_sha256: String,
    kubernetes_version: String,
}

#[derive(Default)]
struct InputObservations {
    lint: Option<PathBuf>,
    rendered: Option<PathBuf>,
}

impl<'a> GroupObservations<'a> {
    /// Observations of `document`'s group, whose lint probe is `lint`.
    pub fn new(
        chart: &'a PinnedHelmChart,
        document: &'a ScreenedDocument,
        lint: Arc<HelmLintProbe>,
        controls: &'a ControlObservations,
    ) -> Self {
        let payload = Arc::new(Mutex::new(InputObservations {
            lint: Some(lint.evidence_dir.clone()),
            rendered: None,
        }));
        Self {
            chart,
            kubernetes_version: &document.kubernetes_version,
            values_file: &document.values_file,
            lint,
            payload,
            controls,
            fresh_controls: ControlObservations::default(),
        }
    }

    fn input(&self, bytes: &[u8]) -> Arc<Mutex<InputObservations>> {
        if bytes == self.values_file {
            return Arc::clone(&self.payload);
        }
        let controls = if matches!(self.chart.render_cacheability().replay(), Replay::Cacheable) {
            self.controls
        } else {
            &self.fresh_controls
        };
        let mut controls = controls
            .inputs
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Arc::clone(
            controls
                .entry(ControlInput {
                    values_sha256: sha256_hex(bytes),
                    kubernetes_version: self.kubernetes_version.to_owned(),
                })
                .or_default(),
        )
    }

    fn rendered(&mut self, bytes: &[u8], directory: &std::path::Path) -> eyre::Result<HelmProbe> {
        let input = self.input(bytes);
        let mut input = input.lock().unwrap_or_else(PoisonError::into_inner);
        let evidence = if let Some(evidence) = &input.rendered {
            evidence.clone()
        } else {
            let observed = self.chart.adjudicate_file(bytes, self.kubernetes_version)?;
            input.rendered = Some(observed.evidence_dir.clone());
            observed.evidence_dir
        };
        drop(input);
        let mut observed = HelmProbe::read(&evidence)?;
        copy_execution_evidence(&observed.evidence_dir, directory)?;
        // Decoding writes below the probe's directory, so every document needs its own copy.
        observed.evidence_dir = directory.to_owned();
        Ok(observed)
    }

    fn schema_free_lint(
        &mut self,
        bytes: &[u8],
        directory: &std::path::Path,
    ) -> eyre::Result<LintClassification> {
        let input = self.input(bytes);
        let mut input = input.lock().unwrap_or_else(PoisonError::into_inner);
        let evidence = if let Some(evidence) = &input.lint {
            evidence.clone()
        } else {
            let observed = self
                .chart
                .adjudicate_schema_free_lint(bytes, self.kubernetes_version)?;
            input.lint = Some(observed.evidence_dir.clone());
            observed.evidence_dir
        };
        drop(input);
        copy_execution_evidence(&evidence, directory)?;
        classified(&HelmExecution::read(&evidence, "lint-schema-free")?)
    }
}

/// Adjudicates one screened document on its own, with fresh controls.
///
/// # Errors
///
/// Returns an error when Helm cannot run or the document cannot be judged.
pub fn adjudicate_lint_flip(
    chart: &PinnedHelmChart,
    probe: &ScreenedProbe,
    document: &ScreenedDocument,
    schemas: &LintSchemas,
    kubernetes: &OfflineKubernetesValidator,
) -> eyre::Result<(HelmFlipVerdict, PathBuf)> {
    let bytes = &document.values_file;
    let lint = chart.adjudicate_lint(&LintProbeRequest {
        values: bytes,
        kubernetes_version: &document.kubernetes_version,
        baseline_schema: &schemas.baseline,
        candidate_schema: &schemas.candidate,
    })?;
    let controls = ControlObservations::default();
    let mut observations = GroupObservations::new(chart, document, lint, &controls);
    adjudicate_lint_observation(probe, document, schemas, kubernetes, &mut observations)
}

/// Judges one document from its group's payload and the chart experiment's controls.
///
/// # Errors
///
/// Returns an error when the document cannot be judged; an [`OracleFinding`] says what it means.
pub fn adjudicate_lint_observation(
    probe: &ScreenedProbe,
    document: &ScreenedDocument,
    schemas: &LintSchemas,
    kubernetes: &OfflineKubernetesValidator,
    observations: &mut GroupObservations<'_>,
) -> eyre::Result<(HelmFlipVerdict, PathBuf)> {
    eyre::ensure!(
        document.values_file == observations.values_file
            && document.kubernetes_version == observations.kubernetes_version,
        "document does not belong to its observation group"
    );
    let lint = Arc::clone(&observations.lint);
    // A replayed lint may outlive this group; decoding still needs a fresh document directory.
    let evidence = tempfile::Builder::new()
        .prefix(&format!("{:?}-", document.kind))
        .tempdir_in(&lint.evidence_dir)?
        .keep();
    copy_execution_evidence(&lint.evidence_dir, &evidence)?;
    let result = judge(document, schemas, kubernetes, observations, &evidence);
    result
        .map(|verdict| (verdict, evidence.clone()))
        .map_err(|err| {
            let message = format!(
                "{err}; case={}; evidence={}",
                probe.name,
                preserve_failure(&evidence).display()
            );
            err.wrap_err(message)
        })
}

fn classified(execution: &HelmExecution) -> eyre::Result<LintClassification> {
    let mut log = execution.stdout.clone();
    log.extend_from_slice(&execution.stderr);
    let class = classify_lint(execution.exit_code, &log);
    if let LintClassification::Unresolved(reason) = &class {
        eyre::bail!("lint invocation unresolved: {reason}");
    }
    Ok(class)
}

fn schema_accepts(class: &LintClassification, kind: AcceptanceDocument) -> bool {
    let LintClassification::Reject { errors } = class else {
        return true;
    };
    !errors.iter().any(|error| {
        matches!(
            (kind, error.kind),
            (AcceptanceDocument::LintRaw, LintErrorKind::RawValuesSchema)
                | (
                    AcceptanceDocument::LintCoalescedTwice,
                    LintErrorKind::TemplatesSchema
                )
        )
    })
}

fn semantic_rejection(class: &LintClassification) -> eyre::Result<bool> {
    match class {
        LintClassification::Pass => Ok(false),
        LintClassification::Reject { errors } => {
            eyre::ensure!(
                errors
                    .iter()
                    .all(|error| error.kind == LintErrorKind::Semantic),
                "schema-free lint reported a schema or unclassified values error"
            );
            Ok(true)
        }
        LintClassification::Unresolved(reason) => {
            eyre::bail!("schema-free lint unresolved: {reason}")
        }
    }
}

fn judge(
    document: &ScreenedDocument,
    schemas: &LintSchemas,
    kubernetes: &OfflineKubernetesValidator,
    observations: &mut GroupObservations<'_>,
    evidence_dir: &std::path::Path,
) -> eyre::Result<HelmFlipVerdict> {
    let lint = Arc::clone(&observations.lint);
    let baseline_lint = classified(&lint.baseline)?;
    let candidate_lint = classified(&lint.candidate)?;
    let plain_lint = classified(&lint.schema_free)?;
    let rendered = observations.rendered(&document.values_file, evidence_dir)?;
    let values = match document.kind {
        AcceptanceDocument::Template => rendered
            .values
            .as_ref()
            .map_or(&document.values, |values| values.as_json()),
        AcceptanceDocument::LintRaw | AcceptanceDocument::LintCoalescedTwice => &document.values,
    };
    let (baseline_errors, candidate_errors) = schemas.profiles.errors(values);
    let before = baseline_errors.is_empty();
    let after = candidate_errors.is_empty();
    let new_rejection = !after && rejects_for_a_new_reason(&baseline_errors, &candidate_errors);
    let uninformative = document.defaults.as_ref().is_some_and(|defaults| {
        let (errors, _) = schemas.profiles.errors(defaults);
        !errors.is_empty()
            && errors.into_iter().collect::<BTreeSet<_>>()
                == baseline_errors.iter().cloned().collect()
    });
    let mut evidence = json!({
        "document_kind": document.kind, "document": values, "origins": document.origins,
        "lint_control": document.lint_control,
        "baseline_sha256": sha256_hex(&schemas.baseline), "candidate_sha256": sha256_hex(&schemas.candidate),
        "baseline_accepts": before, "candidate_accepts": after, "candidate_rejects_for_a_new_reason": new_rejection,
        "baseline_rejects_it_as_its_defaults": uninformative,
        "baseline_errors": baseline_errors.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "candidate_errors": candidate_errors.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "baseline_lint": format!("{baseline_lint:?}"), "candidate_lint": format!("{candidate_lint:?}"),
        "schema_free_lint": format!("{plain_lint:?}"), "render_evidence": rendered.evidence_dir,
        "kubernetes_version": document.kubernetes_version, "transport": "values-file",
        "failure_input": FailureInput::new(
            &document.values_file,
            &document.control_overlays,
        )?
    });
    let verdict = if document.kind != AcceptanceDocument::Template
        && (schema_accepts(&baseline_lint, document.kind) != before
            || schema_accepts(&candidate_lint, document.kind) != after)
    {
        Err(eyre::eyre!(
            "Rust screening disagrees with the corresponding real lint rule"
        ))
    } else if before == after && !new_rejection && (!document.lint_control || after) {
        Ok(HelmFlipVerdict::Collapsed)
    } else {
        judge_independent(
            &OracleRequest {
                chart: observations.chart,
                document,
                kubernetes,
                evidence_dir,
            },
            &plain_lint,
            &rendered,
            observations,
            after,
            uninformative,
            &mut evidence,
        )
    };
    if let Err(err) = &verdict {
        evidence
            .as_object_mut()
            .ok_or_eyre("lint evidence is not an object")?
            .insert("failure".into(), Value::String(err.to_string()));
    }
    std::fs::write(
        evidence_dir.join("schema-verdicts.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    verdict
}

struct Controls {
    defaults: HelmProbe,
    contexts: BTreeMap<Vec<u8>, HelmProbe>,
    lint_errors: BTreeSet<String>,
}

fn controls(
    observations: &mut GroupObservations<'_>,
    document: &ScreenedDocument,
    evidence: &mut Value,
    evidence_dir: &std::path::Path,
) -> eyre::Result<Controls> {
    let mut overlays = BTreeSet::from([serde_json::to_vec(&json!({}))?]);
    for overlay in &document.control_overlays {
        overlays.insert(serde_json::to_vec(overlay)?);
    }
    let mut defaults = None;
    let mut contexts = BTreeMap::new();
    let mut lint_errors = BTreeSet::new();
    evidence["controls"] = json!([]);
    for bytes in overlays {
        let directory = PathBuf::from("controls").join(sha256_hex(&bytes));
        let lint_evidence = directory.join("lint");
        let render_evidence = directory.join("render");
        let class = observations.schema_free_lint(&bytes, &evidence_dir.join(&lint_evidence))?;
        semantic_rejection(&class)?;
        let rendered = observations.rendered(&bytes, &evidence_dir.join(&render_evidence))?;
        evidence["controls"]
            .as_array_mut()
            .ok_or_eyre("controls are not an array")?
            .push(json!({
                "overlay": serde_json::from_slice::<Value>(&bytes)?,
                "schema_free_lint": format!("{class:?}"),
                "lint_evidence": lint_evidence,
                "render_evidence": render_evidence,
                "render_success": rendered.rendered.success(),
            }));
        if !rendered.rendered.success() {
            return Err(finding(
                FailureKind::Undecided,
                "unusable defaults/context render control; template abort cannot establish an attributable rejection",
            ));
        }
        if let LintClassification::Reject { errors } = class {
            lint_errors.extend(errors.into_iter().map(|error| error.diagnostic));
        }
        if bytes == b"{}" {
            defaults = Some(rendered);
        } else {
            contexts.insert(bytes, rendered);
        }
    }
    Ok(Controls {
        defaults: defaults.ok_or_eyre("missing defaults control")?,
        contexts,
        lint_errors,
    })
}

struct OracleRequest<'a> {
    chart: &'a PinnedHelmChart,
    document: &'a ScreenedDocument,
    kubernetes: &'a OfflineKubernetesValidator,
    evidence_dir: &'a std::path::Path,
}

fn judge_independent(
    request: &OracleRequest<'_>,
    plain_lint: &LintClassification,
    rendered: &HelmProbe,
    observations: &mut GroupObservations<'_>,
    after: bool,
    uninformative: bool,
    evidence: &mut Value,
) -> eyre::Result<HelmFlipVerdict> {
    let OracleRequest {
        chart,
        document,
        kubernetes,
        evidence_dir,
    } = request;
    let controls = controls(observations, document, evidence, evidence_dir)?;
    let mut applicable = Vec::new();
    for overlay in &document.control_overlays {
        let bytes = serde_json::to_vec(overlay)?;
        applicable.push(if bytes == b"{}" {
            &controls.defaults
        } else {
            controls
                .contexts
                .get(&bytes)
                .ok_or_eyre("missing applicable context render")?
        });
    }
    let checked = kubernetes.check_controls(chart, &controls.defaults, &applicable)?;
    let lint_rejects = semantic_rejection(plain_lint)?;
    let inherited_lint = !controls.lint_errors.is_empty();
    evidence["inherited_lint_errors"] = json!(controls.lint_errors);
    if lint_rejects && !inherited_lint {
        return Ok(rejection_verdict(
            after,
            uninformative,
            Rejection::LintRejects,
        ));
    }
    if inherited_lint {
        let LintClassification::Reject { errors } = plain_lint else {
            return Err(finding(
                FailureKind::Undecided,
                "unusable schema-free lint control: it rejects while the tested input passes",
            ));
        };
        if !errors
            .iter()
            .all(|error| controls.lint_errors.contains(&error.diagnostic))
        {
            return Err(finding(
                FailureKind::Undecided,
                "unusable schema-free lint control: inherited failures prevent attributing a changed diagnostic",
            ));
        }
    }
    if !rendered.rendered.success() {
        return Ok(rejection_verdict(
            after,
            uninformative,
            Rejection::HelmAborts,
        ));
    }
    judge_kubernetes_contexts(
        kubernetes.compare_with_controls(chart, rendered, &checked)?,
        document,
        after,
        uninformative,
        inherited_lint,
        evidence,
    )
}

fn judge_kubernetes_contexts(
    mut comparisons: Vec<DefaultsComparison>,
    document: &ScreenedDocument,
    after: bool,
    uninformative: bool,
    inherited_lint: bool,
    evidence: &mut Value,
) -> eyre::Result<HelmFlipVerdict> {
    evidence["kubernetes_contexts"] = json!(comparisons.iter().enumerate().map(|(index, comparison)| json!({
        "overlay": document.control_overlays.get(index).cloned().unwrap_or_else(|| json!({})),
        "new_kubernetes_errors": comparison.new_violations.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "inherited_kubernetes_errors": comparison.inherited_violations.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "kubernetes_uncertainty": comparison.uncertain,
    })).collect::<Vec<_>>());
    // Every origin must justify a tightening; one rejecting context disproves an acceptance.
    let unattributed = comparisons
        .iter()
        .filter(|comparison| comparison.new_violations.is_empty())
        .collect::<Vec<_>>();
    if !after && !unattributed.is_empty() {
        let kind = if unattributed
            .iter()
            .all(|comparison| comparison.uncertain.is_empty())
        {
            FailureKind::UnattributedRejection
        } else {
            FailureKind::Undecided
        };
        return Err(finding(
            kind,
            "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution",
        ));
    }
    let index = comparisons
        .iter()
        .position(|comparison| !comparison.new_violations.is_empty())
        .unwrap_or(0);
    eyre::ensure!(
        !comparisons.is_empty(),
        "missing applicable Kubernetes comparison"
    );
    let comparison = comparisons.remove(index);
    if inherited_lint && comparison.new_violations.is_empty() {
        return Err(finding(
            FailureKind::Undecided,
            format!(
                "{}; schema-free lint has only inherited errors from defaults/context controls",
                if after {
                    "unresolved lint acceptance"
                } else {
                    "tightening rejects a lint document without an attributable rejection"
                }
            ),
        ));
    }
    judge_kubernetes(comparison, after, uninformative, evidence)
}

fn rejection_verdict(after: bool, uninformative: bool, rejection: Rejection) -> HelmFlipVerdict {
    if after && uninformative {
        return HelmFlipVerdict::UninformativeBaselineFalseAcceptance(rejection);
    }
    match (after, rejection) {
        (true, Rejection::LintRejects) => HelmFlipVerdict::CandidateAcceptsLintRejects,
        (true, Rejection::HelmAborts) => HelmFlipVerdict::CandidateAcceptsHelmAborts,
        (true, Rejection::KubernetesRejects) => HelmFlipVerdict::CandidateAcceptsKubernetesRejects,
        (false, Rejection::LintRejects) => HelmFlipVerdict::TighteningMatchedLintRejection,
        (false, Rejection::HelmAborts) => HelmFlipVerdict::TighteningMatchedHelmAbort,
        (false, Rejection::KubernetesRejects) => {
            HelmFlipVerdict::TighteningMatchedKubernetesRejection
        }
    }
}

fn copy_execution_evidence(from: &std::path::Path, to: &std::path::Path) -> eyre::Result<()> {
    std::fs::create_dir_all(to)?;
    // The failure preserver materializes nested chart/input records only when needed.
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn judge_kubernetes(
    comparison: DefaultsComparison,
    after: bool,
    uninformative: bool,
    evidence: &mut Value,
) -> eyre::Result<HelmFlipVerdict> {
    let object = evidence
        .as_object_mut()
        .ok_or_eyre("lint evidence is not an object")?;
    object.insert(
        "new_kubernetes_errors".into(),
        json!(
            comparison
                .new_violations
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        ),
    );
    object.insert(
        "inherited_kubernetes_errors".into(),
        json!(
            comparison
                .inherited_violations
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        ),
    );
    object.insert("kubernetes_uncertainty".into(), json!(comparison.uncertain));
    if !comparison.new_violations.is_empty() {
        Ok(rejection_verdict(
            after,
            uninformative,
            Rejection::KubernetesRejects,
        ))
    } else if !after {
        let kind = if comparison.uncertain.is_empty() {
            FailureKind::UnattributedRejection
        } else {
            FailureKind::Undecided
        };
        Err(finding(
            kind,
            "tightening rejects a lint document whose schema-free lint succeeds and render adds no proved Kubernetes violation",
        ))
    } else if !comparison.uncertain.is_empty() {
        Ok(HelmFlipVerdict::LooseningWithUncertainKubernetes(
            comparison.uncertain,
        ))
    } else {
        Ok(HelmFlipVerdict::LooseningMatchedLintValidation)
    }
}

/// A document group's shared observations, or why they could not be made.
pub type LintObservation<'a> = Result<GroupObservations<'a>, String>;

#[tracing::instrument(
    target = "helm_schema::acceptance",
    skip_all,
    fields(
        chart = %live.label,
        probes = flips.len(),
        document_group = flips.first().and_then(|flip| live.flips.get(*flip)).is_some_and(|probe| probe.document.is_some())
    )
)]
/// Adjudicates the screened flips `flips` of `live`, sharing one Helm observation per document input.
pub fn adjudicate_probe_group(
    live: &LiveChart,
    flips: &[usize],
    kubernetes: Option<&OfflineKubernetesValidator>,
) -> Vec<AdjudicatedProbe> {
    let started = std::time::Instant::now();
    let document = flips
        .first()
        .and_then(|flip| live.flips.get(*flip))
        .and_then(|probe| probe.document.as_ref());
    // This observation belongs only to this experiment; another group invokes Helm afresh.
    let mut observations = document.filter(|_| kubernetes.is_some()).map(|document| {
        let chart = live.helm.chart(&live.path).map_err(str::to_string)?;
        let lint = chart
            .adjudicate_lint(&LintProbeRequest {
                values: &document.values_file,
                kubernetes_version: &document.kubernetes_version,
                baseline_schema: &live.lint_schemas.baseline,
                candidate_schema: &live.lint_schemas.candidate,
            })
            .map_err(|err| err.to_string())?;
        Ok(GroupObservations::new(
            chart,
            document,
            lint,
            &live.helm.controls,
        ))
    });
    let observation_ms = elapsed_ms(started);
    let mut results: Vec<_> = flips
        .iter()
        .map(|flip| adjudicate_probe(live, *flip, kubernetes, &mut observations))
        .collect();
    // Shared execution time is charged once while every document retains its own outcome.
    if let Some(first) = results.first_mut() {
        first.adjudication_ms += observation_ms;
    }
    results
}

/// Adjudicates one screened flip using its group's observations when applicable.
#[tracing::instrument(
    target = "helm_schema::acceptance",
    skip_all,
    fields(chart = %live.label, lane = "adjudicate", row = flip)
)]
pub fn adjudicate_probe(
    live: &LiveChart,
    flip: usize,
    kubernetes: Option<&OfflineKubernetesValidator>,
    observations: &mut Option<LintObservation<'_>>,
) -> AdjudicatedProbe {
    let started = std::time::Instant::now();
    let Some(probe) = live.flips.get(flip) else {
        return AdjudicatedProbe {
            flip,
            key: format!("{}: missing flip {flip}", live.label),
            case: format!("{}: flip {flip}", live.label),
            document_kind: None,
            kubernetes_version: String::new(),
            input: None,
            outcome: Err(AdjudicationFailure::infrastructure(format!(
                "{}: no screened flip {flip}",
                live.label
            ))),
            adjudication_ms: 0,
        };
    };
    let case = format!("{}: {}", live.label, probe.name);
    let input = FailureInput::for_probe(probe);
    let kubernetes_version = probe.document.as_ref().map_or_else(
        || {
            live.helm
                .chart(&live.path)
                .ok()
                .and_then(PinnedHelmChart::kubernetes_version)
                .unwrap_or_default()
                .to_string()
        },
        |document| document.kubernetes_version.clone(),
    );
    let outcome = match (&input, live.helm.chart(&live.path), kubernetes) {
        (Err(error), _, _) => Err(AdjudicationFailure::infrastructure(format!(
            "{case}: cannot identify input: {error}"
        ))),
        (_, Err(error), _) => Err(AdjudicationFailure::infrastructure(format!(
            "{case}: {error}"
        ))),
        (_, Ok(_), None) => Err(AdjudicationFailure::infrastructure(format!(
            "{case}: no Kubernetes validator"
        ))),
        (_, Ok(chart), Some(kubernetes)) => {
            let outcome = match &probe.document {
                Some(document) => match observations {
                    Some(Ok(observations)) => adjudicate_lint_observation(
                        probe,
                        document,
                        &live.lint_schemas,
                        kubernetes,
                        observations,
                    ),
                    Some(Err(err)) => Err(eyre::eyre!("{err}")),
                    None => Err(eyre::eyre!("missing lint observation for document group")),
                },
                None => adjudicate_flip(
                    chart,
                    &probe.overlay,
                    &live.profiles,
                    kubernetes,
                    live.annotation.as_ref(),
                ),
            };
            outcome.map_err(|error| AdjudicationFailure {
                kind: FailureKind::of(&error),
                message: format!("{case}: {error}"),
            })
        }
    };
    AdjudicatedProbe {
        flip,
        key: super::probe_key(&live.label, probe),
        case,
        document_kind: probe.document.as_ref().map(|document| document.kind),
        kubernetes_version,
        input: input.ok(),
        outcome,
        adjudication_ms: elapsed_ms(started),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Helm's and Kubernetes's verdict on one screened flip.
pub enum HelmFlipVerdict {
    /// Helm's coalesced document shows no transition.
    Collapsed,
    /// The candidate rejects, and schema-free lint rejects too.
    TighteningMatchedLintRejection,
    /// The candidate accepts, and schema-free lint accepts.
    LooseningMatchedLintValidation,
    /// The candidate accepts a document schema-free lint rejects.
    CandidateAcceptsLintRejects,
    /// The candidate rejects a document Helm aborts on.
    TighteningMatchedHelmAbort,
    /// The candidate rejects a document whose render adds a Kubernetes violation.
    TighteningMatchedKubernetesRejection,
    /// Helm renders and Kubernetes proves no new violation, but the
    /// rejection is exactly a declared default's type assertion on a path no
    /// template reads, and the schema generated under
    /// `--declared-types=annotate` accepts the document: the rejection is the
    /// `assert` authoring policy, not a recovered constraint.
    TighteningMatchedDeclaredTypesPolicy,
    /// Helm renders, and Kubernetes validates every resource.
    LooseningMatchedKubernetesValidation,
    /// Helm renders, and every Kubernetes violation is one the defaults render already carries.
    LooseningMatchedDefaultsViolations,
    /// Helm renders, but a new or changed resource has no decidable schema,
    /// for the reasons listed.
    LooseningWithUncertainKubernetes(Vec<String>),
    /// The candidate accepts a document Helm aborts on.
    CandidateAcceptsHelmAborts,
    /// The candidate accepts a document whose render adds a Kubernetes violation.
    CandidateAcceptsKubernetesRejects,
    /// The candidate accepts a cell Helm or Kubernetes rejects, and the
    /// baseline rejected it exactly as it rejects the chart's own defaults:
    /// a false acceptance judged on the candidate alone.
    UninformativeBaselineFalseAcceptance(Rejection),
}

/// Recorded, not yet decisive: how each profile judges every document
/// Helm's schema checks validate for `overlay`, lint's two included, or why
/// the port composed none.
pub fn acceptance_document_verdicts(
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

/// The verdict on the exact-template flip `overlay`, without declared-types attribution.
///
/// # Errors
///
/// Returns an error when Helm cannot run or the flip cannot be judged.
pub fn adjudicate_round74_flip(
    chart: &PinnedHelmChart,
    overlay: &serde_json::Value,
    profiles: &ProfileSchemas,
    kubernetes: &OfflineKubernetesValidator,
) -> eyre::Result<HelmFlipVerdict> {
    Ok(adjudicate_flip(chart, overlay, profiles, kubernetes, None)?.0)
}

/// The whole-chart recipe the registry generates `dump_name` from.
#[must_use]
pub fn chart_recipe(dump_name: &str) -> Option<registry::ChartRecipe> {
    registry::registry()
        .into_iter()
        .find_map(|spec| match spec.recipe {
            registry::GenerationRecipe::Chart(recipe) if spec.dump_name == dump_name => {
                Some(recipe)
            }
            _ => None,
        })
}

/// A chart regenerated by its candidate's recipe under both declared-types
/// policies, prepared by the first flip that needs it. The two generations
/// differ only in whether declared defaults assert their types.
pub struct DeclaredTypesAnnotation {
    /// The chart directory.
    pub chart_dir: PathBuf,
    /// The candidate's registry recipe.
    pub recipe: registry::ChartRecipe,
    /// A schema conjoined to both regenerations: a control's stand-in for a
    /// constraint the chart's own generation carries under either policy.
    /// The battery passes none.
    pub constraint: Option<serde_json::Value>,
    /// Both regenerations, prepared on first use.
    pub prepared: std::sync::OnceLock<Result<RegeneratedChart, String>>,
}

/// The chart's schemas under both policies, and the annotate session that
/// explains its paths.
pub struct RegeneratedChart {
    /// The assert regeneration.
    pub asserted: PolicySchema,
    /// The annotate regeneration's validator.
    pub annotated: jsonschema::Validator,
    /// The annotate session, which explains paths.
    pub session: helm_schema::AnalysisSession,
}

/// One policy's generated schema and its compiled validator.
pub struct PolicySchema {
    /// The generated schema.
    pub schema: serde_json::Value,
    /// Its compiled validator.
    pub validator: jsonschema::Validator,
}

impl PolicySchema {
    /// Generates `recipe`'s schema in `session`, conjoined with `constraint`.
    ///
    /// # Errors
    ///
    /// Returns why generation or compilation failed.
    pub fn generate(
        session: &helm_schema::AnalysisSession,
        recipe: &registry::ChartRecipe,
        constraint: Option<&serde_json::Value>,
    ) -> Result<Self, String> {
        let policy = recipe.authoring.declared_types;
        let mut schema = generate::session_schema(session, recipe)
            .map_err(|error| format!("generate the {policy:?} schema: {error:#}"))?;
        if let Some(constraint) = constraint {
            schema = conjoin(schema, constraint);
        }
        let validator = jsonschema::validator_for(&schema)
            .map_err(|error| format!("compile the {policy:?} schema: {error}"))?;
        Ok(Self { schema, validator })
    }
}

/// `schema` and `constraint` as one schema: both under `allOf`, their
/// definitions merged at the root so every `#/$defs/...` still resolves,
/// under the constraint's dialect when it names one.
#[must_use]
pub fn conjoin(mut schema: serde_json::Value, constraint: &serde_json::Value) -> serde_json::Value {
    let mut constraint = constraint.clone();
    let mut root = serde_json::Map::new();
    let mut definitions = serde_json::Map::new();
    for part in [&mut schema, &mut constraint] {
        let Some(members) = part.as_object_mut() else {
            continue;
        };
        if let Some(dialect) = members.remove("$schema") {
            root.insert("$schema".to_string(), dialect);
        }
        if let Some(serde_json::Value::Object(defined)) = members.remove("$defs") {
            definitions.extend(defined);
        }
    }
    root.insert("$defs".to_string(), serde_json::Value::Object(definitions));
    root.insert("allOf".to_string(), json!([schema, constraint]));
    serde_json::Value::Object(root)
}

impl DeclaredTypesAnnotation {
    /// The chart at `chart_dir`, generated by `recipe` under each
    /// declared-types policy.
    #[must_use]
    pub fn new(chart_dir: PathBuf, recipe: registry::ChartRecipe) -> Self {
        Self {
            chart_dir,
            recipe,
            constraint: None,
            prepared: std::sync::OnceLock::new(),
        }
    }

    /// Both regenerations, generated on first use.
    ///
    /// # Errors
    ///
    /// Returns why a regeneration failed.
    pub fn prepared(&self) -> Result<&RegeneratedChart, String> {
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
                let constraint = self.constraint.as_ref();
                let asserted =
                    PolicySchema::generate(&session(&asserting), &asserting, constraint)?;
                let annotate_session = session(&annotating);
                let annotated = PolicySchema::generate(&annotate_session, &annotating, constraint)?;
                Ok(RegeneratedChart {
                    asserted,
                    annotated: annotated.validator,
                    session: annotate_session,
                })
            })
            .as_ref()
            .map_err(Clone::clone)
    }
}

/// [`declared_types_attribution`] of a Helm-rendered rejection, when the
/// chart has an annotate regeneration and Helm coalesced the document.
///
/// # Errors
///
/// Returns the first condition that fails.
pub fn attribute_to_declared_types(
    annotation: Option<&DeclaredTypesAnnotation>,
    values: Option<&CoalescedValues>,
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

/// Whether the candidate's rejection of the Helm-coalesced `document` is the
/// `assert` authoring policy's alone. It is when the candidate, which rejects
/// it, is the chart's assert regeneration, the annotate regeneration accepts
/// the document outright, and every value the assert regeneration rejects is
/// an object member explain C1 reports unread (no use of it, its descendants
/// or an ancestor besides the seeded top-level and dependency-root claims,
/// and no generation decision). The two regenerations differ only in the
/// declared-types policy, so when the annotate one accepts, every reason the
/// candidate rejects for is the policy's, whatever the baseline rejects; no
/// failure is compared with another. The error names the first condition
/// that fails.
///
/// # Errors
///
/// Returns the first condition that fails.
pub fn declared_types_attribution(
    annotation: &DeclaredTypesAnnotation,
    profiles: &ProfileSchemas,
    document: &serde_json::Value,
) -> Result<(), String> {
    let (_, candidate_accepts) = profiles.document_verdicts(document);
    if candidate_accepts {
        return Err("the candidate accepts it".to_string());
    }
    let regenerated = annotation.prepared()?;
    if profiles.candidate_schema() != &regenerated.asserted.schema {
        return Err("the candidate is not the assert regeneration".to_string());
    }
    if !regenerated.annotated.is_valid(document) {
        return Err("the annotate regeneration rejects it".to_string());
    }
    for error in regenerated.asserted.validator.iter_errors(document) {
        let path = error.instance_path().as_str().to_string();
        // Whether a JSON-pointer token names an object member or an array
        // element is the document's to say: `Location::segments` reads every
        // numeric token as an index, even an object key such as `"1"`.
        let mut keys = Vec::new();
        let mut instance = document;
        for token in path.split('/').skip(1) {
            let key = token.replace("~1", "/").replace("~0", "~");
            instance = match instance {
                serde_json::Value::Object(members) => match members.get(&key) {
                    Some(member) => member,
                    None => return Err(format!("{path}: `{key}` is not in the document")),
                },
                serde_json::Value::Array(_) => {
                    return Err(format!("{path}: steps into an array"));
                }
                _ => return Err(format!("{path}: steps into a scalar")),
            };
            keys.push(key);
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
/// # Errors
///
/// Returns an error when Helm cannot run or the flip cannot be judged; an [`OracleFinding`] says what it means.
pub fn adjudicate_flip(
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
        // A new violation is decisive; otherwise every resource must be decided.
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
                (Err(_), true) => Err((
                    FailureKind::UnattributedRejection,
                    "tightening rejects a document whose render adds no Kubernetes violation to the defaults render",
                )),
                (Err(_), false) => Err((
                    FailureKind::Undecided,
                    "tightening rejects a document Helm renders without a proved Kubernetes violation",
                )),
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
    let verdict = verdict.map_err(|(kind, reason)| {
        let evidence = preserve_failure(&probe.evidence_dir);
        finding(kind, reason).wrap_err(format!("{reason}; evidence={}", evidence.display()))
    })?;
    Ok((verdict, probe.evidence_dir))
}

/// Exact input bytes and the ordered applicable controls, including the defaults control.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct FailureInput {
    /// SHA-256 of the exact values-file bytes.
    pub values_file_sha256: Cow<'static, str>,
    /// SHA-256 of the canonical defaults and applicable controls.
    pub controls_sha256: Cow<'static, str>,
}

impl FailureInput {
    /// The identity of `values` under `controls`.
    ///
    /// # Errors
    ///
    /// Returns an error when `controls` cannot be serialized.
    pub fn new(values: &[u8], controls: &[serde_json::Value]) -> eyre::Result<Self> {
        let controls = serde_json::json!({"defaults": {}, "applicable": controls});
        Ok(Self {
            values_file_sha256: sha256_hex(values).into(),
            controls_sha256: sha256_hex(&serde_json::to_vec(&controls)?).into(),
        })
    }

    /// The identity of `probe`'s values file and controls.
    ///
    /// # Errors
    ///
    /// Returns an error when the overlay cannot be serialized.
    pub fn for_probe(probe: &ScreenedProbe) -> eyre::Result<Self> {
        match &probe.document {
            Some(document) => Self::new(&document.values_file, &document.control_overlays),
            None => Self::new(
                &serde_json::to_vec_pretty(&probe.overlay)?,
                &[serde_json::json!({})],
            ),
        }
    }
}

#[derive(Debug, Serialize)]
/// An adjudication that reached no verdict, as reported.
pub struct ObservedFailure {
    /// `chart: probe` of the flip.
    pub case: String,
    /// The Kubernetes version Helm rendered for.
    pub kubernetes_version: String,
    /// Why there is no verdict, without the case and evidence suffixes.
    pub reason: String,
    /// The exact input identity.
    pub input: Option<FailureInput>,
    /// The evidence directory the message names.
    pub evidence: Option<std::path::PathBuf>,
}

impl ObservedFailure {
    /// Parses `message`, `{case}: {reason}[; case=...][; evidence=...]`.
    #[must_use]
    pub fn from_adjudication(
        case: String,
        kubernetes_version: String,
        message: &str,
        input: Option<FailureInput>,
    ) -> Self {
        let reason = message
            .strip_prefix(&format!("{case}: "))
            .unwrap_or(message);
        let (reason, evidence) = match reason.rsplit_once("; evidence=") {
            Some((reason, path)) => (reason, Some(std::path::PathBuf::from(path))),
            None => (reason, None),
        };
        let reason = case.split_once(": ").map_or(reason, |(_, probe)| {
            reason
                .strip_suffix(&format!("; case={probe}"))
                .unwrap_or(reason)
        });
        Self {
            case,
            kubernetes_version,
            reason: reason.to_string(),
            input,
            evidence,
        }
    }
}
