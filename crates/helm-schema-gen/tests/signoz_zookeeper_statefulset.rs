#![recursion_limit = "512"]

mod common;

const CASE: common::SchemaCorpusCase<'static> = common::SchemaCorpusCase {
    template_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/templates/statefulset.yaml",
    values_path: "charts/signoz-signoz/charts/clickhouse/charts/zookeeper/values.yaml",
    expected_fixture: include_str!("fixtures/signoz_zookeeper_statefulset.schema.json"),
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
    provider: common::ProviderKind::K8s("v1.35.0"),
    dump_stem: "signoz-zookeeper-statefulset",
};

#[test]
fn schema_from_tree_sitter() {
    common::assert_schema_fixture(&CASE);
}

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata()
        .join("charts/signoz-signoz/charts/clickhouse/charts/zookeeper");
    let rendered = common::helm_template_render(&chart_dir, Some("templates/statefulset.yaml"));
    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
}

#[test]
fn schema_validates_values_yaml() {
    common::assert_values_yaml_validates(&CASE);
}

#[test]
fn schema_keeps_default_enabled_container_security_context_typed() {
    let schema = common::render_schema_case(&CASE);

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "containerSecurityContext": {
                    "runAsUser": "root"
                }
            })
        ),
        "containerSecurityContext.runAsUser must stay integer-like because containerSecurityContext.enabled defaults to true: {schema}"
    );
}
