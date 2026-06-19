#![recursion_limit = "512"]

mod common;

const CASE: common::IrCorpusCase<'static> = common::IrCorpusCase {
    template_path: "charts/surveyor/templates/configmap.yaml",
    expected_fixture: include_str!("fixtures/surveyor_configmap.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/surveyor/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

#[test]
fn symbolic_ir_from_tree_sitter() {
    common::assert_ir_fixture(CASE);
}
