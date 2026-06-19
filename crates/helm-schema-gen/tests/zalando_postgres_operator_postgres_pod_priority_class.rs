#![recursion_limit = "512"]

mod common;

const CASE: common::SchemaCorpusCase<'static> = common::SchemaCorpusCase {
    template_path: "charts/zalando-postgres-operator/templates/postgres-pod-priority-class.yaml",
    values_path: "charts/zalando-postgres-operator/values.yaml",
    expected_fixture: include_str!(
        "fixtures/zalando_postgres_operator_postgres_pod_priority_class.schema.json"
    ),
    define_sources: test_util::DefineSourceSpec {
        helper_templates: &["charts/zalando-postgres-operator/templates/_helpers.tpl"],
        helper_template_dirs: &[],
        file_sources: &[],
    },
    provider: common::ProviderKind::K8s("v1.35.0"),
    dump_stem: "zalando-postgres-operator.postgres-pod-priority-class",
};

#[test]
fn schema_from_tree_sitter() {
    common::assert_schema_fixture(&CASE);
}

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/zalando-postgres-operator");
    let rendered = common::helm_template_render(
        &chart_dir,
        Some("templates/postgres-pod-priority-class.yaml"),
    );
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
fn schema_keeps_live_priority_class_fields_typed() {
    let schema = common::render_schema_case(&CASE);

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "podPriorityClassName": {
                    "name": 7
                }
            })
        ),
        "podPriorityClassName.name must stay string-like when create defaults to true: {schema}"
    );
    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "podPriorityClassName": {
                    "priority": "high"
                }
            })
        ),
        "podPriorityClassName.priority must stay integer-like when create defaults to true: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "podPriorityClassName": {
                    "create": false,
                    "name": 7,
                    "priority": "high"
                }
            })
        ),
        "PriorityClass fields should remain unconstrained when PriorityClass rendering is disabled: {schema}"
    );
}
