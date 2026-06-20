#![recursion_limit = "512"]

mod common;

use common::cases::CERT_MANAGER_SERVICE as CASE;

#[test]
fn schema_keeps_default_rendered_service_metadata_typed() {
    let schema = common::render_schema_case(&CASE);

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "serviceAnnotations": {
                    "example.com/bad": 7
                }
            })
        ),
        "serviceAnnotations must stay a string map because prometheus.enabled defaults to true and podmonitor.enabled defaults to false: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "prometheus": {
                    "enabled": false
                },
                "serviceAnnotations": {
                    "example.com/bad": 7
                }
            })
        ),
        "serviceAnnotations should be unconstrained when the Service template is disabled: {schema}"
    );
}
