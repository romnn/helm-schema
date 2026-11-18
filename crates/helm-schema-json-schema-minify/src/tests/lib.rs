use color_eyre::eyre;
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;

mod complete_deduplication;
mod naming;
mod single_use;

/// Minimizes a schema that has no private definition handles.
fn minimize(schema: Value) -> Value {
    minimize_schema(schema, &BTreeMap::new(), DefinitionNames::Source)
}

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
        ("1".to_string(), json!({"$ref": "#/$defs/2"})),
        ("2".to_string(), json!({"type": "string"})),
    ]);

    inline_generated_references(&mut schema, &replacements, DefinitionsKeyword::Defs);

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

    let result = minimize(schema);

    sim_assert_eq!(
        have: result,
        want: json!({
            "definitions": {
                "values/left": {
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
                "left": { "$ref": "#/definitions/values~1left" },
                "right": { "$ref": "#/definitions/values~1left" }
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

    let result = minimize(schema);
    sim_assert_eq!(
        have: result,
        want: json!({
            "$defs": {
                "values/left": {
                    "type": "object",
                    "required": ["name", "namespace"],
                    "enum": [{"kind": "A"}, {"kind": "B"}]
                }
            },
            "type": "object",
            "properties": {
                "left": { "$ref": "#/$defs/values~1left" },
                "right": { "$ref": "#/$defs/values~1left" }
            }
        })
    );
}

#[test]
fn references_outside_definition_containers_abstain_from_extraction() {
    let repeated = json!({
        "allOf": [
            { "$ref": "#/properties/base" },
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
        "properties": {
            "base": { "type": "object" },
            "left": repeated,
            "right": repeated
        }
    });

    let result = minimize(schema);
    sim_assert_eq!(
        have: result,
        want: json!({
            "type": "object",
            "properties": {
                "base": { "type": "object" },
                "left": {
                    "allOf": [
                        {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" }
                            }
                        },
                        { "$ref": "#/properties/base" }
                    ]
                },
                "right": {
                    "allOf": [
                        {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" }
                            }
                        },
                        { "$ref": "#/properties/base" }
                    ]
                }
            }
        })
    );
}

#[test]
fn extraction_can_leave_a_single_reference_to_inline() {
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

    let result = minimize(schema);

    sim_assert_eq!(
        have: result,
        want: json!({
            "$defs": {
                "values/left": {
                    "allOf": [
                        {
                            "properties": {
                                "enabled": { "type": "boolean" },
                                "name": { "type": "string" }
                            },
                            "type": "object"
                        },
                        {
                            "properties": {
                                "namespace": { "type": "string" }
                            },
                            "type": "object"
                        }
                    ]
                }
            },
            "properties": {
                "left": { "$ref": "#/$defs/values~1left" },
                "right": { "$ref": "#/$defs/values~1left" }
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

    let result = minimize(schema);

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

    let result = minimize(schema);
    sim_assert_eq!(
        have: result.pointer("/properties/left/$ref"),
        want: Some(&Value::String("#/$defs/values~1left".to_string()))
    );
    sim_assert_eq!(
        have: result.pointer("/properties/right/$ref"),
        want: Some(&Value::String("#/$defs/values~1left".to_string()))
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

    let result = minimize(schema.clone());
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

    let result = minimize(schema);
    sim_assert_eq!(
        have: result,
        want: json!({
            "$defs": {
                "1": { "type": "null" },
                "values/left": repeated
            },
            "properties": {
                "left": { "$ref": "#/$defs/values~1left" },
                "right": { "$ref": "#/$defs/values~1left" }
            }
        })
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

    let minimized = minimize(schema);
    let regrouped = minimize(json!({
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
        want: Some(&Value::String("#/$defs/values~1left".to_string()))
    );
    sim_assert_eq!(
        have: minimized.pointer("/properties/right/$ref"),
        want: Some(&Value::String("#/$defs/values~1left".to_string()))
    );
    sim_assert_eq!(
        have: minimized.pointer("/$defs/values~1left/allOf").and_then(Value::as_array).map(Vec::len),
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

    let planned = plan_definitions(BTreeSet::new(), candidates, DefinitionsKeyword::Defs);
    let first_id = planned.definition_id(fingerprint, &first);
    let second_id = planned.definition_id(fingerprint, &second);
    sim_assert_eq!(have: first_id.is_some(), want: true);
    sim_assert_eq!(have: second_id.is_some(), want: true);
    sim_assert_eq!(have: first_id == second_id, want: false);
}

#[test]
fn extraction_and_shortening_follow_the_declared_dialect() -> eyre::Result<()> {
    let payload = json!({"type": "string", "minLength": 2, "description": "example ".repeat(32)});
    for (uri, keyword) in [
        ("http://json-schema.org/draft-07/schema#", "definitions"),
        ("https://json-schema.org/draft/2019-09/schema", "$defs"),
        ("https://json-schema.org/draft/2020-12/schema", "$defs"),
    ] {
        let schema = json!({
            "$schema": uri,
            "properties": {"a": payload, "b": payload}
        });
        let readable = minimize(schema);
        let expected = json!({
            "$schema": uri,
            (keyword): {"values/a": payload},
            "properties": {
                "a": {"$ref": format!("#/{keyword}/values~1a")},
                "b": {"$ref": format!("#/{keyword}/values~1a")}
            }
        });
        sim_assert_eq!(have: &readable, want: &expected);
        let shortened = shorten_definition_names(&readable);
        sim_assert_eq!(have: shortened.schema, want: json!({
            "$schema": uri,
            (keyword): {"1": payload},
            "properties": {
                "a": {"$ref": format!("#/{keyword}/1")},
                "b": {"$ref": format!("#/{keyword}/1")}
            }
        }));
        let validator = jsonschema::validator_for(&shortened.schema)?;
        sim_assert_eq!(have: validator.is_valid(&json!({"a": "ab"})), want: true);
        sim_assert_eq!(have: validator.is_valid(&json!({"a": "a"})), want: false);
    }
    Ok(())
}
