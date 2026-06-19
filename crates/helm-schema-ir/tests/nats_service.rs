#![recursion_limit = "1024"]

mod common;

use helm_schema_ast::TreeSitterParser;
use helm_schema_ir::SymbolicIrContext;

const NATS_DEFINE_SOURCES: test_util::DefineSourceSpec<'static> = test_util::DefineSourceSpec {
    helper_templates: &[
        "charts/nats/templates/_helpers.tpl",
        "charts/nats/templates/_jsonpatch.tpl",
        "charts/nats/templates/_tplYaml.tpl",
        "charts/nats/templates/_toPrettyRawJson.tpl",
    ],
    file_sources: &[("files/service.yaml", "charts/nats/files/service.yaml")],
};

#[test]
#[allow(clippy::too_many_lines)]
fn symbolic_ir_from_tree_sitter() {
    let src = test_util::read_testdata("charts/nats/templates/service.yaml");
    let idx = common::build_define_index(&TreeSitterParser, NATS_DEFINE_SOURCES);
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(&src, &idx)
        .project();

    let actual: serde_json::Value =
        serde_json::to_value(helm_schema_ir::ContractDocument::from_projection(ir))
            .expect("serialize");

    if std::env::var("SYMBOLIC_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/nats_service.ir.json"))
            .expect("expected ir json");

    similar_asserts::assert_eq!(actual, expected);
}
