//! Live evidence for document-specific flips, without collapsing them to template values.

use std::collections::BTreeSet;
use std::path::PathBuf;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::helm::adjudication::{HelmLintProbe, LintProbeRequest};
use helm_schema_test_support::helm::invocation::{HelmExecution, preserve_failure};
use helm_schema_test_support::helm::lint::{LintClassification, LintErrorKind, classify_lint};
use helm_schema_test_support::source_digest::sha256_hex;
use serde_json::{Value, json};
use test_util::helm_values::AcceptanceDocument;

use crate::lint_probes::{LintSchemas, ScreenedDocument, ScreenedProbe};
use crate::{
    HelmFlipVerdict, OfflineKubernetesValidator, PinnedHelmChart, Rejection,
    rejects_for_a_new_reason,
};

pub(crate) fn adjudicate_lint_flip(
    chart: &PinnedHelmChart,
    probe: &ScreenedProbe,
    document: &ScreenedDocument,
    schemas: &LintSchemas,
    kubernetes: &OfflineKubernetesValidator,
) -> eyre::Result<(HelmFlipVerdict, PathBuf)> {
    let bytes = serde_json::to_vec(&probe.overlay)?;
    let lint = chart.adjudicate_lint(&LintProbeRequest {
        values: &bytes,
        kubernetes_version: &document.kubernetes_version,
        baseline_schema: &schemas.baseline,
        candidate_schema: &schemas.candidate,
    })?;
    // Each document keeps its own evidence even when the three lint executions are shared.
    let evidence = lint.evidence_dir.join(format!("{:?}", document.kind));
    std::fs::create_dir_all(&evidence)?;
    copy_execution_evidence(&lint.evidence_dir, &evidence)?;
    let result = judge(
        chart, document, schemas, kubernetes, &bytes, &lint, &evidence,
    );
    result
        .map(|verdict| (verdict, evidence.clone()))
        .map_err(|err| eyre::eyre!("{err}; evidence={}", preserve_failure(&evidence).display()))
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
    chart: &PinnedHelmChart,
    document: &ScreenedDocument,
    schemas: &LintSchemas,
    kubernetes: &OfflineKubernetesValidator,
    bytes: &[u8],
    lint: &HelmLintProbe,
    evidence_dir: &std::path::Path,
) -> eyre::Result<HelmFlipVerdict> {
    let baseline_lint = classified(&lint.baseline)?;
    let candidate_lint = classified(&lint.candidate)?;
    let plain_lint = classified(&lint.schema_free)?;
    let semantic_lint_rejects = semantic_rejection(&plain_lint)?;
    let rendered = chart.adjudicate_file(bytes, &document.kubernetes_version)?;
    copy_execution_evidence(&rendered.evidence_dir, evidence_dir)?;
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
        "baseline_sha256": sha256_hex(&schemas.baseline), "candidate_sha256": sha256_hex(&schemas.candidate),
        "baseline_accepts": before, "candidate_accepts": after, "candidate_rejects_for_a_new_reason": new_rejection,
        "baseline_rejects_it_as_its_defaults": uninformative,
        "baseline_errors": baseline_errors.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "candidate_errors": candidate_errors.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "baseline_lint": format!("{baseline_lint:?}"), "candidate_lint": format!("{candidate_lint:?}"),
        "schema_free_lint": format!("{plain_lint:?}"), "render_evidence": rendered.evidence_dir,
        "kubernetes_version": document.kubernetes_version, "transport": "values-file"
    });
    let verdict = if document.kind != AcceptanceDocument::Template
        && (schema_accepts(&baseline_lint, document.kind) != before
            || schema_accepts(&candidate_lint, document.kind) != after)
    {
        Err(eyre::eyre!(
            "Rust screening disagrees with the corresponding real lint rule"
        ))
    } else if before == after && !new_rejection {
        Ok(HelmFlipVerdict::Collapsed)
    } else if semantic_lint_rejects {
        Ok(rejection_verdict(
            after,
            uninformative,
            Rejection::LintRejects,
        ))
    } else if !rendered.rendered.success() {
        Ok(rejection_verdict(
            after,
            uninformative,
            Rejection::HelmAborts,
        ))
    } else {
        let comparison =
            kubernetes.compare_with_defaults_at(chart, &rendered, &document.kubernetes_version)?;
        judge_kubernetes(comparison, after, uninformative, &mut evidence)
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
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn judge_kubernetes(
    comparison: helm_schema_test_support::helm::adjudication::DefaultsComparison,
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
