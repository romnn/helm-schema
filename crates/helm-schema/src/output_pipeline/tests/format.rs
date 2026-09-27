use super::{BoundedPrettyWriter, HELM_MAX_CHART_FILE_BYTES, write_schema_json};
use crate::output_pipeline::JsonOutputFormat;
use crate::output_pipeline::write_schema_json_without_metrics;
use color_eyre::eyre::{self, OptionExt as _};
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
fn pretty_probe_discards_retained_bytes_at_the_helm_limit() -> eyre::Result<()> {
    let empty = serde_json::json!({ "padding": "" });
    let overhead = serde_json::to_vec_pretty(&empty)?.len();
    let schema = serde_json::json!({
        "padding": "x".repeat(HELM_MAX_CHART_FILE_BYTES - overhead)
    });
    let mut pretty = BoundedPrettyWriter::default();

    serde_json::to_writer_pretty(&mut pretty, &schema)?;

    sim_assert_eq!(have: pretty.written, want: HELM_MAX_CHART_FILE_BYTES);
    sim_assert_eq!(have: pretty.bytes.is_empty(), want: true);

    let mut have = Vec::new();
    write_schema_json(&mut have, &schema, JsonOutputFormat::Pretty)?;
    let mut want = serde_json::to_vec(&schema)?;
    want.push(b'\n');
    sim_assert_eq!(have: have, want: want);
    Ok(())
}

const LONG_NAME: &str = "hlong-definition-name-that-short-names-compress-away";

/// A schema whose serialized size grows one byte per padding byte, with a
/// long-named definition referenced ten times.
fn ladder_schema(padding: usize) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    for index in 0..10 {
        properties.insert(
            format!("p{index}"),
            serde_json::json!({ "$ref": format!("#/$defs/{LONG_NAME}") }),
        );
    }
    serde_json::json!({
        "$defs": { LONG_NAME: { "type": "string" } },
        "description": "x".repeat(padding),
        "properties": properties
    })
}

fn pretty_bytes(schema: &serde_json::Value) -> eyre::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(schema)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn compact_bytes(schema: &serde_json::Value) -> eyre::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(schema)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn short_named(schema: &serde_json::Value) -> serde_json::Value {
    let mut shipped = schema.clone();
    helm_schema_json_schema_minify::rename_definitions(
        &mut shipped,
        &helm_schema_json_schema_minify::shipping_definition_names(schema),
    );
    shipped
}

/// The ladder schema whose `serialize` output is exactly `target` bytes.
fn ladder_schema_of_size(
    target: usize,
    serialize: fn(&serde_json::Value) -> eyre::Result<Vec<u8>>,
) -> eyre::Result<serde_json::Value> {
    let overhead = serialize(&ladder_schema(0))?.len();
    let schema = ladder_schema(target - overhead);
    sim_assert_eq!(have: serialize(&schema)?.len(), want: target);
    Ok(schema)
}

fn write_pretty(schema: &serde_json::Value) -> eyre::Result<Vec<u8>> {
    let mut out = Vec::new();
    write_schema_json(&mut out, schema, JsonOutputFormat::Pretty)?;
    Ok(out)
}

#[test]
fn pretty_rung_holds_up_to_the_newline_inclusive_limit() -> eyre::Result<()> {
    let at_limit = ladder_schema_of_size(HELM_MAX_CHART_FILE_BYTES, pretty_bytes)?;
    sim_assert_eq!(have: write_pretty(&at_limit)?, want: pretty_bytes(&at_limit)?);

    let over_limit = ladder_schema_of_size(HELM_MAX_CHART_FILE_BYTES + 1, pretty_bytes)?;
    sim_assert_eq!(have: write_pretty(&over_limit)?, want: compact_bytes(&over_limit)?);
    Ok(())
}

