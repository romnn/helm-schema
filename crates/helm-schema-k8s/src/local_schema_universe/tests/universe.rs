use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;

fn universe_from_crd_documents<I: IntoIterator<Item = serde_json::Value>>(
    documents: I,
) -> LocalSchemaUniverse {
    let mut universe = LocalSchemaUniverse::default();
    for document in documents {
        for resource_schema in crate::resource_schemas_from_crd_document_with_source(
            &document,
            "chart-local",
            String::new(),
        ) {
            universe.insert_resource_schema(resource_schema);
        }
    }
    universe
}

fn resource(api_version: &str) -> ResourceRef {
    ResourceRef::concrete(api_version.to_string(), "Widget".to_string())
}

#[test]
fn extracts_served_crd_version_schema() {
    let universe = universe_from_crd_documents([json!({
        "apiVersion": "apiextensions.k8s.io/v1",
        "kind": "CustomResourceDefinition",
        "spec": {
            "group": "example.com",
            "names": {"kind": "Widget"},
            "versions": [
                {
                    "name": "v1",
                    "served": true,
                    "schema": {
                        "openAPIV3Schema": {
                            "type": "object",
                            "properties": {
                                "spec": {
                                    "type": "object",
                                    "properties": {
                                        "size": {"type": "integer"}
                                    }
                                }
                            }
                        }
                    }
                }
            ]
        }
    })]);

    let schema = universe
        .schema_doc_for_resource(&resource("example.com/v1"))
        .and_then(|schema_doc| {
            schema_doc
                .root()
                .pointer("/properties/spec/properties/size")
        });

    sim_assert_eq!(have: schema, want: Some(&json!({"type": "integer"})));
}

#[test]
fn ignores_unserved_crd_versions() {
    let universe = universe_from_crd_documents([json!({
        "apiVersion": "apiextensions.k8s.io/v1",
        "kind": "CustomResourceDefinition",
        "spec": {
            "group": "example.com",
            "names": {"kind": "Widget"},
            "versions": [
                {
                    "name": "v1",
                    "served": false,
                    "schema": {"openAPIV3Schema": {"type": "object"}}
                }
            ]
        }
    })]);

    assert!(
        universe
            .schema_doc_for_resource(&resource("example.com/v1"))
            .is_none()
    );
}

#[test]
fn structural_crd_schemas_carry_their_pruning_contract() {
    let universe = universe_from_crd_documents([json!({
        "apiVersion": "apiextensions.k8s.io/v1",
        "kind": "CustomResourceDefinition",
        "spec": {
            "group": "example.com",
            "names": {"kind": "Widget"},
            "versions": [
                {
                    "name": "v1",
                    "served": true,
                    "schema": {
                        "openAPIV3Schema": {
                            "type": "object",
                            "properties": {
                                "metadata": {
                                    "type": "object",
                                    "properties": {"name": {"type": "string"}}
                                },
                                "spec": {
                                    "type": "object",
                                    "properties": {
                                        "labels": {
                                            "type": "object",
                                            "additionalProperties": {"type": "string"}
                                        },
                                        "rules": {
                                            "type": "array",
                                            "items": {
                                                "type": "object",
                                                "properties": {"action": {"type": "string"}}
                                            }
                                        },
                                        "either": {
                                            "type": "object",
                                            "oneOf": [
                                                {"properties": {"left": {"type": "string"}}},
                                                {"properties": {"right": {"type": "string"}}}
                                            ]
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            ]
        }
    })]);

    let root = universe
        .schema_doc_for_resource(&resource("example.com/v1"))
        .map(|schema_doc| schema_doc.root().clone())
        .unwrap_or(Value::Null);

    for (pointer, want, label) in [
        ("/additionalProperties", None, "the root stays open"),
        (
            "/properties/metadata/additionalProperties",
            None,
            "metadata is never pruned",
        ),
        (
            "/properties/spec/additionalProperties",
            Some(&json!(false)),
            "a declared object closes",
        ),
        (
            "/properties/spec/properties/rules/items/additionalProperties",
            Some(&json!(false)),
            "a declared item closes",
        ),
        (
            "/properties/spec/properties/labels/additionalProperties",
            Some(&json!({"type": "string"})),
            "a map keeps its value schema",
        ),
        (
            "/properties/spec/properties/either/oneOf/0/additionalProperties",
            None,
            "a junctor arm states no structure",
        ),
    ] {
        sim_assert_eq!(have: root.pointer(pointer), want: want, "{label}");
    }
}

#[test]
fn inserts_direct_resource_schema_without_crd_envelope() {
    let mut universe = LocalSchemaUniverse::default();
    universe.insert_resource_schema(LocalResourceSchema {
        api_version: "example.com/v1".to_string(),
        kind: "Widget".to_string(),
        schema: json!({
            "type": "object",
            "properties": {
                "spec": {
                    "type": "object",
                    "properties": {
                        "enabled": {"type": "boolean"}
                    }
                }
            }
        }),
        source_id: "chart-local".to_string(),
        filename: "example.com_v1_Widget.schema.json".to_string(),
    });

    let schema = universe
        .schema_doc_for_resource(&resource("example.com/v1"))
        .and_then(|schema_doc| {
            schema_doc
                .root()
                .pointer("/properties/spec/properties/enabled")
        });

    sim_assert_eq!(have: schema, want: Some(&json!({"type": "boolean"})));
}
