#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::SymbolicIrContext;

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats-operator/templates/_helpers.tpl"),
    );
    idx
}

#[test]
fn symbolic_ir_from_tree_sitter() {
    let src = test_util::read_testdata("charts/nats-operator/templates/rbac.yaml");
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

    let crb = serde_json::json!({
        "api_version": "rbac.authorization.k8s.io/v1",
        "kind": "ClusterRoleBinding"
    });
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});

    let expected_uses = serde_json::json!([
        {
            "source_expr": "clusterScoped",
            "path": [],
            "kind": "Scalar",
            "guards": [t("rbacEnabled"), t("clusterScoped")],
            "resource": serde_json::json!({
                "api_version": "rbac.authorization.k8s.io/v1",
                "kind": "ClusterRole"
            })
        },
        {
            "source_expr": "clusterScoped",
            "path": [],
            "kind": "Scalar",
            "guards": [t("rbacEnabled"), t("clusterScoped")],
            "resource": crb
        },
        {
            "source_expr": "rbacEnabled",
            "path": [],
            "kind": "Scalar",
            "guards": [t("rbacEnabled")],
            "resource": null
        }
    ]);
    let expected = serde_json::json!({
        "version": 1,
        "uses": expected_uses
    });

    similar_asserts::assert_eq!(actual, expected);
}
