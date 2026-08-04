//! Monotonicity and semantic-oracle harness for schema emission profiles.

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use serde::{Deserialize, Serialize};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

#[path = "common/emission_profile_harness.rs"]
mod harness;

use harness::{
    ContractVerdict, ControlCategory, ProbeCoverage, ProbeInstance, ProfileSchemas,
    SemanticControl, Transport, generate_profile_outputs, generate_profile_schemas,
    read_chart_schema_fixture, read_json_fixture, read_root_defaults, sparse_override,
    structural_probe_battery, structural_probe_battery_with_coverage,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
struct ProbeCoverageReport {
    baseline_ref: String,
    charts: Vec<ProbeCoverage>,
}

#[derive(Default)]
struct AcceptanceComparison {
    probes_checked: usize,
    flips: Vec<String>,
    coverage: Vec<ProbeCoverage>,
}

const LEAN_FIXTURE_CHARTS: &[&str] = &[
    "schema-emission-controls",
    "schema-emission-local-kind",
    "schema-emission-temporal-wrapper",
    "schema-emission-unconditional-fail",
];

#[test]
fn lean_profile_schemas_match_their_separate_fixture_lane() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let dump_chart = std::env::var("SCHEMA_DUMP_CHART").ok();
    for chart in LEAN_FIXTURE_CHARTS {
        if dump_chart
            .as_deref()
            .is_some_and(|selected| selected != *chart)
        {
            continue;
        }
        let (_, lean) = generate_profile_schemas(chart)?;
        let fixture_path = test_util::workspace_testdata()
            .join("emission-profile-schemas/lean")
            .join(format!("{chart}.schema.json"));
        if std::env::var("SCHEMA_DUMP").is_ok() {
            let dump_path = std::env::temp_dir().join(format!(
                "helm-schema.emission-profile.lean.{chart}.schema.json"
            ));
            let mut bytes = serde_json::to_vec_pretty(&lean).wrap_err("serialize lean schema")?;
            bytes.push(b'\n');
            std::fs::write(&dump_path, bytes)
                .wrap_err_with(|| format!("write {}", dump_path.display()))?;
            continue;
        }
        let expected: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&fixture_path)
                .wrap_err_with(|| format!("read {}", fixture_path.display()))?,
        )
        .wrap_err_with(|| format!("parse {}", fixture_path.display()))?;
        sim_assert_eq!(
            have: lean,
            want: expected,
            "{chart}: lean profile fixture mismatch"
        );
    }
    Ok(())
}

#[test]
fn current_profiles_obey_monotonicity_and_semantic_controls() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let defaults = read_root_defaults("schema-emission-controls")?;
    let (full, lean) = generate_profile_schemas("schema-emission-controls")?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults.clone())?;

    let controls = semantic_controls();

    profiles.assert_controls(&controls)?;
    let mut probes =
        structural_probe_battery("schema-emission-controls", &defaults, &[&full, &lean])?;
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
fn lean_profile_obeys_the_middle_point_fact_floor() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    for chart in [
        "schema-emission-controls",
        "schema-emission-local-kind",
        "schema-emission-temporal-wrapper",
    ] {
        let (full, lean) = generate_profile_outputs(chart)?;
        eyre::ensure!(
            full.emission_report.facts.selected == full.emission_report.facts.lowered,
            "full drops a fact for {chart}"
        );
        for class in [
            helm_schema::generation::EmissionClassKind::OrdinaryRoot,
            helm_schema::generation::EmissionClassKind::KindPartitionRoot,
            helm_schema::generation::EmissionClassKind::KindPartitionLocal,
            helm_schema::generation::EmissionClassKind::TerminalAlways,
            helm_schema::generation::EmissionClassKind::TerminalGuarded,
        ] {
            eyre::ensure!(
                lean.emission_report.counts_for_class(class).selected == 0,
                "lean selects {class:?} facts for {chart}"
            );
        }
        let mandatory = lean
            .emission_report
            .counts_for_class(helm_schema::generation::EmissionClassKind::Mandatory);
        eyre::ensure!(
            mandatory.dropped == 0,
            "lean drops mandatory facts for {chart}"
        );
        let local = lean
            .emission_report
            .counts_for_class(helm_schema::generation::EmissionClassKind::OrdinaryLocal);
        eyre::ensure!(
            local.selected == local.lowered,
            "lean drops local conditional facts for {chart}"
        );
    }
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
    use ControlCategory::{PositiveControl, RemovedTooth, RetainedTooth};
    use Transport::ValuesFileJson;

    vec![
        control(
            "required value deletion",
            RetainedTooth,
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
            false,
            "the unguarded pattern is mandatory and survives every profile",
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
            RetainedTooth,
            ProbeInstance::SparseOverride(
                json!({ "worker": { "enabled": true, "replicas": "three" } }),
            ),
            ValuesFileJson,
            Reject("the enabled worker renders invalid replicas"),
            false,
            "the middle-point lean contract retains dependency-local provider typing",
        ),
    ]
}

