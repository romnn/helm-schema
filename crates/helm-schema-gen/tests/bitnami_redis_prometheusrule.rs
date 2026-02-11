mod common;

use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};
use helm_schema_k8s::{ChainSchemaProvider, CrdCatalogSchemaProvider, UpstreamK8sSchemaProvider};

/// Full schema generation for prometheusrule using fused-Rust parser.
#[test]
fn schema_fused_rust() {
    let src = common::prometheusrule_src();
    let values_yaml = common::values_yaml_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = common::build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let upstream = UpstreamK8sSchemaProvider::new("v1.29.0-standalone-strict")
        .with_cache_dir(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testdata/kubernetes-json-schema"
        ))
        .with_allow_download(false);
    let crds = CrdCatalogSchemaProvider::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/crds-catalog"
    ))
    .expect("crd catalog");
    let provider = ChainSchemaProvider {
        first: upstream,
        second: crds,
    };
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let actual: serde_json::Value = schema;

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "commonAnnotations": {
                "type": "object",
                "additionalProperties": {}
            },
            "commonLabels": {
                "type": "object",
                "additionalProperties": {}
            },
            "metrics": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "enabled": {"type": "boolean"},
                    "prometheusRule": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "additionalLabels": {
                                "type": "object",
                                "additionalProperties": {}
                            },
                            "enabled": {"type": "boolean"},
                            "namespace": {"type": "string"},
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
    });

    similar_asserts::assert_eq!(actual, expected);
}

/// Schema generation using tree-sitter parser should produce same result.
#[test]
fn schema_both_parsers_same() {
    let src = common::prometheusrule_src();
    let values_yaml = common::values_yaml_src();

    let provider = UpstreamK8sSchemaProvider::new("v1.29.0-standalone-strict")
        .with_cache_dir(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testdata/kubernetes-json-schema"
        ))
        .with_allow_download(false);

    let rust_ast = FusedRustParser.parse(&src).expect("fused rust");
    let rust_idx = common::build_define_index(&FusedRustParser);
    let rust_ir = SymbolicIrGenerator.generate(&src, &rust_ast, &rust_idx);
    let rust_schema =
        generate_values_schema_with_values_yaml(&rust_ir, &provider, Some(&values_yaml));

    let ts_ast = TreeSitterParser.parse(&src).expect("tree-sitter");
    let ts_idx = common::build_define_index(&TreeSitterParser);
    let ts_ir = SymbolicIrGenerator.generate(&src, &ts_ast, &ts_idx);
    let ts_schema = generate_values_schema_with_values_yaml(&ts_ir, &provider, Some(&values_yaml));

    similar_asserts::assert_eq!(rust_schema, ts_schema);
}
