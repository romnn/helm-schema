//! Performance changes preserve screening decisions and isolated chart composition.

use color_eyre::eyre;
use helm_schema_test_support::acceptance::discovery::{
    DocumentJudgment, ProbeCoverage, ProbeInstance, ProbeValuesFile, ProfileSchemas,
};
use helm_schema_test_support::acceptance::screen::screen_template_probes;
use indoc::indoc;
use serde_json::json;
use test_util::prelude::sim_assert_eq;

#[test]
fn reused_validation_retains_new_rejection_reasons() -> eyre::Result<()> {
    let baseline = json!({"properties": {"old": {"type": "integer"}}});
    let candidate = json!({"properties": {"old": {"type": "integer"}, "new": {"type": "integer"}}});
    let schemas = ProfileSchemas::compile(&baseline, &candidate, json!({}))?;
    for (document, expected) in [
        (
            json!({}),
            DocumentJudgment {
                baseline_accepts: true,
                candidate_accepts: true,
                transition: false,
            },
        ),
        (
            json!({"old": false}),
            DocumentJudgment {
                baseline_accepts: false,
                candidate_accepts: false,
                transition: false,
            },
        ),
        (
            json!({"new": false}),
            DocumentJudgment {
                baseline_accepts: true,
                candidate_accepts: false,
                transition: true,
            },
        ),
        (
            json!({"old": false, "new": false}),
            DocumentJudgment {
                baseline_accepts: false,
                candidate_accepts: false,
                transition: true,
            },
        ),
    ] {
        sim_assert_eq!(have: schemas.judge_document(&document), want: expected);
    }
    Ok(())
}

#[test]
fn chart_input_reuse_preserves_null_deletion_and_fresh_dependency_state() -> eyre::Result<()> {
    let chart = test_util::scratch::ScratchDir::new("harness-performance")?;
    std::fs::write(
        chart.path().join("Chart.yaml"),
        indoc! {"
        apiVersion: v2
        name: demo
        version: 0.1.0
        dependencies:
          - name: child
            version: 0.1.0
            condition: child.enabled
    "},
    )?;
    std::fs::write(
        chart.path().join("values.yaml"),
        indoc! {"
            child: {enabled: true}
            root: 1
        "},
    )?;
    let child = chart.path().join("charts/child");
    std::fs::create_dir_all(&child)?;
    std::fs::write(
        child.join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: child
            version: 0.1.0
        "},
    )?;
    std::fs::write(
        child.join("values.yaml"),
        indoc! {"
            key: present
            nullable: null
        "},
    )?;
    let defaults = test_util::helm_values::coalesce_chart_values(chart.path(), json!({}))?;
    let documents = [
        json!({}),
        defaults.clone(),
        json!({"root": 2}),
        defaults.clone(),
    ];
    let mut expected_flips = Vec::new();
    let mut expected_unreachable = Vec::new();
    let mut probes = Vec::new();
    for (index, document) in documents.into_iter().enumerate() {
        let name = format!("probe-{index}");
        let probe = ProbeInstance::Coalesced(document);
        match probe.helm_values_file(chart.path(), &defaults)? {
            ProbeValuesFile::Reachable(_) | ProbeValuesFile::ReachableAs { .. } => {
                expected_flips.push(format!("demo: {name}: before=true, after=false"));
            }
            ProbeValuesFile::Unreachable(reason) => {
                expected_unreachable.push(json!({"probe": name, "reason": reason}));
            }
        }
        probes.push((name, probe));
    }
    let schemas = ProfileSchemas::compile(&json!(true), &json!(false), defaults.clone())?;
    let mut coverage = ProbeCoverage {
        label: "demo".into(),
        ..ProbeCoverage::default()
    };
    let result = screen_template_probes(
        chart.path(),
        &schemas,
        &defaults,
        probes,
        &mut coverage,
        false,
        None,
    )?;
    // The cached pass reaches exactly the same documents as independent fresh loads.
    sim_assert_eq!(have: result.flips, want: expected_flips);
    sim_assert_eq!(have: result.failures, want: Vec::<String>::new());
    sim_assert_eq!(have: serde_json::to_value(coverage.unreachable_probes)?, want: json!(expected_unreachable));
    Ok(())
}
