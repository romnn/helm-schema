mod common;

use color_eyre::eyre::{OptionExt as _, WrapErr as _};
use indoc::indoc;

#[test]
fn signoz_signoz_values_yaml_and_fragments_match() -> color_eyre::eyre::Result<()> {
    let schema = common::generate_chart_schema("signoz-signoz")?;
    if std::env::var("SCHEMA_DUMP").is_ok() {
        let path = std::env::temp_dir().join("helm-schema.cli.chart-signoz-signoz.schema.json");
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&schema).wrap_err("serialize signoz schema dump")?,
        )
        .wrap_err("write signoz schema dump")?;
    }
    let values_json = common::values_yaml_as_json("signoz-signoz")?;
    common::assert_values_json_validates(&values_json, &schema);
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
    common::assert_chart_values_comments_apply_to_existing_schema_paths(
        "signoz-signoz",
        &schema,
        50,
    )?;
    common::assert_generated_schema_accepts_helm_samples(
        "signoz-signoz",
        &schema,
        &[
            common::HelmValidationSample {
                name: "default",
                values_yaml: None,
            },
            common::HelmValidationSample {
                name: "enable-otel-gateway",
                values_yaml: Some(indoc! {"
                    signoz-otel-gateway:
                      enabled: true
                "}),
            },
            common::HelmValidationSample {
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
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/chart_signoz_signoz.fragments.json"))
            .wrap_err("parse signoz fixture")?;
    assert_no_empty_name_fragments(&fixture);

    let mut actual_keys: Vec<String> = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .ok_or_eyre("schema.properties must be an object")?
        .keys()
        .cloned()
        .collect();
    actual_keys.sort();

    let mut expected_keys: Vec<String> = serde_json::from_value(
        fixture
            .get("top_level_keys")
            .ok_or_eyre("fixture missing top_level_keys")?
            .clone(),
    )
    .wrap_err("parse fixture top_level_keys")?;
    expected_keys.sort();

    similar_asserts::assert_eq!(actual_keys, expected_keys);

    let pointers = fixture
        .get("pointers")
        .and_then(serde_json::Value::as_object)
        .ok_or_eyre("fixture missing pointers object")?;

    for (pointer, expected) in pointers {
        let mut actual = schema
            .pointer(pointer)
            .ok_or_eyre(format!("schema missing pointer {pointer}"))?
            .clone();
        strip_description_annotations(&mut actual);
        similar_asserts::assert_eq!(&actual, expected, "schema mismatch at {pointer}");
    }

    Ok(())
}

fn assert_no_empty_name_fragments(value: &serde_json::Value) {
    if let Some(pointer) = find_empty_name_fragment(value, "") {
        panic!("curated Signoz fixture must not encode an unconstrained name at {pointer}");
    }
}

fn find_empty_name_fragment(value: &serde_json::Value, pointer: &str) -> Option<String> {
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                let child_pointer =
                    format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                if key == "name" && child.as_object().is_some_and(serde_json::Map::is_empty) {
                    return Some(child_pointer);
                }
                if let Some(found) = find_empty_name_fragment(child, &child_pointer) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(items) => items.iter().enumerate().find_map(|(index, child)| {
            find_empty_name_fragment(child, &format!("{pointer}/{index}"))
        }),
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => None,
    }
}

fn schema_validates_instance(schema: &serde_json::Value, instance: &serde_json::Value) -> bool {
    jsonschema::validator_for(schema)
        .expect("schema validator")
        .is_valid(instance)
}

fn assert_schema_description(schema: &serde_json::Value, pointer: &str, expected: &str) {
    assert_eq!(
        schema.pointer(pointer).and_then(serde_json::Value::as_str),
        Some(expected),
        "schema description mismatch at {pointer}"
    );
}

fn strip_description_annotations(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            object.remove("description");
            for child in object.values_mut() {
                strip_description_annotations(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                strip_description_annotations(item);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}
