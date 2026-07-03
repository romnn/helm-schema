//! Differential harness for the Stage-B fragment interpreter: for every IR
//! corpus case, project value uses from the new `fragment_eval` domain and
//! compare them against the current pipeline's finalized `ContractIr` rows
//! at three progressively strict levels:
//!
//! - level `a`: the set of attributed values paths matches,
//! - level `b`: the `(values_path, yaml_path)` pairs of placed rows match,
//! - level `c`: full rows `(values_path, yaml_path, canonical guards)` match.
//!
//! The committed scoreboard (`fixtures/fragment_parity_scoreboard.json`)
//! pins each fixture's currently achieved level plus divergence counts and
//! samples; regressions in parity fail this test loudly, while documented
//! not-yet-parity entries stay visible instead of being papered over. Set
//! `FRAGMENT_PARITY_DUMP=1` to print the computed scoreboard (without the
//! human-written notes) when updating the fixture.

#![recursion_limit = "1024"]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ir::fragment_eval::{FragmentValueUse, document_value_uses};
use helm_schema_ir::{ContractUse, SymbolicIrContext};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

const SCOREBOARD: &str = include_str!("fixtures/fragment_parity_scoreboard.json");
const SAMPLE_LIMIT: usize = 8;

fn fixture_names_and_cases() -> Vec<(&'static str, common::IrCorpusCase<'static>)> {
    use common::cases;
    vec![
        (
            "bitnami_redis_networkpolicy",
            cases::BITNAMI_REDIS_NETWORKPOLICY,
        ),
        (
            "bitnami_redis_prometheusrule",
            cases::BITNAMI_REDIS_PROMETHEUSRULE,
        ),
        ("cert_manager_deployment", cases::CERT_MANAGER_DEPLOYMENT),
        ("cert_manager_service", cases::CERT_MANAGER_SERVICE),
        ("nats_operator_rbac", cases::NATS_OPERATOR_RBAC),
        ("nats_service", cases::NATS_SERVICE),
        ("nats_service_account", cases::NATS_SERVICE_ACCOUNT),
        (
            "signoz_postgresql_secrets",
            cases::SIGNOZ_POSTGRESQL_SECRETS,
        ),
        (
            "signoz_zookeeper_statefulset",
            cases::SIGNOZ_ZOOKEEPER_STATEFULSET,
        ),
        ("signoz_zookeeper_svc", cases::SIGNOZ_ZOOKEEPER_SVC),
        ("surveyor_configmap", cases::SURVEYOR_CONFIGMAP),
        ("surveyor_hpa", cases::SURVEYOR_HPA),
        ("surveyor_service_monitor", cases::SURVEYOR_SERVICE_MONITOR),
        (
            "zalando_postgres_operator_clusterrole",
            cases::ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE,
        ),
        (
            "zalando_postgres_operator_clusterrolebinding",
            cases::ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING,
        ),
        (
            "zalando_postgres_operator_deployment",
            cases::ZALANDO_POSTGRES_OPERATOR_DEPLOYMENT,
        ),
        (
            "zalando_postgres_operator_postgres_pod_priority_class",
            cases::ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS,
        ),
        (
            "zalando_postgres_operator_ui_ingress",
            cases::ZALANDO_POSTGRES_OPERATOR_UI_INGRESS,
        ),
    ]
}

fn old_rows(case: common::IrCorpusCase<'_>) -> Vec<ContractUse> {
    let src = test_util::read_testdata(case.template_path);
    let idx = common::build_define_index(case.define_sources);
    SymbolicIrContext::new(&idx)
        .generate_contract_ir(&src, &idx)
        .finalize()
        .document()
        .uses
}

fn new_uses(case: common::IrCorpusCase<'_>) -> Vec<FragmentValueUse> {
    let src = test_util::read_testdata(case.template_path);
    let idx = common::build_define_index(case.define_sources);
    let document = SymbolicIrContext::new(&idx).eval_document_fragment(&src);
    document_value_uses(&document)
}

/// One comparable row: values path, joined yaml path, canonical guard JSON.
fn old_row_key(row: &ContractUse) -> (String, String, String) {
    let mut canonical = row.clone();
    canonical.canonicalize();
    (
        canonical.source_expr.clone(),
        canonical.path.0.join("."),
        serde_json::to_string(&canonical.guards).expect("guards serialize"),
    )
}

