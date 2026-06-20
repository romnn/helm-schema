#![recursion_limit = "512"]

mod common;

use common::cases::ZALANDO_POSTGRES_OPERATOR_CLUSTERROLE as CASE;

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/zalando-postgres-operator");
    let rendered = common::helm_template_render(&chart_dir, Some("templates/clusterrole.yaml"));
    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
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
        "serviceAccount.name should remain unconstrained when ClusterRole rendering is disabled: {schema}"
    );
}
