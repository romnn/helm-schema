#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/nats-operator/templates/_helpers.tpl"),
    );
    idx
}

#[test]
fn resource_detection() {
    let src = test_util::read_testdata("charts/nats-operator/templates/rbac.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: "rbac.authorization.k8s.io/v1".to_string(),
            kind: "ClusterRole".to_string(),
            api_version_candidates: Vec::new(),
        })
    );
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
            "resource": null
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
