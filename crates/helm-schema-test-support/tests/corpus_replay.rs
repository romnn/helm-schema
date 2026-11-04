//! Saved evidence replay preserves oracle uncertainty and compiles with pinned Draft 7.

use std::collections::BTreeMap;
use std::path::Path;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::corpus_replay::{
    self, Counts, MissingReference, Oracle, SavedInputs,
};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

fn write(path: &Path, value: &Value) -> eyre::Result<()> {
    std::fs::create_dir_all(path.parent().ok_or_eyre("no parent")?)?;
    std::fs::write(path, serde_json::to_vec(value)?)?;
    Ok(())
}

#[test]
fn draft7_local_references_and_invalid_schema_are_decisive() -> eyre::Result<()> {
    let schema =
        json!({"$defs": {"number": {"type": "integer"}}, "$ref": "#/$defs/number", "const": 7});
    let validator = corpus_replay::compile(&schema, "local ref")?;
    assert!(validator.is_valid(&json!(7)));
    assert!(!validator.is_valid(&json!("7")));
    // Draft 7 ignores the const sibling of $ref; newer drafts would reject this value.
    assert!(validator.is_valid(&json!(8)));
    assert!(corpus_replay::compile(&json!({"type": 123}), "invalid").is_err());
    Ok(())
}

#[test]
fn replay_counts_changes_controls_and_unknowns_without_coalescing() -> eyre::Result<()> {
    let root = test_util::scratch::ScratchDir::new("harness-performance")?;
    let base = root.path().join("cases/chart-demo/run");
    for (name, document, verdict) in [
        ("probe-closed", json!({"value": 0}), json!({"helm_exit": 1})),
        (
            "probe-reopened",
            json!({"value": 1}),
            json!({"helm_exit": 1}),
        ),
        ("probe-fr", json!({"value": 2}), json!({"helm_exit": 0})),
        ("probe-unknown", json!({"value": 3}), json!({})),
        ("probe-unchanged", json!({}), json!({"helm_exit": 0})),
    ] {
        write(&base.join(name).join("coalesced.json"), &document)?;
        write(&base.join(name).join("schema-verdicts.json"), &verdict)?;
    }
    let closed = base
        .join("probe-closed/coalesced.json")
        .display()
        .to_string();
    let classes = BTreeMap::from([(closed, vec!["A".into(), "A".into(), "C".into()])]);
    let before = json!({"not": {"properties": {"value": {"const": 1}}, "required": ["value"]}});
    let after = json!({"properties": {"value": {"const": 1}}});
    let inputs = SavedInputs {
        chart: "demo",
        baseline: &before,
        candidate: &after,
        evidence: root.path(),
        classes: &classes,
    };
    let report = corpus_replay::replay_saved(&inputs)?;
    sim_assert_eq!(have: report.counts, want: Counts { documents: 5, fa_closed: 1, fa_reopened: 1, new_fr_candidates: 1, unchanged: 1, other_changes: 1 });
    sim_assert_eq!(have: report.distinct_documents, want: 5);
    sim_assert_eq!(have: report.classes["A"].fa_closed, want: 2);
    sim_assert_eq!(have: report.changes.iter().filter(|row| row.oracle == Oracle::Unknown).count(), want: 1);
    sim_assert_eq!(have: serde_json::to_value(&report)?, want: serde_json::to_value(corpus_replay::replay_saved(&inputs)?)?);
    Ok(())
}

#[test]
fn saved_document_and_control_paths_are_rebased_and_deduplicated() -> eyre::Result<()> {
    let root = test_util::scratch::ScratchDir::new("harness-performance")?;
    let probe = root.path().join("cases/chart-demo/run/probe-one");
    let control = root.path().join("cases/chart-demo/run/z-control");
    write(&control.join("coalesced.json"), &json!({"valid": true}))?;
    write(&probe.join("coalesced.json"), &json!({"valid": false}))?;
    write(
        &probe.join("schema-verdicts.json"),
        &json!({
            "document": {"valid": false}, "render_evidence": "/old/preview/cases/chart-demo/run/z-control",
            "new_kubernetes_errors": ["invalid resource"],
            "controls": [{"render_evidence": "/old/preview/cases/chart-demo/run/z-control", "render_success": true, "schema_free_lint": "Pass"}]
        }),
    )?;
    std::fs::write(control.join("render.status"), "exit: 0")?;
    let before = json!(true);
    let after = json!({"properties": {"valid": {"const": true}}});
    let report = corpus_replay::replay_saved(&SavedInputs {
        chart: "demo",
        baseline: &before,
        candidate: &after,
        evidence: root.path(),
        classes: &BTreeMap::new(),
    })?;
    sim_assert_eq!(have: report.counts, want: Counts { documents: 3, fa_closed: 2, fa_reopened: 0, new_fr_candidates: 0, unchanged: 1, other_changes: 0 });
    sim_assert_eq!(have: report.distinct_documents, want: 2);
    // An absent saved document is an input failure, never a schema rejection.
    std::fs::remove_file(probe.join("coalesced.json"))?;
    write(
        &probe.join("schema-verdicts.json"),
        &json!({"helm_exit": 1}),
    )?;
    assert!(
        corpus_replay::replay_saved(&SavedInputs {
            chart: "demo",
            baseline: &before,
            candidate: &after,
            evidence: root.path(),
            classes: &BTreeMap::new()
        })
        .is_err()
    );
    Ok(())
}