fn partition_controls() -> Vec<SemanticControl> {
    use ContractVerdict::{Accept, Reject};
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
            Accept,
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
            category: ControlCategory::RetainedTooth,
            instance: sparse_override(&["temporal", "server", "replicaCount"], json!("three")),
            transport: Transport::ValuesFileJson,
            contract: ContractVerdict::Reject("Deployment replicas is not an integer"),
            lean_accepts: false,
            rationale: "the middle-point lean contract retains dependency-local provider typing",
        },
    ])?;
    Ok(())
}

#[test]
fn structural_battery_preserves_helm_v4_dependency_roots() -> eyre::Result<()> {
    let chart = "schema-emission-temporal-wrapper";
    let defaults = read_json_fixture(chart, "coalesced-defaults.json")?;
    let probes = structural_probe_battery(chart, &defaults, &[])?;

    let retained = probes
        .iter()
        .find_map(|(name, probe)| (name == "all declared keys deleted").then_some(probe));
    let Some(ProbeInstance::Coalesced(retained)) = retained else {
        return Err(eyre::eyre!("all-declared-keys probe missing"));
    };
    assert!(
        retained.get("temporal").is_some(),
        "Helm v4 refills the dependency root after a parent null deletion"
    );
    assert!(probes.iter().all(|(_, probe)| {
        !matches!(
            probe,
            ProbeInstance::SparseOverride(value)
                if value.get("temporal").is_some_and(serde_json::Value::is_null)
        )
    }));
    Ok(())
}

#[test]
fn structural_battery_preserves_unlisted_vendored_dependency_roots() -> eyre::Result<()> {
    let chart = "schema-emission-unlisted-dependency";
    let defaults = json!({ "vendored": { "enabled": true } });
    let probes = structural_probe_battery(chart, &defaults, &[])?;

    let retained = probes
        .iter()
        .find_map(|(name, probe)| (name == "all declared keys deleted").then_some(probe));
    let Some(ProbeInstance::Coalesced(retained)) = retained else {
        return Err(eyre::eyre!("all-declared-keys probe missing"));
    };
    sim_assert_eq!(
        have: retained.get("vendored"),
        want: Some(&json!({ "enabled": true }))
    );
    sim_assert_eq!(
        have: probes.iter().any(|(_, probe)| matches!(
            probe,
            ProbeInstance::SparseOverride(value)
                if value.get("vendored").is_some_and(serde_json::Value::is_null)
        )),
        want: false
    );
    Ok(())
}

#[test]
fn structural_battery_samples_depth_three_and_guard_states() -> eyre::Result<()> {
    let chart = "schema-emission-controls";
    let defaults = read_root_defaults(chart)?;
    let (full, lean) = generate_profile_schemas(chart)?;
    let (probes, coverage) =
        structural_probe_battery_with_coverage(chart, &defaults, &[&full, &lean])?;

    eyre::ensure!(
        probes
            .iter()
            .any(|(name, _)| name == "host.nested.value <- null deletion [depth 3]"),
        "the depth-three deletion lane did not reach host.nested.value"
    );
    eyre::ensure!(
        probes
            .iter()
            .any(|(name, _)| name.starts_with("root guard ")
                && name.contains(" satisfied [targeted:")),
        "the guard-state lane produced no satisfying witness"
    );
    eyre::ensure!(
        probes
            .iter()
            .any(|(name, _)| name.starts_with("root guard ")
                && name.contains(" violated [targeted:")),
        "the guard-state lane produced no violating witness"
    );
    eyre::ensure!(
        coverage.guard_pairs_emitted > 0
            && coverage.guards_discovered
                == coverage.guards_attempted + coverage.guards_skipped_by_cap,
        "guard coverage accounting is incomplete: {coverage:?}"
    );
    Ok(())
}

