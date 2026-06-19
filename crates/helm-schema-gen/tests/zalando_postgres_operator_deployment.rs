#![recursion_limit = "512"]

mod common;

const CASE: common::SchemaCorpusCase<'static> = common::SchemaCorpusCase {
    template_path: "charts/zalando-postgres-operator/templates/deployment.yaml",
    values_path: "charts/zalando-postgres-operator/values.yaml",
    expected_fixture: include_str!("fixtures/zalando_postgres_operator_deployment.schema.json"),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/zalando-postgres-operator/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: common::ProviderKind::K8s("v1.35.0"),
    dump_stem: "zalando-postgres-operator.deployment",
};

#[test]
fn schema_from_tree_sitter() {
    common::assert_schema_fixture(&CASE);
}

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/zalando-postgres-operator");
    let rendered = common::helm_template_render(&chart_dir, Some("templates/deployment.yaml"));
    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
}

#[test]
fn schema_validates_values_yaml() {
    common::assert_values_yaml_validates(&CASE);
}
