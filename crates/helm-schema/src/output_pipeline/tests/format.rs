use super::{HELM_MAX_CHART_FILE_BYTES, write_schema_json};
use crate::output_pipeline::JsonOutputFormat;
use crate::output_pipeline::write_schema_json_without_metrics;
use color_eyre::eyre;
use test_util::prelude::sim_assert_eq;

#[test]
fn json_output_format_controls_pretty_vs_compact_serialization() -> eyre::Result<()> {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string"
            }
        }
    });

    let mut pretty = Vec::new();
    let pretty_metrics = write_schema_json(&mut pretty, &schema, JsonOutputFormat::Pretty)?;
    sim_assert_eq!(have: pretty_metrics.serialized_bytes, want: pretty.len());
    sim_assert_eq!(have: pretty_metrics.objects, want: 3);
    let pretty = String::from_utf8(pretty)?;
    assert!(
        pretty.contains("\n  "),
        "pretty output should contain indentation: {pretty}"
    );

    let mut compact = Vec::new();
    write_schema_json(&mut compact, &schema, JsonOutputFormat::Compact)?;
    let compact = String::from_utf8(compact)?;
    sim_assert_eq!(
        have: compact,
        want: r#"{"properties":{"name":{"type":"string"}},"type":"object"}"#.to_string() + "\n"
    );
    Ok(())
}

#[test]
fn final_output_metrics_count_the_serialized_conditional_shape() -> eyre::Result<()> {
    let schema = serde_json::json!({
        "allOf": [
            { "if": { "properties": { "mode": { "const": "on" } } }, "then": { "required": ["value"] } },
            { "if": { "properties": { "mode": { "const": "on" } } }, "then": { "required": ["other"] } },
        ],
        "type": "object",
    });
    let mut out = Vec::new();

    let metrics = write_schema_json(&mut out, &schema, JsonOutputFormat::Compact)?;

    sim_assert_eq!(have: metrics.serialized_bytes, want: out.len());
    sim_assert_eq!(have: metrics.objects, want: 11);
    sim_assert_eq!(have: metrics.condition_nodes, want: 2);
    sim_assert_eq!(have: metrics.unique_conditions, want: 1);
    sim_assert_eq!(have: metrics.unique_then_payloads, want: 2);
    Ok(())
}

#[test]
fn unmeasured_output_matches_metrics_enabled_output() -> eyre::Result<()> {
    let schema = serde_json::json!({ "properties": { "name": { "type": "string" } } });
    for format in [JsonOutputFormat::Compact, JsonOutputFormat::Pretty] {
        let mut measured = Vec::new();
        let mut unmeasured = Vec::new();

        write_schema_json(&mut measured, &schema, format)?;
        write_schema_json_without_metrics(&mut unmeasured, &schema, format)?;

        sim_assert_eq!(have: unmeasured, want: measured);
    }
    Ok(())
}

#[test]
fn oversized_output_keeps_its_format_and_names() -> eyre::Result<()> {
    let mut properties = serde_json::Map::new();
    for index in 0..10 {
        properties.insert(
            format!("p{index}"),
            serde_json::json!({ "$ref": "#/$defs/values~1long-definition-name" }),
        );
    }
    let schema = serde_json::json!({
        "$defs": { "values/long-definition-name": { "type": "string" } },
        "description": "x".repeat(HELM_MAX_CHART_FILE_BYTES),
        "properties": properties
    });

    for format in [JsonOutputFormat::Pretty, JsonOutputFormat::Compact] {
        let mut want = match format {
            JsonOutputFormat::Pretty => serde_json::to_vec_pretty(&schema)?,
            JsonOutputFormat::Compact => serde_json::to_vec(&schema)?,
        };
        want.push(b'\n');
        let mut have = Vec::new();
        let metrics = write_schema_json(&mut have, &schema, format)?;
        assert!(metrics.serialized_bytes > HELM_MAX_CHART_FILE_BYTES);
        sim_assert_eq!(have: have, want: want);
    }
    Ok(())
}

#[test]
fn default_output_keeps_pretty_bytes_through_the_inclusive_helm_limit() -> eyre::Result<()> {
    use crate::output_pipeline::serialize_default_schema_json;
    use helm_schema_json_schema_minify::shorten_definition_names;

    let mut schema = serde_json::json!({
        "$defs": { "values/long-definition-name": { "type": "string" } },
        "description": "",
        "properties": { "name": { "$ref": "#/$defs/values~1long-definition-name" } }
    });
    let overhead = serde_json::to_vec_pretty(&schema)?.len() + 1;
    for pretty_bytes in [
        HELM_MAX_CHART_FILE_BYTES - 1,
        HELM_MAX_CHART_FILE_BYTES,
        HELM_MAX_CHART_FILE_BYTES + 1,
    ] {
        // Count UTF-8 bytes and the final newline at the exact switching boundary.
        schema["description"] = serde_json::Value::String("x".repeat(pretty_bytes - overhead));
        let output = serialize_default_schema_json(&schema)?;
        let compacted = pretty_bytes > HELM_MAX_CHART_FILE_BYTES;
        let mut want = Vec::new();
        if compacted {
            write_schema_json_without_metrics(
                &mut want,
                &shorten_definition_names(&schema).schema,
                JsonOutputFormat::Compact,
            )?;
        } else {
            write_schema_json_without_metrics(&mut want, &schema, JsonOutputFormat::Pretty)?;
        }
        sim_assert_eq!(have: output.compacted, want: compacted);
        sim_assert_eq!(have: output.bytes, want: want);
        assert!(output.bytes.len() <= HELM_MAX_CHART_FILE_BYTES);
    }
    Ok(())
}

#[test]
fn default_output_returns_oversized_compact_bytes_for_other_validators() -> eyre::Result<()> {
    use crate::output_pipeline::serialize_default_schema_json;

    let schema = serde_json::json!({ "description": "é".repeat(HELM_MAX_CHART_FILE_BYTES / 2) });
    let output = serialize_default_schema_json(&schema)?;
    let mut want = serde_json::to_vec(&schema)?;
    want.push(b'\n');
    assert!(output.compacted);
    assert!(output.bytes.len() > HELM_MAX_CHART_FILE_BYTES);
    sim_assert_eq!(have: output.bytes, want: want);
    assert!(output.readable_names.is_empty());
    Ok(())
}