#[test]
fn guard_battery_synthesizes_composite_guard_and_payload_states() -> eyre::Result<()> {
    let defaults = json!({ "enabled": false, "payload": "valid" });
    let schema = json!({
        "allOf": [{
            "if": {
                "properties": { "enabled": { "const": true } },
                "required": ["enabled"],
                "type": "object",
            },
            "then": {
                "properties": { "payload": { "type": "string" } },
                "required": ["payload"],
                "type": "object",
            },
        }],
    });
    let mut coverage = harness::ProbeCoverage::default();

    let probes = harness::guard_state_probes(&defaults, &[&schema], &mut coverage)?;

    sim_assert_eq!(have: coverage.guard_pairs_emitted, want: 1);
    sim_assert_eq!(have: coverage.composite_pairs_emitted, want: 1);
    eyre::ensure!(
        probes
            .iter()
            .filter(|(name, _)| name.contains("[composite "))
            .count()
            == 2,
        "composite witnesses were not emitted as a guard-state pair: {probes:?}"
    );
    Ok(())
}

#[test]
fn archive_dependency_depth_ignores_a_leading_current_directory_component() {
    sim_assert_eq!(
        have: harness::archive_entry_depth(std::path::Path::new("./vendored/Chart.yaml")),
        want: 2
    );
    sim_assert_eq!(
        have: harness::archive_entry_depth(std::path::Path::new("vendored/Chart.yaml")),
        want: 2
    );
}

#[test]
fn ordinary_kind_partition_evidence_keeps_the_complete_range_domain() -> eyre::Result<()> {
    let chart = "schema-emission-kind-range";
    let defaults = read_root_defaults(chart)?;
    let (full, lean) = generate_profile_schemas(chart)?;
    let profiles = ProfileSchemas::compile(&full, &lean, defaults)?;

    for (name, probe) in [
        (
            "integer range",
            sparse_override(&["entries"], serde_json::json!(2)),
        ),
        (
            "map range",
            sparse_override(&["entries"], serde_json::json!({ "configured": "value" })),
        ),
    ] {
        let (full_accepts, lean_accepts) = profiles.verdicts(&probe);
        eyre::ensure!(
            full_accepts && lean_accepts,
            "{name} must survive both the selected and ordinary projections: full={full_accepts}, lean={lean_accepts}"
        );
    }
    Ok(())
}

#[test]
#[ignore = "maintenance: records Step 2 Temporal policy measurements"]
fn temporal_middle_policy_measurements() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let full_session = harness::profile_session(
        "schema-emission-temporal-wrapper",
        helm_schema::generation::SchemaProfile::Full,
        false,
    );
    let lean_session = harness::profile_session(
        "schema-emission-temporal-wrapper",
        helm_schema::generation::SchemaProfile::Lean,
        false,
    );
    let full = full_session.generated_schema()?;
    let lean = lean_session.generated_schema()?;
    let emit_request = helm_schema::output::EmitRequest {
        reference_policy: helm_schema::output::ReferencePolicy::SelfContained,
        output: helm_schema::output::OutputPipelineOptions {
            strip_descriptions: false,
            minimize: true,
        },
    };
    let full_schema = full_session.emit(emit_request)?;
    let lean_schema = lean_session.emit(emit_request)?;
    let mut full_bytes = Vec::new();
    let full_metrics = helm_schema::output::write_schema_json(
        &mut full_bytes,
        &full_schema,
        helm_schema::output::JsonOutputFormat::Compact,
    )?;
    let mut lean_bytes = Vec::new();
    let lean_metrics = helm_schema::output::write_schema_json(
        &mut lean_bytes,
        &lean_schema,
        helm_schema::output::JsonOutputFormat::Compact,
    )?;
    let lean_budget_limit = 9 * 1024 * 1024 / 2;
    eyre::ensure!(
        lean_metrics.serialized_bytes < lean_budget_limit,
        "Temporal lean output is {} bytes, over the {lean_budget_limit}-byte budget",
        lean_metrics.serialized_bytes,
    );
    eprintln!(
        "full={full_metrics:?} lean={lean_metrics:?} lean_budget_bytes={} lean_budget_limit={}",
        lean_metrics.serialized_bytes, lean_budget_limit,
    );
    for class in [
        helm_schema::generation::EmissionClassKind::Mandatory,
        helm_schema::generation::EmissionClassKind::OrdinaryRoot,
        helm_schema::generation::EmissionClassKind::OrdinaryLocal,
        helm_schema::generation::EmissionClassKind::KindPartitionRoot,
        helm_schema::generation::EmissionClassKind::KindPartitionLocal,
        helm_schema::generation::EmissionClassKind::TerminalAlways,
        helm_schema::generation::EmissionClassKind::TerminalGuarded,
    ] {
        let counts = full.emission_report.counts_for_class(class);
        let lean_counts = lean.emission_report.counts_for_class(class);
        eprintln!(
            "class={class:?} lowered={} full_selected={} lean_selected={} delta={}",
            counts.lowered,
            counts.selected,
            lean_counts.selected,
            counts.selected - lean_counts.selected
        );
    }
    eprintln!(
        "full_canonical={:?} full_mandatory={:?} lean_canonical={:?} lean_mandatory={:?}",
        full.emission_report.canonicalization,
        full.emission_report.mandatory_outcomes,
        lean.emission_report.canonicalization,
        lean.emission_report.mandatory_outcomes,
    );
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
#[ignore = "maintenance: compares current full and lean fixtures with a baseline ref"]
fn early_provider_definition_pruning_is_acceptance_equivalent() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let (charts_checked, probes_checked, flips, _) = corpus_acceptance_flips()?;
    eyre::ensure!(
        flips.is_empty(),
        "step 3 changed fixture acceptance:\n{}",
        flips.join("\n")
    );
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=0");
    Ok(())
}

