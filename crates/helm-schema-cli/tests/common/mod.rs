use std::io::Read;

use color_eyre::eyre::{Report, WrapErr};
use helm_schema_cli::{GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
use serde_json::Value;
use vfs::VfsPath;

fn chart_dir(chart: &str) -> std::result::Result<VfsPath, Report> {
    let chart_dir = test_util::workspace_testdata().join("charts").join(chart);
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    Ok(VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str)))
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
    let Ok(validator) = jsonschema::validator_for(schema) else {
        return vec!["failed to compile JSON schema".to_string()];
    };

    validator
        .iter_errors(instance)
        .map(|e| format!("{path}: {msg}", path = e.instance_path, msg = e))
        .collect()
}

pub fn assert_chart_values_yaml_validates(chart: &str) -> std::result::Result<(), Report> {
    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = chart_dir(chart).wrap_err("chart_dir")?;

    let opts = GenerateOptions {
        chart_dir: chart_dir.clone(),
        include_tests: false,
        include_subchart_values: true,
        provider: ProviderOptions {
            k8s_version: "v1.29.0-standalone-strict".to_string(),
            k8s_schema_cache_dir: Some(
                test_util::workspace_root().join("deprecated/crates/helm-schema-mapper/testdata/kubernetes-json-schema"),
            ),
            allow_net: false,
            disable_k8s_schemas: false,
            crd_catalog_dir: Some(test_util::workspace_root().join("target/helm-schema-test-crds-catalog-cache")),
        },
    };

    let schema = generate_values_schema_for_chart(&opts)
        .map_err(Report::from)
        .wrap_err("generate schema")?;

    let values_yaml = read_values_yaml(&chart_dir).wrap_err("read values.yaml")?;
    let values_json: Value = serde_yaml::from_str(&values_yaml).wrap_err("parse values.yaml")?;
    let values_json = drop_nulls(&values_json);

    let errors = validate_json_against_schema(&values_json, &schema);
    similar_asserts::assert_eq!(errors, Vec::<String>::new());

    Ok(())
}
