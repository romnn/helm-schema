#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats-operator/templates/_helpers.tpl"),
    );
    idx
}

#[test]
fn symbolic_ir_full() {
    let src = test_util::read_testdata("charts/nats-operator/templates/rbac.yaml");
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

    let crb = serde_json::json!({
        "api_version": "rbac.authorization.k8s.io/v1",
        "kind": "ClusterRoleBinding"
    });
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});

    let expected = serde_json::json!([
        {
            "source_expr": "clusterScoped",
            "path": [],
            "kind": "Scalar",
            "guards": [t("rbacEnabled")],
            "resource": serde_json::json!({
                "api_version": "rbac.authorization.k8s.io/v1",
                "kind": "ClusterRole"
            })
        },
        {
            "source_expr": "clusterScoped",
            "path": [],
            "kind": "Scalar",
            "guards": [t("rbacEnabled")],
            "resource": crb
        },
        {
            "source_expr": "rbacEnabled",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}
