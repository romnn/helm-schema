#![recursion_limit = "512"]

mod common;

use common::cases::SIGNOZ_ZOOKEEPER_SVC as CASE;

#[test]
fn helm_template_renders_successfully() {
    let chart_dir = test_util::workspace_testdata()
        .join("charts/signoz-signoz/charts/clickhouse/charts/zookeeper");
    let rendered = common::helm_template_render(&chart_dir, Some("templates/svc.yaml"));
    match &rendered {
        Ok(yaml) => assert!(!yaml.is_empty(), "rendered YAML is empty"),
        Err(e) => panic!("helm template failed: {e}"),
    }
}

#[test]
fn schema_keeps_service_port_fields_guarded() {
    let schema = common::render_schema_case(&CASE);

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "service": {
                    "ports": {
                        "client": "client-port"
                    }
                }
            })
        ),
        "service.ports.client must stay integer-like because disableBaseClientPort defaults to false: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "service": {
                    "disableBaseClientPort": true,
                    "ports": {
                        "client": "client-port"
                    }
                }
            })
        ),
        "service.ports.client should be unconstrained when disableBaseClientPort removes that Service port: {schema}"
    );
    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "tls": {
                    "client": {
                        "enabled": true
                    }
                },
                "service": {
                    "ports": {
                        "tls": "tls-port"
                    }
                }
            })
        ),
        "service.ports.tls must stay integer-like when tls.client.enabled renders the TLS port: {schema}"
    );
}
