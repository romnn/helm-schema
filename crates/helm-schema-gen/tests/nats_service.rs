#![recursion_limit = "512"]

mod common;

use common::cases::NATS_SERVICE as CASE;

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