#[test]
fn relative_and_legacy_control_references_are_owned_by_the_verdict_directory() -> eyre::Result<()> {
    let root = test_util::scratch::ScratchDir::new("relative-evidence")?;
    let probe = root.path().join("cases/chart-demo/run/probe-one");
    write(&probe.join("coalesced.json"), &json!({"valid": false}))?;
    write(
        &probe.join("controls/default/render/coalesced.json"),
        &json!({"valid": true}),
    )?;
    write(
        &probe.join("schema-verdicts.json"),
        &json!({
                "controls": [
            {"render_evidence": "controls/default/render", "render_success": true, "schema_free_lint": "Pass"},
            {"render_evidence": "/old/checkout/target/evidence/helm-schema/run/probe-one/controls/default/render", "render_success": true, "schema_free_lint": "Pass"}
        ]
        }),
    )?;
    write(
        &root.path().join("chart-demo-new-false-acceptances.json"),
        &json!([
            {"evidence": "/old/checkout/target/evidence/helm-schema/run/probe-one", "rejection": "HelmAborts"},
        {"evidence": "cases/chart-demo/run/probe-one", "rejection": "HelmAborts"}
        ]),
    )?;
    let report = corpus_replay::replay_saved(&SavedInputs {
        chart: "demo",
        baseline: &json!(true),
        candidate: &json!({"properties": {"valid": {"const": true}}}),
        evidence: root.path(),
        classes: &BTreeMap::new(),
    })?;
    sim_assert_eq!(have: report.counts, want: Counts { documents: 2, fa_closed: 1, fa_reopened: 0, new_fr_candidates: 0, unchanged: 1, other_changes: 0 });
    Ok(())
}

#[test]
fn dangling_index_links_are_reported_without_inventing_documents() -> eyre::Result<()> {
    let root = test_util::scratch::ScratchDir::new("missing-index-reference")?;
    let probe = root.path().join("cases/chart-demo/run/probe-one");
    write(&probe.join("coalesced.json"), &json!({}))?;
    std::fs::write(probe.join("render.status"), "0")?;
    let index = root.path().join("chart-demo-new-false-acceptances.json");
    write(
        &index,
        &json!([{"evidence": "cases/chart-demo/run/missing", "rejection": "HelmAborts"}]),
    )?;
    let before = json!(true);
    let after = json!(false);
    let mut classes = BTreeMap::new();
    let report = corpus_replay::replay_saved(&SavedInputs {
        chart: "demo",
        baseline: &before,
        candidate: &after,
        evidence: root.path(),
        classes: &classes,
    })?;
    sim_assert_eq!(have: report.counts, want: Counts { documents: 1, fa_closed: 0, fa_reopened: 0, new_fr_candidates: 1, unchanged: 0, other_changes: 0 });
    sim_assert_eq!(have: report.unresolved_index_references, want: vec![MissingReference { index: index.display().to_string(), reference: "cases/chart-demo/run/missing".into() }]);
    // Census membership is stronger evidence: omitting a declared census document fails closed.
    classes.insert(
        root.path()
            .join("cases/chart-demo/run/missing/coalesced.json")
            .display()
            .to_string(),
        vec!["A".into()],
    );
    assert!(
        corpus_replay::replay_saved(&SavedInputs {
            chart: "demo",
            baseline: &before,
            candidate: &after,
            evidence: root.path(),
            classes: &classes
        })
        .is_err()
    );
    Ok(())
}

#[test]
fn preserved_render_inputs_are_not_probe_documents() -> eyre::Result<()> {
    let root = test_util::scratch::ScratchDir::new("render-inputs-not-evidence")?;
    let probe = root.path().join("cases/chart-demo/run/probe-one");
    write(&probe.join("coalesced.json"), &json!({}))?;
    for directory in ["charts/vendor", "inputs"] {
        write(
            &probe.join(directory).join("coalesced.json"),
            &json!({"chartOwned": true}),
        )?;
        write(
            &probe.join(directory).join("schema-verdicts.json"),
            &json!({"document": {"chartOwned": true}}),
        )?;
    }
    let report = corpus_replay::replay_saved(&SavedInputs {
        chart: "demo",
        baseline: &json!(true),
        candidate: &json!(true),
        evidence: root.path(),
        classes: &BTreeMap::new(),
    })?;
    sim_assert_eq!(have: report.counts, want: Counts { documents: 1, fa_closed: 0, fa_reopened: 0, new_fr_candidates: 0, unchanged: 1, other_changes: 0 });
    sim_assert_eq!(have: report.distinct_documents, want: 1);
    Ok(())
}
