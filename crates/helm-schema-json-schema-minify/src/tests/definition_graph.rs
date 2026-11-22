use std::collections::BTreeMap;

use color_eyre::eyre;
use serde_json::{Map, Value, json};
use test_util::prelude::sim_assert_eq;

use super::compact;
use crate::DefinitionOrigin;

fn private(names: &[&str]) -> BTreeMap<String, Vec<DefinitionOrigin>> {
    names
        .iter()
        .map(|name| ((*name).to_string(), Vec::new()))
        .collect()
}

#[test]
fn draft07_boolean_decisions_preserve_every_validation_branch() -> eyre::Result<()> {
    let test = json!({"type":"object", "properties":{"p":{"const":true}}, "required":["p"]});
    let arm = json!({"type":"object", "properties":{"q":{"const":true}}, "required":["q"]});
    let cases = [
        (json!(true), json!(false), test.clone()),
        (json!(false), json!(true), json!({"not":test})),
        (
            json!(true),
            arm.clone(),
            json!({"if":test,"then":true,"else":arm}),
        ),
        (
            json!(false),
            arm.clone(),
            json!({"allOf":[{"not":test},arm]}),
        ),
        (
            arm.clone(),
            json!(true),
            json!({"if":test,"then":arm,"else":true}),
        ),
        (arm.clone(), json!(false), json!({"allOf":[test,arm]})),
        (
            json!(true),
            json!(true),
            json!({"if":test,"then":true,"else":true}),
        ),
        (
            json!(false),
            json!(false),
            json!({"if":test,"then":false,"else":false}),
        ),
    ];
    for (yes, no, expected) in cases {
        let source = json!({
            "$schema":"http://json-schema.org/draft-07/schema#",
            "definitions":{"decision":{"if":test,"then":yes,"else":no}},
            "allOf":[{"$ref":"#/definitions/decision"}],
        });
        let mut schema = source.clone();
        compact(&mut schema, &private(&["decision"]));
        sim_assert_eq!(have: &schema, want: &json!({
            "$schema":"http://json-schema.org/draft-07/schema#",
            "definitions":{"decision":expected},"allOf":[{"$ref":"#/definitions/decision"}],
        }));
        let before = jsonschema::validator_for(&source)?;
        let after = jsonschema::validator_for(&schema)?;
        for input in [
            json!({}),
            json!({"p":true}),
            json!({"p":false}),
            json!({"q":true}),
            json!({"p":true,"q":true}),
            json!({"p":true,"q":false}),
            json!({"p":false,"q":true}),
            json!(null),
            json!(7),
            json!([]),
        ] {
            sim_assert_eq!(have: after.is_valid(&input), want: before.is_valid(&input));
        }
    }
    Ok(())
}

#[test]
fn boolean_decisions_preserve_annotations_from_the_selected_arm() -> eyre::Result<()> {
    let test = json!({"const":"x", "title":"Condition"});
    let arm = json!({"default":"fallback", "title":"Fallback"});
    for (yes, no) in [
        (json!(true), json!(false)),
        (json!(false), json!(true)),
        (json!(false), arm.clone()),
        (arm.clone(), json!(false)),
        (json!(true), arm.clone()),
        (arm, json!(true)),
    ] {
        let source = json!({
            "$schema":"http://json-schema.org/draft-07/schema#",
            "definitions":{"decision":{"if":test,"then":yes,"else":no}},
            "allOf":[{"$ref":"#/definitions/decision"}],
        });
        let mut schema = source.clone();
        compact(&mut schema, &private(&["decision"]));
        let before = jsonschema::validator_for(&source)?;
        let after = jsonschema::validator_for(&schema)?;
        for input in [json!("x"), json!("y"), json!(7), json!({})] {
            let valid = before.is_valid(&input);
            sim_assert_eq!(have: after.is_valid(&input), want: valid);
            if valid {
                let annotations = |validator: &jsonschema::Validator| {
                    let mut values = validator
                        .evaluate(&input)
                        .iter_annotations()
                        .map(|entry| {
                            (
                                entry.instance_location.to_string(),
                                entry.annotations.value().to_string(),
                            )
                        })
                        .collect::<Vec<_>>();
                    values.sort();
                    values
                };
                sim_assert_eq!(have: annotations(&after), want: annotations(&before));
            }
        }
    }
    Ok(())
}

