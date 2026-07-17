use color_eyre::eyre::{Report, WrapErr};
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

/// Delete null-valued map keys along MAP chains only. Helm's coalescing
/// treats lists atomically — a list value replaces wholesale and its
/// members (including nulls and null-valued keys inside them) reach the
/// template verbatim — so recursion must stop at arrays.
fn drop_nulls(v: &Value) -> Value {
    match v {
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
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            v.clone()
        }
    }
}

/// Validate a JSON value against a JSON schema.
///
/// Returns a list of human-readable validation error strings.
/// An empty list means validation passed.
pub fn validate_json_against_schema(instance: &Value, schema: &Value) -> Vec<String> {
    let validator = match jsonschema::validator_for(schema) {
        Ok(validator) => validator,
        Err(err) => return vec![format!("failed to compile JSON schema: {err}")],
    };

    validator
        .iter_errors(instance)
        .map(|e| format!("{path}: {msg}", path = e.instance_path(), msg = e))
        .collect()
}

pub fn values_yaml_as_json_for_path(
    chart_relative_path: &str,
) -> std::result::Result<Value, Report> {
    let values_yaml = crate::schema_roundtrip::read_values_yaml_for_path(chart_relative_path)
        .wrap_err("read values.yaml")?;
    let values_json: Value = serde_yaml::from_str(&values_yaml).wrap_err("parse values.yaml")?;
    Ok(drop_nulls(&values_json))
}

pub fn assert_values_json_validates(values_json: &Value, schema: &Value) {
    let errors = validate_json_against_schema(values_json, schema);
    sim_assert_eq!(have: errors, want: Vec::<String>::new());
}
