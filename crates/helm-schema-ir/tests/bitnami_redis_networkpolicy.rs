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
#[allow(clippy::too_many_lines)]
fn symbolic_ir_from_tree_sitter() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/networkpolicy.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = build_define_index(&TreeSitterParser);
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(&src, &ast, &idx)
        .project();

    let actual: serde_json::Value = serde_json::to_value(&ir).expect("serialize");

    if std::env::var("SYMBOLIC_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let expected: serde_json::Value = serde_json::from_str(
        r#"
[
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "architecture",
        "type": "eq",
        "value": "replication"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "architecture"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonAnnotations"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonAnnotations"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonAnnotations"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonAnnotations"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
        "path": "networkPolicy.enabled",
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
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
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
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "architecture",
        "type": "eq",
        "value": "replication"
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
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "architecture",
        "type": "eq",
        "value": "replication"
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "architecture",
        "type": "eq",
        "value": "replication"
      },
      {
        "path": "commonLabels",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "egress[*]",
      "to[*]",
      "podSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "architecture",
        "type": "eq",
        "value": "replication"
      },
      {
        "path": "commonLabels",
        "type": "truthy"
      }
    ],
    "kind": "Fragment",
    "path": [
      "spec",
      "egress[*]",
      "to[*]",
      "podSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "path": "commonLabels",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "ingress[*]",
      "from[*]",
      "podSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "path": "commonLabels",
        "type": "truthy"
      }
    ],
    "kind": "Fragment",
    "path": [
      "spec",
      "ingress[*]",
      "from[*]",
      "podSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "commonLabels",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "podSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "commonLabels",
        "type": "truthy"
      }
    ],
    "kind": "Fragment",
    "path": [
      "spec",
      "podSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "commonLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
        "path": "networkPolicy.enabled",
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
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
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
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
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
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "fullnameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "fullnameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "architecture",
        "type": "eq",
        "value": "replication"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "egress[*]",
      "ports[*]",
      "port"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "master.containerPorts.redis"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "ingress[*]",
      "ports[*]",
      "port"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "master.containerPorts.redis"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "ingress[*]",
      "ports[*]",
      "port"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "metrics.containerPorts.http"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "metrics.enabled"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
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
        "path": "networkPolicy.enabled",
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
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "architecture",
        "type": "eq",
        "value": "replication"
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "architecture",
        "type": "eq",
        "value": "replication"
      },
      {
        "path": "commonLabels",
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
      "egress[*]",
      "to[*]",
      "podSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "architecture",
        "type": "eq",
        "value": "replication"
      },
      {
        "path": "commonLabels",
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
      "egress[*]",
      "to[*]",
      "podSelector",
      "matchLabels",
      "app.kubernetes.io/name"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "architecture",
        "type": "eq",
        "value": "replication"
      },
      {
        "path": "nameOverride",
        "type": "default"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "egress[*]",
      "to[*]",
      "podSelector",
      "matchLabels",
      "app.kubernetes.io/name"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "path": "commonLabels",
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
      "ingress[*]",
      "from[*]",
      "podSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "path": "commonLabels",
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
      "ingress[*]",
      "from[*]",
      "podSelector",
      "matchLabels",
      "app.kubernetes.io/name"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "path": "nameOverride",
        "type": "default"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "ingress[*]",
      "from[*]",
      "podSelector",
      "matchLabels",
      "app.kubernetes.io/name"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "commonLabels",
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
      "podSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "commonLabels",
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
      "podSelector",
      "matchLabels",
      "app.kubernetes.io/name"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
      "podSelector",
      "matchLabels",
      "app.kubernetes.io/name"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "namespaceOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
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
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "namespaceOverride"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.allowExternal"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.allowExternalEgress"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.enabled"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "networkPolicy.extraEgress",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.extraEgress"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "networkPolicy.extraEgress",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.extraEgress",
        "schema_type": "string",
        "type": "type_is"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.extraEgress"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "networkPolicy.extraEgress",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "egress[*]",
      "to"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.extraEgress"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "networkPolicy.extraEgress",
        "type": "truthy"
      }
    ],
    "kind": "Fragment",
    "path": [
      "spec",
      "egress[*]",
      "to"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.extraEgress"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.extraIngress",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.extraIngress"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.extraIngress",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.extraIngress",
        "schema_type": "string",
        "type": "type_is"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.extraIngress"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.extraIngress",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "ingress[*]",
      "from"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.extraIngress"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.extraIngress",
        "type": "truthy"
      }
    ],
    "kind": "Fragment",
    "path": [
      "spec",
      "ingress[*]",
      "from"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.extraIngress"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.ingressNSMatchLabels",
          "networkPolicy.ingressNSPodMatchLabels"
        ],
        "type": "or"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.ingressNSMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.ingressNSMatchLabels",
          "networkPolicy.ingressNSPodMatchLabels"
        ],
        "type": "or"
      },
      {
        "path": "networkPolicy.ingressNSMatchLabels",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.ingressNSMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.ingressNSMatchLabels",
          "networkPolicy.ingressNSPodMatchLabels"
        ],
        "type": "or"
      },
      {
        "path": "networkPolicy.ingressNSMatchLabels",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.ingressNSMatchLabels",
        "type": "range"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.ingressNSMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.ingressNSMatchLabels",
          "networkPolicy.ingressNSPodMatchLabels"
        ],
        "type": "or"
      },
      {
        "path": "networkPolicy.ingressNSMatchLabels",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.ingressNSMatchLabels",
        "type": "range"
      }
    ],
    "kind": "Fragment",
    "path": [
      "spec",
      "ingress[*]",
      "from[*]",
      "namespaceSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.ingressNSMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.ingressNSMatchLabels",
          "networkPolicy.ingressNSPodMatchLabels"
        ],
        "type": "or"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.ingressNSPodMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.ingressNSMatchLabels",
          "networkPolicy.ingressNSPodMatchLabels"
        ],
        "type": "or"
      },
      {
        "path": "networkPolicy.ingressNSPodMatchLabels",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.ingressNSPodMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.ingressNSMatchLabels",
          "networkPolicy.ingressNSPodMatchLabels"
        ],
        "type": "or"
      },
      {
        "path": "networkPolicy.ingressNSPodMatchLabels",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.ingressNSPodMatchLabels",
        "type": "range"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.ingressNSPodMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.ingressNSMatchLabels",
          "networkPolicy.ingressNSPodMatchLabels"
        ],
        "type": "or"
      },
      {
        "path": "networkPolicy.ingressNSPodMatchLabels",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.ingressNSPodMatchLabels",
        "type": "range"
      }
    ],
    "kind": "Fragment",
    "path": [
      "spec",
      "ingress[*]",
      "from[*]",
      "podSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.ingressNSPodMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.allowExternal",
        "type": "not"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.metrics.allowExternal"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.metrics.ingressNSMatchLabels",
          "networkPolicy.metrics.ingressNSPodMatchLabels"
        ],
        "type": "or"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.metrics.ingressNSMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.metrics.ingressNSMatchLabels",
          "networkPolicy.metrics.ingressNSPodMatchLabels"
        ],
        "type": "or"
      },
      {
        "path": "networkPolicy.metrics.ingressNSMatchLabels",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.metrics.ingressNSMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.metrics.ingressNSMatchLabels",
          "networkPolicy.metrics.ingressNSPodMatchLabels"
        ],
        "type": "or"
      },
      {
        "path": "networkPolicy.metrics.ingressNSMatchLabels",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.ingressNSMatchLabels",
        "type": "range"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.metrics.ingressNSMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.metrics.ingressNSMatchLabels",
          "networkPolicy.metrics.ingressNSPodMatchLabels"
        ],
        "type": "or"
      },
      {
        "path": "networkPolicy.metrics.ingressNSMatchLabels",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.ingressNSMatchLabels",
        "type": "range"
      }
    ],
    "kind": "Fragment",
    "path": [
      "spec",
      "ingress[*]",
      "from[*]",
      "namespaceSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.metrics.ingressNSMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.metrics.ingressNSMatchLabels",
          "networkPolicy.metrics.ingressNSPodMatchLabels"
        ],
        "type": "or"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.metrics.ingressNSPodMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.metrics.ingressNSMatchLabels",
          "networkPolicy.metrics.ingressNSPodMatchLabels"
        ],
        "type": "or"
      },
      {
        "path": "networkPolicy.metrics.ingressNSPodMatchLabels",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.metrics.ingressNSPodMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.metrics.ingressNSMatchLabels",
          "networkPolicy.metrics.ingressNSPodMatchLabels"
        ],
        "type": "or"
      },
      {
        "path": "networkPolicy.metrics.ingressNSPodMatchLabels",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.ingressNSPodMatchLabels",
        "type": "range"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.metrics.ingressNSPodMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "metrics.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.allowExternal",
        "type": "not"
      },
      {
        "paths": [
          "networkPolicy.metrics.ingressNSMatchLabels",
          "networkPolicy.metrics.ingressNSPodMatchLabels"
        ],
        "type": "or"
      },
      {
        "path": "networkPolicy.metrics.ingressNSPodMatchLabels",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.metrics.ingressNSPodMatchLabels",
        "type": "range"
      }
    ],
    "kind": "Fragment",
    "path": [
      "spec",
      "ingress[*]",
      "from[*]",
      "podSelector",
      "matchLabels"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "networkPolicy.metrics.ingressNSPodMatchLabels"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "architecture",
        "type": "eq",
        "value": "replication"
      },
      {
        "path": "sentinel.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "egress[*]",
      "ports[*]",
      "port"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "sentinel.containerPorts.sentinel"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "sentinel.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "spec",
      "ingress[*]",
      "ports[*]",
      "port"
    ],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "sentinel.containerPorts.sentinel"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "sentinel.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "sentinel.enabled"
  },
  {
    "guards": [
      {
        "path": "networkPolicy.enabled",
        "type": "truthy"
      },
      {
        "path": "networkPolicy.allowExternalEgress",
        "type": "not"
      },
      {
        "path": "architecture",
        "type": "eq",
        "value": "replication"
      },
      {
        "path": "sentinel.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "networking.k8s.io/v1",
      "kind": "NetworkPolicy"
    },
    "source_expr": "sentinel.enabled"
  }
]
"#,
    )
    .expect("parse expected");

    similar_asserts::assert_eq!(actual, expected);
}