#[test]
#[ignore = "maintenance: compares the Round 68 dump with its baseline ref"]
fn round68_fixture_flips_match_the_helm_adjudicated_list() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let (charts_checked, probes_checked, flips, _) = corpus_acceptance_flips()?;
    let expected = vec![
        "airflow: fullnameOverride <- false: before=false, after=true",
        "airflow: fullnameOverride <- true: before=false, after=true",
        "airflow: fullnameOverride <- integer: before=false, after=true",
        "airflow: fullnameOverride <- number: before=false, after=true",
        "airflow: fullnameOverride <- empty array: before=false, after=true",
        "airflow: fullnameOverride <- empty object item: before=false, after=true",
        "airflow: fullnameOverride <- empty object: before=false, after=true",
        "airflow: fullnameOverride <- unknown object member: before=false, after=true",
        "metallb: fullnameOverride <- false: before=false, after=true",
        "metallb: fullnameOverride <- integer: before=false, after=true",
        "metallb: fullnameOverride <- empty array: before=false, after=true",
        "metallb: fullnameOverride <- empty object: before=false, after=true",
        "traefik: namespaceOverride <- false: before=false, after=true",
        "traefik: namespaceOverride <- integer: before=false, after=true",
        "traefik: namespaceOverride <- empty array: before=false, after=true",
        "traefik: namespaceOverride <- empty object: before=false, after=true",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    sim_assert_eq!(have: flips, want: expected);
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=16");
    Ok(())
}

#[test]
#[ignore = "maintenance: compares the Round 69 dump with its baseline ref"]
fn round69_override_bundling_is_corpus_acceptance_equivalent() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let (charts_checked, probes_checked, flips, _) = corpus_acceptance_flips()?;
    eyre::ensure!(
        flips.is_empty(),
        "round 69 changed fixture acceptance:\n{}",
        flips.join("\n")
    );
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=0");
    Ok(())
}

#[test]
#[ignore = "maintenance: compares the Round 70 dump with its baseline ref"]
fn round70_partition_and_canonicalization_changes_are_acceptance_equivalent() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let (charts_checked, probes_checked, flips, _) = corpus_acceptance_flips()?;
    eyre::ensure!(
        flips.is_empty(),
        "round 70 changed fixture acceptance:\n{}",
        flips.join("\n")
    );
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=0");
    Ok(())
}

