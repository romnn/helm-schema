use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// Recursively remove `"additionalProperties": false` from a JSON schema.
///
/// Our generated schemas are per-template and use `additionalProperties: false`
/// to flag unknown keys. However, the chart's `values.yaml` contains values for
/// ALL templates, so a per-template schema will reject keys it doesn't cover.
/// Relaxing the schema lets us validate that the types/structure of the values
/// we *do* cover are correct without false positives from unrelated keys.
pub fn relax_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if k == "additionalProperties" && *v == Value::Bool(false) {
                    continue;
                }
                out.insert(k.clone(), relax_schema(v));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(relax_schema).collect()),
        other => other.clone(),
    }
}

/// Parse a `values.yaml` string into a [`serde_json::Value`].
///
/// Returns the top-level mapping as a JSON object, or panics on parse failure.
pub fn values_yaml_to_json(values_yaml: &str) -> Value {
    let yaml: Value = serde_yaml::from_str(values_yaml).expect("parse values.yaml as JSON");
    yaml
}

/// Validate a JSON value against a JSON schema.
///
/// Returns a list of human-readable validation error strings.
/// An empty list means validation passed.
pub fn validate_json_against_schema(instance: &Value, schema: &Value) -> Vec<String> {
    let Ok(validator) = jsonschema::validator_for(schema) else {
        return vec!["failed to compile JSON schema".to_string()];
    };
    validator
        .iter_errors(instance)
        .map(|e| format!("{path}: {msg}", path = e.instance_path, msg = e))
        .collect()
}

/// Validate a `values.yaml` string against a generated JSON schema.
///
/// The schema is first relaxed (removing `additionalProperties: false`) so that
/// values for other templates don't cause false positives. Returns a list of
/// validation errors (empty = pass).
pub fn validate_values_yaml(values_yaml: &str, schema: &Value) -> Vec<String> {
    let json_values = values_yaml_to_json(values_yaml);
    let relaxed = relax_schema(schema);
    validate_json_against_schema(&json_values, &relaxed)
}

#[allow(dead_code)]
/// Run `helm template` on a chart directory, optionally showing only a specific template.
///
/// Returns `Ok(rendered_yaml)` on success, or `Err(stderr)` on failure.
/// If `helm` is not installed or the chart can't be rendered, returns an error.
pub fn helm_template_render(chart_dir: &Path, show_only: Option<&str>) -> Result<String, String> {
    let mut cmd = Command::new("helm");
    cmd.arg("template").arg("test-release").arg(chart_dir);

    if let Some(template) = show_only {
        cmd.arg("--show-only").arg(template);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run helm: {e}"))?;

    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|e| format!("helm output is not valid UTF-8: {e}"))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // helm template prints errors to stdout sometimes
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "helm template failed:\nstderr: {stderr}\nstdout: {stdout}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relax_removes_additional_properties_false() {
        let schema = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "foo": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "bar": { "type": "string" }
                    }
                }
            }
        });
        let relaxed = relax_schema(&schema);
        assert_eq!(
            relaxed,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "foo": {
                        "type": "object",
                        "properties": {
                            "bar": { "type": "string" }
                        }
                    }
                }
            })
        );
    }

    #[test]
    fn relax_keeps_additional_properties_object() {
        let schema = serde_json::json!({
            "type": "object",
            "additionalProperties": { "type": "string" }
        });
        let relaxed = relax_schema(&schema);
        assert_eq!(relaxed, schema);
    }
}
