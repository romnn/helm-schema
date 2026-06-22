use std::io::Write;

use serde_json::Value;

use crate::error::CliResult;
use crate::output_pipeline::JsonOutputFormat;

#[tracing::instrument(skip_all, fields(format = ?format))]
pub fn write_schema_json(
    out: &mut impl Write,
    schema: &Value,
    format: JsonOutputFormat,
) -> CliResult<()> {
    match format {
        JsonOutputFormat::Compact => serde_json::to_writer(&mut *out, schema)?,
        JsonOutputFormat::Pretty => serde_json::to_writer_pretty(&mut *out, schema)?,
    }
    out.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/format.rs"]
mod tests;
