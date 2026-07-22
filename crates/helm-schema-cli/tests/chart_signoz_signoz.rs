//! Semantic assertions for the signoz umbrella chart: description placement,
//! `helm template` sample validation, and guard accept/reject behavior.
//! Values validation and the full-schema pin live in `chart_corpus.rs`;
//! these assertions state WHY the schema must look the way the fixture says,
//! so a fixture regeneration cannot silently pin a regression.

use color_eyre::eyre;

use std::collections::BTreeSet;

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/descriptions.rs"]
mod descriptions;
#[path = "common/helm_samples.rs"]
mod helm_samples;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;
#[path = "common/values_yaml.rs"]
mod values_yaml;

use indoc::indoc;
use serde_json::{Map, Value};

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the chart-wide semantic assertions are clearest in one generated-schema regression"
)]
fn signoz_signoz_schema_semantics_hold() -> eyre::Result<()> {
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
    // Helm validates the coalesced document, so every probe composes its
    // sparse override over the chart defaults first.
    let schema_validates_instance = |_: &serde_json::Value, instance: &serde_json::Value| {
        let composed = chart_instances::with_override("signoz-signoz", instance.clone())
            .expect("compose instance over chart defaults");
        validator.is_valid(&composed)
    };
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
    // The zookeeper subchart's `common.images.pullSecrets` ranges
    // `.global.imagePullSecrets` with no truthiness guard, so EVERY scalar
    // spelling — falsy included — aborts `helm template` while the chart's
    // default clickhouse→zookeeper chain is active (`range can't iterate
    // over ""`). Collections and null-deletion render.
    for (value, label) in [
        (serde_json::json!("oops"), "a truthy scalar"),
        (serde_json::json!(""), "the empty string"),
        (serde_json::json!(false), "a raw false"),
    ] {
        assert!(
            !schema_validates_instance(
                &schema,
                &serde_json::json!({ "global": { "imagePullSecrets": value } })
            ),
            "global.imagePullSecrets: {label} cannot be ranged by the zookeeper pull-secrets helper: {schema}"
        );
    }
    for (value, label) in [
        (serde_json::json!(["regcred"]), "an array"),
        (serde_json::json!({ "a": "b" }), "a map"),
        (serde_json::json!(null), "a null deletion"),
    ] {
        assert!(
            schema_validates_instance(
                &schema,
                &serde_json::json!({ "global": { "imagePullSecrets": value } })
            ),
            "global.imagePullSecrets: {label} renders: {schema}"
        );
    }
    // With clickhouse disabled the zookeeper chain is dormant: only the
    // parent's truthiness-guarded `signoz.imagePullSecrets` range remains
    // live, so falsy scalars render while truthy scalars still abort —
    // the nested activation chain must scope the zookeeper-side claim.
    assert!(
        schema_validates_instance(
            &schema,
            &serde_json::json!({
                "clickhouse": { "enabled": false },
                "externalClickhouse": { "host": "ch.example.com", "cluster": "cluster" },
                "global": { "imagePullSecrets": "" }
            })
        ),
        "a falsy scalar renders once the clickhouse chain is dormant: {schema}"
    );
    assert!(
        !schema_validates_instance(
            &schema,
            &serde_json::json!({
                "clickhouse": { "enabled": false },
                "externalClickhouse": { "host": "ch.example.com", "cluster": "cluster" },
                "global": { "imagePullSecrets": "oops" }
            })
        ),
        "a truthy scalar still aborts through the parent's own pull-secrets range: {schema}"
    );
    // The zookeeper templates' `.Values.metrics.*` navigations ride the
    // same chain: junk under a disabled clickhouse renders (the
    // doubly-nested activation product must not cross the member-access
    // fanout cap and leak an unconditional host typing).
    assert!(
        schema_validates_instance(
            &schema,
            &serde_json::json!({
                "clickhouse": { "enabled": false, "zookeeper": { "metrics": "junk" } },
                "externalClickhouse": { "host": "ch.example.com", "cluster": "cluster" }
            })
        ),
        "zookeeper metrics junk renders while clickhouse is disabled: {schema}"
    );
    for field in ["pullPolicy", "repository", "tag"] {
        if field == "pullPolicy" {
            // pullPolicy is spliced plainly into `imagePullPolicy:`, so it
            // keeps its string typing in the clickhouse-enabled branch.
            assert!(
                clickhouse_operator_image_field_has_conditional_string_schema(&schema, field),
                "clickhouseOperator.image.{field} should carry string evidence in a clickhouse-enabled branch: {schema}"
            );
        }
        // tag flows through `toString` and repository through a `printf`
        // data argument — total stringifications, so their slots stay
        // deliberately untyped and any scalar validates.
        let accepts_number = schema_validates_instance(
            &schema,
            &clickhouse_operator_image_field_instance(field, serde_json::json!(7), None),
        );
        if field == "pullPolicy" {
            assert!(
                !accepts_number,
                "clickhouseOperator.image.{field} must stay string-like while clickhouse renders: {schema}"
            );
        } else {
            assert!(
                accepts_number,
                "clickhouseOperator.image.{field} renders through a total stringification and must accept scalars: {schema}"
            );
        }
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
        schema_validates_instance(
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
        "postgresql.auth.replicationUsername is explicitly stringified by the replication helper: {schema}"
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
    if enabled == Some(false) {
        // Disabling the clickhouse subchart makes `externalClickhouse.host`
        // AND `externalClickhouse.cluster` hard requirements (`required
        // "... is required if not clickhouse.enabled"`). The schema
        // validates the coalesced document helm renders from, where the
        // declared `cluster: cluster` default always fills the key — its
        // absence would mean the user null-deleted it, which aborts — so a
        // helm-valid disabled instance must carry both.
        let mut external = Map::new();
        external.insert("host".to_string(), Value::String("ch.example".to_string()));
        external.insert("cluster".to_string(), Value::String("cluster".to_string()));
        root.insert("externalClickhouse".to_string(), Value::Object(external));
    }
    Value::Object(root)
}

fn clickhouse_operator_image_field_has_conditional_string_schema(
    schema: &Value,
    field: &str,
) -> bool {
    // Output interning may move any subtree behind a root-level `$defs`
    // ref, so every pointer step and type check resolves through the root.
    let mut branch_lists = Vec::new();
    schema_values_at_pointer(
        schema,
        schema,
        &["properties", "clickhouse", "allOf"],
        &mut branch_lists,
    );
    branch_lists
        .into_iter()
        .filter_map(Value::as_array)
        .flatten()
        .any(|branch| {
            let mut fields = Vec::new();
            schema_values_at_pointer(
                schema,
                branch,
                &[
                    "then",
                    "properties",
                    "clickhouseOperator",
                    "properties",
                    "image",
                    "properties",
                    field,
                ],
                &mut fields,
            );
            fields
                .into_iter()
                .any(|value| schema_accepts_string_type(schema, value))
        })
}

fn schema_accepts_string_type(root: &Value, schema: &Value) -> bool {
    let schema = resolve_local_ref(root, schema);
    (match schema.get("type") {
        Some(Value::String(value)) => value == "string",
        Some(Value::Array(values)) => values.iter().any(|value| value.as_str() == Some("string")),
        _ => false,
    }) || ["anyOf", "oneOf", "allOf"]
        .into_iter()
        .filter_map(|key| schema.get(key).and_then(Value::as_array))
        .flatten()
        .any(|value| schema_accepts_string_type(root, value))
}

fn resolve_local_ref<'schema>(root: &'schema Value, mut schema: &'schema Value) -> &'schema Value {
    while let Some(name) = schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
    {
        let Some(resolved) = root.get("$defs").and_then(|defs| defs.get(name)) else {
            return schema;
        };
        schema = resolved;
    }
    schema
}

fn assert_schema_description(schema: &Value, pointer: &str, expected: &str) {
    let segments = pointer
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let mut matches = Vec::new();
    schema_values_at_pointer(schema, schema, &segments, &mut matches);
    let descriptions = matches
        .into_iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    // Provider-backed nodes legitimately carry the upstream Kubernetes
    // description beside the chart's values comment; the pin only demands
    // that the values comment survives.
    assert!(
        descriptions.contains(expected),
        "schema description missing at {pointer}: have {descriptions:?}, want {expected:?}"
    );
}

fn schema_values_at_pointer<'schema>(
    root: &'schema Value,
    schema: &'schema Value,
    segments: &[&str],
    matches: &mut Vec<&'schema Value>,
) {
    if segments.is_empty() {
        matches.push(schema);
        return;
    }
    let Some(object) = schema.as_object() else {
        return;
    };
    // Interned subtrees live in root-level `$defs`; follow local refs so
    // pointer-based assertions see through the output interning.
    if let Some(reference) = object.get("$ref").and_then(Value::as_str)
        && let Some(name) = reference.strip_prefix("#/$defs/")
        && let Some(target) = root.pointer(&format!("/$defs/{name}"))
    {
        schema_values_at_pointer(root, target, segments, matches);
    }

    // A values path can live in a union arm or conditional overlay without a direct node.
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = object.get(key).and_then(Value::as_array) {
            for branch in branches {
                schema_values_at_pointer(root, branch, segments, matches);
            }
        }
    }
    for key in ["then", "else"] {
        if let Some(branch) = object.get(key) {
            schema_values_at_pointer(root, branch, segments, matches);
        }
    }
    if let Some((head, tail)) = segments.split_first()
        && let Some(child) = object.get(*head)
    {
        schema_values_at_pointer(root, child, tail, matches);
    }
}
