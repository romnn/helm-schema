use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;

fn options() -> MinimizeOptions {
    MinimizeOptions {
        min_subtree_bytes: 1,
    }
}

#[test]
fn repeated_property_schemas_move_to_defs() {
    let repeated = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "enabled": { "type": "boolean" },
            "name": { "type": "string" }
        }
    });
    let schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "left": repeated,
            "right": repeated
        }
    });

    let result = minimize_schema(schema, &options());

    sim_assert_eq!(
        have: result.schema,
        want: json!({
            "$defs": {
                "schema1": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "enabled": { "type": "boolean" },
                        "name": { "type": "string" }
                    }
                }
            },
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "left": { "$ref": "#/$defs/schema1" },
                "right": { "$ref": "#/$defs/schema1" }
            }
        })
    );
    sim_assert_eq!(have: result.stats.definitions_added, want: 1);
    sim_assert_eq!(have: result.stats.replacements, want: 2);
    assert!(result.stats.bytes_after < result.stats.bytes_before);
}

#[test]
fn non_schema_keyword_payloads_are_not_replaced() {
    let schema = json!({
        "type": "object",
        "properties": {
            "left": {
                "type": "object",
                "required": ["name", "namespace"],
                "enum": [{"kind": "A"}, {"kind": "B"}]
            },
            "right": {
                "type": "object",
                "required": ["name", "namespace"],
                "enum": [{"kind": "A"}, {"kind": "B"}]
            }
        }
    });

    let result = minimize_schema(schema, &options());
    sim_assert_eq!(
        have: result
            .schema
            .pointer("/$defs/schema1/required")
            .and_then(Value::as_array)
            .map(Vec::len),
        want: Some(2)
    );
    sim_assert_eq!(
        have: result
            .schema
            .pointer("/$defs/schema1/enum")
            .and_then(Value::as_array)
            .map(Vec::len),
        want: Some(2)
    );
}

#[test]
fn schemas_containing_refs_are_not_extracted() {
    let repeated = json!({
        "allOf": [
            { "$ref": "#/definitions/base" },
            {
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                }
            }
        ]
    });
    let schema = json!({
        "type": "object",
        "definitions": {
            "base": { "type": "object" }
        },
        "properties": {
            "left": repeated,
            "right": repeated
        }
    });

    let result = minimize_schema(schema.clone(), &options());
    sim_assert_eq!(have: result.schema, want: schema);
    sim_assert_eq!(have: result.stats.replacements, want: 0);
}

#[test]
fn property_names_that_look_like_ref_keywords_do_not_block_extraction() {
    let repeated = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "$ref": { "type": "string" },
            "id": { "type": "string" },
            "name": { "type": "string" },
            "namespace": { "type": "string" }
        }
    });
    let schema = json!({
        "type": "object",
        "properties": {
            "left": repeated,
            "right": repeated,
            "third": repeated
        }
    });

    let result = minimize_schema(schema, &options());
    sim_assert_eq!(
        have: result.schema.pointer("/properties/left/$ref"),
        want: Some(&Value::String("#/$defs/schema1".to_string()))
    );
    sim_assert_eq!(
        have: result.schema.pointer("/properties/right/$ref"),
        want: Some(&Value::String("#/$defs/schema1".to_string()))
    );
}

#[test]
fn repeated_tiny_schemas_are_not_replaced_without_size_win() {
    let schema = json!({
        "type": "object",
        "properties": {
            "left": { "type": "string" },
            "right": { "type": "string" }
        }
    });

    let result = minimize_schema(schema.clone(), &options());
    sim_assert_eq!(have: result.schema, want: schema);
    sim_assert_eq!(have: result.stats.definitions_added, want: 0);
    sim_assert_eq!(have: result.stats.replacements, want: 0);
}

#[test]
fn existing_defs_names_are_not_reused() {
    let repeated = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "namespace": { "type": "string" }
        }
    });
    let schema = json!({
        "$defs": {
            "schema1": { "type": "null" }
        },
        "properties": {
            "left": repeated,
            "right": repeated
        }
    });

    let result = minimize_schema(schema, &options());
    assert!(result.schema.pointer("/$defs/schema1").is_some());
    assert!(result.schema.pointer("/$defs/schema2").is_some());
    sim_assert_eq!(
        have: result.schema.pointer("/properties/left/$ref"),
        want: Some(&Value::String("#/$defs/schema2".to_string()))
    );
}
