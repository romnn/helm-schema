use std::collections::HashMap;
use test_util::prelude::sim_assert_eq;

use serde_json::{Value, json};

use super::*;

fn descend_schema_path(schema: &Value, path: &[String]) -> Option<Value> {
    let mut current = schema;
    for segment in path {
        current = descend_one(current, segment)?;
    }
    Some(current.clone())
}

fn descend_one<'a>(schema: &'a Value, segment: &str) -> Option<&'a Value> {
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            for branch in branches {
                if let Some(value) = descend_one(branch, segment) {
                    return Some(value);
                }
            }
        }
    }

    let (key, is_array_item) = segment
        .strip_suffix("[*]")
        .map_or((segment, false), |key| (key, true));

    let mut next = schema
        .get("properties")
        .and_then(|properties| properties.as_object())
        .and_then(|properties| properties.get(key))
        .or_else(|| {
            schema
                .get("additionalProperties")
                .and_then(|additional_properties| {
                    if additional_properties.is_boolean() {
                        None
                    } else {
                        Some(additional_properties)
                    }
                })
        })?;

    if is_array_item {
        next = next.get("items").or_else(|| {
            next.get("prefixItems")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
        })?;
    }

    Some(next)
}

#[test]
fn lazy_path_descent_matches_full_expansion_for_cross_file_array_ref() {
    let root = SchemaDoc::new(json!({
        "type": "object",
        "properties": {
            "spec": {
                "$ref": "defs.json#/definitions/Spec"
            }
        }
    }));
    let definitions = SchemaDoc::new(json!({
        "definitions": {
            "Spec": {
                "type": "object",
                "properties": {
                    "containers": {
                        "type": "array",
                        "items": {
                            "$ref": "#/definitions/Container"
                        }
                    }
                }
            },
            "Container": {
                "type": "object",
                "properties": {
                    "env": {
                        "type": "object",
                        "additionalProperties": {
                            "type": "string"
                        }
                    }
                }
            }
        }
    }));
    let docs = HashMap::from([("defs.json".to_string(), definitions.clone())]);
    let path = vec![
        "spec".to_string(),
        "containers[*]".to_string(),
        "env".to_string(),
    ];

    let mut full_ctx = ResolveCtx::new(
        {
            let docs = docs.clone();
            move |filename| docs.get(filename).cloned()
        },
        "root.json".to_string(),
        root.clone(),
    );
    let expanded_root_doc = full_ctx
        .doc("root.json")
        .cloned()
        .expect("root doc should be present");
    let expanded_root = descend_schema_path_expanding_leaf_with_location(
        &mut full_ctx,
        "root.json",
        &expanded_root_doc,
        &[],
    )
    .expect("expanded root should be present")
    .schema()
    .clone();
    let expected =
        descend_schema_path(&expanded_root, &path).expect("expanded root should contain path");

    let mut lazy_ctx = ResolveCtx::new(
        move |filename| docs.get(filename).cloned(),
        "root.json".to_string(),
        root,
    );
    let lazy_root = lazy_ctx
        .doc("root.json")
        .cloned()
        .expect("root doc should be present");
    let actual = descend_schema_path_expanding_leaf_with_location(
        &mut lazy_ctx,
        "root.json",
        &lazy_root,
        &path,
    )
    .expect("lazy descent should contain path")
    .schema()
    .clone();

    sim_assert_eq!(have: actual, want: expected);
}

#[test]
fn lazy_path_descent_reports_leaf_source_location_after_cross_file_refs() {
    let root = SchemaDoc::new(json!({
        "type": "object",
        "properties": {
            "spec": {
                "$ref": "defs.json#/definitions/Spec"
            }
        }
    }));
    let definitions = SchemaDoc::new(json!({
        "definitions": {
            "Spec": {
                "type": "object",
                "properties": {
                    "containers": {
                        "type": "array",
                        "items": {
                            "$ref": "#/definitions/Container"
                        }
                    }
                }
            },
            "Container": {
                "type": "object",
                "properties": {
                    "env": {
                        "$ref": "#/definitions/StringMap"
                    }
                }
            },
            "StringMap": {
                "type": "object",
                "additionalProperties": {
                    "type": "string"
                }
            }
        }
    }));
    let docs = HashMap::from([("defs.json".to_string(), definitions)]);
    let path = vec![
        "spec".to_string(),
        "containers[*]".to_string(),
        "env".to_string(),
    ];

    let mut ctx = ResolveCtx::new(
        move |filename| docs.get(filename).cloned(),
        "root.json".to_string(),
        root,
    );
    let root_doc = ctx
        .doc("root.json")
        .cloned()
        .expect("root doc should be present");
    let actual =
        descend_schema_path_expanding_leaf_with_location(&mut ctx, "root.json", &root_doc, &path)
            .expect("lazy descent should contain path");

    sim_assert_eq!(have: actual.location().filename(), want: "defs.json");
    sim_assert_eq!(
        have: actual.location().pointer(),
        want: "/definitions/Container/properties/env"
    );
    sim_assert_eq!(
        have: actual.source_schema(),
        want: &json!({ "$ref": "#/definitions/StringMap" })
    );
    sim_assert_eq!(
        have: actual.schema(),
        want: &json!({
            "type": "object",
            "additionalProperties": {
                "type": "string"
            }
        })
    );
}
