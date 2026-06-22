use super::write_schema_json;
use crate::output_pipeline::JsonOutputFormat;
use test_util::prelude::sim_assert_eq;

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
    sim_assert_eq!(
        have: compact,
        want: r#"{"properties":{"name":{"type":"string"}},"type":"object"}"#.to_string() + "\n"
    );
}
