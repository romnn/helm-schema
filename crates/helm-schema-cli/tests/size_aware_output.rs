//! Size-aware output layouts, explicit legacy flags, name maps, and size diagnostics.

use std::collections::BTreeMap;
use std::process::{Command, Output};

use clap::Parser as _;
use color_eyre::eyre;
use helm_schema::output::{HELM_MAX_CHART_FILE_BYTES, shorten_definition_names};
use helm_schema_cli::Cli;
use helm_schema_cli::cli::OutputLayout;
use helm_schema_json_schema_minify::rename_definitions;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

const HELM_SCHEMA_BIN: &str = env!("CARGO_BIN_EXE_helm-schema");

fn command() -> Command {
    let mut command = Command::new(HELM_SCHEMA_BIN);
    command
        .arg(test_util::workspace_testdata().join("fixture-charts/full-fixture"))
        .args(["--offline", "--no-k8s-schemas", "--no-config"]);
    command
}

fn run(command: &mut Command) -> eyre::Result<Output> {
    let output = command.output()?;
    eyre::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

#[test]
fn auto_compacts_large_output_for_files_and_stdout_and_reports_json_diagnostics() -> eyre::Result<()>
{
    let scratch = ScratchDir::new("size-aware-output")?;
    let override_path = scratch.path().join("override.json");
    let output_path = scratch.path().join("schema.json");
    let map_path = scratch.path().join("defs-map.json");
    // Indentation pushes this schema over the limit while compact JSON still fits.
    std::fs::write(
        &override_path,
        serde_json::to_vec(&json!({
            "$comment": "x".repeat(4_800_000),
            "enum": (0..50_000).collect::<Vec<_>>()
        }))?,
    )?;
    let stdout = run(command().arg("--override-schema").arg(&override_path))?;
    let schema: Value = serde_json::from_slice(&stdout.stdout)?;
    assert!(serde_json::to_vec_pretty(&schema)?.len() > HELM_MAX_CHART_FILE_BYTES);
    assert!(stdout.stdout.len() <= HELM_MAX_CHART_FILE_BYTES);
    sim_assert_eq!(have: std::str::from_utf8(&stdout.stdout)?.lines().count(), want: 1);
    let stderr = String::from_utf8(stdout.stderr)?;
    assert!(
        stderr.contains("5242880") && stderr.contains(&stdout.stdout.len().to_string()),
        "{stderr}"
    );
    sim_assert_eq!(have: stderr.matches("note: pretty schema").count(), want: 1);
    assert!(
        stderr.contains("compact") && stderr.contains("short"),
        "{stderr}"
    );
    let file = run(command()
        .arg("--override-schema")
        .arg(&override_path)
        .args(["--output-layout", "auto"])
        .arg("--output")
        .arg(&output_path)
        .arg("--defs-map")
        .arg(&map_path))?;
    sim_assert_eq!(have: std::fs::read(&output_path)?, want: stdout.stdout);
    sim_assert_eq!(have: String::from_utf8(file.stderr)?, want: stderr);

    let pretty = run(command()
        .arg("--override-schema")
        .arg(&override_path)
        .args(["--output-layout", "pretty"]))?;
    assert!(pretty.stdout.len() > HELM_MAX_CHART_FILE_BYTES);
    assert!(String::from_utf8(pretty.stderr)?.contains("warning:"));
    let readable: Value = serde_json::from_slice(&pretty.stdout)?;
    let names: BTreeMap<String, String> = serde_json::from_slice(&std::fs::read(map_path)?)?;
    assert!(!names.is_empty());
    let mut restored = schema;
    rename_definitions(&mut restored, &names);
    sim_assert_eq!(have: restored, want: readable);

    let compact = run(command()
        .arg("--override-schema")
        .arg(&override_path)
        .args(["--output-layout", "compact"]))?;
    sim_assert_eq!(have: compact.stdout, want: stdout.stdout);
    let json_diagnostics = run(command()
        .arg("--override-schema")
        .arg(&override_path)
        .arg("--diag-format=json"))?;
    let diagnostics = serde_json::Deserializer::from_slice(&json_diagnostics.stderr)
        .into_iter::<Value>()
        .collect::<Result<Vec<_>, _>>()?;
    let notes: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["type"] == "SchemaCompactedForHelm")
        .collect();
    sim_assert_eq!(have: notes.len(), want: 1);
    sim_assert_eq!(have: notes[0]["bytes"], want: json!(json_diagnostics.stdout.len()));
    Ok(())
}

