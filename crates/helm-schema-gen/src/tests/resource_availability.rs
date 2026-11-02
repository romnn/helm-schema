use super::*;
use test_util::prelude::sim_assert_eq;

#[test]
fn unserved_resource_rejects_only_its_executed_activation() {
    let source = indoc! {r"
        {{ if .Values.enabled }}
        apiVersion: policy/v1beta1
        kind: PodSecurityPolicy
        metadata:
          name: probe
        {{ end }}
    "};
    let chain = Chain::new(vec![Box::new(
        KubernetesJsonSchemaProvider::new("v1.29.0-standalone-strict")
            .with_cache_dir(bundle_cache_dir())
            .with_allow_download(false),
    )]);
    let signals = parse_ir(source).finalize().into_schema_signals();
    let schema = generate_values_schema(ValuesSchemaInput::new(&signals, &chain));
    for (instance, accepts) in [
        (serde_json::json!({"enabled": true}), false),
        (serde_json::json!({"enabled": false}), true),
        (serde_json::json!({}), true),
        (serde_json::json!({"enabled": null}), true),
    ] {
        sim_assert_eq!(have: schema_accepts_instance(&schema, &instance), want: accepts, "instance={instance}; schema={schema}");
    }
    // This is the same terminal as a fail at the unavailable destination.
    let terminal = parse_ir("{{ if .Values.enabled }}{{ fail \"unserved API\" }}{{ end }}")
        .finalize()
        .into_schema_signals();
    sim_assert_eq!(have: schema, want: generate_values_schema(ValuesSchemaInput::new(&terminal, &chain)));
}

#[test]
fn incomplete_catalog_does_not_decide_api_extension_server_availability() {
    let source = indoc! {r"
        apiVersion: apiextensions.k8s.io/v1
        kind: CustomResourceDefinition
        metadata:
          name: probes.example.com
    "};
    let chain = Chain::new(vec![Box::new(
        KubernetesJsonSchemaProvider::new("v1.29.0-standalone-strict")
            .with_cache_dir(bundle_cache_dir())
            .with_allow_download(false),
    )]);
    let resource = ResourceRef::concrete(
        "apiextensions.k8s.io/v1".to_owned(),
        "CustomResourceDefinition".to_owned(),
    );
    sim_assert_eq!(have: chain.resource_is_served(&resource), want: None);
    let signals = parse_ir(source).finalize().into_schema_signals();
    let schema = generate_values_schema(ValuesSchemaInput::new(&signals, &chain));
    sim_assert_eq!(have: schema, want: serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": { "global": {} },
        "additionalProperties": false,
    }));
}

#[derive(Debug)]
struct Availability(Option<bool>);

impl ResourceSchemaOracle for Availability {
    fn resource_is_served(&self, _resource: &ResourceRef) -> Option<bool> {
        self.0
    }

    fn schema_fragment_for_use(&self, _use: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        None
    }
}

#[test]
fn resource_availability_is_independent_of_identity_spelling_and_value_slots() {
    let source = indoc! {r"
        {{ if .Values.enabled }}
        apiVersion: arbitrary.example/v3
        kind: ArbitraryDestination
        metadata:
          name: probe
        {{ end }}
    "};
    let signals = parse_ir(source).finalize().into_schema_signals();
    let harmless = parse_ir("{{ if .Values.enabled }}rendered{{ end }}")
        .finalize()
        .into_schema_signals();
    let failure = parse_ir("{{ if .Values.enabled }}{{ fail \"unserved API\" }}{{ end }}")
        .finalize()
        .into_schema_signals();
    for answer in [Some(true), None, Some(false)] {
        let provider = Availability(answer);
        let schema = generate_values_schema(ValuesSchemaInput::new(&signals, &provider));
        let expected_signals = if answer == Some(false) {
            &failure
        } else {
            &harmless
        };
        let expected = generate_values_schema(ValuesSchemaInput::new(expected_signals, &provider));
        sim_assert_eq!(have: &schema, want: &expected);
        for enabled in [
            serde_json::json!(true),
            serde_json::json!("yes"),
            serde_json::json!(1),
            serde_json::json!({"x": 1}),
        ] {
            sim_assert_eq!(have: schema_accepts_instance(&schema, &serde_json::json!({"enabled": enabled})), want: answer != Some(false));
        }
        for document in [
            serde_json::json!({}),
            serde_json::json!({"enabled": false}),
            serde_json::json!({"enabled": null}),
            serde_json::json!({"enabled": {}}),
        ] {
            assert!(schema_accepts_instance(&schema, &document));
        }
    }
}

#[test]
fn unproven_resource_activation_remains_diagnostic() {
    let source = indoc! {r#"
        {{ if eq (randAlphaNum 1) "x" }}
        apiVersion: arbitrary.example/v3
        kind: ArbitraryDestination
        metadata:
          name: probe
        {{ end }}
    "#};
    let signals = parse_ir(source).finalize().into_schema_signals();
    let schema =
        generate_values_schema(ValuesSchemaInput::new(&signals, &Availability(Some(false))));
    sim_assert_eq!(have: schema, want: serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": { "global": {} },
        "additionalProperties": false,
    }));
}
