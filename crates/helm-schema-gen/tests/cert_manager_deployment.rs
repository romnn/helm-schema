#![recursion_limit = "4096"]

mod common;

use common::cases::CERT_MANAGER_DEPLOYMENT as CASE;

#[test]
fn schema_keeps_default_enabled_liveness_probe_fields_typed() {
    let schema = common::render_schema_case(&CASE);

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "livenessProbe": {
                    "failureThreshold": "eight"
                }
            })
        ),
        "livenessProbe.failureThreshold must stay integer-like because livenessProbe.enabled defaults to true: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "livenessProbe": {
                    "enabled": false,
                    "failureThreshold": "eight"
                }
            })
        ),
        "disabled livenessProbe fields should remain unconstrained because the template skips them: {schema}"
    );
}
