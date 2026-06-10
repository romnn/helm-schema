use std::io::Read;

use color_eyre::eyre::{Report, WrapErr};
use helm_schema_ast::extract_values_yaml_descriptions;
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
        .map(|e| format!("{path}: {msg}", path = e.instance_path(), msg = e))
        .collect()
}

pub fn generate_chart_schema(chart: &str) -> std::result::Result<Value, Report> {
    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = chart_dir(chart);

    let opts = GenerateOptions {
        chart_dir: chart_dir.clone(),
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

    generate_values_schema_for_chart(&opts)
        .map_err(Report::from)
        .wrap_err("generate schema")
}

#[allow(dead_code)]
pub fn assert_chart_values_yaml_validates(chart: &str) -> std::result::Result<(), Report> {
    let chart_dir = chart_dir(chart);
    let schema = generate_chart_schema(chart)?;

    let values_yaml = read_values_yaml(&chart_dir).wrap_err("read values.yaml")?;
    let values_json: Value = serde_yaml::from_str(&values_yaml).wrap_err("parse values.yaml")?;
    let values_json = drop_nulls(&values_json);

    assert_values_json_validates(&values_json, &schema);

    Ok(())
}

#[allow(dead_code)]
pub fn values_yaml_as_json(chart: &str) -> std::result::Result<Value, Report> {
    let chart_dir = chart_dir(chart);
    let values_yaml = read_values_yaml(&chart_dir).wrap_err("read values.yaml")?;
    let values_json: Value = serde_yaml::from_str(&values_yaml).wrap_err("parse values.yaml")?;
    Ok(drop_nulls(&values_json))
}

#[allow(dead_code)]
pub fn assert_values_json_validates(values_json: &Value, schema: &Value) {
    let errors = validate_json_against_schema(values_json, schema);
    similar_asserts::assert_eq!(errors, Vec::<String>::new());
}

#[allow(dead_code)]
pub fn assert_chart_values_comments_apply_to_existing_schema_paths(
    chart: &str,
    schema: &Value,
    min_applied: usize,
) -> std::result::Result<(), Report> {
    let chart_dir = chart_dir(chart);
    let values_yaml = read_values_yaml(&chart_dir).wrap_err("read values.yaml")?;
    let descriptions =
        extract_values_yaml_descriptions(&values_yaml).wrap_err("extract values comments")?;

    let mut applied = 0;
    for (path, expected_description) in descriptions {
        let Some(schema_node) = schema_node_for_values_path(schema, &path) else {
            continue;
        };
        applied += 1;
        similar_asserts::assert_eq!(
            schema_node
                .get("description")
                .and_then(serde_json::Value::as_str),
            Some(expected_description.as_str()),
            "description mismatch for values path {path}"
        );
    }

    assert!(
        applied >= min_applied,
        "expected at least {min_applied} values comments to apply for {chart}, got {applied}"
    );
    Ok(())
}

fn schema_node_for_values_path<'schema>(
    schema: &'schema Value,
    path: &str,
) -> Option<&'schema Value> {
    let mut current = schema;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = if segment == "*" {
            current.get("items")?
        } else {
            current.get("properties")?.get(segment)?
        };
    }
    Some(current)
}
