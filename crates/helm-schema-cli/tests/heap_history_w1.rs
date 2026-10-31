//! Compares two binaries against the frozen Helm admission witnesses.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_test_support::admission::{self, DeclaredTypes, HelmClass};
use helm_schema_test_support::registry::PROVIDER_BUNDLE;
use serde_json::json;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

struct RecordedVerdict {
    baseline: bool,
    helm: bool,
}

fn recorded_baseline(
    baseline: &Path,
) -> eyre::Result<Option<BTreeMap<(String, String), RecordedVerdict>>> {
    let Some(path) = std::env::var_os("HEAP_HISTORY_REFERENCE_REPORT") else {
        return Ok(None);
    };
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let digest = helm_schema_test_support::source_digest::sha256_hex(&std::fs::read(baseline)?);
    sim_assert_eq!(have: report.get("baseline_sha256").and_then(serde_json::Value::as_str), want: Some(digest.as_str()));
    let rows = report
        .get("rows")
        .and_then(serde_json::Value::as_array)
        .ok_or_eyre("reference rows missing")?;
    let mut result = BTreeMap::new();
    for row in rows {
        let id = row
            .get("cell_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_eyre("reference ID missing")?
            .to_owned();
        let mode = row
            .get("mode")
            .and_then(serde_json::Value::as_str)
            .ok_or_eyre("reference mode missing")?
            .to_owned();
        let baseline = row
            .get("baseline")
            .and_then(serde_json::Value::as_bool)
            .ok_or_eyre("reference baseline missing")?;
        let helm = row
            .get("helm")
            .and_then(serde_json::Value::as_bool)
            .ok_or_eyre("reference Helm verdict missing")?;
        eyre::ensure!(
            result
                .insert((id, mode), RecordedVerdict { baseline, helm })
                .is_none(),
            "duplicate reference row"
        );
    }
    sim_assert_eq!(have: result.len(), want: 1857);
    Ok(Some(result))
}

type Validators = BTreeMap<(String, DeclaredTypes, &'static str), jsonschema::Validator>;

fn build_validators(
    suite: &admission::Suite,
    root: &Path,
    bundle: &Path,
    scratch: &Path,
    baseline: &Path,
    candidate: &Path,
    has_reference: bool,
) -> eyre::Result<Validators> {
    let jobs: BTreeSet<(String, DeclaredTypes)> = suite
        .cells
        .iter()
        .map(|cell| (cell.chart.clone(), cell.declared_types))
        .collect();
    let mut validators = BTreeMap::new();
    for (index, (chart, mode)) in jobs.iter().enumerate() {
        for (label, binary) in [("baseline", baseline), ("candidate", candidate)] {
            if label == "baseline" && has_reference {
                continue;
            }
            let path = scratch.join(format!("{index}-{label}.json"));
            let arguments: Vec<_> = admission::generation_arguments(*mode)
                .into_iter()
                .map(|argument| {
                    argument
                        .replace("<chart>", &root.join(chart).to_string_lossy())
                        .replace("<bundle>", &bundle.to_string_lossy())
                        .replace("<out>", &path.to_string_lossy())
                })
                .collect();
            let output = Command::new(binary)
                .env_clear()
                .env("HOME", scratch)
                .env("XDG_CONFIG_HOME", scratch)
                .env("XDG_CACHE_HOME", scratch)
                .args(arguments)
                .output()?;
            eyre::ensure!(
                output.status.success(),
                "{chart} {mode:?} {label}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let schema = serde_json::from_slice(&std::fs::read(path)?)?;
            validators.insert(
                (chart.clone(), *mode, label),
                jsonschema::validator_for(&schema)?,
            );
        }
    }
    Ok(validators)
}

struct Comparison {
    rows: Vec<serde_json::Value>,
    changes: Vec<String>,
    new_fr: Vec<String>,
    lost_closures: Vec<String>,
    new_closures: Vec<String>,
    baseline_correct: usize,
    candidate_correct: usize,
}

fn compare_cells(
    suite: &admission::Suite,
    scratch: &Path,
    validators: &Validators,
    reference: Option<&BTreeMap<(String, String), RecordedVerdict>>,
) -> eyre::Result<Comparison> {
    let mut rows = Vec::new();
    let mut changes = Vec::new();
    let mut new_fr = Vec::new();
    let mut lost_closures = Vec::new();
    let mut new_closures = Vec::new();
    let mut baseline_correct = 0;
    let mut candidate_correct = 0;
    for cell in &suite.cells {
        let values = admission::compose(suite, cell, scratch)?;
        let base = if let Some(reference) = reference {
            let verdict = reference
                .get(&(cell.id.clone(), cell.declared_types.as_arg().to_owned()))
                .ok_or_eyre("reference verdict missing")?;
            sim_assert_eq!(have: verdict.helm, want: cell.helm == HelmClass::Pass);
            verdict.baseline
        } else {
            validators
                .get(&(cell.chart.clone(), cell.declared_types, "baseline"))
                .ok_or_eyre("baseline schema missing")?
                .is_valid(&values)
        };
        let next = validators
            .get(&(cell.chart.clone(), cell.declared_types, "candidate"))
            .ok_or_eyre("candidate schema missing")?
            .is_valid(&values);
        let helm = cell.helm == HelmClass::Pass;
        baseline_correct += usize::from(base == helm);
        candidate_correct += usize::from(next == helm);
        if base != next {
            changes.push(cell.id.clone());
            if helm && !next {
                new_fr.push(cell.id.clone());
            }
            if !helm && next {
                lost_closures.push(cell.id.clone());
            }
            if !helm && !next {
                new_closures.push(cell.id.clone());
            }
        }
        rows.push(json!({"cell_id": cell.id, "mode": cell.declared_types.as_arg(), "baseline": base, "candidate": next, "helm": helm}));
    }
    Ok(Comparison {
        rows,
        changes,
        new_fr,
        lost_closures,
        new_closures,
        baseline_correct,
        candidate_correct,
    })
}

#[test]
#[ignore = "maintenance: requires immutable baseline and candidate CLIs and an output report path"]
fn paired_frozen_w1() -> eyre::Result<()> {
    let baseline = PathBuf::from(
        std::env::var_os("HEAP_HISTORY_BASELINE").ok_or_eyre("baseline binary missing")?,
    );
    let candidate = PathBuf::from(
        std::env::var_os("HEAP_HISTORY_CANDIDATE").ok_or_eyre("candidate binary missing")?,
    );
    let report = PathBuf::from(
        std::env::var_os("HEAP_HISTORY_W1_REPORT").ok_or_eyre("report path missing")?,
    );
    let identical = std::env::var_os("HEAP_HISTORY_EXPECT_IDENTICAL").is_some();
    let reference = recorded_baseline(&baseline)?;
    let root = test_util::workspace_testdata().join(admission::SUITE_DIR);
    let bundle = test_util::workspace_testdata().join(PROVIDER_BUNDLE);
    let suite = admission::read_suite(&root)?;
    sim_assert_eq!(have: suite.cells.len(), want: 1857);
    sim_assert_eq!(have: suite.roster_mismatches(), want: Vec::<String>::new());
    sim_assert_eq!(have: suite.obligation_mismatches(), want: Vec::<String>::new());
    sim_assert_eq!(have: suite.coalesced_mismatches()?, want: Vec::<String>::new());
    let scratch = ScratchDir::new("heap-history-w1")?;
    let validators = build_validators(
        &suite,
        &root,
        &bundle,
        scratch.path(),
        &baseline,
        &candidate,
        reference.is_some(),
    )?;
    let Comparison {
        rows,
        changes,
        new_fr,
        lost_closures,
        new_closures,
        baseline_correct,
        candidate_correct,
    } = compare_cells(&suite, scratch.path(), &validators, reference.as_ref())?;
    let result = json!({
        "baseline_sha256": helm_schema_test_support::source_digest::sha256_hex(&std::fs::read(&baseline)?),
        "candidate_sha256": helm_schema_test_support::source_digest::sha256_hex(&std::fs::read(&candidate)?),
        "rows": rows, "changes": changes, "new_fr": new_fr,
        "lost_closures": lost_closures, "new_closures": new_closures,
        "baseline_correct": baseline_correct, "candidate_correct": candidate_correct});
    std::fs::write(&report, serde_json::to_vec_pretty(&result)?)?;
    eprintln!("W1 report: {}", report.display());
    sim_assert_eq!(have: new_fr, want: Vec::<String>::new());
    sim_assert_eq!(have: lost_closures, want: Vec::<String>::new());
    if identical {
        sim_assert_eq!(have: changes, want: Vec::<String>::new());
    }
    Ok(())
}
