//! Root-source and command-line configuration surface regressions.

use std::path::Path;
use std::process::{Command, Output, Stdio};

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use flate2::Compression;
use flate2::write::GzEncoder;
use indoc::indoc;
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

const HELM_SCHEMA_BIN: &str = env!("CARGO_BIN_EXE_helm-schema");
const TEMPORAL_CONFIG: &str = indoc! {"
    version: 1
    profile: lean
    emission:
      local-conditionals: off
"};

fn write_chart(path: &Path, config: Option<&str>) -> eyre::Result<()> {
    std::fs::create_dir_all(path.join("templates"))?;
    std::fs::write(
        path.join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: config-surface
            version: 0.1.0
        "},
    )?;
    std::fs::write(path.join("values.yaml"), "{}\n")?;
    if let Some(config) = config {
        std::fs::write(path.join("helm-schema.yaml"), config)?;
    }
    Ok(())
}

fn package_chart(chart: &Path, archive: &Path) -> eyre::Result<()> {
    let file = std::fs::File::create(archive)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.append_dir_all("config-surface", chart)?;
    builder.into_inner()?.finish()?;
    Ok(())
}

fn print_effective(path: &Path) -> eyre::Result<Output> {
    Command::new(HELM_SCHEMA_BIN)
        .arg("--print-effective-config")
        .arg("--diag-format=json")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .wrap_err("print effective config")
}

fn generate(path: &Path) -> eyre::Result<Output> {
    Command::new(HELM_SCHEMA_BIN)
        .args(["--offline", "--no-k8s-schemas", "--compact"])
        .arg(path)
        .output()
        .wrap_err("generate schema")
}

fn yaml(output: &Output) -> eyre::Result<Value> {
    serde_yaml::from_slice(&output.stdout).wrap_err("parse effective config YAML")
}

#[test]
fn directory_and_packaged_chart_config_resolution_agree() -> eyre::Result<()> {
    let temp = tempfile::tempdir()?;
    let chart = temp.path().join("chart");
    write_chart(&chart, Some(TEMPORAL_CONFIG))?;
    let archive = temp.path().join("config-surface-0.1.0.tgz");
    package_chart(&chart, &archive)?;

    let directory = print_effective(&chart)?;
    let packaged = print_effective(&archive)?;
    assert!(directory.status.success());
    assert!(packaged.status.success());
    let directory_yaml = yaml(&directory)?;
    let packaged_yaml = yaml(&packaged)?;
    for pointer in [
        "/profile/value",
        "/emission/root-anchored-conditionals/value",
        "/emission/local-conditionals/value",
        "/emission/terminal-clauses/value",
        "/emission/kind-partitions/value",
    ] {
        sim_assert_eq!(
            have: directory_yaml.pointer(pointer),
            want: packaged_yaml.pointer(pointer),
            "directory/archive disagreement at {pointer}"
        );
    }

    for output in [&directory, &packaged] {
        let lines = String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        sim_assert_eq!(have: lines.len(), want: 1);
        let diagnostic: serde_json::Value = serde_json::from_str(&lines[0])?;
        sim_assert_eq!(
            have: diagnostic.get("type").and_then(serde_json::Value::as_str),
            want: Some("DiscoveredConfigWeakensEmission")
        );
    }
    Ok(())
}

#[test]
fn relative_explicit_config_is_resolved_from_invocation_directory() -> eyre::Result<()> {
    let temp = tempfile::tempdir()?;
    let chart = temp.path().join("chart");
    write_chart(&chart, None)?;
    std::fs::write(temp.path().join("policy.yaml"), TEMPORAL_CONFIG)?;

    let output = Command::new(HELM_SCHEMA_BIN)
        .current_dir(temp.path())
        .args([
            "--print-effective-config",
            "--config",
            "policy.yaml",
            "chart",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let effective = yaml(&output)?;
    let source = effective
        .pointer("/profile/source")
        .and_then(Value::as_str)
        .ok_or_eyre("profile source missing")?;
    sim_assert_eq!(
        have: source,
        want: temp.path().join("policy.yaml").display().to_string()
    );
    Ok(())
}

#[test]
fn print_effective_config_stops_before_chart_analysis() -> eyre::Result<()> {
    let temp = tempfile::tempdir()?;
    std::fs::write(temp.path().join("helm-schema.yaml"), "version: 1\n")?;
    std::fs::write(
        temp.path().join("not-a-chart"),
        "analysis must not read this\n",
    )?;

    let output = print_effective(temp.path())?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    sim_assert_eq!(
        have: yaml(&output)?.pointer("/profile/value").and_then(Value::as_str),
        want: Some("full")
    );
    Ok(())
}

#[test]
fn no_config_ignores_a_malformed_discovered_document() -> eyre::Result<()> {
    let temp = tempfile::tempdir()?;
    let chart = temp.path().join("chart");
    write_chart(&chart, Some("version: [\n"))?;

    let output = Command::new(HELM_SCHEMA_BIN)
        .arg("--print-effective-config")
        .arg("--no-config")
        .arg(&chart)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    sim_assert_eq!(
        have: yaml(&output)?.pointer("/profile/value").and_then(Value::as_str),
        want: Some("full")
    );
    Ok(())
}

#[test]
fn explicit_cli_profile_resets_file_knob_deltas() -> eyre::Result<()> {
    let temp = tempfile::tempdir()?;
    let chart = temp.path().join("chart");
    write_chart(&chart, Some(TEMPORAL_CONFIG))?;

    let output = Command::new(HELM_SCHEMA_BIN)
        .arg("--print-effective-config")
        .arg("--profile=lean")
        .arg(&chart)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let effective = yaml(&output)?;
    sim_assert_eq!(
        have: effective
            .pointer("/emission/local-conditionals/value")
            .and_then(Value::as_str),
        want: Some("on")
    );
    sim_assert_eq!(have: output.stderr, want: Vec::<u8>::new());
    Ok(())
}

#[test]
fn temporal_config_drives_generation_and_annotation() -> eyre::Result<()> {
    let temp = tempfile::tempdir()?;
    let chart = temp.path().join("chart");
    write_chart(&chart, Some(TEMPORAL_CONFIG))?;
    std::fs::write(
        chart.join("templates/configmap.yaml"),
        indoc! {"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: test
            data:
              value: '{{ .Values.value }}'
        "},
    )?;
    let archive = temp.path().join("config-surface-0.1.0.tgz");
    package_chart(&chart, &archive)?;

    let output = generate(&chart)?;
    let packaged = generate(&archive)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        packaged.status.success(),
        "{}",
        String::from_utf8_lossy(&packaged.stderr)
    );
    sim_assert_eq!(have: &packaged.stdout, want: &output.stdout);
    let schema: Value = serde_json::from_slice(&output.stdout)?;
    sim_assert_eq!(
        have: schema
            .pointer("/x-helm-schema-policy/requested-profile")
            .and_then(Value::as_str),
        want: Some("lean")
    );
    sim_assert_eq!(
        have: schema
            .pointer("/x-helm-schema-policy/resolved/local-conditionals")
            .and_then(Value::as_bool),
        want: Some(false)
    );
    Ok(())
}
