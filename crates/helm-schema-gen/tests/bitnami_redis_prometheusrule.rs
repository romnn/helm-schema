mod common;

use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_gen::{DefaultValuesSchemaGenerator, ValuesSchemaGenerator};
use helm_schema_ir::{DefaultIrGenerator, IrGenerator};
use helm_schema_k8s::DefaultK8sSchemaProvider;

/// Full schema generation for prometheusrule using fused-Rust parser.
#[test]
fn schema_fused_rust() {
    let src = common::prometheusrule_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = common::build_define_index(&FusedRustParser);
    let ir = DefaultIrGenerator.generate(&ast, &idx);
    let schema = DefaultValuesSchemaGenerator.generate(&ir, &DefaultK8sSchemaProvider);

    let actual: serde_json::Value = schema;

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "commonAnnotations": {
                "anyOf": [
                    {
                        "type": "object",
                        "additionalProperties": {}
                    },
                    {
                        "type": "string"
                    }
                ]
            },
            "commonLabels": {
                "type": "object",
                "additionalProperties": {"type": "string"}
            },
            "metrics": {
                "type": "object",
                "properties": {
                    "enabled": {
                        "type": "boolean"
                    },
                    "prometheusRule": {
                        "type": "object",
                        "properties": {
                            "additionalLabels": {
                                "anyOf": [
                                    {
                                        "type": "object",
                                        "additionalProperties": {}
                                    },
                                    {
                                        "type": "string"
                                    }
                                ]
                            },
                            "enabled": {
                                "type": "boolean"
                            },
                            "namespace": {
                                "type": "string"
                            },
                            "rules": {
                                "type": "object",
                                "additionalProperties": {}
                            }
                        },
                        "additionalProperties": false
                    }
                },
                "additionalProperties": false
            }
        },
        "additionalProperties": false
    });

    similar_asserts::assert_eq!(actual, expected);
}

/// Schema generation using tree-sitter parser should produce same result.
#[test]
fn schema_both_parsers_same() {
    let src = common::prometheusrule_src();

    let rust_ast = FusedRustParser.parse(&src).expect("fused rust");
    let rust_idx = common::build_define_index(&FusedRustParser);
    let rust_ir = DefaultIrGenerator.generate(&rust_ast, &rust_idx);
    let rust_schema = DefaultValuesSchemaGenerator.generate(&rust_ir, &DefaultK8sSchemaProvider);

    let ts_ast = TreeSitterParser.parse(&src).expect("tree-sitter");
    let ts_idx = common::build_define_index(&TreeSitterParser);
    let ts_ir = DefaultIrGenerator.generate(&ts_ast, &ts_idx);
    let ts_schema = DefaultValuesSchemaGenerator.generate(&ts_ir, &DefaultK8sSchemaProvider);

    similar_asserts::assert_eq!(rust_schema, ts_schema);
}
