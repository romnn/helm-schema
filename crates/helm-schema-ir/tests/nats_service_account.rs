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

    idx.add_file_source(
        "files/service-account.yaml",
        &test_util::read_testdata("charts/nats/files/service-account.yaml"),
    );

    idx
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_full() {
    let src = test_util::read_testdata("charts/nats/templates/service-account.yaml");
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

    let sa = serde_json::json!({"api_version": "v1", "kind": "ServiceAccount"});
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});

    let expected = serde_json::json!([
        {
            "source_expr": "global.labels",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "nameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "namespaceOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "serviceAccount",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "serviceAccount",
            "path": [],
            "kind": "Scalar",
            "guards": [t("serviceAccount"), t("serviceAccount.enabled")],
            "resource": null
        },
        {
            "source_expr": "serviceAccount.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("serviceAccount")],
            "resource": null
        },
        {
            "source_expr": "serviceAccount.merge",
            "path": [],
            "kind": "Scalar",
            "guards": [t("serviceAccount"), t("serviceAccount.enabled")],
            "resource": null
        },
        {
            "source_expr": "serviceAccount.name",
            "path": ["metadata", "name"],
            "kind": "Scalar",
            "guards": [t("serviceAccount"), t("serviceAccount.enabled")],
            "resource": sa
        },
        {
            "source_expr": "serviceAccount.patch",
            "path": [],
            "kind": "Scalar",
            "guards": [t("serviceAccount"), t("serviceAccount.enabled")],
            "resource": null
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}