fn new_row_key(row: &FragmentValueUse) -> (String, String, String) {
    let mut canonical = ContractUse::new(
        row.values_path.clone(),
        row.yaml_path.clone(),
        row.kind,
        row.guards.clone(),
        None,
    );
    canonical.canonicalize();
    (
        canonical.source_expr.clone(),
        canonical.path.0.join("."),
        serde_json::to_string(&canonical.guards).expect("guards serialize"),
    )
}

struct Comparison {
    level: &'static str,
    missing_paths: BTreeSet<String>,
    extra_paths: BTreeSet<String>,
    missing_pairs: BTreeSet<String>,
    extra_pairs: BTreeSet<String>,
    row_diffs: usize,
}

fn compare(old: &[ContractUse], new: &[FragmentValueUse]) -> Comparison {
    let old_paths: BTreeSet<String> = old
        .iter()
        .map(|row| row.source_expr.clone())
        .filter(|path| !path.is_empty())
        .collect();
    let new_paths: BTreeSet<String> = new
        .iter()
        .map(|row| row.values_path.clone())
        .filter(|path| !path.is_empty())
        .collect();

    let old_pairs: BTreeSet<String> = old
        .iter()
        .filter(|row| !row.path.0.is_empty())
        .map(|row| format!("{} @ {}", row.source_expr, row.path.0.join(".")))
        .collect();
    let new_pairs: BTreeSet<String> = new
        .iter()
        .filter(|row| !row.yaml_path.0.is_empty())
        .map(|row| format!("{} @ {}", row.values_path, row.yaml_path.0.join(".")))
        .collect();

    let old_rows: BTreeSet<(String, String, String)> = old.iter().map(old_row_key).collect();
    let new_rows: BTreeSet<(String, String, String)> = new.iter().map(new_row_key).collect();

    let paths_equal = old_paths == new_paths;
    let pairs_equal = old_pairs == new_pairs;
    let rows_equal = old_rows == new_rows;

    let level = if rows_equal && pairs_equal && paths_equal {
        "c"
    } else if pairs_equal && paths_equal {
        "b"
    } else if paths_equal {
        "a"
    } else {
        "none"
    };

    Comparison {
        level,
        missing_paths: old_paths.difference(&new_paths).cloned().collect(),
        extra_paths: new_paths.difference(&old_paths).cloned().collect(),
        missing_pairs: old_pairs.difference(&new_pairs).cloned().collect(),
        extra_pairs: new_pairs.difference(&old_pairs).cloned().collect(),
        row_diffs: old_rows.symmetric_difference(&new_rows).count(),
    }
}

fn samples(values: &BTreeSet<String>) -> Value {
    Value::Array(
        values
            .iter()
            .take(SAMPLE_LIMIT)
            .map(|value| Value::String(value.clone()))
            .collect(),
    )
}

fn computed_scoreboard() -> Value {
    let mut fixtures = BTreeMap::new();
    for (name, case) in fixture_names_and_cases() {
        let old = old_rows(case);
        let new = new_uses(case);
        let comparison = compare(&old, &new);
        fixtures.insert(
            name.to_string(),
            json!({
                "level": comparison.level,
                "missing_paths": comparison.missing_paths.len(),
                "extra_paths": comparison.extra_paths.len(),
                "missing_pairs": comparison.missing_pairs.len(),
                "extra_pairs": comparison.extra_pairs.len(),
                "row_diffs": comparison.row_diffs,
                "sample_missing_paths": samples(&comparison.missing_paths),
                "sample_extra_paths": samples(&comparison.extra_paths),
                "sample_missing_pairs": samples(&comparison.missing_pairs),
                "sample_extra_pairs": samples(&comparison.extra_pairs),
            }),
        );
    }
    json!({ "fixtures": fixtures })
}

/// The committed scoreboard with the human-written `notes` fields removed,
/// so it compares against the computed facts.
fn expected_scoreboard() -> Value {
    let mut expected: Value = serde_json::from_str(SCOREBOARD).expect("scoreboard json");
    if let Some(fixtures) = expected.get_mut("fixtures").and_then(Value::as_object_mut) {
        for entry in fixtures.values_mut() {
            if let Some(entry) = entry.as_object_mut() {
                entry.remove("notes");
            }
        }
    }
    expected
}

#[test]
fn fragment_parity_scoreboard_matches() {
    let computed = computed_scoreboard();
    if std::env::var("FRAGMENT_PARITY_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&computed).expect("pretty json")
        );
    }
    sim_assert_eq!(have: computed, want: expected_scoreboard());
}
