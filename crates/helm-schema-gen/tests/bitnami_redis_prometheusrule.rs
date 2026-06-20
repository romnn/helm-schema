mod common;

use common::cases::BITNAMI_REDIS_PROMETHEUSRULE as CASE;

#[test]
fn schema_keeps_prometheus_rule_namespace_guarded() {
    let schema = common::render_schema_case(&CASE);

    assert!(
        !common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "metrics": {
                    "enabled": true,
                    "prometheusRule": {
                        "enabled": true,
                        "namespace": 7
                    }
                }
            })
        ),
        "metrics.prometheusRule.namespace must stay namespace/string-like when the PrometheusRule renders: {schema}"
    );
    assert!(
        common::schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "metrics": {
                    "enabled": false,
                    "prometheusRule": {
                        "enabled": true,
                        "namespace": 7
                    }
                }
            })
        ),
        "PrometheusRule-only namespace should remain unconstrained when metrics disables the resource: {schema}"
    );
}
