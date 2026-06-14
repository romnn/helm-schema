#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::SymbolicIrContext;

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
#[allow(clippy::too_many_lines)]
fn symbolic_ir_from_tree_sitter() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator/templates/deployment.yaml");
    let idx = build_define_index(&TreeSitterParser);
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(&src, &idx)
        .project();

    let actual: serde_json::Value = serde_json::to_value(&ir).expect("serialize");

    if std::env::var("SYMBOLIC_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let _t = |p: &str| serde_json::json!({"type": "truthy", "path": p});

    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/zalando_postgres_operator_deployment.ir.json"
    ))
    .expect("expected ir json");

    similar_asserts::assert_eq!(actual, expected);
}
