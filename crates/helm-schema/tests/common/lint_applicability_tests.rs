//! Battery and pinned CLI agreement when the template lint rule does not run.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use color_eyre::eyre;
use helm_schema_test_support::helm::adjudication::{LintProbeRequest, PinnedHelmChart};
use helm_schema_test_support::helm::invocation::{HelmRunner, tree_sha256};
use helm_schema_test_support::helm::lint::{LintClassification, classify_lint};
use helm_schema_test_support::source_digest::sha256_hex;
use indoc::indoc;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

use super::lint_probes::{
    ContextRow, DocumentCoverage, LintCoverage, LintSchemas, ScreenRequest, screen,
};

fn assert_skipped_template_rule(coverage: &LintCoverage, rows: usize) -> eyre::Result<()> {
    coverage.validate()?;
    sim_assert_eq!(have: coverage.distinct_overlays, want: rows);
    sim_assert_eq!(have: &coverage.documents, want: &BTreeMap::from([
        ("LintRaw".to_string(), DocumentCoverage {
            composed: rows,
            screened: rows,
            ..DocumentCoverage::default()
        }),
        ("LintCoalescedTwice".to_string(), DocumentCoverage {
            not_validated: rows,
            ..DocumentCoverage::default()
        }),
        ("Template".to_string(), DocumentCoverage {
            composed: rows,
            screened: rows,
            ..DocumentCoverage::default()
        }),
    ]));
    Ok(())
}

fn assert_pinned_lint_skips_templates(
    chart: &Path,
    schemas: &LintSchemas,
    overlay: &Value,
) -> eyre::Result<()> {
    let chart = PinnedHelmChart::prepare_for_explicit_versions(HelmRunner::shared()?, chart)?;
    let values = serde_json::to_vec(overlay)?;
    let lint = chart.adjudicate_lint(&LintProbeRequest {
        values: &values,
        kubernetes_version: "1.29.0",
        baseline_schema: &schemas.baseline,
        candidate_schema: &schemas.candidate,
    })?;
    for execution in [&lint.baseline, &lint.candidate, &lint.schema_free] {
        let mut log = execution.stdout.clone();
        log.extend_from_slice(&execution.stderr);
        sim_assert_eq!(have: classify_lint(execution.exit_code, &log), want: LintClassification::Pass);
        assert!(
            String::from_utf8_lossy(&log).contains("templates/: directory does not exist"),
            "{}",
            String::from_utf8_lossy(&log)
        );
    }
    Ok(())
}

#[test]
fn no_templates_gate_null_context_screens_no_hypothetical_twice_lint_flip() -> eyre::Result<()> {
    let chart = ScratchDir::new("lint-applicability-gate")?;
    fs::write(
        chart.path().join("Chart.yaml"),
        indoc! {"
        apiVersion: v2
        name: gate
        version: 1.0.0
    "},
    )?;
    fs::write(
        chart.path().join("values.yaml"),
        indoc! {"
        gate: false
        nested:
          'n': 3
    "},
    )?;
    let schemas = LintSchemas::compile(
        &json!({}),
        &json!({
            "if": {"required": ["gate"], "properties": {"gate": {"const": true}}},
            "then": {"properties": {"nested": {"properties": {"n": {"maximum": 2}}}}}
        }),
        None,
    )?;
    let overlay = json!({"gate": true, "nested": {"n": null}});
    let values_file = serde_json::to_string(&overlay)?;
    let imported = [ContextRow {
        id: "gate-null".to_string(),
        chart: "control".to_string(),
        overlay: overlay.clone(),
        transport: "values-file".to_string(),
        kubernetes_version: "1.29.0".to_string(),
        chart_sha256: tree_sha256(chart.path())?,
        baseline_sha256: sha256_hex(&schemas.baseline),
        candidate_sha256: sha256_hex(&schemas.candidate),
        canonical_overlay_sha256: sha256_hex(&serde_json::to_vec(&overlay)?),
        values_file_sha256: sha256_hex(values_file.as_bytes()),
        values_file,
        context_overlay: json!({}),
        context_overlay_sha256: sha256_hex(b"{}"),
    }];
    let screened = screen(&ScreenRequest {
        chart: chart.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &imported,
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    sim_assert_eq!(have: screened.coverage.builtin_rows, want: 1);
    sim_assert_eq!(have: screened.coverage.imported_rows, want: 1);
    sim_assert_eq!(have: screened.coverage.rows_screened, want: 2);
    assert_skipped_template_rule(&screened.coverage, 2)?;
    assert!(screened.flips.is_empty());
    // Real lint reads the same overlay and candidate, without restoring nested.n.
    assert_pinned_lint_skips_templates(chart.path(), &schemas, &overlay)?;
    Ok(())
}

#[test]
fn prometheus_operator_crds_lint_coverage_matches_pinned_rule_applicability() -> eyre::Result<()> {
    let chart = test_util::workspace_testdata().join("charts/prometheus-operator-crds");
    assert!(!chart.join("templates").exists());
    let schemas = LintSchemas::compile(&json!({}), &json!({}), None)?;
    let screened = screen(&ScreenRequest {
        chart: &chart,
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    sim_assert_eq!(have: screened.coverage.builtin_rows, want: 2);
    sim_assert_eq!(have: screened.coverage.imported_rows, want: 0);
    sim_assert_eq!(have: screened.coverage.rows_screened, want: 2);
    // The root default crds table consumes the null, then Helm refills the dependency.
    // Both input rows therefore compose template documents; only lint's rule is absent.
    assert_skipped_template_rule(&screened.coverage, 2)?;
    let inputs = test_util::helm_values::AcceptanceInputs::load(&chart)?;
    sim_assert_eq!(
        have: inputs.dependency_probes()?.into_iter().map(|probe| probe.overlay).collect::<Vec<_>>(),
        want: vec![json!({}), json!({"crds": null})]
    );
    assert!(screened.flips.is_empty());
    assert_pinned_lint_skips_templates(&chart, &schemas, &json!({}))?;
    Ok(())
}
