#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/zalando-postgres-operator/templates/_helpers.tpl"),
    )
    .expect("helpers");
    idx
}

#[test]
fn resource_detection() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator/templates/clusterrole.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: "rbac.authorization.k8s.io/v1".to_string(),
            kind: "ClusterRole".to_string(),
        })
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_full() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator/templates/clusterrole.yaml");
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

    let cluster_role =
        serde_json::json!({"api_version": "rbac.authorization.k8s.io/v1", "kind": "ClusterRole"});
    let t = |p: &str| serde_json::json!({"type": "truthy", "path": p});

    let expected = serde_json::json!([
        {
            "guards": [t("rbac.create")],
            "kind": "Scalar",
            "path": [],
            "resource": cluster_role,
            "source_expr": "configGeneral.enable_crd_registration"
        },
        {
            "guards": [t("rbac.create")],
            "kind": "Scalar",
            "path": [],
            "resource": cluster_role,
            "source_expr": "configGeneral.kubernetes_use_configmaps"
        },
        {
            "guards": [t("rbac.create")],
            "kind": "Scalar",
            "path": [],
            "resource": cluster_role,
            "source_expr": "configKubernetes.spilo_privileged"
        },
        {
            "guards": [t("rbac.create")],
            "kind": "Scalar",
            "path": [],
            "resource": cluster_role,
            "source_expr": "configKubernetes.storage_resize_mode"
        },
        {
            "guards": [t("rbac.create")],
            "kind": "Scalar",
            "path": [],
            "resource": cluster_role,
            "source_expr": "enableStreams"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": null,
            "source_expr": "nameOverride"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": null,
            "source_expr": "rbac.create"
        },
        {
            "guards": [],
            "kind": "Scalar",
            "path": [],
            "resource": null,
            "source_expr": "serviceAccount.name"
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}
