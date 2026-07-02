use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;

fn universe_from_crd_documents<I: IntoIterator<Item = serde_json::Value>>(
    documents: I,
) -> LocalSchemaUniverse {
    let mut universe = LocalSchemaUniverse::default();
    for document in documents {
        for resource_schema in crate::resource_schemas_from_crd_document_with_source(
            document,
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
fn inserts_direct_resource_schema_without_crd_envelope() {
    let mut universe = LocalSchemaUniverse::default();
    universe.insert_resource_schema(LocalResourceSchema::new(
        "example.com/v1",
        "Widget",
        json!({
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
    ));

    let schema = universe
        .schema_doc_for_resource(&resource("example.com/v1"))
        .and_then(|schema_doc| {
            schema_doc
                .root()
                .pointer("/properties/spec/properties/enabled")
        });

    sim_assert_eq!(have: schema, want: Some(&json!({"type": "boolean"})));
}
