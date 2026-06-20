#![recursion_limit = "512"]

mod common;

use common::cases::ZALANDO_POSTGRES_OPERATOR_POSTGRES_POD_PRIORITY_CLASS as CASE;

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
