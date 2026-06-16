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
mod tests {
    use super::write_schema_json;
    use crate::output_pipeline::JsonOutputFormat;

    #[test]
    fn json_output_format_controls_pretty_vs_compact_serialization() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string"
                }
            }
        });

        let mut pretty = Vec::new();
        write_schema_json(&mut pretty, &schema, JsonOutputFormat::Pretty).expect("write pretty");
        let pretty = String::from_utf8(pretty).expect("pretty utf8");
        assert!(
            pretty.contains("\n  "),
            "pretty output should contain indentation: {pretty}"
        );

        let mut compact = Vec::new();
        write_schema_json(&mut compact, &schema, JsonOutputFormat::Compact).expect("write compact");
        let compact = String::from_utf8(compact).expect("compact utf8");
        assert_eq!(
            compact,
            r#"{"properties":{"name":{"type":"string"}},"type":"object"}"#.to_string() + "\n"
        );
    }
}