#[test]
#[ignore = "maintenance: retro-adjudicates the Round 70 oauth2-proxy schema change"]
fn round70_oauth2_proxy_tpl_change_kept_the_eager_string_tooth() -> eyre::Result<()> {
    let baseline = read_schema_at_ref(
        "34e58cc",
        "testdata/chart-corpus-schemas/oauth2-proxy.schema.json",
    )?;
    let current = read_chart_schema_fixture("oauth2-proxy")?;
    let defaults = read_root_defaults("oauth2-proxy")?;
    let transition = ProfileSchemas::compile(&baseline, &current, defaults)?;

    for (name, probe) in [
        (
            "selected non-string tpl fallback",
            sparse_override(&["global", "imageRegistry"], serde_json::Value::from(7)),
        ),
        (
            "eager non-string tpl fallback behind a live primary",
            ProbeInstance::SparseOverride(json!({
                "image": { "registry": "quay.io" },
                "global": { "imageRegistry": 7 },
            })),
        ),
    ] {
        sim_assert_eq!(
            have: transition.verdicts(&probe),
            want: (false, false),
            "{name} must remain rejected by the unconditional tpl program contract"
        );
    }
    Ok(())
}

#[test]
#[ignore = "maintenance: compares the Round 72 dump with its baseline ref"]
fn round72_pipeline_changes_are_acceptance_equivalent() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let (charts_checked, probes_checked, flips, _) = corpus_acceptance_flips()?;
    eyre::ensure!(
        flips.is_empty(),
        "round 72 changed fixture acceptance:\n{}",
        flips.join("\n")
    );
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=0");
    Ok(())
}

#[test]
#[ignore = "maintenance: compares the Round 73 dump and records probe coverage"]
fn round73_fixture_flips_are_adjudicated_and_probe_caps_are_disclosed() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let (charts_checked, probes_checked, flips, coverage) = corpus_acceptance_flips()?;
    for chart in &coverage {
        eyre::ensure!(
            chart.base_candidates == chart.base_emitted + chart.base_dropped,
            "base probe accounting mismatch: {chart:?}"
        );
        eyre::ensure!(
            chart.third_level_candidates == chart.third_level_emitted + chart.third_level_dropped,
            "depth-three probe accounting mismatch: {chart:?}"
        );
        eyre::ensure!(
            chart.guards_discovered == chart.guards_attempted + chart.guards_skipped_by_cap,
            "guard accounting mismatch: {chart:?}"
        );
        eyre::ensure!(
            chart.guards_attempted == chart.guard_pairs_emitted + chart.guards_without_witness_pair,
            "guard witness accounting mismatch: {chart:?}"
        );
        eyre::ensure!(
            chart.composite_targets
                == chart.composite_pairs_emitted
                    + chart.composite_targets_without_payload
                    + chart.composite_pairs_dropped_by_cap,
            "composite probe accounting mismatch: {chart:?}"
        );
        eyre::ensure!(chart.total_emitted > 0, "empty probe battery: {chart:?}");
    }
    let baseline_ref = std::env::var("SCHEMA_ACCEPTANCE_BASELINE_REF")
        .wrap_err("SCHEMA_ACCEPTANCE_BASELINE_REF must name the comparison commit")?;
    let report = ProbeCoverageReport {
        baseline_ref,
        charts: coverage,
    };
    let report_path = std::path::PathBuf::from(
        std::env::var_os("SCHEMA_PROBE_COVERAGE_REPORT")
            .ok_or_eyre("SCHEMA_PROBE_COVERAGE_REPORT must name the report file")?,
    );
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent).wrap_err_with(|| format!("create {}", parent.display()))?;
    }
    let mut bytes = serde_json::to_vec_pretty(&report).wrap_err("serialize probe coverage")?;
    bytes.push(b'\n');
    std::fs::write(&report_path, bytes)
        .wrap_err_with(|| format!("write {}", report_path.display()))?;
    let recorded: ProbeCoverageReport = serde_json::from_slice(
        &std::fs::read(&report_path).wrap_err_with(|| format!("read {}", report_path.display()))?,
    )
    .wrap_err("parse recorded probe coverage")?;
    sim_assert_eq!(have: recorded, want: report);
    eyre::ensure!(
        flips.is_empty(),
        "round 73 changed fixture acceptance:\n{}",
        flips.join("\n")
    );
    eprintln!("charts_checked={charts_checked} probes_checked={probes_checked} flips=0");
    Ok(())
}

