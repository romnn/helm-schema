//! Readable `$defs` names in every committed schema, and the explicit
//! shortening that is the only way to short keys.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser as _;
use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema::output::HELM_MAX_CHART_FILE_BYTES;
use helm_schema_cli::Cli;
use helm_schema_json_schema_minify::{rename_definitions, shorten_definition_names};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

const HELM_SCHEMA_BIN: &str = env!("CARGO_BIN_EXE_helm-schema");

/// Every committed schema fixture that went through the output pipeline.
fn schema_fixtures() -> eyre::Result<Vec<PathBuf>> {
    let testdata = test_util::workspace_testdata();
    let mut fixtures = Vec::new();
    for dir in [
        "chart-corpus-schemas",
        "final-output-schemas",
        "emission-profile-schemas/lean",
    ] {
        for entry in std::fs::read_dir(testdata.join(dir)).wrap_err_with(|| dir.to_string())? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                fixtures.push(path);
            }
        }
    }
    fixtures.sort();
    Ok(fixtures)
}

fn read_json(path: &Path) -> eyre::Result<Value> {
    let bytes = std::fs::read(path).wrap_err_with(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).wrap_err_with(|| format!("parse {}", path.display()))
}

fn definition_names(schema: &Value) -> Vec<String> {
    schema
        .get("$defs")
        .and_then(Value::as_object)
        .map(|definitions| definitions.keys().cloned().collect())
        .unwrap_or_default()
}

/// Whether `name` looks like a content hash, a private handle, or a short key.
fn is_unreadable(name: &str) -> bool {
    let hex_tail =
        |text: &str| text.len() == 12 && text.bytes().all(|byte| byte.is_ascii_hexdigit());
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    let hashed = name.strip_prefix('h').is_some_and(hex_tail)
        || name
            .rsplit_once('_')
            .is_some_and(|(_, tail)| hex_tail(tail));
    let handle = ["providerSchema", "providerShared"]
        .iter()
        .any(|prefix| name.strip_prefix(prefix).is_some_and(digits))
        || name.starts_with("providerSource_");
    // `t` is the generator's own name for its Helm-truthiness definition.
    let short = name != "t"
        && (1..=3).contains(&name.len())
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric());
    hashed || handle || short
}

#[test]
fn committed_fixtures_carry_readable_definition_names() -> eyre::Result<()> {
    let mut unreadable = BTreeMap::new();
    let mut checked = 0;
    for path in schema_fixtures()? {
        let names = definition_names(&read_json(&path)?);
        checked += names.len();
        let bad = names
            .into_iter()
            .filter(|name| is_unreadable(name))
            .take(5)
            .collect::<Vec<_>>();
        if !bad.is_empty() {
            unreadable.insert(path.display().to_string(), bad);
        }
    }
    assert!(checked > 10_000, "only {checked} definition names checked");
    sim_assert_eq!(have: unreadable, want: BTreeMap::new());
    Ok(())
}

#[test]
fn unreadable_name_shapes_are_recognized() {
    for name in [
        "h0d6da7ea663d",
        "providerSchema_2d8986a10330",
        "providerSource_k8s_0123456789ab",
        "providerShared12",
        "1b",
        "Zz",
    ] {
        assert!(is_unreadable(name), "{name}");
    }
    for name in [
        "t",
        "helm-double-quoted-safe",
        "values/agents.containers",
        "k8s/io.k8s.api.core.v1.Probe",
        "values/@allOf(3)@then.web",
    ] {
        assert!(!is_unreadable(name), "{name}");
    }
}

#[test]
fn the_five_largest_fixtures_shorten_under_the_helm_limit() -> eyre::Result<()> {
    let mut sized = Vec::new();
    for path in schema_fixtures()? {
        sized.push((std::fs::metadata(&path)?.len(), path));
    }
    sized.sort();
    for (readable_bytes, path) in sized.iter().rev().take(5) {
        let readable = read_json(path)?;
        let shortened = shorten_definition_names(&readable);
        let mut shipped = serde_json::to_vec(&shortened.schema)?;
        shipped.push(b'\n');
        assert!(
            shipped.len() <= HELM_MAX_CHART_FILE_BYTES,
            "{} shortens from {readable_bytes} to {} bytes",
            path.display(),
            shipped.len()
        );
        sim_assert_eq!(
            have: shortened.readable_names.len(),
            want: definition_names(&readable).len()
        );
        let mut restored = shortened.schema;
        rename_definitions(&mut restored, &shortened.readable_names);
        sim_assert_eq!(have: restored, want: readable);
    }
    Ok(())
}

