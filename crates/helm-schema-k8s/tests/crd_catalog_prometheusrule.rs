use helm_schema_ir::{ResourceRef, YamlPath};
use helm_schema_k8s::{CrdCatalogSchemaProvider, K8sSchemaProvider};

#[test]
#[allow(clippy::too_many_lines)]
fn materialize_prometheusrule_from_catalog() {
    let provider =
        CrdCatalogSchemaProvider::new(test_util::workspace_testdata().join("crds-catalog"))
            .expect("provider");

    let r = ResourceRef {
        api_version: "monitoring.coreos.com/v1".to_string(),
        kind: "PrometheusRule".to_string(),
        api_version_candidates: Vec::new(),
    };

    let schema = provider
        .materialize_schema_for_resource(&r)
        .expect("materialize");

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/schema#",
        "type": "object",
        "properties": {
            "apiVersion": {
                "type": ["string", "null"],
                "enum": ["monitoring.coreos.com/v1"]
            },
            "kind": {
                "type": ["string", "null"],
                "enum": ["PrometheusRule"]
            },
            "spec": {
                "type": "object",
                "properties": {
                    "groups": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "rules": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "alert": {"type": "string"},
                                            "expr": {"type": "string"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        "definitions": {
            "PrometheusRuleSpec": {
                "type": "object",
                "properties": {
                    "groups": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "rules": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "alert": {"type": "string"},
                                            "expr": {"type": "string"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "RuleGroup": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "rules": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "alert": {"type": "string"},
                                "expr": {"type": "string"}
                            }
                        }
                    }
                }
            },
            "Rule": {
                "type": "object",
                "properties": {
                    "alert": {"type": "string"},
                    "expr": {"type": "string"}
                }
            }
        },
        "x-kubernetes-group-version-kind": [
            {"group": "monitoring.coreos.com", "kind": "PrometheusRule", "version": "v1"}
        ]
    });

    similar_asserts::assert_eq!(schema, expected);
}

#[test]
fn prometheusrule_leaf_schema_rules_items() {
    let provider =
        CrdCatalogSchemaProvider::new(test_util::workspace_testdata().join("crds-catalog"))
            .expect("provider");

    let r = ResourceRef {
        api_version: "monitoring.coreos.com/v1".to_string(),
        kind: "PrometheusRule".to_string(),
        api_version_candidates: Vec::new(),
    };

    let path = YamlPath(vec![
        "spec".to_string(),
        "groups[*]".to_string(),
        "rules[*]".to_string(),
        "expr".to_string(),
    ]);

    let leaf = provider.schema_for_resource_path(&r, &path).expect("leaf");

    let expected = serde_json::json!({"type": "string"});
    similar_asserts::assert_eq!(leaf, expected);
}