#[test]
fn compacted_boolean_decisions_share_their_referenced_child() -> eyre::Result<()> {
    let source = json!({
        "$schema":"http://json-schema.org/draft-07/schema#",
        "definitions":{
            "test":{"type":"string"},
            "decision":{"if":{"$ref":"#/definitions/test"},"then":true,"else":false},
        },
        "allOf":[{"$ref":"#/definitions/decision"}],
    });
    let mut schema = source.clone();
    compact(&mut schema, &private(&["test", "decision"]));
    sim_assert_eq!(have: &schema, want: &json!({
        "$schema":"http://json-schema.org/draft-07/schema#",
        "definitions":{"test":{"type":"string"}},"allOf":[{"$ref":"#/definitions/test"}],
    }));
    let before = jsonschema::validator_for(&source)?;
    let after = jsonschema::validator_for(&schema)?;
    for input in [json!("text"), json!(1), json!(null), json!({})] {
        sim_assert_eq!(have: after.is_valid(&input), want: before.is_valid(&input));
    }
    Ok(())
}

#[test]
fn boolean_decisions_respect_dialects_origins_and_reference_targets() {
    let source = json!({
        "$schema":"http://json-schema.org/draft-07/schema#",
        "definitions":{"decision":{"if":{"type":"string"},"then":true,"else":false}},
        "allOf":[{"$ref":"#/definitions/decision"}],
    });
    // Unknown and annotation-sensitive dialects keep the original decision structure.
    for dialect in [
        None,
        Some("https://json-schema.org/draft/2020-12/schema"),
        Some("urn:other"),
    ] {
        let mut schema = source.clone();
        if let Some(dialect) = dialect {
            schema["$schema"] = json!(dialect);
        } else if let Some(object) = schema.as_object_mut() {
            object.remove("$schema");
        }
        let expected = schema.clone();
        compact(&mut schema, &private(&["decision"]));
        sim_assert_eq!(have: schema, want: expected);
    }
    for added in [
        json!({"allOf":[{"$ref":"#/definitions/decision"},{"$ref":"#/definitions/decision/if"}]}),
        json!({"allOf":[{"$ref":"#/definitions/decision"},{"$ref":"#/definitions/decision/then"}]}),
        json!({"definitions":{"decision":{"if":{"$ref":"#/definitions/decision"},"then":true,"else":false}}}),
        json!({"definitions":{"decision":{"if":{"$id":"urn:inner","type":"string"},"then":true,"else":false}}}),
        json!({"definitions":{"decision":{"if":{"$ref":"urn:external"},"then":true,"else":false}}}),
        json!({"definitions":{"decision":{"if":"invalid","then":true,"else":false}}}),
    ] {
        let mut schema = source.clone();
        if let (Some(schema), Some(added)) = (schema.as_object_mut(), added.as_object()) {
            schema.extend(added.clone());
        }
        let expected = schema.clone();
        compact(&mut schema, &private(&["decision"]));
        sim_assert_eq!(have: schema, want: expected);
    }
    for origins in [
        BTreeMap::new(),
        BTreeMap::from([(
            "decision".to_owned(),
            vec![DefinitionOrigin {
                provider: "k8s".to_owned(),
                document: "example".to_owned(),
                pointer: "/field".to_owned(),
            }],
        )]),
    ] {
        let mut schema = source.clone();
        compact(&mut schema, &origins);
        sim_assert_eq!(have: schema, want: source);
    }
    let mut schema = source.clone();
    schema["definitions"]["decision"]["description"] = Value::String("keep annotations".to_owned());
    let expected = schema.clone();
    compact(&mut schema, &private(&["decision"]));
    sim_assert_eq!(have: schema, want: expected);
}

