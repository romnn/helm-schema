//! The development binary shares CLI behavior and owns optional Perfetto output.

use std::process::Command;

use color_eyre::eyre;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

#[test]
fn dev_trace_preserves_schema_and_writes_spans() -> eyre::Result<()> {
    let scratch = ScratchDir::new("dev-trace")?;
    let chart = scratch.path().join("chart");
    std::fs::create_dir_all(chart.join("templates"))?;
    std::fs::write(
        chart.join("Chart.yaml"),
        indoc! {"
        apiVersion: v2
        name: trace-test
        version: 0.1.0
    "},
    )?;
    std::fs::write(chart.join("values.yaml"), "enabled: true\n")?;
    std::fs::write(
        chart.join("templates/cm.yaml"),
        indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: trace-test
        data:
          enabled: {{ .Values.enabled | quote }}
    "},
    )?;
    let plain = Command::new(env!("CARGO_BIN_EXE_helm-schema-dev"))
        .arg(&chart)
        .args(["--no-k8s-schemas", "--offline", "--no-config"])
        .output()?;
    eyre::ensure!(plain.status.success(), "{plain:?}");
    let trace = scratch.path().join("nested/run.pftrace");
    let traced = Command::new(env!("CARGO_BIN_EXE_helm-schema-dev"))
        .arg(&chart)
        .args([
            "--no-k8s-schemas",
            "--offline",
            "--no-config",
            "--trace-output",
        ])
        .arg(&trace)
        .output()?;
    eyre::ensure!(traced.status.success(), "{traced:?}");
    sim_assert_eq!(have: &traced.stdout, want: &plain.stdout);
    sim_assert_eq!(have: &traced.stderr, want: &plain.stderr);
    let bytes = std::fs::read(&trace)?;
    eyre::ensure!(!bytes.is_empty(), "trace is empty");
    eyre::ensure!(
        bytes
            .windows(b"helm_schema_run".len())
            .any(|window| window == b"helm_schema_run"),
        "root span missing"
    );
    Ok(())
}

#[test]
fn dev_trace_extends_shared_subcommands() -> eyre::Result<()> {
    let scratch = ScratchDir::new("dev-shorten")?;
    let input = scratch.path().join("schema.json");
    let output = scratch.path().join("short.json");
    let trace = scratch.path().join("short.pftrace");
    std::fs::write(&input, r#"{"type":"object"}"#)?;
    let result = Command::new(env!("CARGO_BIN_EXE_helm-schema-dev"))
        .args(["shorten", "--trace-output"])
        .arg(&trace)
        .arg(&input)
        .arg(&output)
        .output()?;
    eyre::ensure!(result.status.success(), "{result:?}");
    eyre::ensure!(trace.is_file(), "trace file missing");
    let schema: serde_json::Value = serde_json::from_slice(&std::fs::read(output)?)?;
    sim_assert_eq!(have: schema, want: serde_json::json!({"type": "object"}));
    Ok(())
}
