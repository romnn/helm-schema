use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;

#[test]
fn generated_definition_names_use_compact_base62() {
    for (value, expected) in [
        (1, "1"),
        (10, "a"),
        (35, "z"),
        (36, "A"),
        (61, "Z"),
        (62, "10"),
    ] {
        sim_assert_eq!(have: base62(value), want: expected.to_string());
    }
}

#[test]
fn most_referenced_definitions_receive_the_shortest_names() {
    let mut schema = json!({
        "allOf": [
            { "$ref": "#/$defs/old-a" },
            { "$ref": "#/$defs/old-b" },
            { "$ref": "#/$defs/old-b" },
            { "$ref": "#/$defs/old-b" }
        ]
    });
    let definitions = BTreeMap::from([
        ("old-a".to_string(), json!({ "type": "string" })),
        ("old-b".to_string(), json!({ "type": "boolean" })),
    ]);

    let compacted = compact_definition_names(&mut schema, definitions);

    sim_assert_eq!(
        have: schema,
        want: json!({
            "allOf": [
                { "$ref": "#/$defs/2" },
                { "$ref": "#/$defs/1" },
                { "$ref": "#/$defs/1" },
                { "$ref": "#/$defs/1" }
            ]
        })
    );
    sim_assert_eq!(
        have: Value::Object(compacted.into_iter().collect()),
        want: json!({
            "1": { "type": "boolean" },
            "2": { "type": "string" }
        })
    );
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

    let result = minimize_schema(schema);

    sim_assert_eq!(
        have: result,
        want: json!({
            "$defs": {
                "1": {
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
                "left": { "$ref": "#/$defs/1" },
                "right": { "$ref": "#/$defs/1" }
            }
        })
    );
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

    let result = minimize_schema(schema);
    sim_assert_eq!(
        have: result
            .pointer("/$defs/1/required")
            .and_then(Value::as_array)
            .map(Vec::len),
        want: Some(2)
    );
    sim_assert_eq!(
        have: result
            .pointer("/$defs/1/enum")
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

    let result = minimize_schema(schema.clone());
    sim_assert_eq!(have: result, want: schema);
}

#[test]
fn repeated_schemas_may_reference_unchanged_root_definitions() {
    let repeated = json!({
        "allOf": [
            { "$ref": "#/$defs/base" },
            {
                "properties": {
                    "enabled": { "type": "boolean" },
                    "name": { "type": "string" }
                },
                "type": "object"
            }
        ]
    });
    let schema = json!({
        "$defs": {
            "base": {
                "properties": {
                    "namespace": { "type": "string" }
                },
                "type": "object"
            }
        },
        "properties": {
            "left": repeated,
            "right": repeated
        },
        "type": "object"
    });

    let result = minimize_schema(schema);

    sim_assert_eq!(
        have: result,
        want: json!({
            "$defs": {
                "1": {
                    "allOf": [
                        { "$ref": "#/$defs/base" },
                        {
                            "properties": {
                                "enabled": { "type": "boolean" },
                                "name": { "type": "string" }
                            },
                            "type": "object"
                        }
                    ]
                },
                "base": {
                    "properties": {
                        "namespace": { "type": "string" }
                    },
                    "type": "object"
                }
            },
            "properties": {
                "left": { "$ref": "#/$defs/1" },
                "right": { "$ref": "#/$defs/1" }
            },
            "type": "object"
        })
    );
}

#[test]
fn existing_definition_bodies_are_not_rewritten() {
    let repeated = json!({
        "allOf": [
            { "$ref": "#/$defs/base" },
            {
                "properties": {
                    "name": { "type": "string" }
                },
                "type": "object"
            }
        ]
    });
    let schema = json!({
        "$defs": {
            "base": repeated
        },
        "properties": {
            "left": repeated,
            "right": repeated
        }
    });

    let result = minimize_schema(schema);

    sim_assert_eq!(
        have: result.pointer("/$defs/base/allOf/0/$ref"),
        want: Some(&Value::String("#/$defs/base".to_string()))
    );
    assert!(
        result.pointer("/$defs/base/$ref").is_none(),
        "the existing definition must not be replaced by a generated definition"
    );
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

    let result = minimize_schema(schema);
    sim_assert_eq!(
        have: result.pointer("/properties/left/$ref"),
        want: Some(&Value::String("#/$defs/1".to_string()))
    );
    sim_assert_eq!(
        have: result.pointer("/properties/right/$ref"),
        want: Some(&Value::String("#/$defs/1".to_string()))
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

    let result = minimize_schema(schema.clone());
    sim_assert_eq!(have: result, want: schema);
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
            "1": { "type": "null" }
        },
        "properties": {
            "left": repeated,
            "right": repeated
        }
    });

    let result = minimize_schema(schema);
    assert!(result.pointer("/$defs/1").is_some());
    assert!(result.pointer("/$defs/2").is_some());
    sim_assert_eq!(
        have: result.pointer("/properties/left/$ref"),
        want: Some(&Value::String("#/$defs/2".to_string()))
    );
}
