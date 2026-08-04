//! Live Helm and rendered-sink replay for schema-emission profile controls.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Output};

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use indoc::indoc;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

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
fn replay_opaque_formatter_default_against_helm() -> eyre::Result<()> {
    assert_helm_version()?;
    let scratch_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/round74-live");
    std::fs::create_dir_all(&scratch_root).wrap_err("create live-control scratch root")?;
    let chart = tempfile::Builder::new()
        .prefix("opaque-formatter-default-")
        .tempdir_in(scratch_root)
        .wrap_err("create opaque-default chart")?;
    std::fs::create_dir(chart.path().join("templates"))?;
    std::fs::write(
        chart.path().join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: opaque-formatter-default
            version: 0.1.0
        "},
    )?;
    std::fs::write(
        chart.path().join("values.yaml"),
        indoc! {"
            alpha: seta
            beta: setb
            omega: fallo
        "},
    )?;

    for (expression, selected_number_renders, raw_falsy_number_renders) in [
        (
            r#"printf "%s" .Values.alpha | default .Values.omega | b64enc"#,
            false,
            true,
        ),
        (
            r#"printf "%q" .Values.alpha | default .Values.omega | b64enc"#,
            true,
            true,
        ),
        (
            r#"printf "%s" .Values.alpha | default .Values.omega | trunc 5"#,
            false,
            false,
        ),
        (
            r#"printf "%s" .Values.alpha | default .Values.omega | sha256sum"#,
            false,
            true,
        ),
        (
            r#"printf "%s" .Values.alpha | default .Values.omega | quote"#,
            true,
            true,
        ),
        (
            r#"printf "%s" .Values.alpha | default .Values.omega | trimSuffix "-x""#,
            false,
            false,
        ),
        (
            r#"(printf "%s" .Values.alpha) | default .Values.omega | b64enc"#,
            false,
            true,
        ),
        (
            r#"default .Values.omega (printf "%s" .Values.alpha) | b64enc"#,
            false,
            true,
        ),
        (
            r#"printf "%s" .Values.alpha | default .Values.beta | default .Values.omega | b64enc"#,
            false,
            true,
        ),
    ] {
        replay_opaque_formatter_expression(
            chart.path(),
            expression,
            selected_number_renders,
            raw_falsy_number_renders,
        )?;
    }
    Ok(())
}

fn replay_opaque_formatter_expression(
    chart: &Path,
    expression: &str,
    selected_number_renders: bool,
    raw_falsy_number_renders: bool,
) -> eyre::Result<()> {
    std::fs::write(
        chart.join("templates/configmap.yaml"),
        format!(
            "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: opaque-formatter-default\ndata:\n  token: {{{{ {expression} }}}}\n"
        ),
    )?;
    for (string_overrides, typed_overrides, renders, label) in [
        (None, Some("omega=null"), true, "deleted dormant fallback"),
        (None, Some("omega=7"), true, "numeric dormant fallback"),
        (None, Some("omega=false"), true, "falsy dormant fallback"),
        (
            Some("beta="),
            Some("alpha=false,omega=7"),
            raw_falsy_number_renders,
            "raw-falsy formatter operand renders a truthy string",
        ),
        (
            Some("alpha=,beta=,omega=selected"),
            None,
            true,
            "selected string fallback",
        ),
        (
            Some("alpha=,beta="),
            Some("omega=7"),
            selected_number_renders,
            "selected numeric fallback",
        ),
    ] {
        let mut command = Command::new("helm");
        command
            .args(["template", "opaque-formatter-default"])
            .arg(chart)
            .arg("--skip-schema-validation");
        if let Some(overrides) = string_overrides {
            command.args(["--set-string", overrides]);
        }
        if let Some(overrides) = typed_overrides {
            command.args(["--set", overrides]);
        }
        let output = command
            .output()
            .wrap_err("render opaque formatter default control")?;
        eyre::ensure!(
            output.status.success() == renders,
            "opaque fallback {label}: expression={expression}; renders={}; want={renders}; stderr={}",
            output.status.success(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[test]
#[ignore = "live maintenance lane: requires pinned Helm"]
fn replay_literal_default_primary_reachability_against_helm() -> eyre::Result<()> {
    assert_helm_version()?;
    let scratch_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/round74-live");
    std::fs::create_dir_all(&scratch_root).wrap_err("create live-control scratch root")?;
    let chart = tempfile::Builder::new()
        .prefix("literal-default-primary-")
        .tempdir_in(scratch_root)
        .wrap_err("create literal-default chart")?;
    std::fs::create_dir(chart.path().join("templates"))?;
    std::fs::write(
        chart.path().join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: literal-default-primary
            version: 0.1.0
        "},
    )?;
    std::fs::write(
        chart.path().join("values.yaml"),
        indoc! {"
            choose: false
            omega: fallo
        "},
    )?;

    for (primary, selected) in [
        ("\"\"", true),
        ("\"x\"", false),
        (r#"ternary "" "" .Values.choose"#, true),
        (r#"ternary "x" "y" .Values.choose"#, false),
    ] {
        std::fs::write(
            chart.path().join("templates/configmap.yaml"),
            format!(
                "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: literal-default-primary\ndata:\n  token: {{{{ ({primary}) | default .Values.omega | b64enc }}}}\n"
            ),
        )?;
        for (set_values, renders, label) in [
            ("omega=null", !selected, "deleted fallback"),
            ("omega=7", !selected, "numeric fallback"),
            ("omega=selected", true, "string fallback"),
        ] {
            let output = Command::new("helm")
                .args(["template", "literal-default-primary"])
                .arg(chart.path())
                .arg("--skip-schema-validation")
                .args(["--set", set_values])
                .output()
                .wrap_err("render literal default control")?;
            eyre::ensure!(
                output.status.success() == renders,
                "literal primary {primary} {label}: renders={}; want={renders}; stderr={}",
                output.status.success(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    std::fs::write(
        chart.path().join("templates/configmap.yaml"),
        indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: literal-default-primary
            data:
              token: {{ "live" | default (required "omega is required" .Values.omega) | b64enc }}
        "#},
    )?;
    for (set_values, renders, label) in [
        ("omega=null", false, "eager fallback argument deleted"),
        ("omega=7", true, "eager fallback argument present"),
    ] {
        let output = Command::new("helm")
            .args(["template", "literal-default-primary"])
            .arg(chart.path())
            .arg("--skip-schema-validation")
            .args(["--set", set_values])
            .output()
            .wrap_err("render eager literal-default fallback control")?;
        eyre::ensure!(
            output.status.success() == renders,
            "literal primary {label}: renders={}; want={renders}; stderr={}",
            output.status.success(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[test]
#[ignore = "live maintenance lane: requires pinned Helm"]
fn replay_oauth2_proxy_tpl_default_eagerness_against_helm() -> eyre::Result<()> {
    assert_helm_version()?;
    let chart = test_util::workspace_testdata().join("charts/oauth2-proxy");
    for (image_registry, global_registry, renders, label) in [
        ("", "7", false, "selected non-string fallback"),
        (
            "quay.io",
            "7",
            false,
            "non-string fallback is still evaluated eagerly",
        ),
        ("", "ghcr.io", true, "selected string fallback"),
    ] {
        let output = Command::new("helm")
            .args(["template", "oauth2-proxy"])
            .arg(&chart)
            .arg("--skip-schema-validation")
            .args(["--set-string", &format!("image.registry={image_registry}")])
            .args(["--set", &format!("global.imageRegistry={global_registry}")])
            .output()
            .wrap_err("render oauth2-proxy tpl default control")?;
        eyre::ensure!(
            output.status.success() == renders,
            "oauth2-proxy {label}: renders={}; want={renders}; stderr={}",
            output.status.success(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[test]
#[ignore = "live maintenance lane: requires pinned Helm"]
fn replay_yaml_boolean_key_composition_against_helm() -> eyre::Result<()> {
    assert_helm_version()?;
    let scratch_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/round71-live");
    std::fs::create_dir_all(&scratch_root).wrap_err("create live-control scratch root")?;
    let chart = create_yaml_boolean_key_chart(&scratch_root)?;

    let defaults = render_yaml_boolean_key_chart(chart.path(), None, None)?;
    sim_assert_eq!(have: defaults["dot-y"].as_str(), want: Some("nil"));
    sim_assert_eq!(have: defaults["true-key"].as_str(), want: Some("last"));
    sim_assert_eq!(have: defaults["nested-dot-no"].as_str(), want: Some("nil"));
    sim_assert_eq!(
        have: defaults["nested-false-key"].as_str(),
        want: Some("nested-false")
    );
    sim_assert_eq!(have: defaults["quoted-on"].as_str(), want: Some("quoted-on"));
    let mut mixed_winners = BTreeSet::from([defaults["mixed-true"]
        .as_str()
        .ok_or_eyre("mixed collision did not render a string")?
        .to_string()]);
    for _ in 0..31 {
        let replay = render_yaml_boolean_key_chart(chart.path(), None, None)?;
        mixed_winners.insert(
            replay["mixed-true"]
                .as_str()
                .ok_or_eyre("mixed collision did not render a string")?
                .to_string(),
        );
    }
    eyre::ensure!(
        mixed_winners
            .iter()
            .all(|winner| matches!(winner.as_str(), "legacy" | "quoted")),
        "unexpected mixed boolean/string key winner: {mixed_winners:?}"
    );
    eprintln!("mixed boolean/string key winners: {mixed_winners:?}");

    let values_file = chart.path().join("override.yaml");
    std::fs::write(
        &values_file,
        indoc! {r#"
            y: file-first
            yes: file-last
            nested:
              off: file-false
              "on": file-quoted
        "#},
    )?;
    let layered = render_yaml_boolean_key_chart(chart.path(), Some(&values_file), None)?;
    sim_assert_eq!(have: layered["dot-y"].as_str(), want: Some("nil"));
    sim_assert_eq!(have: layered["true-key"].as_str(), want: Some("file-last"));
    sim_assert_eq!(
        have: layered["nested-false-key"].as_str(),
        want: Some("file-false")
    );
    sim_assert_eq!(have: layered["quoted-on"].as_str(), want: Some("file-quoted"));

    let set = render_yaml_boolean_key_chart(chart.path(), None, Some("y=set-y,nested.no=set-no"))?;
    sim_assert_eq!(have: set["dot-y"].as_str(), want: Some("set-y"));
    sim_assert_eq!(have: set["true-key"].as_str(), want: Some("last"));
    sim_assert_eq!(have: set["nested-dot-no"].as_str(), want: Some("set-no"));
    sim_assert_eq!(
        have: set["nested-false-key"].as_str(),
        want: Some("nested-false")
    );
    Ok(())
}

#[test]
#[ignore = "live maintenance lane: requires pinned Helm"]
fn replay_selector_independent_ranged_provider_use_against_helm() -> eyre::Result<()> {
    assert_helm_version()?;
    let chart = test_util::workspace_testdata().join("charts/schema-emission-kind-range");
    for set_arg in [None, Some(("--set", "entries=2"))] {
        let mut command = Command::new("helm");
        command
            .args(["template", "schema-emission-kind-range"])
            .arg(&chart)
            .arg("--skip-schema-validation");
        if let Some((flag, value)) = set_arg {
            command.args([flag, value]);
        }
        let output = command
            .output()
            .wrap_err("render selector-independent ranged provider control")?;
        eyre::ensure!(
            output.status.success(),
            "Helm rejected ranged provider control {set_arg:?}: {}",
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

fn render_yaml_boolean_key_chart(
    chart: &Path,
    values_file: Option<&Path>,
    set: Option<&str>,
) -> eyre::Result<Value> {
    let mut command = Command::new("helm");
    command
        .args(["template", "yaml-boolean-keys"])
        .arg(chart)
        .arg("--skip-schema-validation");
    if let Some(values_file) = values_file {
        command.arg("-f").arg(values_file);
    }
    if let Some(set) = set {
        command.arg("--set").arg(set);
    }
    let output = command
        .output()
        .wrap_err("render YAML boolean-key control")?;
    eyre::ensure!(
        output.status.success(),
        "YAML boolean-key control failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest: serde_yaml::Value = serde_yaml::from_slice(&output.stdout)
        .wrap_err("parse rendered YAML boolean-key control")?;
    let data = manifest
        .get("data")
        .ok_or_eyre("rendered YAML boolean-key control has no data")?;
    serde_json::to_value(data).wrap_err("convert rendered YAML boolean-key data")
}

fn create_yaml_boolean_key_chart(scratch_root: &Path) -> eyre::Result<tempfile::TempDir> {
    let chart = tempfile::Builder::new()
        .prefix("yaml-boolean-keys-")
        .tempdir_in(scratch_root)
        .wrap_err("create YAML boolean-key chart")?;
    std::fs::create_dir(chart.path().join("templates"))?;
    std::fs::write(
        chart.path().join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: yaml-boolean-keys
            version: 0.1.0
        "},
    )?;
    std::fs::write(
        chart.path().join("values.yaml"),
        indoc! {r#"
            y: first
            on: last
            n: first-false
            off: last-false
            nested:
              yes: nested-true
              no: nested-false
              "on": quoted-on
            mixed:
              on: legacy
              "true": quoted
        "#},
    )?;
    std::fs::write(
        chart.path().join("templates/configmap.yaml"),
        indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: yaml-boolean-keys
            data:
              dot-y: {{ .Values.y | default "nil" | quote }}
              true-key: {{ index .Values "true" | quote }}
              nested-dot-no: {{ .Values.nested.no | default "nil" | quote }}
              nested-false-key: {{ index .Values.nested "false" | quote }}
              quoted-on: {{ index .Values.nested "on" | quote }}
              mixed-true: {{ index .Values.mixed "true" | quote }}
              values-json: {{ toJson .Values | quote }}
        "#},
    )?;
    Ok(chart)
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