fn corpus_acceptance_flips() -> eyre::Result<(usize, usize, Vec<String>, Vec<ProbeCoverage>)> {
    let baseline_ref = std::env::var("SCHEMA_ACCEPTANCE_BASELINE_REF")
        .wrap_err("SCHEMA_ACCEPTANCE_BASELINE_REF must name the comparison commit")?;
    let fixture_dir = test_util::workspace_testdata().join("chart-corpus-schemas");
    let mut fixture_paths = std::fs::read_dir(&fixture_dir)
        .wrap_err_with(|| format!("read {}", fixture_dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    fixture_paths.sort();

    let mut comparison = AcceptanceComparison::default();
    let mut charts_checked = 0;
    for fixture_path in fixture_paths {
        let Some(filename) = fixture_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(chart) = filename.strip_suffix(".schema.json") else {
            continue;
        };
        let relative_path = format!("testdata/chart-corpus-schemas/{filename}");
        let baseline = read_schema_at_ref(&baseline_ref, &relative_path)?;
        let dump_filename = format!("helm-schema.cli.chart-corpus.{chart}.schema.json");
        let current = read_acceptance_candidate(&fixture_path, &dump_filename)?;
        let defaults = read_root_defaults(chart)?;
        collect_acceptance_flips(
            chart,
            chart,
            &baseline,
            &current,
            &defaults,
            &mut comparison,
        )?;
        charts_checked += 1;
    }
    let lean_fixture_dir = test_util::workspace_testdata().join("emission-profile-schemas/lean");
    for chart in LEAN_FIXTURE_CHARTS {
        let filename = format!("{chart}.schema.json");
        let fixture_path = lean_fixture_dir.join(&filename);
        let relative_path = format!("testdata/emission-profile-schemas/lean/{filename}");
        let baseline = read_schema_at_ref(&baseline_ref, &relative_path)?;
        let dump_filename = format!("helm-schema.emission-profile.lean.{chart}.schema.json");
        let current = read_acceptance_candidate(&fixture_path, &dump_filename)?;
        let defaults = if *chart == "schema-emission-temporal-wrapper" {
            read_json_fixture(chart, "coalesced-defaults.json")?
        } else {
            read_root_defaults(chart)?
        };
        collect_acceptance_flips(
            &format!("lean/{chart}"),
            chart,
            &baseline,
            &current,
            &defaults,
            &mut comparison,
        )?;
        charts_checked += 1;
    }

    Ok((
        charts_checked,
        comparison.probes_checked,
        comparison.flips,
        comparison.coverage,
    ))
}

fn read_schema_at_ref(reference: &str, relative_path: &str) -> eyre::Result<serde_json::Value> {
    let output = std::process::Command::new("git")
        .args(["show", &format!("{reference}:{relative_path}")])
        .output()
        .wrap_err_with(|| format!("read {relative_path} from {reference}"))?;
    eyre::ensure!(
        output.status.success(),
        "git show failed for {reference}:{relative_path}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
        .wrap_err_with(|| format!("parse {reference}:{relative_path}"))
}

fn read_acceptance_candidate(
    fixture_path: &std::path::Path,
    dump_filename: &str,
) -> eyre::Result<serde_json::Value> {
    let candidate_path = std::env::var_os("SCHEMA_ACCEPTANCE_CANDIDATE_DUMP").map_or_else(
        || fixture_path.to_path_buf(),
        |dir| std::path::PathBuf::from(dir).join(dump_filename),
    );
    serde_json::from_str(
        &std::fs::read_to_string(&candidate_path)
            .wrap_err_with(|| format!("read {}", candidate_path.display()))?,
    )
    .wrap_err_with(|| format!("parse {}", candidate_path.display()))
}

fn collect_acceptance_flips(
    label: &str,
    chart_relative_path: &str,
    baseline: &serde_json::Value,
    current: &serde_json::Value,
    defaults: &serde_json::Value,
    comparison: &mut AcceptanceComparison,
) -> eyre::Result<()> {
    let profiles = ProfileSchemas::compile(baseline, current, defaults.clone())?;
    let (probes, mut chart_coverage) = structural_probe_battery_with_coverage(
        chart_relative_path,
        defaults,
        &[baseline, current],
    )?;
    chart_coverage.label = label.to_string();
    for (probe_name, probe) in probes {
        comparison.probes_checked += 1;
        let (before, after) = profiles.verdicts(&probe);
        if before != after {
            comparison.flips.push(format!(
                "{label}: {probe_name}: before={before}, after={after}"
            ));
        }
    }
    comparison.coverage.push(chart_coverage);
    Ok(())
}

#[test]
#[ignore = "maintenance: requires LEGACY_LEAN_SCHEMA_DIR"]
fn middle_lean_transition_has_only_preregistered_tightenings() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let baseline_dir = std::path::PathBuf::from(
        std::env::var("LEGACY_LEAN_SCHEMA_DIR")
            .wrap_err("LEGACY_LEAN_SCHEMA_DIR must contain legacy lean schemas")?,
    );
    let mut probes_checked = 0;
    let mut tightenings = Vec::new();
    let mut inverse = Vec::new();
    let adjudicate_live = std::env::var("ADJUDICATE_WITH_HELM").is_ok();
    if adjudicate_live {
        let output = std::process::Command::new("helm")
            .args(["version", "--template", "{{.Version}}"])
            .output()
            .wrap_err("read Helm version for transition adjudication")?;
        eyre::ensure!(output.status.success(), "helm version failed");
        let version = String::from_utf8(output.stdout).wrap_err("decode Helm version")?;
        eyre::ensure!(
            version.trim() == "v4.2.3",
            "transition adjudication requires Helm v4.2.3, found {}",
            version.trim()
        );
    }

    for chart in LEAN_FIXTURE_CHARTS {
        let baseline_path = baseline_dir.join(format!("{chart}.schema.json"));
        let baseline: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&baseline_path)
                .wrap_err_with(|| format!("read {}", baseline_path.display()))?,
        )
        .wrap_err_with(|| format!("parse {}", baseline_path.display()))?;
        let current = generate_profile_schemas(chart)?.1;
        let defaults = if *chart == "schema-emission-temporal-wrapper" {
            read_json_fixture(chart, "coalesced-defaults.json")?
        } else {
            read_root_defaults(chart)?
        };
        let transition = ProfileSchemas::compile(&baseline, &current, defaults.clone())?;
        for (probe_name, probe) in
            structural_probe_battery(chart, &defaults, &[&baseline, &current])?
        {
            probes_checked += 1;
            let (legacy, middle) = transition.verdicts(&probe);
            match (legacy, middle) {
                (true, false) => {
                    if adjudicate_live {
                        adjudicate_transition_tightening(chart, &probe_name, &probe)?;
                    }
                    tightenings.push(format!("{chart}: {probe_name}"));
                }
                (false, true) => inverse.push(format!("{chart}: {probe_name}")),
                (false, false) | (true, true) => {}
            }
        }
    }

    eyre::ensure!(
        inverse.is_empty(),
        "middle lean unexpectedly loosens legacy lean:\n{}",
        inverse.join("\n")
    );
    eyre::ensure!(
        !tightenings.is_empty(),
        "middle lean produced no preregistered transition tightenings"
    );
    eprintln!(
        "probes_checked={probes_checked} tightenings={} inverse=0",
        tightenings.len()
    );
    for tightening in tightenings {
        eprintln!("TIGHTEN {tightening}");
    }
    Ok(())
}

