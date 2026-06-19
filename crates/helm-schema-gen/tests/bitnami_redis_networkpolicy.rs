#![recursion_limit = "512"]

mod common;

const CASE: common::SchemaCorpusCase<'static> = common::SchemaCorpusCase {
    template_path: "charts/bitnami-redis/templates/networkpolicy.yaml",
    values_path: "charts/bitnami-redis/values.yaml",
    expected_fixture: include_str!("fixtures/bitnami_redis_networkpolicy.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/bitnami-redis/templates/_helpers.tpl"],
        helper_template_dirs: &[("charts/common/templates", "tpl")],
        file_sources: &[],
    },
    provider: common::ProviderKind::K8s("v1.35.0"),
    dump_stem: "bitnami-redis.networkpolicy",
};

#[test]
fn schema_from_tree_sitter() {
    common::assert_schema_fixture(&CASE);
}

#[test]
fn schema_validates_values_yaml() {
    common::assert_values_yaml_validates(&CASE);
}
