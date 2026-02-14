use std::path::Path;

use clap::Parser;
use color_eyre::eyre::{Result, WrapErr, eyre};
use helm_schema_cli::{Cli, GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
use vfs::VfsPath;

#[test]
fn cli_parses_defaults() -> Result<()> {
    let cli =
        Cli::try_parse_from(["helm-schema", "/tmp/chart"]).map_err(|e| eyre!(e.to_string()))?;

    assert_eq!(cli.k8s.k8s_version, "v1.35.0");
    assert!(cli.output.output.is_none());
    assert!(!cli.k8s.offline);
    assert!(!cli.k8s.no_k8s_schemas);
    assert!(!cli.chart.exclude_tests);
    assert!(!cli.chart.no_subchart_values);
    assert!(cli.override_schema.is_none());
    assert!(!cli.output.compact);

    Ok(())
}

#[test]
fn generates_schema_for_fixture_chart_without_k8s_provider() -> Result<()> {
    let chart_dir = test_util::workspace_testdata().join("fixture-charts/full-fixture");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        provider: ProviderOptions {
            k8s_version: "v1.35.0".to_string(),
            k8s_schema_cache_dir: None,
            allow_net: true,
            disable_k8s_schemas: true,
            crd_catalog_dir: None,
        },
    };

    let actual = generate_values_schema_for_chart(&opts).wrap_err("generate schema")?;

    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/full_fixture.disable_k8s.schema.json"
    ))
    .wrap_err("parse expected schema fixture")?;

    similar_asserts::assert_eq!(actual, expected);
    Ok(())
}

#[test]
fn subchart_values_are_scoped_and_global_is_merged() -> Result<()> {
    let dir = tempfile::tempdir().wrap_err("tempdir")?;
    let root = dir.path();

    write_file(
        &root.join("Chart.yaml"),
        "apiVersion: v2\nname: root\nversion: 0.1.0\ndependencies:\n  - name: child\n    alias: kid\n    version: 0.1.0\n",
    )?;
    write_file(&root.join("values.yaml"), "{}\n")?;

    let child = root.join("charts").join("child");
    std::fs::create_dir_all(child.join("templates")).wrap_err("create dirs")?;

    write_file(
        &child.join("Chart.yaml"),
        "apiVersion: v2\nname: child\nversion: 0.1.0\n",
    )?;

    write_file(&child.join("values.yaml"), "foo: 1\nglobal:\n  bar: true\n")?;

    write_file(
        &child.join("templates").join("configmap.yaml"),
        "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: test\ndata:\n  foo: {{ .Values.foo | quote }}\n  bar: {{ .Values.global.bar | quote }}\n",
    )?;

    let opts = GenerateOptions {
        chart_dir: {
            let s = root.to_string_lossy().to_string();
            VfsPath::new(vfs::PhysicalFS::new(&s))
        },
        include_tests: false,
        include_subchart_values: true,
        provider: ProviderOptions {
            k8s_version: "v1.35.0".to_string(),
            k8s_schema_cache_dir: None,
            allow_net: true,
            disable_k8s_schemas: true,
            crd_catalog_dir: None,
        },
    };

    let actual = generate_values_schema_for_chart(&opts).wrap_err("generate schema")?;

    let expected = serde_json::json!({
      "$schema": "http://json-schema.org/draft-07/schema#",
      "additionalProperties": false,
      "properties": {
        "global": {
          "additionalProperties": false,
          "properties": {
            "bar": {
              "type": "boolean"
            }
          },
          "type": "object"
        },
        "kid": {
          "additionalProperties": false,
          "properties": {
            "foo": {
              "type": "integer"
            }
          },
          "type": "object"
        }
      },
      "type": "object"
    });

    similar_asserts::assert_eq!(actual, expected);
    Ok(())
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).wrap_err("create parent dir")?;
    }
    std::fs::write(path, content).wrap_err("write file")?;
    Ok(())
}
