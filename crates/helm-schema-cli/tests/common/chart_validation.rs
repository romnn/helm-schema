use std::io::Read;

use color_eyre::eyre::{Report, WrapErr};
use helm_schema::AnalysisSession;
use helm_schema_cli::{GenerateOptions, ProviderOptions};
use serde_json::Value;
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

fn chart_dir(chart_relative_path: &str) -> VfsPath {
    let chart_dir = test_util::workspace_testdata()
        .join("charts")
        .join(chart_relative_path);
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

fn drop_nulls(v: &Value) -> Value {
    match v {
        Value::Null => Value::Null,
        Value::Bool(_) | Value::Number(_) | Value::String(_) => v.clone(),
        Value::Array(arr) => Value::Array(
            arr.iter()
                .filter(|x| !x.is_null())
                .map(drop_nulls)
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if v.is_null() {
                    continue;
                }
                out.insert(k.clone(), drop_nulls(v));
            }
            Value::Object(out)
        }
    }
}

fn validate_json_against_schema(instance: &Value, schema: &Value) -> Vec<String> {
    let validator = match jsonschema::validator_for(schema) {
        Ok(validator) => validator,
        Err(err) => return vec![format!("failed to compile JSON schema: {err}")],
    };

    validator
        .iter_errors(instance)
        .map(|e| format!("{path}: {msg}", path = e.instance_path(), msg = e))
        .collect()
}

fn generate_chart_schema_for_path(chart_relative_path: &str) -> std::result::Result<Value, Report> {
    let _guard = test_util::builder().with_tracing(false).build();

    let opts = GenerateOptions {
        chart_dir: chart_dir(chart_relative_path),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.29.0-standalone-strict".to_string()],
            k8s_schema_cache_dir: Some(
                test_util::workspace_root().join(".cache/kubernetes-json-schema-cache"),
            ),
            allow_net: false,
            disable_k8s_schemas: false,
            crd_override_dir: Some(test_util::workspace_root().join(".cache/crds-catalog-cache")),
            ..Default::default()
        },
    };

    AnalysisSession::new(opts)
        .generated_schema()
        .map(|generated| generated.schema)
        .map_err(Report::from)
        .wrap_err("generate schema")
}

pub fn assert_chart_values_yaml_validates(
    chart_relative_path: &str,
) -> std::result::Result<(), Report> {
    let chart_dir = chart_dir(chart_relative_path);
    let schema = generate_chart_schema_for_path(chart_relative_path)?;

    let values_yaml = read_values_yaml(&chart_dir).wrap_err("read values.yaml")?;
    let values_json: Value = serde_yaml::from_str(&values_yaml).wrap_err("parse values.yaml")?;
    let values_json = drop_nulls(&values_json);
    let errors = validate_json_against_schema(&values_json, &schema);
    sim_assert_eq!(have: errors, want: Vec::<String>::new());

    Ok(())
}
