//! Monotonicity and semantic-oracle harness for schema emission profiles.

use color_eyre::eyre::{self, WrapErr as _};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

#[path = "common/emission_profile_harness.rs"]
mod harness;

use harness::{
    ContractVerdict, ControlCategory, ProbeInstance, ProfileSchemas, SemanticControl, Transport,
    generate_profile_outputs, generate_profile_schemas, read_chart_schema_fixture,
    read_json_fixture, read_root_defaults, sparse_override, structural_probe_battery,
};

#[test]
fn current_profiles_obey_monotonicity_and_semantic_controls() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let defaults = read_root_defaults("schema-emission-controls")?;
    let (full, lean) = generate_profile_schemas("schema-emission-controls")?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults.clone())?;

    let controls = semantic_controls();

    profiles.assert_controls(&controls)?;
    let mut probes = structural_probe_battery(&defaults);
    probes.extend(controls.iter().map(|control| {
        (
            format!("semantic control: {}", control.name),
            control.instance.clone(),
        )
    }));
    profiles.assert_monotone(
        probes
            .iter()
            .map(|(name, instance)| (name.as_str(), instance)),
    )?;

    Ok(())
}

#[test]
fn legacy_lean_reports_step_2_projection_differences() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let mut have = Vec::new();
    for chart in [
        "schema-emission-controls",
        "schema-emission-local-kind",
        "schema-emission-temporal-wrapper",
    ] {
        let (full, lean) = generate_profile_outputs(chart)?;
        eyre::ensure!(
            full.emission_report.selection_differences.is_empty(),
            "full has a legacy/projection disagreement for {chart}"
        );
        eyre::ensure!(
            lean.emission_report
                .selection_differences
                .iter()
                .all(|difference| difference.direction
                    == helm_schema::generation::SelectionDifferenceDirection::ProjectionOnly),
            "legacy lean retains a fact that the decision-table projection drops for {chart}"
        );
        have.push((
            chart,
            lean.emission_report.selection_differences.len(),
            lean.emission_report.selection_differences_sha256(),
        ));
    }
    sim_assert_eq!(
        have: have,
        want: vec![
            (
                "schema-emission-controls",
                9,
                "3594d70cd4304790641f1d5ee12a157da54a9215846bb181260f4f79b6f271e7"
                    .to_string(),
            ),
            (
                "schema-emission-local-kind",
                4,
                "5b51fa618edae0059e8a9dfac20091d7228a4a6b76af01912ce2c6cfa27dd255"
                    .to_string(),
            ),
            (
                "schema-emission-temporal-wrapper",
                6_859,
                "602e49d724de9477fb87081089e29a49e6e28b0b22474dd4ca5e6b93d85c38ae"
                    .to_string(),
            ),
        ]
    );
    Ok(())
}

#[test]
fn unconditional_fail_is_an_independent_terminal_tooth() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let chart = "schema-emission-unconditional-fail";
    let defaults = read_root_defaults(chart)?;
    let (full, lean) = generate_profile_outputs(chart)?;
    let fixture = read_chart_schema_fixture(chart)?;

    sim_assert_eq!(have: &full.schema, want: &fixture);
    let full_validator = jsonschema::validator_for(&full.schema)?;
    for instance in [
        serde_json::Value::Null,
        json!(false),
        json!(0),
        json!("value"),
        json!([]),
        json!({}),
        json!({ "enabled": true }),
    ] {
        eyre::ensure!(
            !full_validator.is_valid(&instance),
            "full accepted {instance} despite the unconditional terminal"
        );
    }
    eyre::ensure!(
        full.emission_report.facts.lowered == 1 && full.emission_report.facts.selected == 1,
        "full must report one unconditional terminal fact"
    );
    eyre::ensure!(
        lean.emission_report.facts.lowered == 1 && lean.emission_report.facts.selected == 0,
        "legacy lean must drop the unconditional terminal fact"
    );

    let profiles = ProfileSchemas::compile(&full.schema, &lean.schema, defaults)?;
    let controls = [SemanticControl {
        name: "unconditional fail",
        category: ControlCategory::RemovedTooth,
        instance: ProbeInstance::Defaults,
        transport: Transport::ValuesFileJson,
        contract: ContractVerdict::Reject("the chart always aborts rendering"),
        lean_accepts: true,
        rationale: "terminal-clauses off soundly removes an always-false constraint",
    }];
    profiles.assert_controls(&controls)?;
    profiles.assert_monotone(
        controls
            .iter()
            .map(|control| (control.name, &control.instance)),
    )?;
    Ok(())
}

