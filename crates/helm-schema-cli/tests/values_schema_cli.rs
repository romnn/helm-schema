use assert_cmd::Command;
use color_eyre::eyre;
use std::fs;
use std::path::PathBuf;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
    let snapshot = ws.join(
        "crates/helm-schema-mapper/tests/snapshots/full-fixture.values.schema.json",
    );

    let expected = fs::read_to_string(snapshot)?;

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
    eyre::ensure!(out == expected, "CLI output did not match snapshot");
    Ok(())
}

#[test]
fn values_schema_matches_hard_fixture_snapshot() -> eyre::Result<()> {
    let ws = workspace_root();
    let chart_dir = ws.join("crates/helm-schema-mapper/testdata/fixture-charts/hard-fixture");
    let snapshot = ws.join(
        "crates/helm-schema-mapper/tests/snapshots/hard-fixture.values.schema.json",
    );

    let expected = fs::read_to_string(snapshot)?;

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
    eyre::ensure!(out == expected, "CLI output did not match snapshot");
    Ok(())
}