#[test]
fn deduplication_follows_canonical_children_and_transparent_aliases() -> eyre::Result<()> {
    let source = json!({
        "$defs": {
            "a-leaf": {"type": "integer"},
            "b-leaf": {"type": "integer"},
            "a-parent": {"properties": {"item": {"$ref": "#/$defs/a-leaf"}}},
            "b-parent": {"properties": {"item": {"$ref": "#/$defs/b-leaf"}}},
            "z-alias": {"$ref": "#/$defs/b-parent"},
        },
        "allOf": [
            {"$ref": "#/$defs/a-parent"},
            {"$ref": "#/$defs/b-parent"},
            {"$ref": "#/$defs/z-alias"},
        ],
        "default": {"$ref": "#/$defs/z-alias"},
    });
    let mut schema = source.clone();
    compact(
        &mut schema,
        &private(&["a-leaf", "b-leaf", "a-parent", "b-parent", "z-alias"]),
    );
    sim_assert_eq!(
        have: &schema,
        want: &json!({
            "$defs": {
                "a-leaf": {"type": "integer"},
                "a-parent": {"properties": {"item": {"$ref": "#/$defs/a-leaf"}}},
            },
            "allOf": [
                {"$ref": "#/$defs/a-parent"},
                {"$ref": "#/$defs/a-parent"},
                {"$ref": "#/$defs/a-parent"},
            ],
            "default": {"$ref": "#/$defs/z-alias"},
        }),
    );
    let before = jsonschema::validator_for(&source)?;
    let after = jsonschema::validator_for(&schema)?;
    for input in [json!({"item": 1}), json!({"item": "text"}), json!({})] {
        sim_assert_eq!(have: after.is_valid(&input), want: before.is_valid(&input));
    }
    Ok(())
}

#[test]
fn only_private_originless_definitions_share() {
    let body = json!({"type": "string", "minLength": 3});
    let mut schema = json!({
        "$defs": {"named": body, "provider": body, "private-a": body, "private-b": body},
        "allOf": [
            {"$ref": "#/$defs/named"},
            {"$ref": "#/$defs/provider"},
            {"$ref": "#/$defs/private-a"},
            {"$ref": "#/$defs/private-b"},
        ],
    });
    let mut anonymous = private(&["private-a", "private-b"]);
    anonymous.insert(
        "provider".to_string(),
        vec![DefinitionOrigin {
            provider: "crd".to_string(),
            document: "example".to_string(),
            pointer: "/spec".to_string(),
        }],
    );
    compact(&mut schema, &anonymous);
    sim_assert_eq!(
        have: schema,
        want: json!({
            "$defs": {"named": body, "provider": body, "private-a": body},
            "allOf": [
                {"$ref": "#/$defs/named"},
                {"$ref": "#/$defs/provider"},
                {"$ref": "#/$defs/private-a"},
                {"$ref": "#/$defs/private-a"},
            ],
        }),
    );
}

#[test]
fn both_definition_namespaces_keep_their_reference_syntax() {
    let mut schema = json!({
        "$defs": {"ambiguous": true, "first": true, "second": true},
        "definitions": {"ambiguous": false, "a/name%": false, "second-legacy": false},
        "allOf": [
            {"$ref": "#/$defs/ambiguous"},
            {"$ref": "#/definitions/ambiguous"},
            {"$ref": "#/$defs/first"},
            {"$ref": "#/$defs/second"},
            {"$ref": "#/definitions/a~1name%25"},
            {"$ref": "#/definitions/second-legacy"},
        ],
    });
    compact(
        &mut schema,
        &private(&["ambiguous", "first", "second", "a/name%", "second-legacy"]),
    );
    sim_assert_eq!(
        have: schema,
        want: json!({
            "$defs": {"ambiguous": true, "first": true},
            "definitions": {"ambiguous": false, "a/name%": false},
            "allOf": [
                {"$ref": "#/$defs/ambiguous"},
                {"$ref": "#/definitions/ambiguous"},
                {"$ref": "#/$defs/first"},
                {"$ref": "#/$defs/first"},
                {"$ref": "#/definitions/a~1name%25"},
                {"$ref": "#/definitions/a~1name%25"},
            ],
        }),
    );
}

#[test]
fn scoped_sibling_and_suffix_targets_stay_addressable() {
    let pinned_body = json!({"type": "object", "properties": {"item": {"type": "string"}}});
    let scoped = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "allOf": [{"$ref": "#/$defs/suffix"}],
    });
    let mut schema = json!({
        "$defs": {
            "suffix": pinned_body,
            "sibling": pinned_body,
            "safe-a": pinned_body,
            "safe-b": pinned_body,
            "scoped": scoped,
        },
        "allOf": [
            {"$ref": "#/$defs/suffix/properties/item"},
            {"$ref": "#/$defs/sibling", "title": "At its original address"},
            {"$ref": "#/$defs/safe-a"},
            {"$ref": "#/$defs/safe-b"},
            {"$ref": "#/$defs/scoped"},
        ],
    });
    compact(
        &mut schema,
        &private(&["suffix", "sibling", "safe-a", "safe-b", "scoped"]),
    );
    sim_assert_eq!(
        have: schema,
        want: json!({
            "$defs": {
                "suffix": pinned_body,
                "sibling": pinned_body,
                "safe-a": pinned_body,
                "scoped": scoped,
            },
            "allOf": [
                {"$ref": "#/$defs/suffix/properties/item"},
                {"$ref": "#/$defs/sibling", "title": "At its original address"},
                {"$ref": "#/$defs/safe-a"},
                {"$ref": "#/$defs/safe-a"},
                {"$ref": "#/$defs/scoped"},
            ],
        }),
    );
}

