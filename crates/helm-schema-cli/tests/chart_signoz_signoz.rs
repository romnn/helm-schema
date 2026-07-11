//! Semantic assertions for the signoz umbrella chart: description placement,
//! `helm template` sample validation, and guard accept/reject behavior.
//! Values validation and the full-schema pin live in `chart_corpus.rs`;
//! these assertions state WHY the schema must look the way the fixture says,
//! so a fixture regeneration cannot silently pin a regression.

use test_util::prelude::sim_assert_eq;
#[path = "common/descriptions.rs"]
mod descriptions;
#[path = "common/helm_samples.rs"]
mod helm_samples;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

use indoc::indoc;
use serde_json::{Map, Value};

#[test]
fn signoz_signoz_schema_semantics_hold() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("signoz-signoz")?;
    assert_schema_description(
        &schema,
        "/properties/alertmanager/properties/ingress/properties/enabled/description",
        "Enable ingress for Alertmanager",
    );
    assert_schema_description(
        &schema,
        "/properties/alertmanager/properties/ingress/properties/className/description",
        "Ingress Class Name to be used to identify ingress controllers",
    );
    assert_schema_description(
        &schema,
        "/properties/alertmanager/properties/ingress/properties/annotations/description",
        "Annotations to Alertmanager Ingress\nkubernetes.io/ingress.class: nginx\nkubernetes.io/tls-acme: \"true\"\ncert-manager.io/cluster-issuer: letsencrypt-prod",
    );
    assert_schema_description(
        &schema,
        "/properties/alertmanager/properties/ingress/properties/hosts/description",
        "Alertmanager Ingress Host names with their path details",
    );
    descriptions::assert_chart_values_comments_apply_to_existing_schema_paths(
        "signoz-signoz",
        &schema,
        50,
    )?;
    helm_samples::assert_generated_schema_accepts_helm_samples_for_path(
        "signoz-signoz",
        &schema,
        &[
            helm_samples::HelmValidationSample {
                name: "default",
                values_yaml: None,
            },
            helm_samples::HelmValidationSample {
                name: "enable-otel-gateway",
                values_yaml: Some(indoc! {"
                    signoz-otel-gateway:
                      enabled: true
                "}),
            },
            helm_samples::HelmValidationSample {
                name: "otel-gateway-empty-service-account-name",
                values_yaml: Some(indoc! {"
                    signoz-otel-gateway:
                      enabled: true
                      serviceAccount:
                        create: true
                        name: \"\"
                "}),
            },
        ],
    )?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");
    let schema_validates_instance =
        |_: &serde_json::Value, instance: &serde_json::Value| validator.is_valid(instance);
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "signoz": {
                    "serviceAccount": {
                        "create": false,
                        "name": 7
                    }
                }
            })
        ),
        "signoz.serviceAccount.name must not collapse to an unconstrained schema on the false branch: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "signoz": {
                    "serviceAccount": {
                        "name": 7
                    }
                }
            })
        ),
        "signoz.serviceAccount.name must stay string-like when create defaults to true: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "signoz": {
                    "serviceAccount": {
                        "create": true,
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "signoz.serviceAccount.annotations must stay a string map when create is true: {schema}"
    );
    assert!(
        schema_validates_instance(
            &schema,
            &serde_json::json!({
                "signoz": {
                    "serviceAccount": {
                        "create": false,
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "disabled signoz serviceAccount annotations should not be constrained by guarded-only metadata evidence: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": true,
                    "name": "alertmanager",
                    "serviceAccount": {
                        "create": false,
                        "name": 7
                    }
                }
            })
        ),
        "alertmanager.serviceAccount.name must stay string-like on the false branch too: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": true,
                    "name": "alertmanager",
                    "serviceAccount": {
                        "name": 7
                    }
                }
            })
        ),
        "alertmanager.serviceAccount.name must stay string-like when enabled and create defaults to true: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": true,
                    "name": "alertmanager",
                    "serviceAccount": {
                        "create": true,
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "alertmanager.serviceAccount.annotations must stay a string map when the ServiceAccount renders: {schema}"
    );
    assert!(
        schema_validates_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": false,
                    "serviceAccount": {
                        "create": true,
                        "name": 7
                    }
                }
            })
        ),
        "disabled alertmanager values should not be constrained by guarded-only serviceAccount.name evidence: {schema}"
    );
    assert!(
        schema_validates_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": false,
                    "serviceAccount": {
                        "create": true,
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "disabled alertmanager values should not be constrained by guarded-only serviceAccount annotation evidence: {schema}"
    );
    assert!(
        schema_validates_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": true,
                    "serviceAccount": {
                        "create": false,
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "disabled alertmanager ServiceAccount annotations should not be constrained by guarded-only metadata evidence: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": true,
                    "ingress": {
                        "enabled": true,
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "alertmanager.ingress.annotations must stay a string map when ingress is enabled: {schema}"
    );
    assert!(
        schema_validates_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": false,
                    "ingress": {
                        "enabled": true,
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "disabled alertmanager values should not be constrained by guarded-only ingress annotation evidence: {schema}"
    );
    assert!(
        schema_validates_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": true,
                    "ingress": {
                        "enabled": false,
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "disabled alertmanager ingress annotations should not be constrained by guarded-only metadata evidence: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "otelCollector": {
                    "ingress": {
                        "enabled": true,
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "otelCollector.ingress.annotations must stay a string map when ingress is enabled: {schema}"
    );
    assert!(
        schema_validates_instance(
            &schema,
            &serde_json::json!({
                "otelCollector": {
                    "ingress": {
                        "enabled": false,
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "disabled otelCollector ingress annotations should not be constrained by guarded-only metadata evidence: {schema}"
    );
    for field in ["pullPolicy", "repository", "tag"] {
        assert!(
            clickhouse_operator_image_field_has_conditional_string_schema(&schema, field),
            "clickhouseOperator.image.{field} should carry string evidence in a clickhouse-enabled branch: {schema}"
        );
        assert!(
            !schema_validates_instance(
                &schema,
                &clickhouse_operator_image_field_instance(field, serde_json::json!(7), None),
            ),
            "clickhouseOperator.image.{field} must stay string-like while clickhouse renders: {schema}"
        );
        assert!(
            schema_validates_instance(
                &schema,
                &clickhouse_operator_image_field_instance(field, serde_json::json!(7), Some(false)),
            ),
            "disabled clickhouse subchart values should not be constrained by guarded-only clickhouseOperator.image.{field} evidence: {schema}"
        );
    }
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "schemaMigrator": {
                    "serviceAccount": {
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "schemaMigrator.serviceAccount.annotations must stay a string map when create defaults to true: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "signoz": {
                    "smtpVars": {
                        "enabled": true,
                        "existingSecret": {
                            "fromKey": "smtp-from",
                            "name": 7
                        }
                    }
                }
            })
        ),
        "signoz.smtpVars.existingSecret.name must stay string-like when SMTP vars render: {schema}"
    );
    assert!(
        schema_validates_instance(
            &schema,
            &serde_json::json!({
                "signoz": {
                    "smtpVars": {
                        "enabled": false,
                        "existingSecret": {
                            "fromKey": "smtp-from",
                            "name": 7
                        }
                    }
                }
            })
        ),
        "disabled SMTP vars should not be constrained by guarded-only secret name evidence: {schema}"
    );
    assert!(
        schema_validates_instance(
            &schema,
            &serde_json::json!({
                "signoz-otel-gateway": {
                    "enabled": true,
                    "serviceAccount": {
                        "create": true,
                        "name": null
                    }
                }
            })
        ),
        "signoz-otel-gateway.serviceAccount.name uses Helm default and must accept null when create is true: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "signoz-otel-gateway": {
                    "enabled": true,
                    "postgresql": {
                        "enabled": true,
                        "architecture": "replication",
                        "auth": {
                            "replicationUsername": 7
                        }
                    }
                }
            })
        ),
        "postgresql.auth.replicationUsername must stay string-like when replication renders: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "signoz-otel-gateway": {
                    "enabled": true,
                    "serviceAccount": {
                        "create": true,
                        "name": 7
                    }
                }
            })
        ),
        "signoz-otel-gateway.serviceAccount.name must stay string-like when create is true: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "signoz-otel-gateway": {
                    "enabled": true,
                    "serviceAccount": {
                        "create": false,
                        "name": 7
                    }
                }
            })
        ),
        "signoz-otel-gateway.serviceAccount.name must stay string-like when create is false because the Deployment still references it: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "signoz-otel-gateway": {
                    "enabled": true,
                    "serviceAccount": {
                        "create": true,
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "signoz-otel-gateway.serviceAccount.annotations must stay a string map when the ServiceAccount renders: {schema}"
    );
    assert!(
        schema_validates_instance(
            &schema,
            &serde_json::json!({
                "signoz-otel-gateway": {
                    "enabled": true,
                    "serviceAccount": {
                        "create": false,
                        "annotations": {
                            "example.com/bad": 7
                        }
                    }
                }
            })
        ),
        "disabled signoz-otel-gateway ServiceAccount annotations should not be constrained by guarded-only metadata evidence: {schema}"
    );
    Ok(())
}

