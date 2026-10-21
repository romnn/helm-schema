//! Live evidence for document-specific flips, without collapsing them to template values.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::helm::adjudication::{
    DefaultsComparison, HelmLintProbe, HelmProbe, LintProbeRequest,
};
use helm_schema_test_support::helm::invocation::{HelmExecution, Replay, preserve_failure};
use helm_schema_test_support::helm::lint::{LintClassification, LintErrorKind, classify_lint};
use helm_schema_test_support::source_digest::sha256_hex;
use serde_json::{Value, json};
use test_util::helm_values::AcceptanceDocument;

use crate::lint_probes::{LintSchemas, ScreenedDocument, ScreenedProbe};
use crate::{
    HelmFlipVerdict, OfflineKubernetesValidator, PinnedHelmChart, Rejection,
    rejects_for_a_new_reason,
};

/// Fresh payload observations and the owning chart experiment's shared controls.
pub(crate) struct GroupObservations<'a> {
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
pub(crate) struct ControlObservations {
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
    pub(crate) fn new(
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

pub(crate) fn adjudicate_lint_flip(
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
pub(crate) fn adjudicate_lint_observation(
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
            eyre::eyre!(
                "{err}; case={}; evidence={}",
                probe.name,
                preserve_failure(&evidence).display()
            )
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
        "failure_input": crate::known_inherited_failures::FailureInput::new(
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
        eyre::ensure!(
            rendered.rendered.success(),
            "unusable defaults/context render control; template abort cannot establish an attributable rejection"
        );
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
            eyre::bail!(
                "unusable schema-free lint control: it rejects while the tested input passes"
            );
        };
        eyre::ensure!(
            errors
                .iter()
                .all(|error| controls.lint_errors.contains(&error.diagnostic)),
            "unusable schema-free lint control: inherited failures prevent attributing a changed diagnostic"
        );
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
    if !after
        && comparisons
            .iter()
            .any(|comparison| comparison.new_violations.is_empty())
    {
        eyre::bail!(
            "tightening rejects a lint document without a new Kubernetes violation in every applicable context; inherited errors cannot establish attribution"
        );
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
        eyre::bail!(
            "{}; schema-free lint has only inherited errors from defaults/context controls",
            if after {
                "unresolved lint acceptance"
            } else {
                "tightening rejects a lint document without an attributable rejection"
            }
        );
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
        Err(eyre::eyre!(
            "tightening rejects a lint document whose schema-free lint succeeds and render adds no proved Kubernetes violation"
        ))
    } else if !comparison.uncertain.is_empty() {
        Ok(HelmFlipVerdict::LooseningWithUncertainKubernetes(
            comparison.uncertain,
        ))
    } else {
        Ok(HelmFlipVerdict::LooseningMatchedLintValidation)
    }
}
