use helm_schema_core::{ResourceRef, YamlPath};
use helm_schema_k8s::{
    K8sSchemaProvider, LocalSchemaProvider, local_override::debug_materialize_schema_for_resource,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use test_util::prelude::sim_assert_eq;

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
        api_version_branches: Vec::new(),
    };

    let actual = debug_materialize_schema_for_resource(&provider, &r).expect("materialize");

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/schema#",
        "type": "object",
        "properties": {
            "apiVersion": {"type": ["string", "null"], "enum": [api_version]},
            "kind": {"type": ["string", "null"], "enum": [kind]},
            "metadata": {
                "type": "object",
                "properties": {
                    "annotations": {
                        "type": "object",
                        "additionalProperties": {"type": "string"}
                    },
                    "labels": {
                        "type": "object",
                        "additionalProperties": {"type": "string"}
                    },
                    "name": {"type": "string"},
                    "namespace": {"type": "string"}
                }
            },
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

    sim_assert_eq!(have: actual, want: expected);
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
        api_version_branches: Vec::new(),
    };

    let path = YamlPath(vec![
        "spec".to_string(),
        "items[*]".to_string(),
        "name".to_string(),
    ]);

    let schema = provider
        .lookup(&r, &path)
        .into_schema_fragment()
        .expect("leaf schema")
        .into_schema();

    let expected = serde_json::json!({"type": "string"});
    sim_assert_eq!(have: schema, want: expected);
}

#[test]
fn returns_none_for_missing_schema() {
    let root_dir = make_temp_dir("acme.example.com");

    let provider = LocalSchemaProvider::new(&root_dir);

    let r = ResourceRef {
        api_version: "acme.example.com/v1".to_string(),
        kind: "NonExistent".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };

    assert!(debug_materialize_schema_for_resource(&provider, &r).is_none());
}

// Pins Finding (round 3) #4 — `LocalSchemaProvider` (a.k.a. the
// `--crd-override-dir` layer) is general-purpose by design: it accepts
// schemas for ANY grouped resource, including built-in K8s ones. The
// docs were corrected to match this behavior. If anyone later tries
// to restrict the override layer to "CRD-only" (e.g. by adding an
// `is_k8s_builtin_group` guard), this test will fail and force them
// to read the README's explicit power-user contract first.
#[test]
fn local_provider_accepts_builtin_k8s_resource_override() {
    // Pin a custom (locally-modified) schema for the BUILT-IN
    // `rbac.authorization.k8s.io/v1` `ClusterRole` resource.
    let group = "rbac.authorization.k8s.io";
    let api_version = "rbac.authorization.k8s.io/v1";
    let kind = "ClusterRole";
    let filename = "rbac.authorization.k8s.io/clusterrole_v1.json";

    let root_dir = make_temp_dir(group);
    let schema_doc = serde_json::json!({
        "$schema": "http://json-schema.org/schema#",
        "type": "object",
        "title": "LOCAL_OVERRIDE_MARKER",
        "properties": {
            "apiVersion": {"type": ["string", "null"], "enum": [api_version]},
            "kind": {"type": ["string", "null"], "enum": [kind]},
        }
    });
    std::fs::write(
        root_dir.join(filename),
        serde_json::to_vec(&schema_doc).expect("schema bytes"),
    )
    .expect("write override");

    let provider = LocalSchemaProvider::new(&root_dir);
    let r = ResourceRef {
        api_version: api_version.to_string(),
        kind: kind.to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let actual = debug_materialize_schema_for_resource(&provider, &r)
        .expect("LocalSchemaProvider must answer for built-in group overrides");
    sim_assert_eq!(
        have: actual.get("title").and_then(|v| v.as_str()),
        want: Some("LOCAL_OVERRIDE_MARKER"),
        "the local override layer must serve the user's custom schema for built-in K8s kinds; \
         restricting it to CRD-only would silently fall through to the upstream schema"
    );
}
