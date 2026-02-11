#![recursion_limit = "512"]

mod common;

use helm_schema_ast::{FusedRustParser, HelmParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};
use helm_schema_k8s::UpstreamK8sSchemaProvider;

/// Full schema generation for networkpolicy using fused-Rust parser.
///
/// The generated schema should capture all `.Values.*` references from the
/// networkpolicy template and produce a well-structured JSON schema that a
/// devops engineer would recognize as describing the values.yaml structure.
#[test]
fn schema_fused_rust() {
    let src = common::networkpolicy_src();
    let values_yaml = common::values_yaml_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = common::build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let provider = UpstreamK8sSchemaProvider::new("v1.35.0")
        .with_cache_dir(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testdata/kubernetes-json-schema"
        ))
        .with_allow_download(false);
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "architecture": {
                "anyOf": [
                    { "enum": ["replication"] },
                    { "type": "string" }
                ]
            },
            "commonAnnotations": {
                "type": ["object", "null"],
                "additionalProperties": {"type": "string"}
            },
            "commonLabels": {
                "type": ["object", "null"],
                "additionalProperties": {"type": "string"}
            },
            "master": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "containerPorts": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "redis": {
                                "anyOf": [
                                    {"type": "integer"},
                                    {"type": "string"}
                                ]
                            }
                        }
                    }
                }
            },
            "metrics": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "containerPorts": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "http": {
                                "anyOf": [
                                    {"type": "integer"},
                                    {"type": "string"}
                                ]
                            }
                        }
                    },
                    "enabled": {"type": "boolean"}
                }
            },
            "networkPolicy": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "allowExternal": {"type": "boolean"},
                    "allowExternalEgress": {"type": "boolean"},
                    "enabled": {"type": "boolean"},
                    "extraEgress": {
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
                    "extraIngress": {
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
                    "ingressNSMatchLabels": {
                        "type": ["object", "null"],
                        "additionalProperties": {"type": "string"}
                    },
                    "ingressNSPodMatchLabels": {
                        "type": ["object", "null"],
                        "additionalProperties": {"type": "string"}
                    },
                    "metrics": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "allowExternal": {"type": "boolean"},
                            "ingressNSMatchLabels": {
                                "type": ["object", "null"],
                                "additionalProperties": {"type": "string"}
                            },
                            "ingressNSPodMatchLabels": {
                                "type": ["object", "null"],
                                "additionalProperties": {"type": "string"}
                            }
                        }
                    }
                }
            },
            "sentinel": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "containerPorts": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "sentinel": {
                                "anyOf": [
                                    {"type": "integer"},
                                    {"type": "string"}
                                ]
                            }
                        }
                    },
                    "enabled": {"type": "boolean"}
                }
            }
        }
    });

    similar_asserts::assert_eq!(schema, expected);
}
