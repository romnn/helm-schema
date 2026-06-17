use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::SymbolicIrContext;

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
fn symbolic_ir_from_tree_sitter() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/prometheusrule.yaml");
    let idx = build_define_index(&TreeSitterParser);
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(&src, &idx)
        .project();

    let actual: serde_json::Value =
        serde_json::to_value(helm_schema_ir::ContractDocumentV1::from_projection(ir))
            .expect("serialize");

    if std::env::var("SYMBOLIC_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let _t = |p: &str| serde_json::json!({"type": "truthy", "path": p});
    let _pr =
        serde_json::json!({"api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule"});

    let expected_uses: serde_json::Value = serde_json::from_str(
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
      },
      {
        "path": "nameOverride",
        "type": "truthy"
      },
      {
        "path": "nameOverride",
        "type": "default"
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
        "type": "default"
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
    "source_expr": "namespaceOverride"
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
    let expected = serde_json::json!({
        "version": 1,
        "uses": expected_uses
    });

    similar_asserts::assert_eq!(actual, expected);
}