#[test]
fn compact_rung_keeps_readable_names_up_to_the_limit() -> eyre::Result<()> {
    let at_limit = ladder_schema_of_size(HELM_MAX_CHART_FILE_BYTES, compact_bytes)?;
    sim_assert_eq!(have: write_pretty(&at_limit)?, want: compact_bytes(&at_limit)?);

    let over_limit = ladder_schema_of_size(HELM_MAX_CHART_FILE_BYTES + 1, compact_bytes)?;
    let written = write_pretty(&over_limit)?;
    sim_assert_eq!(have: written.clone(), want: compact_bytes(&short_named(&over_limit))?);

    // The short-named document is the readable one under the inverse rename.
    let mut restored: serde_json::Value = serde_json::from_slice(&written)?;
    let inverse = helm_schema_json_schema_minify::shipping_definition_names(&over_limit)
        .into_iter()
        .map(|(readable, short)| (short, readable))
        .collect();
    helm_schema_json_schema_minify::rename_definitions(&mut restored, &inverse);
    sim_assert_eq!(have: restored, want: over_limit);
    Ok(())
}

#[test]
fn short_name_rung_is_the_last_one_before_a_typed_error() -> eyre::Result<()> {
    let at_limit = ladder_schema_of_size(HELM_MAX_CHART_FILE_BYTES, |schema| {
        compact_bytes(&short_named(schema))
    })?;
    sim_assert_eq!(
        have: write_pretty(&at_limit)?,
        want: compact_bytes(&short_named(&at_limit))?
    );

    let over_limit = ladder_schema_of_size(HELM_MAX_CHART_FILE_BYTES + 1, |schema| {
        compact_bytes(&short_named(schema))
    })?;
    for format in [JsonOutputFormat::Pretty, JsonOutputFormat::Compact] {
        let mut out = Vec::new();
        let error = write_schema_json_without_metrics(&mut out, &over_limit, format)
            .err()
            .ok_or_eyre("oversized output must be rejected")?;
        assert!(
            matches!(
                error,
                crate::error::CliError::SchemaExceedsHelmFileLimit {
                    bytes,
                    limit: HELM_MAX_CHART_FILE_BYTES,
                } if bytes == HELM_MAX_CHART_FILE_BYTES + 1
            ),
            "unexpected error: {error}"
        );
        sim_assert_eq!(have: out.is_empty(), want: true);
    }
    Ok(())
}

#[test]
fn compact_format_takes_short_names_past_the_limit() -> eyre::Result<()> {
    let over_limit = ladder_schema_of_size(HELM_MAX_CHART_FILE_BYTES + 1, compact_bytes)?;
    let mut out = Vec::new();
    write_schema_json(&mut out, &over_limit, JsonOutputFormat::Compact)?;
    sim_assert_eq!(have: out, want: compact_bytes(&short_named(&over_limit))?);
    Ok(())
}

#[test]
fn oversized_output_whose_names_cannot_be_shortened_is_rejected() -> eyre::Result<()> {
    let mut over_limit = ladder_schema_of_size(HELM_MAX_CHART_FILE_BYTES + 1, compact_bytes)?;
    // A dynamic reference makes the rename abstain even though short names
    // would fit; resize so compact output is again one byte over.
    over_limit["properties"]["dynamic"] =
        serde_json::json!({ "$dynamicRef": format!("#/$defs/{LONG_NAME}") });
    let extra = compact_bytes(&over_limit)?.len() - (HELM_MAX_CHART_FILE_BYTES + 1);
    let padding = over_limit["description"]
        .as_str()
        .ok_or_eyre("padding description expected")?
        .len();
    over_limit["description"] = serde_json::json!("x".repeat(padding - extra));
    sim_assert_eq!(
        have: compact_bytes(&over_limit)?.len(),
        want: HELM_MAX_CHART_FILE_BYTES + 1
    );
    sim_assert_eq!(
        have: helm_schema_json_schema_minify::shipping_definition_names(&over_limit).is_empty(),
        want: true
    );

    let mut out = Vec::new();
    let error = write_schema_json_without_metrics(&mut out, &over_limit, JsonOutputFormat::Pretty)
        .err()
        .ok_or_eyre("oversized output must be rejected")?;
    assert!(
        matches!(
            error,
            crate::error::CliError::SchemaExceedsHelmFileLimit { bytes, .. }
                if bytes == HELM_MAX_CHART_FILE_BYTES + 1
        ),
        "unexpected error: {error}"
    );
    sim_assert_eq!(have: out.is_empty(), want: true);
    Ok(())
}
