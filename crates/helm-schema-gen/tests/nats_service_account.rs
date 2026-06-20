#![recursion_limit = "512"]

mod common;

use common::cases::NATS_SERVICE_ACCOUNT as CASE;

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