fn clickhouse_operator_image_field_instance(
    field: &str,
    value: Value,
    enabled: Option<bool>,
) -> Value {
    let mut image = Map::new();
    image.insert(field.to_string(), value);

    let mut clickhouse_operator = Map::new();
    clickhouse_operator.insert("image".to_string(), Value::Object(image));

    let mut clickhouse = Map::new();
    if let Some(enabled) = enabled {
        clickhouse.insert("enabled".to_string(), Value::Bool(enabled));
    }
    clickhouse.insert(
        "clickhouseOperator".to_string(),
        Value::Object(clickhouse_operator),
    );

    let mut root = Map::new();
    root.insert("clickhouse".to_string(), Value::Object(clickhouse));
    Value::Object(root)
}

fn clickhouse_operator_image_field_has_conditional_string_schema(
    schema: &Value,
    field: &str,
) -> bool {
    schema
        .pointer("/properties/clickhouse/allOf")
        .and_then(Value::as_array)
        .is_some_and(|branches| {
            branches.iter().any(|branch| {
                branch
                    .pointer(&format!(
                        "/then/properties/clickhouseOperator/properties/image/properties/{field}"
                    ))
                    .is_some_and(schema_accepts_string_type)
            })
        })
}

fn schema_accepts_string_type(schema: &Value) -> bool {
    (match schema.get("type") {
        Some(Value::String(value)) => value == "string",
        Some(Value::Array(values)) => values.iter().any(|value| value.as_str() == Some("string")),
        _ => false,
    }) || ["anyOf", "oneOf", "allOf"]
        .into_iter()
        .filter_map(|key| schema.get(key).and_then(Value::as_array))
        .flatten()
        .any(schema_accepts_string_type)
}

fn assert_schema_description(schema: &serde_json::Value, pointer: &str, expected: &str) {
    sim_assert_eq!(
        have: schema.pointer(pointer).and_then(serde_json::Value::as_str),
        want: Some(expected),
        "schema description mismatch at {pointer}"
    );
}