#[test]
fn every_layout_writes_oversized_output_with_a_warning_and_exits_successfully() -> eyre::Result<()>
{
    let scratch = ScratchDir::new("size-aware-output-oversized")?;
    let override_path = scratch.path().join("override.json");
    let output_path = scratch.path().join("schema.json");
    std::fs::write(
        &override_path,
        serde_json::to_vec(&json!({
            "$comment": "é".repeat(HELM_MAX_CHART_FILE_BYTES / 2)
        }))?,
    )?;
    for layout in ["auto", "pretty", "compact"] {
        let stdout = run(command()
            .arg("--override-schema")
            .arg(&override_path)
            .args(["--output-layout", layout]))?;
        assert!(stdout.stdout.len() > HELM_MAX_CHART_FILE_BYTES);
        let stderr = String::from_utf8(stdout.stderr)?;
        assert!(
            stderr.contains("warning:") && stderr.contains("Helm will refuse the chart"),
            "{stderr}"
        );
        sim_assert_eq!(have: stderr.contains("note: pretty schema"), want: layout == "auto");
        std::fs::write(&output_path, b"existing file")?;
        run(command()
            .arg("--override-schema")
            .arg(&override_path)
            .args(["--output-layout", layout])
            .arg("--output")
            .arg(&output_path))?;
        sim_assert_eq!(have: std::fs::read(&output_path)?, want: stdout.stdout);
        let schema: Value = serde_json::from_slice(&stdout.stdout)?;
        let mut expected = if layout == "pretty" {
            serde_json::to_vec_pretty(&schema)?
        } else {
            serde_json::to_vec(&schema)?
        };
        expected.push(b'\n');
        sim_assert_eq!(have: stdout.stdout, want: expected);
    }
    Ok(())
}

#[test]
fn small_auto_and_pretty_output_stay_readable_and_compact_always_shortens() -> eyre::Result<()> {
    let scratch = ScratchDir::new("output-layout-small")?;
    let map_path = scratch.path().join("defs-map.json");
    let default = run(&mut command())?;
    let pretty = run(command().args(["--output-layout", "pretty"]))?;
    sim_assert_eq!(have: &default.stdout, want: &pretty.stdout);
    let readable: Value = serde_json::from_slice(&pretty.stdout)?;
    for layout in ["auto", "pretty", "compact"] {
        let output = run(command()
            .args(["--output-layout", layout])
            .arg("--defs-map")
            .arg(&map_path))?;
        let stderr = String::from_utf8(output.stderr)?;
        assert!(
            !stderr.contains("chart-file limit") && !stderr.contains("note: pretty schema"),
            "{stderr}"
        );
        let names: BTreeMap<String, String> = serde_json::from_slice(&std::fs::read(&map_path)?)?;
        let mut restored: Value = serde_json::from_slice(&output.stdout)?;
        if layout == "compact" {
            assert!(!names.is_empty());
            sim_assert_eq!(have: std::str::from_utf8(&output.stdout)?.lines().count(), want: 1);
        } else {
            assert!(names.is_empty());
            sim_assert_eq!(have: &output.stdout, want: &pretty.stdout);
        }
        rename_definitions(&mut restored, &names);
        sim_assert_eq!(have: &restored, want: &readable);
    }
    Ok(())
}

#[test]
fn legacy_flags_keep_their_independent_formatting_and_conflict_with_explicit_layouts()
-> eyre::Result<()> {
    let pretty = run(&mut command())?;
    let readable: Value = serde_json::from_slice(&pretty.stdout)?;
    for flags in [
        vec!["--compact"],
        vec!["--shorten-defs"],
        vec!["--compact", "--shorten-defs"],
    ] {
        let output = run(command().args(&flags))?;
        let expected_schema = if flags.contains(&"--shorten-defs") {
            shorten_definition_names(&readable).schema
        } else {
            readable.clone()
        };
        let mut expected = if flags.contains(&"--compact") {
            serde_json::to_vec(&expected_schema)?
        } else {
            serde_json::to_vec_pretty(&expected_schema)?
        };
        expected.push(b'\n');
        sim_assert_eq!(have: output.stdout, want: expected);
        for layout in ["auto", "pretty", "compact"] {
            let mut args = vec!["helm-schema", "/tmp/chart", "--output-layout", layout];
            args.extend(&flags);
            assert!(Cli::try_parse_from(args).is_err());
        }
    }
    let parsed = Cli::try_parse_from(["helm-schema", "/tmp/chart"])?;
    sim_assert_eq!(have: parsed.output.output_layout, want: OutputLayout::Auto);
    Ok(())
}
