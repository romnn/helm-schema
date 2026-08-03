//! Live Helm and rendered-sink replay for schema-emission profile controls.

use std::path::Path;
use std::process::{Command, Output};

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use indoc::indoc;
use serde_json::{Value, json};

const HELM_VERSION: &str = "v4.2.3";
const PROVIDER_VERSION: &str = "1.29.0";

#[derive(Clone, Debug)]
enum LiveTransport {
    ValuesFileJson(Value),
    Set(&'static str),
    SetString(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SinkVerdict {
    Accept,
    Reject,
    Unresolved,
}

struct LiveControl {
    name: &'static str,
    transport: LiveTransport,
    renders: bool,
    sink: SinkVerdict,
}

#[test]
#[ignore = "live maintenance lane: requires pinned Helm and kubeconform"]
fn replay_semantic_controls_against_helm_and_provider() -> eyre::Result<()> {
    assert_helm_version()?;
    let chart = test_util::workspace_testdata().join("charts/schema-emission-controls");
    let controls = live_controls();

    let mut failures = Vec::new();
    for control in controls {
        let rendered = render_control(&chart, &control.transport)
            .wrap_err_with(|| format!("render {}", control.name))?;
        if rendered.status.success() != control.renders {
            failures.push(format!(
                "{}: Helm success={}, expected {}; stderr={}",
                control.name,
                rendered.status.success(),
                control.renders,
                String::from_utf8_lossy(&rendered.stderr)
            ));
            continue;
        }
        if !control.renders || control.sink == SinkVerdict::Unresolved {
            continue;
        }
        let sink_accepts = validate_rendered_sink(&rendered.stdout)
            .wrap_err_with(|| format!("validate rendered sink for {}", control.name))?;
        let expected = control.sink == SinkVerdict::Accept;
        if sink_accepts != expected {
            failures.push(format!(
                "{}: provider acceptance={sink_accepts}, expected {expected}",
                control.name
            ));
        }
    }
    eyre::ensure!(
        failures.is_empty(),
        "live semantic control failures:\n{}",
        failures.join("\n")
    );
    Ok(())
}

#[test]
#[ignore = "live maintenance lane: requires pinned Helm"]
fn replay_unconditional_fail_against_helm() -> eyre::Result<()> {
    assert_helm_version()?;
    let chart = test_util::workspace_testdata().join("charts/schema-emission-unconditional-fail");
    let output = Command::new("helm")
        .args(["template", "unconditional-fail"])
        .arg(chart)
        .arg("--skip-schema-validation")
        .output()
        .wrap_err("render unconditional-fail control")?;

    eyre::ensure!(
        !output.status.success(),
        "the unconditional-fail chart unexpectedly rendered"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    eyre::ensure!(
        stderr.contains("schema emission unconditional fail"),
        "Helm failure did not come from the control: {stderr}"
    );
    Ok(())
}

#[test]
#[ignore = "live maintenance lane: requires pinned Helm"]
fn replay_else_with_successor_against_helm() -> eyre::Result<()> {
    assert_helm_version()?;
    let scratch_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/round68-live");
    std::fs::create_dir_all(&scratch_root).wrap_err("create live-control scratch root")?;
    let chart = tempfile::Builder::new()
        .prefix("else-with-successor-")
        .tempdir_in(scratch_root)
        .wrap_err("create else-with chart")?;
    std::fs::create_dir(chart.path().join("templates"))?;
    std::fs::write(
        chart.path().join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: else-with-successor
            version: 0.1.0
        "},
    )?;
    std::fs::write(
        chart.path().join("values.yaml"),
        indoc! {"
            first: false
            second: ''
            payload:
              invalid: true
        "},
    )?;
    std::fs::write(
        chart.path().join("templates/configmap.yaml"),
        indoc! {r"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: else-with-successor
            data:
            {{- with .Values.first }}
              payload: first
            {{- else with .Values.second }}
              payload: second
            {{- else }}
              payload: {{ .Values.payload | b64enc }}
            {{- end }}
        "},
    )?;

    for (set_value, renders) in [
        (None, false),
        (Some("first=selected"), true),
        (Some("second=selected"), true),
    ] {
        let mut command = Command::new("helm");
        command
            .args(["template", "else-with-successor"])
            .arg(chart.path())
            .arg("--skip-schema-validation");
        if let Some(set_value) = set_value {
            command.arg("--set-string").arg(set_value);
        }
        let output = command
            .output()
            .wrap_err("render else-with successor control")?;
        eyre::ensure!(
            output.status.success() == renders,
            "else-with successor polarity mismatch for {set_value:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[test]
#[ignore = "live maintenance lane: requires pinned Helm"]
fn replay_chained_default_printf_against_helm() -> eyre::Result<()> {
    assert_helm_version()?;
    let scratch_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/round68-live");
    std::fs::create_dir_all(&scratch_root).wrap_err("create live-control scratch root")?;
    let chart = tempfile::Builder::new()
        .prefix("chained-default-")
        .tempdir_in(scratch_root)
        .wrap_err("create chained-default chart")?;
    std::fs::create_dir(chart.path().join("templates"))?;
    std::fs::write(
        chart.path().join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: chained-default
            version: 0.1.0
        "},
    )?;
    std::fs::write(
        chart.path().join("values.yaml"),
        indoc! {"
            x: set
            y: fally
            z: fallz
        "},
    )?;
    std::fs::write(
        chart.path().join("templates/configmap.yaml"),
        indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: chained-default
            data:
              pipeline: {{ printf "%s-x" (.Values.x | default .Values.y | default .Values.z) }}
              call: {{ printf "%s-x" (default .Values.z (default .Values.y .Values.x)) }}
        "#},
    )?;

    for (set_values, renders) in [
        ("z=null", true),
        ("x=null,y=null,z=selected", true),
        ("x=null,y=null,z=null", false),
    ] {
        let output = Command::new("helm")
            .args(["template", "chained-default"])
            .arg(chart.path())
            .arg("--skip-schema-validation")
            .arg("--set")
            .arg(set_values)
            .output()
            .wrap_err("render chained-default control")?;
        eyre::ensure!(
            output.status.success() == renders,
            "chained-default polarity mismatch for {set_values}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[test]
#[ignore = "live maintenance lane: requires pinned Helm"]
fn replay_round68_corpus_loosenings_against_helm() -> eyre::Result<()> {
    assert_helm_version()?;
    let cases = [
        ("airflow", "fullnameOverride", "false", true),
        ("airflow", "fullnameOverride", "true", false),
        ("airflow", "fullnameOverride", "0", true),
        ("airflow", "fullnameOverride", "1.5", false),
        ("airflow", "fullnameOverride", "[]", true),
        ("airflow", "fullnameOverride", "[{}]", false),
        ("airflow", "fullnameOverride", "{}", true),
        ("airflow", "fullnameOverride", r#"{"unknown":true}"#, false),
        ("metallb", "fullnameOverride", "false", true),
        ("metallb", "fullnameOverride", "0", true),
        ("metallb", "fullnameOverride", "[]", true),
        ("metallb", "fullnameOverride", "{}", true),
        ("traefik", "namespaceOverride", "false", true),
        ("traefik", "namespaceOverride", "0", true),
        ("traefik", "namespaceOverride", "[]", true),
        ("traefik", "namespaceOverride", "{}", true),
    ];
    let charts = test_util::workspace_testdata().join("charts");
    let mut failures = Vec::new();
    for (chart, path, value, renders) in cases {
        let output = Command::new("helm")
            .args(["template", "round68"])
            .arg(charts.join(chart))
            .arg("--skip-schema-validation")
            .arg("--set-json")
            .arg(format!("{path}={value}"))
            .output()
            .wrap_err_with(|| format!("render {chart} with {path}={value}"))?;
        if output.status.success() != renders {
            failures.push(format!(
                "{chart} {path}={value}: Helm success={}, expected {renders}; {}",
                output.status.success(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    eyre::ensure!(
        failures.is_empty(),
        "Round 68 corpus loosening failures:\n{}",
        failures.join("\n")
    );
    Ok(())
}

fn live_controls() -> Vec<LiveControl> {
    use LiveTransport::{Set, SetString, ValuesFileJson};
    use SinkVerdict::{Accept, Reject, Unresolved};

    vec![
        live_control("valid defaults", ValuesFileJson(json!({})), true, Accept),
        live_control("set integer replica", Set("replicas=3"), true, Accept),
        live_control(
            "set-string coercible replica",
            SetString("replicas=3"),
            true,
            Accept,
        ),
        live_control(
            "non-coercible replica",
            ValuesFileJson(json!({ "replicas": "three" })),
            true,
            Reject,
        ),
        live_control(
            "required value deletion",
            ValuesFileJson(json!({ "requiredText": null })),
            false,
            Unresolved,
        ),
        live_control(
            "version pattern near miss",
            ValuesFileJson(json!({ "version": "v1" })),
            false,
            Unresolved,
        ),
        live_control(
            "nil-safe object host deletion",
            ValuesFileJson(json!({ "host": null })),
            true,
            Accept,
        ),
        live_control(
            "disabled dependency wrong replica",
            ValuesFileJson(json!({ "worker": { "enabled": false, "replicas": "three" } })),
            true,
            Accept,
        ),
        live_control(
            "enabled dependency wrong replica",
            ValuesFileJson(json!({ "worker": { "enabled": true, "replicas": "three" } })),
            true,
            Reject,
        ),
        live_control(
            "ConfigMap branch with Service spelling",
            ValuesFileJson(json!({ "local": { "kind": "ConfigMap", "setting": "ClusterIP" } })),
            true,
            Reject,
        ),
        live_control(
            "Service branch with Service spelling",
            ValuesFileJson(json!({ "local": { "kind": "Service", "setting": "ClusterIP" } })),
            true,
            Accept,
        ),
        live_control(
            "unknown dynamic provider kind",
            ValuesFileJson(json!({ "dynamic": { "kind": "FutureKind" } })),
            true,
            Unresolved,
        ),
    ]
}

fn live_control(
    name: &'static str,
    transport: LiveTransport,
    renders: bool,
    sink: SinkVerdict,
) -> LiveControl {
    LiveControl {
        name,
        transport,
        renders,
        sink,
    }
}

#[test]
#[ignore = "live maintenance lane: requires pinned Helm"]
fn helm_embedded_validator_matches_rust_for_touched_constructs() -> eyre::Result<()> {
    assert_helm_version()?;
    let cases = [
        ("boolean true", json!(true), json!({}), true),
        ("boolean false", json!(false), json!({}), false),
        (
            "if then accept",
            json!({
                "type": "object",
                "properties": {
                    "enabled": { "type": "boolean" },
                    "count": { "type": "integer" }
                },
                "if": { "properties": { "enabled": { "const": true } } },
                "then": { "required": ["count"] }
            }),
            json!({ "enabled": true, "count": 1 }),
            true,
        ),
        (
            "if then reject",
            json!({
                "type": "object",
                "properties": { "enabled": { "type": "boolean" } },
                "if": { "properties": { "enabled": { "const": true } } },
                "then": { "required": ["count"] }
            }),
            json!({ "enabled": true }),
            false,
        ),
        (
            "type array",
            json!({
                "type": "object",
                "properties": { "value": { "type": ["string", "integer"] } }
            }),
            json!({ "value": 3 }),
            true,
        ),
        (
            "internal ref",
            json!({
                "$defs": { "count": { "type": "integer" } },
                "type": "object",
                "properties": { "value": { "$ref": "#/$defs/count" } }
            }),
            json!({ "value": "three" }),
            false,
        ),
        (
            "extension annotation",
            json!({
                "type": "object",
                "x-helm-schema-profile": "full"
            }),
            json!({}),
            true,
        ),
    ];

    let mut failures = Vec::new();
    for (name, schema, instance, expected) in cases {
        let rust = jsonschema::validator_for(&schema)
            .map_err(|error| eyre::eyre!("compile {name} schema: {error}"))?
            .is_valid(&instance);
        let helm = helm_validates_schema(&schema, &instance)
            .wrap_err_with(|| format!("run Helm parity case {name}"))?;
        if rust != expected || helm != expected {
            failures.push(format!(
                "{name}: expected={expected}, rust={rust}, helm={helm}"
            ));
        }
    }
    eyre::ensure!(
        failures.is_empty(),
        "validator parity failures:\n{}",
        failures.join("\n")
    );
    Ok(())
}

fn assert_helm_version() -> eyre::Result<()> {
    let output = Command::new("helm")
        .args(["version", "--template", "{{.Version}}"])
        .output()
        .wrap_err("run helm version")?;
    eyre::ensure!(output.status.success(), "helm version failed");
    let have = String::from_utf8(output.stdout).wrap_err("decode helm version")?;
    eyre::ensure!(
        have.trim() == HELM_VERSION,
        "live lane requires Helm {HELM_VERSION}, found {}",
        have.trim()
    );
    Ok(())
}

fn render_control(chart: &Path, transport: &LiveTransport) -> eyre::Result<Output> {
    let mut command = Command::new("helm");
    command
        .args(["template", "profile-controls"])
        .arg(chart)
        .arg("--skip-schema-validation");
    let values_dir;
    match transport {
        LiveTransport::ValuesFileJson(instance) => {
            values_dir = tempfile::tempdir().wrap_err("create values tempdir")?;
            let values_path = values_dir.path().join("values.json");
            std::fs::write(&values_path, serde_json::to_vec(instance)?)
                .wrap_err("write values transport")?;
            command.arg("-f").arg(values_path);
        }
        LiveTransport::Set(value) => {
            command.arg("--set").arg(value);
        }
        LiveTransport::SetString(value) => {
            command.arg("--set-string").arg(value);
        }
    }
    command.output().wrap_err("run helm template")
}

fn validate_rendered_sink(rendered: &[u8]) -> eyre::Result<bool> {
    let dir = tempfile::tempdir().wrap_err("create rendered-manifest tempdir")?;
    let manifest = dir.path().join("rendered.yaml");
    std::fs::write(&manifest, rendered).wrap_err("write rendered manifest")?;
    let output = Command::new("kubeconform")
        .args(["-strict", "-kubernetes-version", PROVIDER_VERSION])
        .arg(manifest)
        .output()
        .wrap_err("run kubeconform")?;
    Ok(output.status.success())
}

fn helm_validates_schema(schema: &Value, instance: &Value) -> eyre::Result<bool> {
    let dir = tempfile::tempdir().wrap_err("create parity chart")?;
    let templates = dir.path().join("templates");
    std::fs::create_dir(&templates).wrap_err("create parity templates directory")?;
    std::fs::write(
        dir.path().join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: validator-parity
            version: 0.1.0
        "},
    )
    .wrap_err("write parity Chart.yaml")?;
    std::fs::write(
        templates.join("configmap.yaml"),
        indoc! {"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: validator-parity
        "},
    )
    .wrap_err("write parity template")?;
    std::fs::write(dir.path().join("values.yaml"), "{}\n").wrap_err("write parity defaults")?;
    std::fs::write(
        dir.path().join("values.schema.json"),
        serde_json::to_vec(schema)?,
    )
    .wrap_err("write parity schema")?;
    let values_path = dir.path().join("instance.json");
    std::fs::write(&values_path, serde_json::to_vec(instance)?)
        .wrap_err("write parity instance")?;

    let chart = dir
        .path()
        .to_str()
        .ok_or_eyre("parity chart path is not UTF-8")?;
    let output = Command::new("helm")
        .args(["template", "validator-parity", chart, "-f"])
        .arg(values_path)
        .output()
        .wrap_err("run Helm embedded validator")?;
    Ok(output.status.success())
}
