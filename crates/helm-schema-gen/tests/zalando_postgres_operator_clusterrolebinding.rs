#![recursion_limit = "512"]

mod common;

const CASE: common::SchemaCorpusCase<'static> = common::SchemaCorpusCase {
    template_path: "charts/zalando-postgres-operator/templates/clusterrolebinding.yaml",
    values_path: "charts/zalando-postgres-operator/values.yaml",
    expected_fixture: include_str!(
        "fixtures/zalando_postgres_operator_clusterrolebinding.schema.json"
    ),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/zalando-postgres-operator/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: common::ProviderKind::K8s("v1.35.0"),
    dump_stem: "zalando-postgres-operator.clusterrolebinding",
};

#[test]
fn schema_from_tree_sitter() {
    common::assert_schema_fixture(&CASE);
}

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/zalando-postgres-operator");
    let rendered =
        common::helm_template_render(&chart_dir, Some("templates/clusterrolebinding.yaml"));
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
fn schema_keeps_live_service_account_name_typed() {
    let schema = common::render_schema_case(&CASE);

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "serviceAccount": {
                    "name": 7
                }
            })
        ),
        "serviceAccount.name must stay string-like when rbac.create defaults to true: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "rbac": {
                    "create": false
                },
                "serviceAccount": {
                    "name": 7
                }
            })
        ),
        "serviceAccount.name should remain unconstrained when ClusterRoleBinding rendering is disabled: {schema}"
    );
}
