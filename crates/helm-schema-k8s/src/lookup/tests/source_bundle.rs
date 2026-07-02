use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;

#[test]
fn bundles_provider_document_refs_into_local_definitions() {
    let root_leaf = json!({
        "type": "object",
        "additionalProperties": { "$ref": "#/definitions/StringMap" }
    });
    let bundled = bundle_source_schema(
        SourceBundleNode::new(
            "source.json",
            "/definitions/Container/properties/env",
            root_leaf,
        ),
        |current_location, reference| {
            sim_assert_eq!(have: current_location.document.as_str(), want: "source.json");
            (reference == "#/definitions/StringMap").then(|| {
                SourceBundleNode::new(
                    "source.json",
                    "/definitions/StringMap",
                    json!({
                        "type": "object",
                        "additionalProperties": { "type": "string" }
                    }),
                )
            })
        },
    );

    sim_assert_eq!(
        have: bundled,
        want: json!({
            "type": "object",
            "additionalProperties": { "$ref": "#/$defs/StringMap" },
            "$defs": {
                "StringMap": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                }
            }
        })
    );
}

#[test]
fn keeps_leaf_local_definitions_intact() {
    let source_schema = json!({
        "type": "object",
        "properties": {
            "labels": { "$ref": "#/$defs/StringMap" }
        },
        "$defs": {
            "StringMap": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            }
        }
    });

    assert!(json_schema_walk::schema_refs_point_inside(&source_schema));
}

#[test]
fn bundles_cross_file_refs_into_local_definitions() {
    let root_leaf = json!({
        "type": "object",
        "properties": {
            "selector": { "$ref": "common.json#/definitions/Selector" }
        }
    });
    let bundled = bundle_source_schema(
        SourceBundleNode::new("pod.json", "/definitions/Spec", root_leaf),
        |_, reference| {
            (reference == "common.json#/definitions/Selector")
                .then(|| {
                    SourceBundleNode::new(
                        "common.json",
                        "/definitions/Selector",
                        json!({
                            "type": "object",
                            "properties": {
                                "matchLabels": {
                                    "$ref": "#/definitions/StringMap"
                                }
                            }
                        }),
                    )
                })
                .or_else(|| {
                    (reference == "#/definitions/StringMap").then(|| {
                        SourceBundleNode::new(
                            "common.json",
                            "/definitions/StringMap",
                            json!({
                                "type": "object",
                                "additionalProperties": { "type": "string" }
                            }),
                        )
                    })
                })
        },
    );

    sim_assert_eq!(
        have: bundled,
        want: json!({
            "type": "object",
            "properties": {
                "selector": { "$ref": "#/$defs/Selector" }
            },
            "$defs": {
                "Selector": {
                    "type": "object",
                    "properties": {
                        "matchLabels": { "$ref": "#/$defs/StringMap" }
                    }
                },
                "StringMap": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                }
            }
        })
    );
}
