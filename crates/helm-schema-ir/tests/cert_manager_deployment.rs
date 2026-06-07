#![recursion_limit = "1024"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    DefaultResourceDetector, IrGenerator, ResourceDetector, ResourceRef, SymbolicIrGenerator,
};

fn build_cert_manager_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    idx.add_source(
        parser,
        &test_util::read_testdata("charts/cert-manager/templates/_helpers.tpl"),
    )
    .expect("cert-manager helpers");
    idx
}

#[test]
fn resource_detection() {
    let src = test_util::read_testdata("charts/cert-manager/templates/deployment.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let resource = DefaultResourceDetector.detect(&ast);
    assert_eq!(
        resource,
        Some(ResourceRef {
            api_version: "apps/v1".to_string(),
            kind: "Deployment".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        })
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_full() {
    let src = test_util::read_testdata("charts/cert-manager/templates/deployment.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = build_cert_manager_define_index(&TreeSitterParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);

    let actual: serde_json::Value = serde_json::to_value(&ir).expect("serialize");

    if std::env::var("SYMBOLIC_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let _dep = serde_json::json!({"api_version": "apps/v1", "kind": "Deployment"});
    let _t = |p: &str| serde_json::json!({"type": "truthy", "path": p});
    let _w = |p: &str| serde_json::json!({"type": "with", "path": p});
    let _n = |p: &str| serde_json::json!({"type": "not", "path": p});
    let _o = |a: &str, b: &str| serde_json::json!({"type": "or", "paths": [a, b]});

    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/cert_manager_deployment.ir.json"))
            .expect("expected ir json");

    similar_asserts::assert_eq!(have: actual, want: expected);
}
