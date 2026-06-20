#![recursion_limit = "512"]

mod common;

use common::cases::NATS_SERVICE as CASE;

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata().join("charts/nats");
    let rendered = common::helm_template_render(&chart_dir, Some("templates/service.yaml"));
    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
}

#[test]
fn schema_keeps_live_service_name_paths_typed() {
    let schema = common::render_schema_case(&CASE);

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "service": {
                    "name": 7
                }
            })
        ),
        "service.name must stay string-like when service.enabled defaults to true: {schema}"
    );
    assert!(
        !common::schema_accepts_instance(&schema, &serde_json::json!({ "nameOverride": 7 })),
        "nameOverride must stay string-like when the Service is rendered by default: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "service": {
                    "enabled": false,
                    "name": 7
                },
                "nameOverride": 7
            })
        ),
        "Service-only name inputs should remain unconstrained when the Service is disabled: {schema}"
    );
}
