#![recursion_limit = "512"]

mod common;

use common::cases::SIGNOZ_ZOOKEEPER_STATEFULSET as CASE;

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
