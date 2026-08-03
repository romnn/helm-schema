//! End-to-end regressions for reference bundling across multiple overrides.

use std::process::Command;

use color_eyre::eyre::{self, WrapErr as _};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

const HELM_SCHEMA_BIN: &str = env!("CARGO_BIN_EXE_helm-schema");

#[test]
fn multiple_override_external_refs_use_distinct_bundled_definitions() -> eyre::Result<()> {
    let temp = tempfile::tempdir()?;
    let chart = temp.path().join("chart");
    std::fs::create_dir_all(chart.join("templates"))?;
    std::fs::write(
        chart.join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: override-bundle
            version: 0.1.0
        "},
    )?;
    std::fs::write(chart.join("values.yaml"), "{}\n")?;
    std::fs::write(temp.path().join("alpha.json"), r#"{"const":"alpha"}"#)?;
    std::fs::write(temp.path().join("beta.json"), r#"{"const":7}"#)?;
    let alpha_override = temp.path().join("alpha-override.json");
    std::fs::write(
        &alpha_override,
        r#"{"properties":{"alpha":{"$ref":"./alpha.json"}}}"#,
    )?;
    let beta_override = temp.path().join("beta-override.json");
    std::fs::write(
        &beta_override,
        r#"{"properties":{"beta":{"$ref":"./beta.json"}}}"#,
    )?;

    let output = Command::new(HELM_SCHEMA_BIN)
        .args(["--offline", "--no-k8s-schemas"])
        .arg("--override-schema")
        .arg(&alpha_override)
        .arg("--override-schema")
        .arg(&beta_override)
        .arg(&chart)
        .output()
        .wrap_err("run helm-schema with two external-ref overrides")?;
    if !output.status.success() {
        return Err(eyre::eyre!(
            "helm-schema failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let expected = serde_json::json!({
        "$defs": {
            "schema1": { "const": "alpha" },
            "schema2": { "const": 7 },
        },
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "alpha": { "$ref": "#/$defs/schema1" },
            "beta": { "$ref": "#/$defs/schema2" },
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "modifiers": {
                "overrides": {
                    "count": 2,
                    "digest": "dbf0b44743927824e4eda08f8e568c5673ae33af232ad5dd4f52023874abf99a",
                },
                "reference-mode": "bundled",
            },
            "narrowing": [],
            "policy-fingerprint": "7fca3b4bb01f00fc18128195e45cb8ef77cb0ec58b1d2de7ecbb2bf86fd3c1d8",
            "policy-vocabulary-version": 1,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true,
            },
        },
    });

    sim_assert_eq!(have: actual, want: expected);
    Ok(())
}
