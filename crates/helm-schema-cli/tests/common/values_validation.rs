use color_eyre::eyre::{self, WrapErr};
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

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

/// The chart's coalesced default values: what Helm hands the schema when the
/// user supplies nothing.
pub fn values_yaml_as_json_for_path(chart_relative_path: &str) -> eyre::Result<Value> {
    crate::chart_instances::with_override(
        chart_relative_path,
        Value::Object(serde_json::Map::new()),
    )
    .wrap_err("coalesce chart defaults")
}

pub fn assert_values_json_validates(values_json: &Value, schema: &Value) {
    let errors = validate_json_against_schema(values_json, schema);
    sim_assert_eq!(have: errors, want: Vec::<String>::new());
}
