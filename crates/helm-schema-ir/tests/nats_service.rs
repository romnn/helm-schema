#![recursion_limit = "1024"]

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};

fn build_nats_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();

    idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats/templates/_helpers.tpl"),
    )
    .expect("nats helpers");
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats/templates/_jsonpatch.tpl"),
    )
    .expect("nats jsonpatch");
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats/templates/_tplYaml.tpl"),
    )
    .expect("nats tplYaml");
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats/templates/_toPrettyRawJson.tpl"),
    )
    .expect("nats toPrettyRawJson");

    // Files loaded via `.Files.Get`.
    idx.add_file_source(
        "files/service.yaml",
        &test_util::read_testdata("charts/nats/files/service.yaml"),
    );
    idx
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_from_tree_sitter() {
    let src = test_util::read_testdata("charts/nats/templates/service.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = build_nats_define_index(&TreeSitterParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);

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
    "guards": [],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "config"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.cluster.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.cluster.port"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.cluster.tls.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.gateway.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.gateway.port"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.gateway.tls.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.leafnodes.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.leafnodes.port"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.leafnodes.tls.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.monitor.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.monitor.port"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.monitor.tls.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.mqtt.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.mqtt.port"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.mqtt.tls.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.nats.port"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.nats.tls.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.profiling.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.profiling.port"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.profiling.tls.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.websocket.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.websocket.port"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "config.websocket.tls.enabled"
  },
  {
    "guards": [
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
        "path": "fullnameOverride",
        "schema_type": "string",
        "type": "type_is"
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
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "global.labels"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      },
      {
        "path": "global.labels",
        "type": "truthy"
      }
    ],
    "kind": "Fragment",
    "path": [
      "metadata",
      "labels"
    ],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "global.labels"
  },
  {
    "guards": [
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
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
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
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
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
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
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
      "selector",
      "app.kubernetes.io/name"
    ],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "namespaceOverride"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
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
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "namespaceOverride"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      },
      {
        "path": "namespaceOverride",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [
      "metadata",
      "namespace"
    ],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "namespaceOverride"
  },
  {
    "guards": [],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "natsBox.contexts"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "service"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "service.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      },
      {
        "path": "service.name",
        "type": "default"
      }
    ],
    "kind": "Scalar",
    "path": [
      "metadata",
      "name"
    ],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "service.name"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "service.ports"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "service.ports.cluster.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "service.ports.gateway.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "service.ports.leafnodes.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "service.ports.monitor.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "service.ports.mqtt.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "service.ports.nats.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "service.ports.profiling.enabled"
  },
  {
    "guards": [
      {
        "path": "service",
        "type": "with"
      },
      {
        "path": "service.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": {
      "api_version": "v1",
      "kind": "Service"
    },
    "source_expr": "service.ports.websocket.enabled"
  }
]
"#,
    )
    .expect("parse expected");

    similar_asserts::assert_eq!(actual, expected);
}
