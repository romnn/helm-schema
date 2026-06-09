use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/bitnami-redis/templates/_helpers.tpl"),
    )
    .expect("helpers");
    for src in test_util::read_testdata_dir("charts/common/templates", "tpl") {
        let _ = idx.add_source(parser, &src);
    }
    idx
}

#[test]
fn symbolic_ir_full() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/prometheusrule.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = build_define_index(&TreeSitterParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);

    let actual: serde_json::Value = serde_json::to_value(&ir).expect("serialize");

    if std::env::var("SYMBOLIC_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let _t = |p: &str| serde_json::json!({"type": "truthy", "path": p});
    let _pr =
        serde_json::json!({"api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule"});

    let expected: serde_json::Value = serde_json::from_str(
        r#"
[
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "commonAnnotations",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "commonAnnotations"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "commonAnnotations"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "commonAnnotations",
        "type": "truthy"
      },
      {
        "path": "commonAnnotations",
        "schema_type": "string",
        "type": "type_is"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "commonAnnotations"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "commonAnnotations",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "metadata",
      "annotations"
    ],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "commonAnnotations"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "commonAnnotations",
        "type": "truthy"
      }
    ],
    "kind": "Fragment",
    "path": [
      "metadata",
      "annotations"
    ],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "commonAnnotations"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "commonLabels",
        "schema_type": "string",
        "type": "type_is"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "commonLabels",
        "type": "truthy"
      }
    ],
    "kind": "Fragment",
    "path": [
      "metadata",
      "labels"
    ],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "fullnameOverride"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "fullnameOverride",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "fullnameOverride"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "fullnameOverride",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "groups[*]",
      "name"
    ],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "fullnameOverride"
  },
  {
    "guards": [],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "metrics.enabled"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.additionalLabels",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "metrics.prometheusRule.additionalLabels"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "metrics.prometheusRule.additionalLabels"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.additionalLabels",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.additionalLabels",
        "schema_type": "string",
        "type": "type_is"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "metrics.prometheusRule.additionalLabels"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.additionalLabels",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "metadata",
      "labels"
    ],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "metrics.prometheusRule.additionalLabels"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.additionalLabels",
        "type": "truthy"
      }
    ],
    "kind": "Fragment",
    "path": [
      "metadata",
      "labels"
    ],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "metrics.prometheusRule.additionalLabels"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "metrics.prometheusRule.enabled"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.namespace",
        "type": "default"
      }
    ],
    "kind": "Scalar",
    "path": [
      "metadata",
      "namespace"
    ],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "metrics.prometheusRule.namespace"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "metrics.prometheusRule.rules"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.rules",
        "schema_type": "string",
        "type": "type_is"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "metrics.prometheusRule.rules"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "groups[*]",
      "rules"
    ],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "metrics.prometheusRule.rules"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.rules",
        "type": "truthy"
      }
    ],
    "kind": "Fragment",
    "path": [
      "spec",
      "groups[*]",
      "rules"
    ],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "metrics.prometheusRule.rules"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "nameOverride",
        "schema_type": "string",
        "type": "type_is"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "nameOverride",
        "type": "default"
      }
    ],
    "kind": "Scalar",
    "path": [
      "metadata",
      "labels",
      "app.kubernetes.io/name"
    ],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "nameOverride",
        "type": "default"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "groups[*]",
      "name"
    ],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.prometheusRule.enabled",
        "type": "truthy"
      },
      {
        "path": "namespaceOverride",
        "type": "default"
      }
    ],
    "kind": "Scalar",
    "path": [
      "metadata",
      "namespace"
    ],
    "resource": {
      "api_version": "monitoring.coreos.com/v1",
      "kind": "PrometheusRule"
    },
    "source_expr": "namespaceOverride"
  }
]

"#,
    )
    .expect("parse expected");

    similar_asserts::assert_eq!(actual, expected);
}
