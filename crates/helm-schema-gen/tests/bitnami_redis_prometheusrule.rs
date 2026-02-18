mod common;

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};
use helm_schema_k8s::{
    ChainSchemaProvider, CrdsCatalogSchemaProvider, KubernetesJsonSchemaProvider,
};

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/bitnami-redis/templates/_helpers.tpl"),
    );
    idx
}

/// Full schema generation for prometheusrule using fused-Rust parser.
#[test]
#[allow(clippy::too_many_lines)]
fn schema_fused_rust() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/prometheusrule.yaml");
    let values_yaml = test_util::read_testdata("charts/bitnami-redis/values.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let crds = CrdsCatalogSchemaProvider::new().with_allow_download(true);
    let upstream = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);
    let provider = ChainSchemaProvider {
        first: crds,
        second: upstream,
    };
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let actual: serde_json::Value = schema;

    if std::env::var("SCHEMA_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

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
                                "description": "List of alerting and recording rules.",
                                "type": "array",
                                "items": {
                                    "additionalProperties": false,
                                    "description": "Rule describes an alerting or recording rule\nSee Prometheus documentation: [alerting](https://www.prometheus.io/docs/prometheus/latest/configuration/alerting_rules/) or [recording](https://www.prometheus.io/docs/prometheus/latest/configuration/recording_rules/#recording-rules) rule",
                                    "properties": {
                                        "alert": {
                                            "description": "Name of the alert. Must be a valid label value.\nOnly one of `record` and `alert` must be set.",
                                            "type": "string"
                                        },
                                        "annotations": {
                                            "additionalProperties": {"type": "string"},
                                            "description": "Annotations to add to each alert.\nOnly valid for alerting rules.",
                                            "type": "object"
                                        },
                                        "expr": {
                                            "anyOf": [
                                                {"type": "integer"},
                                                {"type": "string"}
                                            ],
                                            "description": "PromQL expression to evaluate.",
                                            "x-kubernetes-int-or-string": true
                                        },
                                        "for": {
                                            "description": "Alerts are considered firing once they have been returned for this long.",
                                            "pattern": "^(0|(([0-9]+)y)?(([0-9]+)w)?(([0-9]+)d)?(([0-9]+)h)?(([0-9]+)m)?(([0-9]+)s)?(([0-9]+)ms)?)$",
                                            "type": "string"
                                        },
                                        "keep_firing_for": {
                                            "description": "KeepFiringFor defines how long an alert will continue firing after the condition that triggered it has cleared.",
                                            "minLength": 1,
                                            "pattern": "^(0|(([0-9]+)y)?(([0-9]+)w)?(([0-9]+)d)?(([0-9]+)h)?(([0-9]+)m)?(([0-9]+)s)?(([0-9]+)ms)?)$",
                                            "type": "string"
                                        },
                                        "labels": {
                                            "additionalProperties": {"type": "string"},
                                            "description": "Labels to add or overwrite.",
                                            "type": "object"
                                        },
                                        "record": {
                                            "description": "Name of the time series to output to. Must be a valid metric name.\nOnly one of `record` and `alert` must be set.",
                                            "type": "string"
                                        }
                                    },
                                    "required": ["expr"],
                                    "type": "object"
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

#[test]
fn schema_validates_values_yaml() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/prometheusrule.yaml");
    let values_yaml = test_util::read_testdata("charts/bitnami-redis/values.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let crds = CrdsCatalogSchemaProvider::new().with_allow_download(true);
    let upstream = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);
    let provider = ChainSchemaProvider {
        first: crds,
        second: upstream,
    };
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let errors = common::validate_values_yaml(&values_yaml, &schema);
    assert!(
        errors.is_empty(),
        "values.yaml failed schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}

/// Schema generation using tree-sitter parser should produce same result.
#[test]
fn schema_both_parsers_same() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/prometheusrule.yaml");
    let values_yaml = test_util::read_testdata("charts/bitnami-redis/values.yaml");

    let provider = KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true);

    let rust_ast = FusedRustParser.parse(&src).expect("fused rust");
    let rust_idx = build_define_index(&FusedRustParser);
    let rust_ir = SymbolicIrGenerator.generate(&src, &rust_ast, &rust_idx);
    let rust_schema =
        generate_values_schema_with_values_yaml(&rust_ir, &provider, Some(&values_yaml));

    let ts_ast = TreeSitterParser.parse(&src).expect("tree-sitter");
    let ts_idx = build_define_index(&TreeSitterParser);
    let ts_ir = SymbolicIrGenerator.generate(&src, &ts_ast, &ts_idx);
    let ts_schema = generate_values_schema_with_values_yaml(&ts_ir, &provider, Some(&values_yaml));

    similar_asserts::assert_eq!(rust_schema, ts_schema);
}
