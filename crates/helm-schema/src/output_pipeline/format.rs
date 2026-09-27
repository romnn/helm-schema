use std::collections::BTreeSet;
use std::io::Write;

use serde_json::Value;

use crate::error::EngineResult;
use crate::output_pipeline::JsonOutputFormat;

/// Helm refuses to load any chart file larger than 5 MiB, and a chart's
/// `values.schema.json` counts against that limit.
///
/// The writer never changes a document to fit: callers compare
/// [`FinalOutputMetrics::serialized_bytes`] against this limit and shorten
/// definition names explicitly when they ship to Helm.
pub const HELM_MAX_CHART_FILE_BYTES: usize = 5 * 1024 * 1024;

/// Measurements of the exact final document written for Helm to compile.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FinalOutputMetrics {
    /// Bytes written, including the trailing newline.
    pub serialized_bytes: usize,
    /// JSON object nodes in the final document.
    pub objects: usize,
    /// JSON Schema `if` nodes in the final document.
    pub condition_nodes: usize,
    /// Distinct serialized `if` payloads.
    pub unique_conditions: usize,
    /// Distinct serialized `then` payloads.
    pub unique_then_payloads: usize,
}

/// Serializes a schema in the requested JSON format and appends a newline.
///
/// # Errors
///
/// Returns an error when JSON serialization or writing to `out` fails.
#[tracing::instrument(skip_all, fields(format = ?format))]
pub fn write_schema_json(
    out: &mut impl Write,
    schema: &Value,
    format: JsonOutputFormat,
) -> EngineResult<FinalOutputMetrics> {
    let serialized_bytes = write_schema_json_without_metrics(out, schema, format)?;
    Ok(final_output_metrics(schema, serialized_bytes))
}

/// Serializes a schema without computing the other [`FinalOutputMetrics`];
/// returns the bytes written, including the trailing newline.
///
/// # Errors
///
/// Returns an error when JSON serialization or writing to `out` fails.
pub fn write_schema_json_without_metrics(
    out: &mut impl Write,
    schema: &Value,
    format: JsonOutputFormat,
) -> EngineResult<usize> {
    let mut bytes = match format {
        JsonOutputFormat::Pretty => serde_json::to_vec_pretty(schema)?,
        JsonOutputFormat::Compact => serde_json::to_vec(schema)?,
    };
    bytes.push(b'\n');
    out.write_all(&bytes)?;
    Ok(bytes.len())
}

fn final_output_metrics(schema: &Value, serialized_bytes: usize) -> FinalOutputMetrics {
    fn visit(
        value: &Value,
        metrics: &mut FinalOutputMetrics,
        conditions: &mut BTreeSet<String>,
        then_payloads: &mut BTreeSet<String>,
    ) {
        match value {
            Value::Object(object) => {
                metrics.objects += 1;
                if let Some(condition) = object.get("if") {
                    metrics.condition_nodes += 1;
                    conditions.insert(helm_schema_json_schema_walk::canonical_json_string(
                        condition,
                    ));
                }
                if let Some(then_payload) = object.get("then") {
                    then_payloads.insert(helm_schema_json_schema_walk::canonical_json_string(
                        then_payload,
                    ));
                }
                for child in object.values() {
                    visit(child, metrics, conditions, then_payloads);
                }
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, metrics, conditions, then_payloads);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }

    let mut metrics = FinalOutputMetrics {
        serialized_bytes,
        ..FinalOutputMetrics::default()
    };
    let mut conditions = BTreeSet::new();
    let mut then_payloads = BTreeSet::new();
    visit(schema, &mut metrics, &mut conditions, &mut then_payloads);
    metrics.unique_conditions = conditions.len();
    metrics.unique_then_payloads = then_payloads.len();
    metrics
}

#[cfg(test)]
#[path = "tests/format.rs"]
mod tests;
