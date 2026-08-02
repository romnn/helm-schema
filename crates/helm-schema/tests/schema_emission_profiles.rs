//! Monotonicity and semantic-oracle harness for schema emission profiles.

use color_eyre::eyre;
use serde_json::json;

#[path = "common/emission_profile_harness.rs"]
mod harness;

use harness::{
    ContractVerdict, ControlCategory, ProbeInstance, ProfileSchemas, SemanticControl, Transport,
    generate_profile_schemas, read_json_fixture, read_root_defaults, sparse_override,
    structural_probe_battery,
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
