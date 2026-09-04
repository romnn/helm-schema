//! Canonical metadata integration tests.

use helm_schema_json_schema_walk::{
    SchemaMetadataIndex, SchemaValueMetadata, canonical_json_string,
};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

fn assert_exact_lengths(value: &Value, index: &SchemaMetadataIndex) {
    sim_assert_eq!(
        have: index.get(value).map(|metadata| metadata.canonical().byte_len()),
        want: Some(canonical_json_string(value).len())
    );
    match value {
        Value::Array(items) => {
            for item in items {
                assert_exact_lengths(item, index);
            }
        }
        Value::Object(object) => {
            for value in object.values() {
                assert_exact_lengths(value, index);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[test]
fn metadata_lengths_match_canonical_serialization() {
    let schema = json!({
        "allOf": [
            { "const": "quote: \"; control: \n; unicode: π" },
            { "enum": [null, true, false, -12.5, 42] }
        ],
        "properties": {
            "nested": {
                "items": { "type": ["string", "null"] },
                "type": "array"
            }
        }
    });
    let index = SchemaMetadataIndex::new(&schema);

    assert_exact_lengths(&schema, &index);
}

#[test]
fn unsafe_scope_follows_schema_positions_not_data_payloads() {
    let schema = json!({
        "properties": {
            "safe": {
                "examples": [{ "$id": "ordinary data" }],
                "type": "object"
            },
            "unsafe": {
                "allOf": [{ "$id": "nested schema scope" }]
            }
        }
    });
    let index = SchemaMetadataIndex::new(&schema);
    let safe = schema.pointer("/properties/safe");
    let unsafe_schema = schema.pointer("/properties/unsafe");

    sim_assert_eq!(
        have: safe.and_then(|value| index.get(value)).map(SchemaValueMetadata::contains_unsafe_reference_scope_keyword),
        want: Some(false)
    );
    sim_assert_eq!(
        have: unsafe_schema.and_then(|value| index.get(value)).map(SchemaValueMetadata::contains_unsafe_reference_scope_keyword),
        want: Some(true)
    );
    sim_assert_eq!(
        have: index.get(&schema).map(SchemaValueMetadata::contains_unsafe_reference_scope_keyword),
        want: Some(true)
    );
}
