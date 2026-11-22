//! Saved evidence replay preserves oracle uncertainty and compiles with pinned Draft 7.

use std::collections::BTreeMap;
use std::path::Path;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::corpus_replay::{
    self, Counts, MissingReference, Oracle, SavedInputs,
};
use helm_schema_test_support::{manifest, registry, source_digest};
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
        include_unchanged: false,
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
        include_unchanged: false,
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
            classes: &BTreeMap::new(),
            include_unchanged: false
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
        include_unchanged: false,
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
        &json!([{"case": "demo: unseen", "evidence": "cases/chart-demo/run/missing", "rejection": "HelmAborts"}]),
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
        include_unchanged: false,
    })?;
    sim_assert_eq!(have: report.counts, want: Counts { documents: 1, fa_closed: 0, fa_reopened: 0, new_fr_candidates: 1, unchanged: 0, other_changes: 0 });
    sim_assert_eq!(have: report.unresolved_index_references, want: vec![MissingReference { index: index.display().to_string(), reference: "cases/chart-demo/run/missing".into(), case: Some("demo: unseen".into()) }]);
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
            classes: &classes,
            include_unchanged: false
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
        include_unchanged: false,
    })?;
    sim_assert_eq!(have: report.counts, want: Counts { documents: 1, fa_closed: 0, fa_reopened: 0, new_fr_candidates: 0, unchanged: 1, other_changes: 0 });
    sim_assert_eq!(have: report.distinct_documents, want: 1);
    Ok(())
}

#[test]
fn audit_ledger_retains_unchanged_verdicts_without_changing_counts() -> eyre::Result<()> {
    let root = test_util::scratch::ScratchDir::new("replay-audit-ledger")?;
    let probe = root.path().join("cases/chart-demo/run/probe-one");
    write(&probe.join("coalesced.json"), &json!({"saved": true}))?;
    write(
        &probe.join("schema-verdicts.json"),
        &json!({"helm_exit": 1}),
    )?;
    for include_unchanged in [false, true] {
        let report = corpus_replay::replay_saved(&SavedInputs {
            chart: "demo",
            baseline: &json!(true),
            candidate: &json!(true),
            evidence: root.path(),
            classes: &BTreeMap::new(),
            include_unchanged,
        })?;
        sim_assert_eq!(have: report.counts, want: Counts { documents: 1, unchanged: 1, ..Counts::default() });
        assert!(report.changes.is_empty());
        let expected = include_unchanged.then(|| json!([{
            "source": probe.join("coalesced.json").display().to_string(),
            "document_sha256": "82a210beacaa01c555bf2a7ee6f4390ebb4f3879a8e8625b19f954db24d678d6",
            "before": true,
            "after": true,
            "oracle": "Rejects",
            "classes": ["unclassified"]
        }]));
        sim_assert_eq!(have: report.observations.map(serde_json::to_value).transpose()?, want: expected);
    }
    Ok(())
}

#[test]
fn relocated_snapshots_reject_unstaged_external_references() -> eyre::Result<()> {
    let root = test_util::scratch::ScratchDir::new("isolated-replay-snapshot")?;
    let outside = test_util::scratch::ScratchDir::new("unstaged-replay-control")?;
    let control = outside.path().join("render");
    write(&control.join("coalesced.json"), &json!({"external": true}))?;
    let probe = root.path().join("cases/chart-demo/run/probe-one");
    write(&probe.join("coalesced.json"), &json!({}))?;
    write(
        &probe.join("schema-verdicts.json"),
        &json!({"controls": [{"render_evidence": control, "render_success": true, "schema_free_lint": "Pass"}]}),
    )?;
    let result = corpus_replay::replay_saved(&SavedInputs {
        chart: "demo",
        baseline: &json!(true),
        candidate: &json!(true),
        evidence: root.path(),
        classes: &BTreeMap::new(),
        include_unchanged: true,
    });
    assert!(matches!(result, Err(corpus_replay::ReplayError::Input(_))));
    Ok(())
}

fn write_partial_dump(
    directory: &Path,
    registry: &[registry::ArtifactSpec],
    keys: &[&str],
    schema: &Value,
) -> eyre::Result<()> {
    std::fs::create_dir_all(directory)?;
    let mut artifacts = Vec::new();
    let mut inputs = manifest::InputDigester::new(test_util::workspace_testdata());
    let bytes = serde_json::to_vec(schema)?;
    for spec in registry
        .iter()
        .filter(|spec| keys.contains(&spec.id.key().as_str()))
    {
        std::fs::write(directory.join(manifest::artifact_file(spec)), &bytes)?;
        let mut entry = manifest::ManifestEntry::new(spec, inputs.digest(&spec.recipe)?, &bytes);
        let [schema, definitions] =
            manifest::helm_ready_files(spec, &bytes)?.ok_or_eyre("chart companions missing")?;
        for file in [&schema, &definitions] {
            let path = directory.join(&file.file);
            std::fs::create_dir_all(path.parent().ok_or_eyre("companion parent missing")?)?;
            std::fs::write(path, &file.bytes)?;
        }
        entry.helm_ready = Some(manifest::HelmReadyFiles {
            schema: manifest::CompanionFile::new(schema.file, &schema.bytes),
            defs_map: manifest::CompanionFile::new(definitions.file, &definitions.bytes),
        });
        artifacts.push(entry);
    }
    let manifest = manifest::Manifest {
        harness_version: manifest::HARNESS_VERSION,
        build: manifest::BuildProvenance::compiled(),
        producer_binary_sha256: "0".repeat(64),
        testdata: test_util::workspace_testdata().display().to_string(),
        artifacts,
    };
    std::fs::write(
        directory.join(manifest::MANIFEST_FILE),
        serde_json::to_vec(&manifest)?,
    )?;
    Ok(())
}

