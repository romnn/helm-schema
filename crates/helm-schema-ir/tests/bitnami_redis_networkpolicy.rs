mod common;

const CASE: common::IrCorpusCase<'static> = common::IrCorpusCase {
    template_path: "charts/bitnami-redis/templates/networkpolicy.yaml",
    expected_fixture: include_str!("fixtures/bitnami_redis_networkpolicy.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/bitnami-redis/templates/_helpers.tpl"],
        helper_template_dirs: &[("charts/common/templates", "tpl")],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

#[test]
fn symbolic_ir_from_tree_sitter() {
    common::assert_ir_fixture(CASE);
}
