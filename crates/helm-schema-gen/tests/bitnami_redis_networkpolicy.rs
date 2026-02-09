mod common;

use helm_schema_ast::{FusedRustParser, HelmParser};
use helm_schema_gen::{DefaultValuesSchemaGenerator, ValuesSchemaGenerator};
use helm_schema_ir::{DefaultIrGenerator, IrGenerator};
use helm_schema_k8s::DefaultK8sSchemaProvider;

/// Full schema generation for networkpolicy using fused-Rust parser.
///
/// The generated schema should capture all `.Values.*` references from the
/// networkpolicy template and produce a well-structured JSON schema that a
/// devops engineer would recognize as describing the values.yaml structure.
#[test]
fn schema_fused_rust() {
    let src = common::networkpolicy_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = common::build_define_index(&FusedRustParser);
    let ir = DefaultIrGenerator.generate(&ast, &idx);
    let schema = DefaultValuesSchemaGenerator.generate(&ir, &DefaultK8sSchemaProvider);

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
                "anyOf": [
                    { "type": "object", "additionalProperties": {} },
                    { "type": "string" }
                ]
            },
            "commonLabels": {
                "type": "object",
                "properties": {},
                "additionalProperties": { "type": "string" }
            },
            "master": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "containerPorts": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "redis": { "type": "integer" }
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
                            "http": { "type": "integer" }
                        }
                    },
                    "enabled": { "type": "boolean" }
                }
            },
            "networkPolicy": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "allowExternal": { "type": "string" },
                    "allowExternalEgress": { "type": "string" },
                    "enabled": { "type": "boolean" },
                    "extraEgress": {
                        "anyOf": [
                            { "type": "object", "additionalProperties": {} },
                            { "type": "string" }
                        ]
                    },
                    "extraIngress": {
                        "anyOf": [
                            { "type": "object", "additionalProperties": {} },
                            { "type": "string" }
                        ]
                    },
                    "ingressNSMatchLabels": { "type": "string" },
                    "ingressNSPodMatchLabels": { "type": "string" },
                    "metrics": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "allowExternal": { "type": "string" },
                            "ingressNSMatchLabels": { "type": "string" },
                            "ingressNSPodMatchLabels": { "type": "string" }
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
                            "sentinel": { "type": "integer" }
                        }
                    },
                    "enabled": { "type": "boolean" }
                }
            }
        }
    });

    similar_asserts::assert_eq!(schema, expected);
}
