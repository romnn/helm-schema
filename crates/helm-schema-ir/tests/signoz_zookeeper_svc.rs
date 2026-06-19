#![recursion_limit = "1024"]

mod common;

const CASE: common::IrCorpusCase<'static> = common::IrCorpusCase {
    template_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/svc.yaml",
    expected_fixture: include_str!("fixtures/signoz_zookeeper_svc.ir.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &[
            "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/_helpers.tpl",
        ],
        helper_template_dirs: &[(
            "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/charts/common/templates",
            "tpl",
        )],
        file_sources: &[],
    },
    dump_env: "SYMBOLIC_DUMP",
};

#[test]
fn symbolic_ir_from_tree_sitter() {
    common::assert_ir_fixture(CASE);
}
