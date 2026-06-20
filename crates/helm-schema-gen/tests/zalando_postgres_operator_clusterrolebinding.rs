#![recursion_limit = "512"]

mod common;

use common::cases::ZALANDO_POSTGRES_OPERATOR_CLUSTERROLEBINDING as CASE;

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
        "serviceAccount.name should remain unconstrained when ClusterRoleBinding rendering is disabled: {schema}"
    );
}
