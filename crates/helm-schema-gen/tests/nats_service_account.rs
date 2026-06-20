#![recursion_limit = "512"]

mod common;

use common::cases::NATS_SERVICE_ACCOUNT as CASE;

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/nats");
    let rendered = common::helm_template_render_with_args(
        &chart_dir,
        Some("templates/service-account.yaml"),
        &["--set", "serviceAccount.enabled=true"],
    );

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
                    "enabled": true,
                    "name": 7
                }
            })
        ),
        "serviceAccount.name must stay string-like when ServiceAccount rendering is enabled: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "serviceAccount": {
                    "enabled": false,
                    "name": 7
                }
            })
        ),
        "serviceAccount.name should remain unconstrained when the ServiceAccount is disabled: {schema}"
    );
}
