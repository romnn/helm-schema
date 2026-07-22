use std::io::Write;

use serde_json::Value;

use crate::error::EngineResult;
use crate::output_pipeline::JsonOutputFormat;

/// Helm refuses to load any chart file larger than 5 MiB, and a chart's
/// `values.schema.json` counts against that limit.
pub const HELM_MAX_CHART_FILE_BYTES: usize = 5 * 1024 * 1024;

/// Serializes a schema in the requested JSON format and appends a newline.
///
/// Pretty output automatically falls back to compact JSON before crossing
/// Helm's per-file size limit.
///
/// # Errors
///
/// Returns an error when JSON serialization or writing to `out` fails.
#[tracing::instrument(skip_all, fields(format = ?format))]
pub fn write_schema_json(
    out: &mut impl Write,
    schema: &Value,
    format: JsonOutputFormat,
) -> EngineResult<()> {
    match format {
        JsonOutputFormat::Compact => serde_json::to_writer(&mut *out, schema)?,
        JsonOutputFormat::Pretty => {
            // A schema whose pretty serialization crosses Helm's chart-file
            // limit still fits comfortably in compact form (whitespace is
            // most of the size at that scale), so pretty degrades to
            // compact rather than emitting a schema the chart cannot ship.
            let bytes = serde_json::to_vec_pretty(schema)?;
            if bytes.len() >= HELM_MAX_CHART_FILE_BYTES {
                serde_json::to_writer(&mut *out, schema)?;
            } else {
                out.write_all(&bytes)?;
            }
        }
    }
    out.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/format.rs"]
mod tests;
