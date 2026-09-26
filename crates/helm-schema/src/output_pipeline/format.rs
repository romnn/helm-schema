use std::collections::BTreeSet;
use std::io::Write;

use helm_schema_json_schema_minify::{rename_definitions, shipping_definition_names};
use serde_json::Value;

use crate::error::{CliError, EngineResult};
use crate::output_pipeline::JsonOutputFormat;

/// Helm refuses to load any chart file larger than 5 MiB, and a chart's
/// `values.schema.json` counts against that limit.
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
/// Output that would cross Helm's per-file size limit falls back along a
/// ladder: pretty, then compact, then compact with short definition names.
/// Each rung is taken only when the previous one is oversized.
///
/// # Errors
///
/// Returns an error when JSON serialization or writing to `out` fails, or
/// when even the last rung exceeds Helm's limit.
#[tracing::instrument(skip_all, fields(format = ?format))]
pub fn write_schema_json(
    out: &mut impl Write,
    schema: &Value,
    format: JsonOutputFormat,
) -> EngineResult<FinalOutputMetrics> {
    let serialized_bytes = write_schema_json_bytes(out, schema, format)?;
    Ok(final_output_metrics(schema, serialized_bytes))
}

/// Serializes a schema without computing [`FinalOutputMetrics`].
///
/// Oversized output falls back along the same ladder as
/// [`write_schema_json`].
///
/// # Errors
///
/// Returns an error when JSON serialization or writing to `out` fails, or
/// when even the last rung exceeds Helm's limit.
pub fn write_schema_json_without_metrics(
    out: &mut impl Write,
    schema: &Value,
    format: JsonOutputFormat,
) -> EngineResult<()> {
    write_schema_json_bytes(out, schema, format).map(|_| ())
}

fn write_schema_json_bytes(
    out: &mut impl Write,
    schema: &Value,
    format: JsonOutputFormat,
) -> EngineResult<usize> {
    let bytes = serialize_within_helm_limit(schema, format)?;
    out.write_all(&bytes)?;
    Ok(bytes.len())
}

/// Serializes `schema` on the first ladder rung within Helm's limit.
///
/// Whitespace is most of a large pretty document, so compact JSON usually
/// fits.
/// Short names are the last resort: they rename every root definition to a
/// frequency-ranked base-62 key, a bijective rename of the same graph.
fn serialize_within_helm_limit(schema: &Value, format: JsonOutputFormat) -> EngineResult<Vec<u8>> {
    if format == JsonOutputFormat::Pretty {
        let mut pretty = BoundedPrettyWriter::default();
        serde_json::to_writer_pretty(&mut pretty, schema)?;
        if let Some(mut bytes) = pretty.into_bytes() {
            bytes.push(b'\n');
            return Ok(bytes);
        }
    }
    let mut compact = serde_json::to_vec(schema)?;
    compact.push(b'\n');
    if compact.len() <= HELM_MAX_CHART_FILE_BYTES {
        return Ok(compact);
    }
    let mut shipped = schema.clone();
    rename_definitions(&mut shipped, &shipping_definition_names(schema));
    let mut short = serde_json::to_vec(&shipped)?;
    short.push(b'\n');
    if short.len() <= HELM_MAX_CHART_FILE_BYTES {
        return Ok(short);
    }
    Err(CliError::SchemaExceedsHelmFileLimit {
        bytes: short.len(),
        limit: HELM_MAX_CHART_FILE_BYTES,
    })
}

#[derive(Default)]
struct BoundedPrettyWriter {
    bytes: Vec<u8>,
    written: usize,
}

impl BoundedPrettyWriter {
    /// The pretty bytes, when they fit Helm's limit with a trailing newline.
    fn into_bytes(self) -> Option<Vec<u8>> {
        (self.written < HELM_MAX_CHART_FILE_BYTES).then_some(self.bytes)
    }
}

impl Write for BoundedPrettyWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let previous = self.written;
        self.written = self.written.saturating_add(buffer.len());
        if previous < HELM_MAX_CHART_FILE_BYTES {
            if self.written < HELM_MAX_CHART_FILE_BYTES {
                self.bytes.extend_from_slice(buffer);
            } else {
                self.bytes = Vec::new();
            }
        }
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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