fn semantic_controls() -> Vec<SemanticControl> {
    let mut controls = provider_controls();
    controls.extend(conditional_controls());
    controls.extend(partition_controls());
    controls
}

fn provider_controls() -> Vec<SemanticControl> {
    use ContractVerdict::{Accept, Reject};
    use ControlCategory::{PositiveControl, RetainedTooth};
    use Transport::{Set, SetString, ValuesFileJson};

    vec![
        control(
            "valid composed defaults",
            PositiveControl,
            ProbeInstance::Defaults,
            ValuesFileJson,
            Accept,
            true,
            "renderable defaults remain inside the lint floor",
        ),
        control(
            "provider-backed replica integer",
            RetainedTooth,
            sparse_override(&["replicas"], json!(3)),
            Set,
            Accept,
            true,
            "an integer reaches the Deployment replica sink",
        ),
        control(
            "coercible replica spelling",
            RetainedTooth,
            sparse_override(&["replicas"], json!("3")),
            SetString,
            Accept,
            true,
            "an unquoted numeric string renders as an integer token",
        ),
        control(
            "non-coercible provider replica",
            RetainedTooth,
            sparse_override(&["replicas"], json!("three")),
            ValuesFileJson,
            Reject("rendered Deployment replicas is not an integer"),
            false,
            "unconditional provider typing survives every profile",
        ),
    ]
}

fn conditional_controls() -> Vec<SemanticControl> {
    use ContractVerdict::{Accept, Reject};
    use ControlCategory::{PositiveControl, RemovedTooth};
    use Transport::ValuesFileJson;

    vec![
        control(
            "required value deletion",
            RemovedTooth,
            sparse_override(&["requiredText"], json!(null)),
            ValuesFileJson,
            Reject("required aborts template rendering"),
            true,
            "today's lean profile intentionally drops terminal clauses",
        ),
        control(
            "version pattern near miss",
            RemovedTooth,
            sparse_override(&["version"], json!("v1")),
            ValuesFileJson,
            Reject("the chart rejects a non-matching version"),
            true,
            "the pattern is guarded by a terminal clause removed from lean",
        ),
        control(
            "nil-safe object host deletion",
            PositiveControl,
            sparse_override(&["host"], json!(null)),
            ValuesFileJson,
            Accept,
            true,
            "a dropped conditional must not retain a stricter object host",
        ),
        control(
            "disabled dependency ignores a wrong replica kind",
            PositiveControl,
            ProbeInstance::SparseOverride(
                json!({ "worker": { "enabled": false, "replicas": "three" } }),
            ),
            ValuesFileJson,
            Accept,
            true,
            "the dependency does not render while its condition is false",
        ),
        control(
            "enabled dependency rejects a wrong replica kind",
            RemovedTooth,
            ProbeInstance::SparseOverride(
                json!({ "worker": { "enabled": true, "replicas": "three" } }),
            ),
            ValuesFileJson,
            Reject("the enabled worker renders invalid replicas"),
            true,
            "today's lean profile drops dependency-gated provider typing",
        ),
    ]
}

