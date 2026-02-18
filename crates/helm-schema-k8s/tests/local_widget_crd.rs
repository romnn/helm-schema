use helm_schema_ir::{ResourceRef, YamlPath};
use helm_schema_k8s::{K8sSchemaProvider, LocalSchemaProvider};
use std::sync::atomic::{AtomicUsize, Ordering};

static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn make_temp_dir(group_dir: &str) -> std::path::PathBuf {
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "helm-schema.local-test.{}.{}",
        std::process::id(),
        n
    ));
    std::fs::create_dir_all(dir.join(group_dir)).expect("create temp dir");
    dir
}

#[test]
fn materialize_expands_refs() {
    let group = "acme.example.com";
    let api_version = "acme.example.com/v1";
    let kind = "Widget";
    let filename = "acme.example.com/widget_v1.json";

    let root_dir = make_temp_dir(group);

    let schema_doc = serde_json::json!({
        "$schema": "http://json-schema.org/schema#",
        "type": "object",
        "properties": {
            "apiVersion": {"type": ["string", "null"], "enum": [api_version]},
            "kind": {"type": ["string", "null"], "enum": [kind]},
            "spec": {"$ref": "#/definitions/WidgetSpec"}
        },
        "definitions": {
            "WidgetSpec": {
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {"$ref": "#/definitions/WidgetItem"}
                    }
                }
            },
            "WidgetItem": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"}
                }
            }
        }
    });
    std::fs::write(
        root_dir.join(filename),
        serde_json::to_vec(&schema_doc).expect("schema json bytes"),
    )
    .expect("write schema doc");

    let provider = LocalSchemaProvider::new(&root_dir);

    let r = ResourceRef {
        api_version: api_version.to_string(),
        kind: kind.to_string(),
        api_version_candidates: Vec::new(),
    };

    let actual = provider
        .materialize_schema_for_resource(&r)
        .expect("materialize");

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/schema#",
        "type": "object",
        "properties": {
            "apiVersion": {"type": ["string", "null"], "enum": [api_version]},
            "kind": {"type": ["string", "null"], "enum": [kind]},
            "spec": {
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"}
                            }
                        }
                    }
                }
            }
        },
        "definitions": {
            "WidgetSpec": {
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"}
                            }
                        }
                    }
                }
            },
            "WidgetItem": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"}
                }
            }
        }
    });

    similar_asserts::assert_eq!(actual, expected);
}

#[test]
fn leaf_schema() {
    let group = "acme.example.com";
    let api_version = "acme.example.com/v1";
    let kind = "Widget";
    let filename = "acme.example.com/widget_v1.json";

    let root_dir = make_temp_dir(group);

    let schema_doc = serde_json::json!({
        "$schema": "http://json-schema.org/schema#",
        "type": "object",
        "properties": {
            "apiVersion": {"type": ["string", "null"], "enum": [api_version]},
            "kind": {"type": ["string", "null"], "enum": [kind]},
            "spec": {
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"}
                            }
                        }
                    }
                }
            }
        }
    });
    std::fs::write(
        root_dir.join(filename),
        serde_json::to_vec(&schema_doc).expect("schema json bytes"),
    )
    .expect("write schema doc");

    let provider = LocalSchemaProvider::new(&root_dir);

    let r = ResourceRef {
        api_version: api_version.to_string(),
        kind: kind.to_string(),
        api_version_candidates: Vec::new(),
    };

    let path = YamlPath(vec![
        "spec".to_string(),
        "items[*]".to_string(),
        "name".to_string(),
    ]);

    let schema = provider
        .schema_for_resource_path(&r, &path)
        .expect("leaf schema");

    let expected = serde_json::json!({"type": "string"});
    similar_asserts::assert_eq!(schema, expected);
}

#[test]
fn returns_none_for_missing_schema() {
    let root_dir = make_temp_dir("acme.example.com");

    let provider = LocalSchemaProvider::new(&root_dir);

    let r = ResourceRef {
        api_version: "acme.example.com/v1".to_string(),
        kind: "NonExistent".to_string(),
        api_version_candidates: Vec::new(),
    };

    assert!(provider.materialize_schema_for_resource(&r).is_none());
}
