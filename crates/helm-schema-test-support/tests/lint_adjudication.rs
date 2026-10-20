//! Real Helm controls for schema-isolated lint adjudication.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::helm::adjudication::{LintProbeRequest, PinnedHelmChart};
use helm_schema_test_support::helm::invocation::{HelmExecution, HelmRunner, tree_sha256};
use helm_schema_test_support::helm::lint::{LintClassification, LintErrorKind, classify_lint};
use indoc::indoc;
use serde_json::{Value, json};
use test_util::helm_values::{AcceptanceDocument, AcceptanceInputs};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

#[test]
fn root_schema_lints_share_exact_inputs_and_isolate_shipped_schemas() -> eyre::Result<()> {
    let source = ScratchDir::new("lint-schema-controls")?;
    fs::write(
        source.path().join("Chart.yaml"),
        indoc! {"
        apiVersion: v2
        name: lint-controls
        version: 1.0.0
    "},
    )?;
    fs::write(source.path().join("values.yaml"), "message: valid\n")?;
    // Shipped schema assertions must never contaminate the schema-free control.
    fs::write(source.path().join("values.schema.json"), "false")?;
    let chart =
        PinnedHelmChart::prepare_for_explicit_versions(HelmRunner::shared()?, source.path())?;
    let request = LintProbeRequest {
        values: b"{}",
        kubernetes_version: "1.35.0",
        baseline_schema: br#"{"type":"object"}"#,
        candidate_schema: br#"{"type":"object","required":["missing"]}"#,
    };
    let probe = chart.adjudicate_lint(&request)?;
    sim_assert_eq!(have: probe.baseline.exit_code, want: 0);
    sim_assert_eq!(have: probe.candidate.exit_code, want: 1);
    sim_assert_eq!(have: probe.schema_free.exit_code, want: 0);
    sim_assert_eq!(have: fs::read(probe.evidence_dir.join("values.json"))?, want: b"{}".to_vec());
    sim_assert_eq!(have: chart.invocations().len(), want: 3);

    // Both lint document kinds consume the same executions rather than rerunning Helm.
    let repeated = chart.adjudicate_lint(&request)?;
    assert!(Arc::ptr_eq(&probe, &repeated));
    sim_assert_eq!(have: chart.invocations().len(), want: 3);

    // Exact overlay bytes participate in identity even when values are semantically equal.
    chart.adjudicate_lint(&LintProbeRequest {
        values: b"{ }",
        ..request
    })?;
    sim_assert_eq!(have: chart.invocations().len(), want: 6);
    let control = chart.adjudicate_schema_free_lint(b"{}", request.kubernetes_version)?;
    sim_assert_eq!(have: control.execution.exit_code, want: 0);
    let invocations = chart.invocations();
    sim_assert_eq!(have: invocations.len(), want: 7);
    sim_assert_eq!(have: invocations.last().map(|record| record.stage.as_str()), want: Some("lint-schema-free"));
    Ok(())
}

#[test]
fn nondeterministic_chart_lints_are_never_memoized() -> eyre::Result<()> {
    let source = ScratchDir::new("lint-nondeterministic-controls")?;
    fs::write(
        source.path().join("Chart.yaml"),
        indoc! {"
        apiVersion: v2
        name: lint-random
        version: 1.0.0
    "},
    )?;
    fs::create_dir(source.path().join("templates"))?;
    fs::write(
        source.path().join("templates/config.yaml"),
        indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: lint-random
        data:
          random: {{ randAlphaNum 12 | quote }}
    "},
    )?;
    let chart =
        PinnedHelmChart::prepare_for_explicit_versions(HelmRunner::shared()?, source.path())?;
    let request = LintProbeRequest {
        values: b"{}",
        kubernetes_version: "1.35.0",
        baseline_schema: br#"{"type":"object"}"#,
        candidate_schema: br#"{"type":"object"}"#,
    };
    let first = chart.adjudicate_lint(&request)?;
    let second = chart.adjudicate_lint(&request)?;
    assert!(!Arc::ptr_eq(&first, &second));
    sim_assert_eq!(have: chart.invocations().len(), want: 6);
    Ok(())
}

#[test]
fn dependency_composition_matches_real_helm_for_all_three_documents() -> eyre::Result<()> {
    let charts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/charts");
    for (name, overlay) in [
        ("oauth2-proxy", json!({"redis-ha": {"enabled": true}})),
        (
            "openebs",
            json!({"engines": {"replicated": {"mayastor": {"enabled": null}}}}),
        ),
        ("airflow", json!({"postgresql": {"enabled": false}})),
    ] {
        assert_dependency_composition(&charts.join(name), &overlay)?;
    }
    Ok(())
}

fn assert_dependency_composition(source: &Path, overlay: &Value) -> eyre::Result<()> {
    let source_identity = tree_sha256(source)?;
    let inputs = AcceptanceInputs::load(source)?;
    let raw = inputs
        .compose(overlay.clone(), AcceptanceDocument::LintRaw)?
        .root;
    let twice = inputs
        .compose(overlay.clone(), AcceptanceDocument::LintCoalescedTwice)?
        .root;
    let template = inputs
        .compose(overlay.clone(), AcceptanceDocument::Template)?
        .root;

    // Preserve dependency values and metadata while removing semantic template failures.
    // This makes a full-document const rejection identify the exact lint composition rule.
    let prepared = PinnedHelmChart::prepare(HelmRunner::shared()?, source)?;
    let chart = PinnedHelmChart::prepare(HelmRunner::shared()?, prepared.coalesce_tree().path())?;
    let values = serde_json::to_vec(overlay)?;
    let baseline_schema = serde_json::to_vec(&json!({"const": raw}))?;
    let candidate_schema = serde_json::to_vec(&json!({"const": twice}))?;
    let kubernetes_version = chart
        .kubernetes_version()
        .ok_or_eyre("missing chart Kubernetes version")?;
    let probe = chart.adjudicate_lint(&LintProbeRequest {
        values: &values,
        kubernetes_version,
        baseline_schema: &baseline_schema,
        candidate_schema: &candidate_schema,
    })?;
    fs::write(
        probe.evidence_dir.join("port-raw.json"),
        serde_json::to_vec_pretty(&raw)?,
    )?;
    fs::write(
        probe.evidence_dir.join("port-twice.json"),
        serde_json::to_vec_pretty(&twice)?,
    )?;
    fs::write(
        probe.evidence_dir.join("port-template.json"),
        serde_json::to_vec_pretty(&template)?,
    )?;
    let baseline = rule_rejections(&probe.baseline)?;
    let candidate = rule_rejections(&probe.candidate)?;

    // Const schemas prove equality of whole documents, including dependency scopes and nulls.
    sim_assert_eq!(have: baseline.raw, want: false);
    sim_assert_eq!(have: baseline.twice, want: raw != twice);
    sim_assert_eq!(have: candidate.raw, want: raw != twice);
    sim_assert_eq!(have: candidate.twice, want: false);
    sim_assert_eq!(have: probe.schema_free.exit_code, want: 0);
    let rendered = chart.adjudicate_file(&values, kubernetes_version)?;
    sim_assert_eq!(have: rendered.rendered.exit_code, want: 0);
    let actual = rendered
        .values
        .ok_or_eyre("Helm failed to coalesce the template control")?;
    sim_assert_eq!(have: actual.as_json(), want: &template);
    sim_assert_eq!(have: tree_sha256(source)?, want: source_identity);
    Ok(())
}

#[derive(Default)]
struct RuleRejections {
    raw: bool,
    twice: bool,
}

fn rule_rejections(execution: &HelmExecution) -> eyre::Result<RuleRejections> {
    let mut log = execution.stdout.clone();
    log.extend_from_slice(&execution.stderr);
    let mut rejected = RuleRejections::default();
    match classify_lint(execution.exit_code, &log) {
        LintClassification::Pass => {}
        LintClassification::Reject { errors } => {
            for error in errors {
                match error.kind {
                    LintErrorKind::RawValuesSchema => rejected.raw = true,
                    LintErrorKind::TemplatesSchema => rejected.twice = true,
                    LintErrorKind::Semantic => {
                        eyre::bail!("unexpected semantic lint failure: {}", error.diagnostic)
                    }
                }
            }
        }
        LintClassification::Unresolved(reason) => eyre::bail!(
            "unresolved composition control: {reason}; {}",
            String::from_utf8_lossy(&log)
        ),
    }
    Ok(rejected)
}
