mod common;

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/bitnami-redis/templates/_helpers.tpl"),
    );
    for src in test_util::read_testdata_dir("charts/common/templates", "tpl") {
        let _ = idx.add_source(parser, &src);
    }
    idx
}

/// Full schema generation for prometheusrule using tree-sitter parser.
#[test]
#[allow(clippy::too_many_lines)]
fn schema_from_tree_sitter() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/prometheusrule.yaml");
    let values_yaml = test_util::read_testdata("charts/bitnami-redis/values.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = build_define_index(&TreeSitterParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let provider = common::production_crd_k8s_chain("v1.35.0");
    let schema = common::generate_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let actual: serde_json::Value = schema;

    if std::env::var("SCHEMA_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let expected: serde_json::Value = serde_json::from_str(
        r#"
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "additionalProperties": false,
  "properties": {
    "commonAnnotations": {
      "anyOf": [
        {
          "additionalProperties": {
            "type": "string"
          },
          "type": "object"
        },
        {
          "type": "string"
        }
      ]
    },
    "commonLabels": {
      "anyOf": [
        {
          "additionalProperties": {
            "type": "string"
          },
          "type": "object"
        },
        {
          "type": "string"
        }
      ]
    },
    "fullnameOverride": {
      "anyOf": [
        {
          "description": "Name of the rule group.",
          "minLength": 1,
          "type": "string"
        },
        {
          "enum": [
            ""
          ],
          "type": "string"
        },
        {
          "type": "null"
        }
      ]
    },
    "metrics": {
      "additionalProperties": false,
      "properties": {
        "enabled": {
          "type": "boolean"
        },
        "prometheusRule": {
          "additionalProperties": false,
          "properties": {
            "additionalLabels": {
              "anyOf": [
                {
                  "additionalProperties": {
                    "type": "string"
                  },
                  "type": "object"
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
              "anyOf": [
                {
                  "type": "null"
                },
                {
                  "type": "string"
                }
              ]
            },
            "rules": {
              "anyOf": [
                {
                  "description": "List of alerting and recording rules.",
                  "items": {
                    "additionalProperties": false,
                    "description": "Rule describes an alerting or recording rule\nSee Prometheus documentation: [alerting](https://www.prometheus.io/docs/prometheus/latest/configuration/alerting_rules/) or [recording](https://www.prometheus.io/docs/prometheus/latest/configuration/recording_rules/#recording-rules) rule",
                    "properties": {
                      "alert": {
                        "description": "Name of the alert. Must be a valid label value.\nOnly one of `record` and `alert` must be set.",
                        "type": "string"
                      },
                      "annotations": {
                        "additionalProperties": {
                          "type": "string"
                        },
                        "description": "Annotations to add to each alert.\nOnly valid for alerting rules.",
                        "type": "object"
                      },
                      "expr": {
                        "anyOf": [
                          {
                            "type": "integer"
                          },
                          {
                            "type": "string"
                          }
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
                        "additionalProperties": {
                          "type": "string"
                        },
                        "description": "Labels to add or overwrite.",
                        "type": "object"
                      },
                      "record": {
                        "description": "Name of the time series to output to. Must be a valid metric name.\nOnly one of `record` and `alert` must be set.",
                        "type": "string"
                      }
                    },
                    "required": [
                      "expr"
                    ],
                    "type": "object"
                  },
                  "type": "array"
                },
                {
                  "type": "string"
                }
              ]
            }
          },
          "type": "object"
        }
      },
      "type": "object"
    },
    "nameOverride": {
      "anyOf": [
        {
          "type": "null"
        },
        {
          "type": "string"
        }
      ]
    },
    "namespaceOverride": {
      "anyOf": [
        {
          "type": "null"
        },
        {
          "type": "string"
        }
      ]
    }
  },
  "type": "object"
}
"#,
    )
    .expect("parse expected");

    similar_asserts::assert_eq!(actual, expected);
}

#[test]
fn schema_validates_values_yaml() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/prometheusrule.yaml");
    let values_yaml = test_util::read_testdata("charts/bitnami-redis/values.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = build_define_index(&TreeSitterParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let provider = common::production_crd_k8s_chain("v1.35.0");
    let schema = common::generate_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let errors = common::validate_values_yaml(&values_yaml, &schema);
    assert!(
        errors.is_empty(),
        "values.yaml failed schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}
