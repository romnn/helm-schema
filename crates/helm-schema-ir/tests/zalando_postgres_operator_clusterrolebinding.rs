#![recursion_limit = "512"]

mod common;

const CASE: common::IrCorpusCase<'static> = common::IrCorpusCase {
    template_path: "charts/zalando-postgres-operator/templates/clusterrolebinding.yaml",
    expected_fixture: include_str!("fixtures/zalando_postgres_operator_clusterrolebinding.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/zalando-postgres-operator/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

#[test]
fn symbolic_ir_from_tree_sitter() {
    common::assert_ir_fixture(CASE);
}