#[test]
fn draft7_ignored_nested_identifiers_leave_outer_references_addressable() -> eyre::Result<()> {
    for identifier in ["id", "$id"] {
        let source = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "definitions": {"left": {"type": "string"}, "right": {"type": "string"}},
            "allOf": [
                {"$ref": "#/definitions/left"},
                {"$ref": "#/definitions/right"},
                {(identifier): "urn:ignored", "$ref": "#/definitions/right"},
            ],
        });
        let mut schema = source.clone();
        compact(&mut schema, &private(&["left", "right"]));
        sim_assert_eq!(have: &schema, want: &source);
        let before = jsonschema::validator_for(&source)?;
        let after = jsonschema::validator_for(&schema)?;
        for (input, valid) in [(json!("string"), true), (json!(12), false)] {
            sim_assert_eq!(have: before.is_valid(&input), want: valid);
            sim_assert_eq!(have: after.is_valid(&input), want: valid);
        }
    }
    Ok(())
}

#[test]
fn opaque_references_do_not_relocate_private_targets() {
    for reference in [
        json!({"$ref": "other.schema.json#/$defs/right"}),
        json!({"$dynamicRef": "#/definitions/right"}),
        json!({"$ref": "#/default"}),
    ] {
        let mut schema = json!({
            "$defs": {"left": {"type": "string"}, "right": {"type": "string"}},
            "default": {"$ref": "#/$defs/right"},
            "allOf": [
                {"$ref": "#/$defs/left"},
                {"$ref": "#/$defs/right"},
                reference,
            ],
        });
        let expected = schema.clone();
        compact(&mut schema, &private(&["left", "right"]));
        sim_assert_eq!(have: schema, want: expected);
    }
}

#[test]
fn cycles_through_document_root_and_other_definitions_stay_separate() {
    let body = json!({
        "type": "object",
        "properties": {"child": {"$ref": "#"}},
    });
    let mut schema = json!({
        "$defs": {
            "a": body,
            "b": body,
            "a-cycle": {"$ref": "#/$defs/b-cycle"},
            "b-cycle": {"$ref": "#/$defs/a-cycle"},
        },
        "allOf": [
            {"$ref": "#/$defs/a"}, {"$ref": "#/$defs/b"}, {"$ref": "#/$defs/a-cycle"},
        ],
    });
    let expected = schema.clone();
    compact(&mut schema, &private(&["a", "b", "a-cycle", "b-cycle"]));
    sim_assert_eq!(have: schema, want: expected);
}

#[test]
fn representative_is_independent_of_definition_insertion_order() {
    let bodies = [
        (
            "first".to_string(),
            json!({"type": "object", "required": ["flag"]}),
        ),
        (
            "second".to_string(),
            json!({"type": "object", "required": ["flag"]}),
        ),
    ];
    let mut left = json!({
        "$defs": bodies.clone().into_iter().collect::<Map<_, _>>(),
        "allOf": [{"$ref": "#/$defs/second"}, {"$ref": "#/$defs/first"}],
    });
    let mut right = json!({
        "$defs": bodies.into_iter().rev().collect::<Map<_, _>>(),
        "allOf": [{"$ref": "#/$defs/second"}, {"$ref": "#/$defs/first"}],
    });
    let anonymous = private(&["first", "second"]);
    compact(&mut left, &anonymous);
    compact(&mut right, &anonymous);
    sim_assert_eq!(have: &left, want: &right);
    sim_assert_eq!(
        have: left,
        want: json!({
            "$defs": {"first": {"type": "object", "required": ["flag"]}},
            "allOf": [{"$ref": "#/$defs/first"}, {"$ref": "#/$defs/first"}],
        }),
    );
}