#[test]
fn partial_replay_requires_the_flag_and_reports_every_skipped_key() -> eyre::Result<()> {
    let root = test_util::scratch::ScratchDir::new("partial-corpus-replay")?;
    let registry = registry::registry();
    let candidate = root.path().join("candidate");
    let baseline = root.path().join("baseline");
    write_partial_dump(
        &candidate,
        &registry,
        &["chart/mailhog", "chart/dex"],
        &json!(false),
    )?;
    write_partial_dump(&baseline, &registry, &["chart/mailhog"], &json!(true))?;
    let evidence = root.path().join("evidence");
    for chart in ["mailhog", "dex"] {
        let probe = evidence.join(format!("cases/chart-{chart}/probe"));
        write(&probe.join("coalesced.json"), &json!({"value": false}))?;
        write(
            &probe.join("schema-verdicts.json"),
            &json!({"helm_exit": 1}),
        )?;
    }
    let output = root.path().join("report");
    let command = || {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_corpus_replay"));
        command
            .arg("--candidate")
            .arg(&candidate)
            .arg("--baseline-dump")
            .arg(&baseline)
            .arg("--evidence")
            .arg(&evidence)
            .arg("--out")
            .arg(&output)
            .args(["--jobs", "2", "--include-unchanged"]);
        command
    };
    let strict = command().output()?;
    sim_assert_eq!(have: strict.status.success(), want: false);
    assert!(
        String::from_utf8_lossy(&strict.stderr)
            .contains("incomplete or duplicate producer registry")
    );
    assert!(!output.exists());

    // Dex has candidate output and evidence but no baseline artifact, so it stays skipped.
    let partial = command().arg("--allow-partial").output()?;
    sim_assert_eq!(have: partial.status.success(), want: true, "{}", String::from_utf8_lossy(&partial.stderr));
    let report: Value = serde_json::from_slice(&std::fs::read(output.join("replay.json"))?)?;
    for (field, directory) in [
        ("candidate_manifest_sha256", &candidate),
        ("baseline", &baseline),
    ] {
        let digest =
            source_digest::sha256_hex(&std::fs::read(directory.join(manifest::MANIFEST_FILE))?);
        sim_assert_eq!(have: report.get(field), want: Some(&json!(digest)));
    }
    let skipped = registry
        .iter()
        .map(|spec| spec.id.key())
        .filter(|key| key != "chart/mailhog")
        .collect::<std::collections::BTreeSet<_>>();
    sim_assert_eq!(have: report.get("skipped_keys"), want: Some(&serde_json::to_value(&skipped)?));
    sim_assert_eq!(have: report.pointer("/charts/0/chart"), want: Some(&json!("mailhog")));
    sim_assert_eq!(have: report.get("counts"), want: Some(&serde_json::to_value(Counts { documents: 1, fa_closed: 1, ..Counts::default() })?));
    sim_assert_eq!(have: report.get("charts").and_then(Value::as_array).map(Vec::len), want: Some(1));
    let markdown = std::fs::read_to_string(output.join("replay.md"))?;
    for key in skipped {
        assert!(
            markdown.contains(&format!("- `{key}`\n")),
            "missing skipped key {key}: {markdown}"
        );
    }

    // Even an unscheduled chart's present artifact must still verify.
    let dex = registry
        .iter()
        .find(|spec| spec.id.key() == "chart/dex")
        .ok_or_eyre("dex registry entry missing")?;
    std::fs::write(candidate.join(manifest::artifact_file(dex)), b"{}")?;
    let corrupt = command().arg("--allow-partial").output()?;
    sim_assert_eq!(have: corrupt.status.success(), want: false);
    assert!(
        String::from_utf8_lossy(&corrupt.stderr)
            .contains("producer recipe, inputs or bytes differ")
    );
    Ok(())
}

#[test]
fn partial_replay_rejects_disjoint_dumps() -> eyre::Result<()> {
    let root = test_util::scratch::ScratchDir::new("disjoint-corpus-replay")?;
    let registry = registry::registry();
    let candidate = root.path().join("candidate");
    let baseline = root.path().join("baseline");
    write_partial_dump(&candidate, &registry, &["chart/dex"], &json!(false))?;
    write_partial_dump(&baseline, &registry, &["chart/mailhog"], &json!(true))?;
    let evidence = root.path().join("evidence");
    std::fs::create_dir(&evidence)?;
    let output = root.path().join("report");
    let replay = std::process::Command::new(env!("CARGO_BIN_EXE_corpus_replay"))
        .arg("--candidate")
        .arg(&candidate)
        .arg("--baseline-dump")
        .arg(&baseline)
        .arg("--evidence")
        .arg(&evidence)
        .arg("--out")
        .arg(&output)
        .arg("--allow-partial")
        .output()?;

    sim_assert_eq!(have: replay.status.success(), want: false);
    let stderr = String::from_utf8_lossy(&replay.stderr);
    assert!(
        stderr.contains("nothing to replay: every artifact was skipped"),
        "{stderr}"
    );
    assert!(!output.exists());
    Ok(())
}