fn run(args: &[&str]) -> eyre::Result<std::process::Output> {
    let output = Command::new(HELM_SCHEMA_BIN)
        .args(args)
        .output()
        .wrap_err("run helm-schema")?;
    eyre::ensure!(
        output.status.success(),
        "helm-schema {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

fn path_arg(path: &Path) -> eyre::Result<&str> {
    path.to_str().ok_or_eyre("UTF-8 path expected")
}

fn readable_schema(padding: usize) -> Value {
    json!({
        "$defs": {
            "values/web.securityContext": { "type": "object" },
            "k8s/io.k8s.api.core.v1.Probe": { "type": "object" }
        },
        "description": "x".repeat(padding),
        "properties": {
            "web": { "properties": { "securityContext": { "$ref": "#/$defs/values~1web.securityContext" } } },
            "probe": { "$ref": "#/$defs/k8s~1io.k8s.api.core.v1.Probe" },
            "probe2": { "$ref": "#/$defs/k8s~1io.k8s.api.core.v1.Probe" }
        },
        "type": "object"
    })
}

#[test]
fn shorten_writes_short_keys_and_the_map_back() -> eyre::Result<()> {
    let dir = ScratchDir::new("defs_names")?;
    let input = dir.path().join("values.schema.json");
    let output = dir.path().join("helm.schema.json");
    let map = dir.path().join("defs-map.json");
    let readable = readable_schema(0);
    std::fs::write(&input, serde_json::to_vec_pretty(&readable)?)?;

    let result = run(&[
        "shorten",
        path_arg(&input)?,
        path_arg(&output)?,
        "--map",
        path_arg(&map)?,
        "--compact",
    ])?;

    sim_assert_eq!(have: String::from_utf8_lossy(&result.stderr).into_owned(), want: String::new());
    let shortened = read_json(&output)?;
    let readable_names: BTreeMap<String, String> = serde_json::from_value(read_json(&map)?)?;
    sim_assert_eq!(
        have: readable_names.clone(),
        want: BTreeMap::from([
            ("1".to_string(), "k8s/io.k8s.api.core.v1.Probe".to_string()),
            ("2".to_string(), "values/web.securityContext".to_string()),
        ])
    );
    sim_assert_eq!(
        have: definition_names(&shortened),
        want: vec!["1".to_string(), "2".to_string()]
    );
    let mut restored = shortened;
    rename_definitions(&mut restored, &readable_names);
    sim_assert_eq!(have: restored, want: readable);
    Ok(())
}

#[test]
fn oversized_output_keeps_its_bytes_and_warns() -> eyre::Result<()> {
    let dir = ScratchDir::new("defs_names")?;
    let input = dir.path().join("values.schema.json");
    let output = dir.path().join("helm.schema.json");
    std::fs::write(
        &input,
        serde_json::to_vec_pretty(&readable_schema(HELM_MAX_CHART_FILE_BYTES))?,
    )?;

    let result = run(&["shorten", path_arg(&input)?, path_arg(&output)?])?;

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("even with short definition names")
            && stderr.contains(&format!("over Helm's {HELM_MAX_CHART_FILE_BYTES}-byte")),
        "unexpected stderr: {stderr}"
    );
    let shortened = shorten_definition_names(&read_json(&input)?);
    let mut want = serde_json::to_vec_pretty(&shortened.schema)?;
    want.push(b'\n');
    sim_assert_eq!(have: std::fs::read(&output)?, want: want);
    Ok(())
}

#[test]
fn generate_shortens_only_on_request_and_names_the_short_keys() -> eyre::Result<()> {
    let dir = ScratchDir::new("defs_names")?;
    let chart = test_util::workspace_testdata().join("fixture-charts/full-fixture");
    let readable_path = dir.path().join("readable.json");
    let shortened_path = dir.path().join("shortened.json");
    let map = dir.path().join("defs-map.json");
    let common = ["--offline", "--no-k8s-schemas", "--no-config"];

    let mut readable_args = vec![path_arg(&chart)?, "-o", path_arg(&readable_path)?];
    readable_args.extend(common);
    run(&readable_args)?;
    let mut shortened_args = vec![
        path_arg(&chart)?,
        "-o",
        path_arg(&shortened_path)?,
        "--shorten-defs",
        "--defs-map",
        path_arg(&map)?,
    ];
    shortened_args.extend(common);
    run(&shortened_args)?;

    let readable = read_json(&readable_path)?;
    let names = definition_names(&readable);
    assert!(!names.is_empty(), "the fixture chart has definitions");
    assert!(
        names.iter().all(|name| !is_unreadable(name)),
        "readable by default: {names:?}"
    );
    let readable_names: BTreeMap<String, String> = serde_json::from_value(read_json(&map)?)?;
    sim_assert_eq!(
        have: readable_names.values().cloned().collect::<BTreeSet<_>>(),
        want: names.into_iter().collect::<BTreeSet<_>>()
    );
    let mut restored = read_json(&shortened_path)?;
    rename_definitions(&mut restored, &readable_names);
    sim_assert_eq!(have: restored, want: readable);
    Ok(())
}

#[test]
fn definition_name_options_parse() -> eyre::Result<()> {
    let cli = Cli::try_parse_from(["helm-schema", "/tmp/chart", "--defs-names", "destination"])?;
    sim_assert_eq!(have: cli.output.defs_names, want: helm_schema_cli::cli::DefsNames::Destination);
    assert!(!cli.output.shorten_defs);

    let map_without_shortening =
        Cli::try_parse_from(["helm-schema", "/tmp/chart", "--defs-map", "/tmp/map.json"]);
    assert!(
        map_without_shortening.is_err(),
        "--defs-map requires --shorten-defs"
    );

    let shorten = Cli::try_parse_from(["helm-schema", "shorten", "in.json", "out.json"])?;
    assert!(
        matches!(
            shorten.command,
            Some(helm_schema_cli::cli::Command::Shorten(_))
        ),
        "{shorten:?}"
    );
    sim_assert_eq!(have: shorten.chart_dir, want: None);
    assert!(
        Cli::try_parse_from(["helm-schema"]).is_err(),
        "a chart directory is required without a command"
    );
    Ok(())
}

#[test]
fn helm_logs_are_translated_by_lint_and_template_not_expand_defs() -> eyre::Result<()> {
    let expand = Cli::try_parse_from([
        "helm-schema",
        "expand-defs",
        "--map",
        "defs-map.json",
        "helm.log",
        "helm.readable.log",
    ]);
    assert!(expand.is_err(), "{expand:?}");
    let help = Command::new(HELM_SCHEMA_BIN).arg("--help").output()?;
    let help = String::from_utf8(help.stdout)?;
    assert!(!help.contains("expand-defs"), "{help}");
    assert!(
        help.contains("  lint ") && help.contains("  template "),
        "{help}"
    );
    Ok(())
}
