use helm_schema_core::{ResourceRef, YamlPath};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use crate::doc_backed_schema::{
    LocalSchemaLeaf, descend_schema_path_expanding_leaf_with_root_metadata_source,
    descend_schema_path_expanding_leaf_with_source,
};

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
                .filter(|&additional_properties| !additional_properties.is_boolean())
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

fn descend_schema_path_expanding_leaf(root: &Value, path: &[String]) -> Option<Value> {
    descend_schema_path_expanding_leaf_with_source(root, path).map(LocalSchemaLeaf::into_schema)
}

fn descend_schema_path_expanding_leaf_with_root_metadata(
    root: &Value,
    path: &[String],
) -> Option<Value> {
    descend_schema_path_expanding_leaf_with_root_metadata_source(root, path)
        .map(LocalSchemaLeaf::into_schema)
}

fn widget_resource() -> ResourceRef {
    ResourceRef::concrete("example.com/v1".to_string(), "Widget".to_string())
}

#[test]
fn lazy_local_path_descent_matches_full_expansion_for_array_ref() {
    let root = json!({
        "type": "object",
        "properties": {
            "spec": {
                "$ref": "#/definitions/Spec"
            }
        },
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
    });
    let path = vec![
        "spec".to_string(),
        "containers[*]".to_string(),
        "env".to_string(),
    ];

    let expanded =
        descend_schema_path_expanding_leaf(&root, &[]).expect("expanded root should be available");
    let expected =
        descend_schema_path(&expanded, &path).expect("expanded root should contain path");
    let actual =
        descend_schema_path_expanding_leaf(&root, &path).expect("lazy descent should contain path");

    sim_assert_eq!(have: actual, want: expected);
}

#[test]
fn source_aware_local_path_descent_reports_ref_target_pointer() {
    let root = json!({
        "type": "object",
        "properties": {
            "spec": {
                "$ref": "#/definitions/Spec"
            }
        },
        "definitions": {
            "Spec": {
                "type": "object",
                "properties": {
                    "size": { "type": "integer" }
                }
            }
        }
    });

    let leaf = descend_schema_path_expanding_leaf_with_source(
        &root,
        &["spec".to_string(), "size".to_string()],
    )
    .expect("lazy descent should resolve ref-backed path");

    sim_assert_eq!(have: leaf.clone().into_schema(), want: json!({ "type": "integer" }));
    sim_assert_eq!(have: leaf.source_schema(), want: Some(&json!({ "type": "integer" })));
    sim_assert_eq!(have: leaf.pointer(), want: Some("/definitions/Spec/properties/size"));
}

#[test]
fn source_aware_local_path_descent_preserves_raw_leaf_before_expansion() {
    let root = json!({
        "type": "object",
        "properties": {
            "spec": {
                "$ref": "#/definitions/Spec"
            }
        },
        "definitions": {
            "Spec": {
                "type": "object",
                "properties": {
                    "labels": { "$ref": "#/definitions/StringMap" }
                }
            },
            "StringMap": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            }
        }
    });

    let leaf = descend_schema_path_expanding_leaf_with_source(
        &root,
        &["spec".to_string(), "labels".to_string()],
    )
    .expect("lazy descent should resolve ref-backed leaf");

    sim_assert_eq!(
        have: leaf.source_schema(),
        want: Some(&json!({ "$ref": "#/definitions/StringMap" }))
    );
    sim_assert_eq!(
        have: leaf.clone().into_schema(),
        want: json!({
            "type": "object",
            "additionalProperties": { "type": "string" }
        })
    );
    sim_assert_eq!(have: leaf.pointer(), want: Some("/definitions/Spec/properties/labels"));
}

#[test]
fn lazy_root_metadata_descent_enriches_only_metadata_leaf() {
    let root = json!({
        "type": "object",
        "properties": {
            "metadata": {
                "type": "object",
                "properties": {
                    "labels": { "$ref": "#/definitions/StringMap" }
                }
            },
            "spec": {
                "type": "object",
                "properties": {
                    "replicas": { "type": "integer" }
                }
            }
        },
        "definitions": {
            "StringMap": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            }
        }
    });

    let metadata_name = descend_schema_path_expanding_leaf_with_root_metadata(
        &root,
        &["metadata".to_string(), "name".to_string()],
    )
    .expect("metadata.name should be synthesized from object metadata");
    sim_assert_eq!(have: metadata_name, want: json!({ "type": "string" }));

    let metadata_name_leaf = descend_schema_path_expanding_leaf_with_root_metadata_source(
        &root,
        &["metadata".to_string(), "name".to_string()],
    )
    .expect("metadata.name should be synthesized from object metadata");
    sim_assert_eq!(have: metadata_name_leaf.pointer(), want: None);

    let metadata_labels = descend_schema_path_expanding_leaf_with_root_metadata(
        &root,
        &["metadata".to_string(), "labels".to_string()],
    )
    .expect("metadata.labels should resolve local refs");
    sim_assert_eq!(
        have: metadata_labels,
        want: json!({
            "type": "object",
            "additionalProperties": { "type": "string" }
        })
    );

    let spec_replicas = descend_schema_path_expanding_leaf_with_root_metadata(
        &root,
        &["spec".to_string(), "replicas".to_string()],
    )
    .expect("non-metadata path should still descend the raw document");
    sim_assert_eq!(have: spec_replicas, want: json!({ "type": "integer" }));
}

#[test]
fn local_override_lookup_attaches_provider_source() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let root_dir = std::env::temp_dir().join(format!("helm-schema-local-override-source-{unique}"));
    let group_dir = root_dir.join("example.com");
    std::fs::create_dir_all(&group_dir).expect("create local override test directory");
    std::fs::write(
        group_dir.join("widget_v1.json"),
        serde_json::to_vec(&json!({
            "type": "object",
            "properties": {
                "spec": {
                    "$ref": "#/definitions/Spec"
                }
            },
            "definitions": {
                "Spec": {
                    "type": "object",
                    "properties": {
                        "size": { "type": "integer" }
                    }
                }
            }
        }))
        .expect("serialize local override schema"),
    )
    .expect("write local override schema");

    let provider = LocalSchemaProvider::new(&root_dir);
    let result = provider.lookup(
        &widget_resource(),
        &YamlPath(vec!["spec".to_string(), "size".to_string()]),
    );
    let ProviderLookupResult::Found { schema, .. } = result else {
        panic!("local override lookup should resolve spec.size");
    };
    let source = schema
        .source()
        .expect("local override source should attach");

    sim_assert_eq!(have: source.origin(), want: ProviderOrigin::LocalOverride);
    sim_assert_eq!(have: source.source_id(), want: root_dir.display().to_string());
    sim_assert_eq!(have: source.filename(), want: "example.com/widget_v1.json");
    sim_assert_eq!(have: source.pointer(), want: "/definitions/Spec/properties/size");
}
