//! Every external comparison must enforce the lint screening boundary.

use color_eyre::eyre::{self, OptionExt as _};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use test_util::scratch::ScratchDir;

fn external_chart() -> eyre::Result<ScratchDir> {
    let chart = ScratchDir::new("external-lint-completeness")?;
    fs::write(
        chart.path().join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: external-control
        version: 1.0.0
    "},
    )?;
    fs::write(chart.path().join("values.yaml"), "enabled: false\n")?;
    fs::create_dir(chart.path().join("templates"))?;
    Ok(chart)
}

fn screen_external(
    chart: &ScratchDir,
    roster: Option<&super::lint_probes::ImportedRoster>,
) -> eyre::Result<super::ScreenedChart> {
    let path = chart
        .path()
        .to_str()
        .ok_or_eyre("chart path is not UTF-8")?;
    let (screened, _) = super::screen_chart(
        super::ComparedChart {
            label: "external".into(),
            chart_relative_path: path.into(),
            inputs: super::ChartInputs::Given {
                baseline: json!({}),
                baseline_bytes: b"{}".to_vec(),
                candidate: json!({}),
                candidate_bytes: b"{}".to_vec(),
                defaults: json!({"enabled":false}),
            },
            annotation: None,
            replay: helm_schema_test_support::acceptance::Replay::default(),
        },
        false,
        roster,
    )?;
    Ok(screened)
}

fn external_comparison(
    screened: super::ScreenedChart,
) -> eyre::Result<super::AcceptanceComparison> {
    super::fold_results(
        vec![(
            (0, 0),
            super::AcceptanceResult::Chart(Ok(Box::new(screened))),
        )],
        false,
    )
}

#[test]
fn external_pair_rejects_stale_chart_context_instead_of_certifying_zero_lint_documents()
-> eyre::Result<()> {
    let chart = external_chart()?;
    let schemas = super::LintSchemas::compile(&json!({}), &json!({}), None)?;
    let row = super::lint_probes::ContextRow {
        id: "external-stale-control".into(),
        chart: "external".into(),
        overlay: json!({}),
        transport: "values-file".into(),
        kubernetes_version: "1.29.0".into(),
        chart_sha256: "stale-chart-hash".into(),
        baseline_sha256: schemas.baseline_sha256.clone(),
        candidate_sha256: helm_schema_test_support::source_digest::sha256_hex(&schemas.candidate),
        canonical_overlay_sha256: helm_schema_test_support::source_digest::sha256_hex(b"{}"),
        values_file: "{}".into(),
        values_file_sha256: helm_schema_test_support::source_digest::sha256_hex(b"{}"),
        context_overlay: json!({}),
        context_overlay_sha256: helm_schema_test_support::source_digest::sha256_hex(b"{}"),
    };
    let roster = super::lint_probes::ImportedRoster {
        sha256: "roster-control".into(),
        rows: BTreeMap::from([("external".into(), vec![row])]),
        rebind: false,
    };
    let comparison = external_comparison(screen_external(&chart, Some(&roster))?)?;
    assert!(
        super::validate_external_comparison(&comparison).is_err(),
        "external entry certified a stale-context screening failure"
    );
    Ok(())
}

#[test]
fn external_pair_rejects_exhausted_lint_budget_instead_of_certifying_zero_lint_documents()
-> eyre::Result<()> {
    let chart = external_chart()?;
    let schemas = super::LintSchemas::compile(&json!({}), &json!({}), None)?;
    let mut screened = screen_external(&chart, None)?;
    let capped = super::lint_probes::screen(&super::lint_probes::ScreenRequest {
        chart: chart.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: 0,
    })?;
    assert_ne!(capped.coverage.failures, [] as [String; 0]);
    screened.coverage.lint = Some(capped.coverage);
    let comparison = external_comparison(screened)?;
    assert!(
        super::validate_external_comparison(&comparison).is_err(),
        "external entry certified an omitted lint lane after budget exhaustion"
    );
    Ok(())
}

#[test]
fn external_pair_requires_an_actual_lint_lane_and_complete_live_outcomes() -> eyre::Result<()> {
    let chart = external_chart()?;
    let complete = external_comparison(screen_external(&chart, None)?)?;
    super::validate_external_comparison(&complete)?;
    let mut missing = external_comparison(screen_external(&chart, None)?)?;
    missing
        .coverage
        .first_mut()
        .ok_or_eyre("external chart coverage absent")?
        .lint = None;
    assert!(super::validate_external_comparison(&missing).is_err());
    let mut pending = external_comparison(screen_external(&chart, None)?)?;
    let schemas = super::LintSchemas::compile(&json!({}), &json!({"required":["missing"]}), None)?;
    let flipped = super::lint_probes::screen(&super::lint_probes::ScreenRequest {
        chart: chart.path(),
        schemas: &schemas,
        kubernetes_version: "1.29.0",
        imported: &[],
        roster_sha256: None,
        available_probes: usize::MAX,
    })?;
    assert!(!flipped.flips.is_empty());
    pending
        .coverage
        .first_mut()
        .ok_or_eyre("external chart coverage absent")?
        .lint = Some(flipped.coverage);
    assert!(super::validate_external_comparison(&pending).is_err());
    for counts in pending
        .coverage
        .first_mut()
        .and_then(|coverage| coverage.lint.as_mut())
        .ok_or_eyre("external lint coverage absent")?
        .documents
        .values_mut()
    {
        counts.adjudicated = counts.flips;
    }
    super::validate_external_comparison(&pending)?;
    Ok(())
}
