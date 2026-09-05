use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;

mod complete_deduplication;

#[test]
fn generated_reference_inlining_is_one_step_and_preserves_other_scopes() {
    let mut schema = json!({
        "$id": "https://example.test/root",
        "properties": {
            "value": {"$ref": "#/$defs/1"},
            "nested": {
                "$id": "child",
                "properties": {"value": {"$ref": "#/$defs/1"}}
            }
        },
        "default": {"$ref": "#/$defs/1"}
    });
    let replacements = BTreeMap::from([
        ("#/$defs/1".to_string(), json!({"$ref": "#/$defs/2"})),
        ("#/$defs/2".to_string(), json!({"type": "string"})),
    ]);

    inline_generated_references(&mut schema, &replacements);

    // An inserted reference is not recursively substituted, and neither data
    // payloads nor a nested resource belong to the outer definition namespace.
    sim_assert_eq!(have: schema, want: json!({
        "$id": "https://example.test/root",
        "properties": {
            "value": {"$ref": "#/$defs/2"},
            "nested": {
                "$id": "child",
                "properties": {"value": {"$ref": "#/$defs/1"}}
            }
        },
        "default": {"$ref": "#/$defs/1"}
    }));
}

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

    let result = minimize_schema(schema);
    sim_assert_eq!(
        have: result,
        want: json!({
            "type": "object",
            "definitions": {
                "base": { "type": "object" }
            },
            "properties": {
                "left": {
                    "allOf": [
                        { "$ref": "#/definitions/base" },
                        {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" }
                            }
                        }
                    ]
                },
                "right": {
                    "allOf": [
                        { "$ref": "#/definitions/base" },
                        {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" }
                            }
                        }
                    ]
                }
            }
        })
    );
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
                        {
                            "properties": {
                                "enabled": { "type": "boolean" },
                                "name": { "type": "string" }
                            },
                            "type": "object"
                        },
                        { "$ref": "#/$defs/base" }
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
fn nested_references_preserve_existing_definition_addresses() {
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
            "byPointer": {"$ref": "#/$defs/base/allOf/0"},
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

#[test]
fn equivalent_junctor_grouping_produces_one_stable_definition() {
    let arm = |name: &str| {
        json!({
            "properties": {
                (name): {
                    "description": "A deliberately substantial repeated validation payload.",
                    "pattern": "^[A-Za-z][A-Za-z0-9._/-]{8,128}$",
                    "type": "string"
                }
            },
            "required": [name],
            "type": "object"
        })
    };
    let first = arm("first");
    let second = arm("second");
    let third = arm("third");
    let schema = json!({
        "properties": {
            "left": {
                "allOf": [first.clone(), second.clone(), third.clone()]
            },
            "right": {
                "allOf": [third, { "allOf": [second, first] }]
            }
        },
        "type": "object"
    });

    let minimized = minimize_schema(schema);
    let regrouped = minimize_schema(json!({
        "properties": {
            "left": {
                "allOf": [arm("third"), { "allOf": [arm("first"), arm("second")] }]
            },
            "right": {
                "allOf": [arm("second"), arm("third"), arm("first")]
            }
        },
        "type": "object"
    }));

    // Grouping and source order cannot create two definition identities.
    sim_assert_eq!(have: regrouped, want: minimized.clone());
    sim_assert_eq!(
        have: minimized.pointer("/properties/left/$ref"),
        want: Some(&Value::String("#/$defs/2".to_string()))
    );
    sim_assert_eq!(
        have: minimized.pointer("/properties/right/$ref"),
        want: Some(&Value::String("#/$defs/2".to_string()))
    );
    sim_assert_eq!(
        have: minimized.pointer("/$defs/2/allOf").and_then(Value::as_array).map(Vec::len),
        want: Some(3)
    );
}

#[test]
fn logical_normal_form_does_not_flatten_annotated_junctor_wrappers() {
    let schema = json!({
        "allOf": [
            {
                "allOf": [
                    { "type": "string" },
                    { "minLength": 3 }
                ],
                "title": "annotation boundary"
            },
            { "type": "string" }
        ]
    });

    let mut normalized = schema.clone();
    normalize_logical_schema(&mut normalized);

    sim_assert_eq!(
        have: normalized.pointer("/allOf/0/title"),
        want: Some(&Value::String("annotation boundary".to_string()))
    );
    sim_assert_eq!(
        have: normalized.pointer("/allOf/0/allOf").and_then(Value::as_array).map(Vec::len),
        want: Some(2)
    );
}

#[test]
fn logical_normal_form_keeps_duplicate_one_of_arms() {
    let schema = json!({
        "oneOf": [
            { "type": "string" },
            { "type": "string" }
        ],
        "allOf": [
            { "type": "number" },
            { "type": "number" }
        ]
    });

    let mut normalized = schema.clone();
    normalize_logical_schema(&mut normalized);

    sim_assert_eq!(
        have: normalized.pointer("/oneOf").and_then(Value::as_array).map(Vec::len),
        want: Some(2)
    );
    sim_assert_eq!(
        have: normalized.pointer("/allOf").and_then(Value::as_array).map(Vec::len),
        want: Some(1)
    );
}

#[test]
fn digest_collisions_still_require_exact_canonical_identity() {
    let first = json!({ "const": "a".repeat(200) });
    let second = json!({ "const": "b".repeat(200) });
    let first_canonical = helm_schema_json_schema_walk::canonical_json_string(&first);
    let second_canonical = helm_schema_json_schema_walk::canonical_json_string(&second);
    sim_assert_eq!(have: first_canonical.len(), want: second_canonical.len());
    let fingerprint = CandidateFingerprint {
        digest: 0,
        byte_len: first_canonical.len(),
    };
    let candidates = HashMap::from([(
        fingerprint,
        vec![
            ExactCandidate {
                schema: first.clone(),
                canonical: first_canonical,
                occurrences: 3,
            },
            ExactCandidate {
                schema: second.clone(),
                canonical: second_canonical,
                occurrences: 3,
            },
        ],
    )]);

    let planned = plan_definitions(BTreeSet::new(), candidates);
    let first_name = planned.definition_name(fingerprint, &first);
    let second_name = planned.definition_name(fingerprint, &second);
    sim_assert_eq!(have: first_name.is_some(), want: true);
    sim_assert_eq!(have: second_name.is_some(), want: true);
    sim_assert_eq!(have: first_name == second_name, want: false);
}
