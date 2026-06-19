#![recursion_limit = "1024"]

mod common;

const CASE: common::IrCorpusCase<'static> = common::IrCorpusCase {
    template_path: "charts/nats/templates/service.yaml",
    expected_fixture: include_str!("fixtures/nats_service.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &[
            "charts/nats/templates/_helpers.tpl",
            "charts/nats/templates/_jsonpatch.tpl",
            "charts/nats/templates/_tplYaml.tpl",
            "charts/nats/templates/_toPrettyRawJson.tpl",
        ],
        helper_template_dirs: &[],
        file_sources: &[("files/service.yaml", "charts/nats/files/service.yaml")],
    },
    dump_env: "SYMBOLIC_DUMP",
};

#[test]
fn symbolic_ir_from_tree_sitter() {
    common::assert_ir_fixture(CASE);
}