fn partition_controls() -> Vec<SemanticControl> {
    use ContractVerdict::{Accept, Reject, Unresolved};
    use ControlCategory::{PositiveControl, RemovedTooth};
    use Transport::ValuesFileJson;

    vec![
        control(
            "ConfigMap branch rejects a Service spelling",
            RemovedTooth,
            ProbeInstance::SparseOverride(
                json!({ "local": { "kind": "ConfigMap", "setting": "ClusterIP" } }),
            ),
            ValuesFileJson,
            Reject("ConfigMap immutable must be a boolean"),
            true,
            "lean deletes local kind-partition refinements",
        ),
        control(
            "Service branch accepts its own spelling",
            PositiveControl,
            ProbeInstance::SparseOverride(
                json!({ "local": { "kind": "Service", "setting": "ClusterIP" } }),
            ),
            ValuesFileJson,
            Accept,
            true,
            "the adjacent kind branch remains renderable",
        ),
        control(
            "unknown dynamic provider kind",
            PositiveControl,
            sparse_override(&["dynamic", "kind"], json!("FutureKind")),
            ValuesFileJson,
            Unresolved("the provider has no schema for FutureKind"),
            true,
            "provider uncertainty must not become a rejection",
        ),
    ]
}

fn control(
    name: &'static str,
    category: ControlCategory,
    instance: ProbeInstance,
    transport: Transport,
    contract: ContractVerdict,
    lean_accepts: bool,
    rationale: &'static str,
) -> SemanticControl {
    SemanticControl {
        name,
        category,
        instance,
        transport,
        contract,
        lean_accepts,
        rationale,
    }
}

#[test]
fn lean_profile_keeps_nil_safe_host_relaxation() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let defaults = read_root_defaults("schema-emission-controls")?;
    let (full, lean) = generate_profile_schemas("schema-emission-controls")?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults)?;
    let probe = sparse_override(&["host"], json!(null));

    let (full_accepts, lean_accepts) = profiles.verdicts(&probe);
    eyre::ensure!(full_accepts, "full must accept the nil-safe host deletion");
    eyre::ensure!(
        lean_accepts,
        "lean retained a strict object host after dropping its conditional arm"
    );
    Ok(())
}

#[test]
fn temporal_wrapper_pairwise_matrix_is_monotone() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let chart = "schema-emission-temporal-wrapper";
    let defaults = read_json_fixture(chart, "coalesced-defaults.json")?;
    let (full, lean) = generate_profile_schemas(chart)?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults)?;

    let replica_values = [
        json!(null),
        json!(0),
        json!(1),
        json!(1.5),
        json!("3"),
        json!("three"),
        json!([]),
        json!({}),
    ];
    let label_values = [
        json!(null),
        json!({}),
        json!({ "app": "temporal" }),
        json!({ "app": 3 }),
        json!([]),
    ];
    let mut probes = vec![("defaults".to_string(), ProbeInstance::Defaults)];
    for replica in replica_values {
        for labels in &label_values {
            probes.push((
                format!("replica={replica}, podLabels={labels}"),
                ProbeInstance::SparseOverride(json!({
                    "temporal": {
                        "server": {
                            "replicaCount": replica,
                            "podLabels": labels,
                        }
                    }
                })),
            ));
        }
    }

    profiles.assert_monotone(
        probes
            .iter()
            .map(|(name, instance)| (name.as_str(), instance)),
    )?;
    profiles.assert_controls(&[
        SemanticControl {
            name: "temporal defaults",
            category: ControlCategory::PositiveControl,
            instance: ProbeInstance::Defaults,
            transport: Transport::ValuesFileJson,
            contract: ContractVerdict::Accept,
            lean_accepts: true,
            rationale: "the pinned wrapper defaults render and must satisfy both profiles",
        },
        SemanticControl {
            name: "temporal replicaCount non-coercible spelling",
            category: ControlCategory::RemovedTooth,
            instance: sparse_override(&["temporal", "server", "replicaCount"], json!("three")),
            transport: Transport::ValuesFileJson,
            contract: ContractVerdict::Reject("Deployment replicas is not an integer"),
            lean_accepts: true,
            rationale: "today's lean profile drops this dependency-local provider refinement",
        },
    ])?;
    Ok(())
}

