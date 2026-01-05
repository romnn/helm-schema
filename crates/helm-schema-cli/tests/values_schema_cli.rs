use assert_cmd::Command;
use color_eyre::eyre;
use helm_schema_chart::{load_chart, LoadOptions};
use helm_schema_mapper::schema::{
    DefaultVytSchemaProvider, UpstreamK8sSchemaProvider, UpstreamThenDefaultVytSchemaProvider,
};
use helm_schema_mapper::{
    generate_values_schema_for_chart_vyt_with_options, GenerateValuesSchemaOptions,
};
use serde_json::{Map, Value};
use std::fs;
use std::path::PathBuf;
use vfs::VfsPath;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn canonicalize_json(v: &Value) -> Value {
    match v {
        Value::Object(obj) => {
            let mut keys: Vec<&String> = obj.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                if let Some(val) = obj.get(k) {
                    out.insert(k.clone(), canonicalize_json(val));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonicalize_json).collect()),
        _ => v.clone(),
    }
}

fn expected_schema_json(ws: &PathBuf, chart_dir: &PathBuf) -> eyre::Result<Value> {
    let chart_root = VfsPath::new(vfs::PhysicalFS::new(ws)).join(
        chart_dir
            .strip_prefix(ws)
            .map_err(|e| eyre::eyre!(e))?
            .to_string_lossy()
            .as_ref(),
    )?;

    let chart = load_chart(
        &chart_root,
        &LoadOptions {
            include_tests: false,
            recurse_subcharts: true,
            auto_extract_tgz: true,
            respect_gitignore: false,
            include_hidden: false,
        },
    )?;

    let upstream = UpstreamK8sSchemaProvider::new("v1.29.0-standalone-strict")
        .with_cache_dir(ws.join("crates/helm-schema-mapper/testdata/kubernetes-json-schema"))
        .with_allow_download(false);
    let provider = UpstreamThenDefaultVytSchemaProvider {
        upstream,
        fallback: DefaultVytSchemaProvider::default(),
    };

    let options = GenerateValuesSchemaOptions {
        add_values_yaml_baseline: true,
        compose_subcharts: true,
        ingest_values_schema_json: false,
    };

    generate_values_schema_for_chart_vyt_with_options(&chart, &provider, &options)
        .map_err(Into::into)
}

fn workspace_root() -> PathBuf {
    crate_root()
        .parent()
        .expect("crates/helm-schema-cli")
        .parent()
        .expect("crates")
        .to_path_buf()
}

#[test]
fn values_schema_matches_full_fixture_snapshot() -> eyre::Result<()> {
    let ws = workspace_root();
    let chart_dir = ws.join("crates/helm-schema-mapper/testdata/fixture-charts/full-fixture");
    let expected = expected_schema_json(&ws, &chart_dir)?;

    let mut cmd = Command::cargo_bin("helm-schema")?;
    let out_bytes = cmd
        .arg("values-schema")
        .arg(chart_dir)
        .arg("--k8s-schema-cache")
        .arg(ws.join("crates/helm-schema-mapper/testdata/kubernetes-json-schema"))
        .arg("--k8s-version")
        .arg("v1.29.0-standalone-strict")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let out = String::from_utf8(out_bytes)?;
    let out_json: Value = serde_json::from_str(&out)?;
    eyre::ensure!(
        canonicalize_json(&out_json) == canonicalize_json(&expected),
        "CLI output did not match library generator"
    );
    Ok(())
}

#[test]
fn values_schema_matches_hard_fixture_snapshot() -> eyre::Result<()> {
    let ws = workspace_root();
    let chart_dir = ws.join("crates/helm-schema-mapper/testdata/fixture-charts/hard-fixture");
    let expected = expected_schema_json(&ws, &chart_dir)?;

    let mut cmd = Command::cargo_bin("helm-schema")?;
    let out_bytes = cmd
        .arg("values-schema")
        .arg(chart_dir)
        .arg("--k8s-schema-cache")
        .arg(ws.join("crates/helm-schema-mapper/testdata/kubernetes-json-schema"))
        .arg("--k8s-version")
        .arg("v1.29.0-standalone-strict")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let out = String::from_utf8(out_bytes)?;
    let out_json: Value = serde_json::from_str(&out)?;
    eyre::ensure!(
        canonicalize_json(&out_json) == canonicalize_json(&expected),
        "CLI output did not match library generator"
    );
    Ok(())
}