fn adjudicate_transition_tightening(
    chart: &str,
    probe_name: &str,
    probe: &ProbeInstance,
) -> eyre::Result<()> {
    let values = probe
        .helm_values_file()
        .ok_or_else(|| eyre::eyre!("{chart}: {probe_name} is not a Helm values-file probe"))?;
    let tempdir = tempfile::tempdir().wrap_err("create live adjudication directory")?;
    let values_path = tempdir.path().join("values.json");
    std::fs::write(&values_path, serde_json::to_vec(&values)?)
        .wrap_err("write live adjudication values")?;
    let chart_path = test_util::workspace_testdata().join("charts").join(chart);
    let rendered = std::process::Command::new("helm")
        .args(["template", "step2-lean-transition"])
        .arg(chart_path)
        .arg("--skip-schema-validation")
        .arg("-f")
        .arg(values_path)
        .output()
        .wrap_err_with(|| format!("render {chart}: {probe_name}"))?;
    if !rendered.status.success() {
        eprintln!("HELM_REJECT {chart}: {probe_name}");
        return Ok(());
    }

    let manifest_path = tempdir.path().join("rendered.yaml");
    std::fs::write(&manifest_path, rendered.stdout).wrap_err("write rendered manifest")?;
    let provider = std::process::Command::new("kubeconform")
        .args(["-strict", "-kubernetes-version", "1.29.0"])
        .arg(manifest_path)
        .output()
        .wrap_err_with(|| format!("validate {chart}: {probe_name}"))?;
    eyre::ensure!(
        !provider.status.success(),
        "{chart}: {probe_name} renders and passes the provider; the middle-lean tightening is a false rejection"
    );
    eprintln!("PROVIDER_REJECT {chart}: {probe_name}");
    Ok(())
}
