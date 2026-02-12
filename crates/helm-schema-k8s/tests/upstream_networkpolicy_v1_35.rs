use helm_schema_ir::{ResourceRef, YamlPath};
use helm_schema_k8s::{K8sSchemaProvider, UpstreamK8sSchemaProvider};

fn test_cache_dir() -> String {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/kubernetes-json-schema"
    )
    .to_string()
}

#[test]
#[allow(clippy::too_many_lines)]
fn materialize_networkpolicy_v1_35() {
    let provider = UpstreamK8sSchemaProvider::new("v1.35.0")
        .with_cache_dir(test_cache_dir())
        .with_allow_download(false);

    let r = ResourceRef {
        api_version: "networking.k8s.io/v1".to_string(),
        kind: "NetworkPolicy".to_string(),
    };

    let schema = provider
        .materialize_schema_for_resource(&r)
        .expect("materialize schema");

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/schema#",
        "description": "NetworkPolicy describes what network traffic is allowed for a set of Pods",
        "type": "object",
        "properties": {
            "apiVersion": {
                "type": ["string", "null"],
                "enum": ["networking.k8s.io/v1"]
            },
            "kind": {
                "type": ["string", "null"],
                "enum": ["NetworkPolicy"]
            },
            "metadata": {
                "type": "object",
                "properties": {
                    "name": {"type": ["string", "null"]},
                    "namespace": {"type": ["string", "null"]},
                    "labels": {
                        "type": ["object", "null"],
                        "additionalProperties": {"type": "string"}
                    },
                    "annotations": {
                        "type": ["object", "null"],
                        "additionalProperties": {"type": "string"}
                    }
                }
            },
            "spec": {
                "type": "object",
                "properties": {
                    "podSelector": {
                        "type": "object",
                        "properties": {
                            "matchLabels": {
                                "type": ["object", "null"],
                                "additionalProperties": {"type": "string"}
                            }
                        }
                    },
                    "ingress": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "ports": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "port": {
                                                "anyOf": [
                                                    {"type": "integer"},
                                                    {"type": "string"}
                                                ]
                                            },
                                            "protocol": {"type": "string"}
                                        }
                                    }
                                },
                                "from": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "podSelector": {
                                                "type": "object",
                                                "properties": {
                                                    "matchLabels": {
                                                        "type": ["object", "null"],
                                                        "additionalProperties": {"type": "string"}
                                                    }
                                                }
                                            },
                                            "namespaceSelector": {
                                                "type": "object",
                                                "properties": {
                                                    "matchLabels": {
                                                        "type": ["object", "null"],
                                                        "additionalProperties": {"type": "string"}
                                                    }
                                                }
                                            },
                                            "ipBlock": {
                                                "type": "object",
                                                "properties": {
                                                    "cidr": {"type": "string"},
                                                    "except": {"type": "array", "items": {"type": "string"}}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "egress": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "ports": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "port": {
                                                "anyOf": [
                                                    {"type": "integer"},
                                                    {"type": "string"}
                                                ]
                                            },
                                            "protocol": {"type": "string"}
                                        }
                                    }
                                },
                                "to": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "podSelector": {
                                                "type": "object",
                                                "properties": {
                                                    "matchLabels": {
                                                        "type": ["object", "null"],
                                                        "additionalProperties": {"type": "string"}
                                                    }
                                                }
                                            },
                                            "namespaceSelector": {
                                                "type": "object",
                                                "properties": {
                                                    "matchLabels": {
                                                        "type": ["object", "null"],
                                                        "additionalProperties": {"type": "string"}
                                                    }
                                                }
                                            },
                                            "ipBlock": {
                                                "type": "object",
                                                "properties": {
                                                    "cidr": {"type": "string"},
                                                    "except": {"type": "array", "items": {"type": "string"}}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "policyTypes": {
                        "type": "array",
                        "items": {
                            "type": "string",
                            "enum": ["Ingress", "Egress"]
                        }
                    }
                }
            }
        },
        "x-kubernetes-group-version-kind": [
            {"group": "networking.k8s.io", "kind": "NetworkPolicy", "version": "v1"}
        ]
    });

    similar_asserts::assert_eq!(schema, expected);
}

#[test]
fn networkpolicy_leaf_schema_matchlabels() {
    let provider = UpstreamK8sSchemaProvider::new("v1.35.0")
        .with_cache_dir(test_cache_dir())
        .with_allow_download(false);

    let r = ResourceRef {
        api_version: "networking.k8s.io/v1".to_string(),
        kind: "NetworkPolicy".to_string(),
    };

    let path = YamlPath(vec![
        "spec".to_string(),
        "ingress[*]".to_string(),
        "from[*]".to_string(),
        "podSelector".to_string(),
        "matchLabels".to_string(),
    ]);

    let leaf = provider
        .schema_for_resource_path(&r, &path)
        .expect("leaf schema");

    let expected = serde_json::json!({
        "type": ["object", "null"],
        "additionalProperties": {"type": "string"}
    });

    similar_asserts::assert_eq!(leaf, expected);
}

#[test]
fn networkpolicy_by_kind_scan_when_api_version_missing() {
    let provider = UpstreamK8sSchemaProvider::new("v1.35.0")
        .with_cache_dir(test_cache_dir())
        .with_allow_download(false);

    let r = ResourceRef {
        api_version: String::new(),
        kind: "NetworkPolicy".to_string(),
    };

    let schema = provider
        .materialize_schema_for_resource(&r)
        .expect("materialize schema by kind");

    assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));
    assert!(
        schema
            .get("properties")
            .and_then(|v| v.as_object())
            .is_some()
    );
}
