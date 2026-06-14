#![recursion_limit = "1024"]

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::SymbolicIrContext;

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

    idx.add_file_source(
        "files/service-account.yaml",
        &test_util::read_testdata("charts/nats/files/service-account.yaml"),
    );

    idx
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_from_tree_sitter() {
    let src = test_util::read_testdata("charts/nats/templates/service-account.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = build_nats_define_index(&TreeSitterParser);
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
    "guards": [],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "config"
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
        "path": "serviceAccount",
        "type": "with"
      },
      {
        "path": "serviceAccount.enabled",
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
        "path": "serviceAccount",
        "type": "with"
      },
      {
        "path": "serviceAccount.enabled",
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
      "kind": "ServiceAccount"
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
        "path": "serviceAccount",
        "type": "with"
      },
      {
        "path": "serviceAccount.enabled",
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
      "kind": "ServiceAccount"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "serviceAccount",
        "type": "with"
      },
      {
        "path": "serviceAccount.enabled",
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
      "kind": "ServiceAccount"
    },
    "source_expr": "nameOverride"
  },
  {
    "guards": [
      {
        "path": "serviceAccount",
        "type": "with"
      },
      {
        "path": "serviceAccount.enabled",
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
        "path": "serviceAccount",
        "type": "with"
      },
      {
        "path": "serviceAccount.enabled",
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
      "kind": "ServiceAccount"
    },
    "source_expr": "namespaceOverride"
  },
  {
    "guards": [
      {
        "path": "serviceAccount",
        "type": "with"
      },
      {
        "path": "serviceAccount.enabled",
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
      "kind": "ServiceAccount"
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
        "path": "serviceAccount",
        "type": "with"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "serviceAccount"
  },
  {
    "guards": [
      {
        "path": "serviceAccount",
        "type": "with"
      },
      {
        "path": "serviceAccount.enabled",
        "type": "truthy"
      }
    ],
    "kind": "Scalar",
    "path": [],
    "resource": null,
    "source_expr": "serviceAccount.enabled"
  },
  {
    "guards": [
      {
        "path": "serviceAccount",
        "type": "with"
      },
      {
        "path": "serviceAccount.enabled",
        "type": "truthy"
      },
      {
        "path": "serviceAccount.name",
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
      "kind": "ServiceAccount"
    },
    "source_expr": "serviceAccount.name"
  }
]
"#,
    )
    .expect("parse expected");

    similar_asserts::assert_eq!(actual, expected);
}
