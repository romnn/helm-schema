#![recursion_limit = "512"]

mod common;

use common::cases::SIGNOZ_ZOOKEEPER_STATEFULSET as CASE;

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