#[test]
fn local_kind_partition_is_a_local_policy_fact() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let chart = "schema-emission-local-kind";
    let defaults = read_root_defaults(chart)?;
    let (full, lean) = generate_profile_schemas(chart)?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults)?;
    let controls = [
        SemanticControl {
            name: "Deployment accepts Deployment strategy",
            category: ControlCategory::PositiveControl,
            instance: ProbeInstance::Defaults,
            transport: Transport::ValuesFileJson,
            contract: ContractVerdict::Accept,
            lean_accepts: true,
            rationale: "the selected provider arm owns the Deployment strategy shape",
        },
        SemanticControl {
            name: "Deployment rejects StatefulSet strategy",
            category: ControlCategory::RemovedTooth,
            instance: ProbeInstance::SparseOverride(json!({
                "workload": {
                    "kind": "Deployment",
                    "strategy": { "rollingUpdate": { "partition": 1 } },
                }
            })),
            transport: Transport::ValuesFileJson,
            contract: ContractVerdict::Reject(
                "Deployment strategy has no rollingUpdate.partition member",
            ),
            lean_accepts: true,
            rationale: "today's lean profile drops every conditional partition",
        },
        SemanticControl {
            name: "StatefulSet accepts StatefulSet strategy",
            category: ControlCategory::PositiveControl,
            instance: ProbeInstance::SparseOverride(json!({
                "workload": {
                    "kind": "StatefulSet",
                    "strategy": { "rollingUpdate": { "partition": 1 } },
                }
            })),
            transport: Transport::ValuesFileJson,
            contract: ContractVerdict::Accept,
            lean_accepts: true,
            rationale: "the adjacent local partition remains renderable",
        },
    ];

    profiles.assert_controls(&controls)?;
    profiles.assert_monotone(
        controls
            .iter()
            .map(|control| (control.name, &control.instance)),
    )?;
    Ok(())
}

#[test]
#[ignore = "maintenance: requires SCHEMA_ACCEPTANCE_BASELINE_REF"]
fn early_provider_definition_pruning_is_acceptance_equivalent() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let baseline_ref = std::env::var("SCHEMA_ACCEPTANCE_BASELINE_REF")
        .wrap_err("SCHEMA_ACCEPTANCE_BASELINE_REF must name the pre-prune commit")?;
    let fixture_dir = test_util::workspace_testdata().join("chart-corpus-schemas");
    let mut fixture_paths = std::fs::read_dir(&fixture_dir)
        .wrap_err_with(|| format!("read {}", fixture_dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    fixture_paths.sort();

    let mut probes_checked = 0;
    let mut flips = Vec::new();
    let mut charts_checked = 0;
    for fixture_path in fixture_paths {
        let Some(filename) = fixture_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(chart) = filename.strip_suffix(".schema.json") else {
            continue;
        };
        let relative_path = format!("testdata/chart-corpus-schemas/{filename}");
        let output = std::process::Command::new("git")
            .args(["show", &format!("{baseline_ref}:{relative_path}")])
            .output()
            .wrap_err_with(|| format!("read {relative_path} from {baseline_ref}"))?;
        eyre::ensure!(
            output.status.success(),
            "git show failed for {baseline_ref}:{relative_path}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let baseline: serde_json::Value = serde_json::from_slice(&output.stdout)
            .wrap_err_with(|| format!("parse {baseline_ref}:{relative_path}"))?;
        let current: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&fixture_path)
                .wrap_err_with(|| format!("read {}", fixture_path.display()))?,
        )
        .wrap_err_with(|| format!("parse {}", fixture_path.display()))?;
        let defaults = read_root_defaults(chart)?;
        let profiles = ProfileSchemas::compile(&baseline, &current, defaults.clone())?;
        for (probe_name, probe) in structural_probe_battery(&defaults) {
            probes_checked += 1;
            let (before, after) = profiles.verdicts(&probe);
            if before != after {
                flips.push(format!(
                    "{chart}: {probe_name}: before={before}, after={after}"
                ));
            }
        }
        charts_checked += 1;
    }

    eyre::ensure!(
        flips.is_empty(),
        "early provider-definition pruning changed acceptance:\n{}",
        flips.join("\n")
    );
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=0");
    Ok(())
}
