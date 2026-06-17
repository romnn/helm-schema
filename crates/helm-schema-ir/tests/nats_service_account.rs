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
    let idx = build_nats_define_index(&TreeSitterParser);
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
        serde_json::from_str(include_str!("fixtures/nats_service_account.ir.json"))
            .expect("expected ir json");

    similar_asserts::assert_eq!(actual, expected);
}
