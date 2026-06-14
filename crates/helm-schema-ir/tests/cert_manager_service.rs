#![recursion_limit = "1024"]

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::SymbolicIrContext;

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
#[allow(clippy::too_many_lines)]
fn symbolic_ir_from_tree_sitter() {
    let src = test_util::read_testdata("charts/cert-manager/templates/service.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    let idx = build_cert_manager_define_index(&TreeSitterParser);
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

    let _svc = serde_json::json!({"api_version": "v1", "kind": "Service"});
    let _t = |p: &str| serde_json::json!({"type": "truthy", "path": p});
    let _w = |p: &str| serde_json::json!({"type": "with", "path": p});

    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/cert_manager_service.ir.json"))
            .expect("expected ir json");

    similar_asserts::assert_eq!(actual, expected);
}
