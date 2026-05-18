//! Regression test for the `--infer-required` × seeded-top-level-keys bug.
//!
//! The CLI seeds one synthetic empty-path/empty-guards `ValueUse` per
//! top-level `values.yaml` key so the schema enumerates that key as a
//! property even when no template references it. Those seeded uses look
//! identical to real unconditional `if .Values.X` header uses, so the
//! `required`-inference predicate used to misclassify every top-level key
//! as `required`. This test pins the fixed behaviour.

use std::io::Read;

use color_eyre::eyre::{Report, WrapErr};
use helm_schema_cli::{GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
use serde_json::Value;
use vfs::VfsPath;

fn chart_dir(chart: &str) -> VfsPath {
    let chart_dir = test_util::workspace_testdata().join("charts").join(chart);
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str))
}

fn read_values_yaml(chart_dir: &VfsPath) -> std::result::Result<String, Report> {
    let mut out = String::new();
    chart_dir
        .join("values.yaml")
        .wrap_err("join values.yaml")?
        .open_file()
        .wrap_err("open values.yaml")?
        .read_to_string(&mut out)
        .wrap_err("read values.yaml")?;
    Ok(out)
}

fn top_level_values_keys(values_yaml: &str) -> Vec<String> {
    let doc: serde_yaml::Value = serde_yaml::from_str(values_yaml).expect("parse values.yaml");
    let serde_yaml::Value::Mapping(m) = doc else {
        return Vec::new();
    };
    m.into_iter()
        .filter_map(|(k, _)| k.as_str().map(str::to_string))
        .collect()
}

#[test]
fn infer_required_skips_synthetic_top_level_value_keys() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let dir = chart_dir("cert-manager");
    let opts = GenerateOptions {
        chart_dir: dir.clone(),
        include_tests: false,
        include_subchart_values: true,
        infer_required: true,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.29.0-standalone-strict".to_string()],
            k8s_schema_cache_dir: Some(
                test_util::workspace_root()
                    .join("deprecated/crates/helm-schema-mapper/testdata/kubernetes-json-schema"),
            ),
            allow_net: false,
            disable_k8s_schemas: false,
            crd_override_dir: Some(
                test_util::workspace_root().join("target/helm-schema-test-crds-catalog-cache"),
            ),
            ..Default::default()
        },
    };

    let schema = generate_values_schema_for_chart(&opts)
        .map_err(Report::from)
        .wrap_err("generate schema")?;

    let values_yaml = read_values_yaml(&dir).wrap_err("read values.yaml")?;
    let seeded: std::collections::BTreeSet<String> =
        top_level_values_keys(&values_yaml).into_iter().collect();

    // The root schema must not declare any of the seeded top-level keys as
    // required — they're a completeness crutch, not real header references.
    let required: Vec<String> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let offenders: Vec<&String> = required.iter().filter(|p| seeded.contains(*p)).collect();
    assert!(
        offenders.is_empty(),
        "root `required` includes seeded top-level keys: {offenders:?}\nfull required list: {required:?}",
    );

    Ok(())
}
