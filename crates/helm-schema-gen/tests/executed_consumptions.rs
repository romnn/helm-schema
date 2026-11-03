//! Executed demand domains are checked against Helm-derived alternatives.

use std::collections::BTreeMap;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{
    ConditionalGuard, ContractPathSchemaEvidence, ContractRequirementImplication,
    ContractRequirementTarget, ContractSchemaSignals, FailValueRequirement, ProviderSchemaFragment,
    ProviderSchemaUse, ResourceSchemaOracle, ValuesPath,
};
use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::SymbolicIrContext;
use indoc::indoc;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

#[derive(Debug)]
struct ReplicaDestination;

impl ResourceSchemaOracle for ReplicaDestination {
    fn schema_fragment_for_use(&self, use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        (use_.resource.kind == "Deployment" && use_.path.0 == ["spec", "replicas"])
            .then(|| ProviderSchemaFragment::new(json!({"type": "integer"})))
    }
}

fn schema(source: &str) -> Value {
    let context = SymbolicIrContext::new(&DefineIndex::new());
    let signals = context
        .generate_contract_ir(source)
        .finalize()
        .into_schema_signals();
    generate_values_schema(ValuesSchemaInput::new(&signals, &ReplicaDestination))
}

fn required_host_schema() -> Value {
    let path = ValuesPath::parse("config");
    let evidence = BTreeMap::from([
        (
            path.clone(),
            ContractPathSchemaEvidence {
                is_referenced_value_path: true,
                requirement_implications: vec![ContractRequirementImplication::new(
                    Vec::new(),
                    ContractRequirementTarget::Value,
                    vec![FailValueRequirement::MemberHost {
                        handled_kinds: Vec::new(),
                        complete_domain: true,
                    }],
                )],
                ..ContractPathSchemaEvidence::default()
            },
        ),
        (
            ValuesPath::parse("config.value"),
            ContractPathSchemaEvidence {
                is_referenced_value_path: true,
                ..ContractPathSchemaEvidence::default()
            },
        ),
    ]);
    let signals =
        ContractSchemaSignals::new(evidence, vec![vec![ConditionalGuard::Absent { path }]]);
    generate_values_schema(ValuesSchemaInput::new(&signals, &ReplicaDestination))
}

#[test]
fn complementary_environment_arms_fold_before_projection() -> eyre::Result<()> {
    // Either Helm mode consumes the same raw host. Neither environment arm alone is values-decidable.
    let source = indoc! {"
        {{ if .Release.IsUpgrade }}
        {{ $_ := .Values.config.value }}
        {{ else }}
        {{ $_ := .Values.config.value }}
        {{ end }}
    "};
    let observed = schema(source);
    sim_assert_eq!(have: &observed, want: &required_host_schema());
    let validator = jsonschema::validator_for(&observed)?;
    for document in [
        json!({}),
        json!({"config": null}),
        json!({"config": true}),
        json!({"config": 7}),
    ] {
        assert!(
            !validator.is_valid(&document),
            "host must be a present object: {document}"
        );
    }
    assert!(validator.is_valid(&json!({"config": {"value": 7}})));
    sim_assert_eq!(have: serde_json::to_vec(&observed)?, want: serde_json::to_vec(&schema(source))?);
    let defines = DefineIndex::new();
    let context = SymbolicIrContext::new(&defines);
    for _ in 0..3 {
        let signals = context
            .generate_contract_ir(source)
            .finalize()
            .into_schema_signals();
        let warm = generate_values_schema(ValuesSchemaInput::new(&signals, &ReplicaDestination));
        sim_assert_eq!(have: serde_json::to_vec(&warm)?, want: serde_json::to_vec(&observed)?);
    }
    Ok(())
}

#[test]
fn structured_one_variable_range_retains_nonpositive_integer_counts() -> eyre::Result<()> {
    // Helm 4.2.3 renders surveyor's accounts=0/-1; a positive count reaches `.tls` and aborts.
    let signals = ContractSchemaSignals::new(
        BTreeMap::from([(
            ValuesPath::parse("accounts"),
            ContractPathSchemaEvidence {
                is_referenced_value_path: true,
                requirement_implications: vec![ContractRequirementImplication::new(
                    Vec::new(),
                    ContractRequirementTarget::Members {
                        allow_integer: true,
                    },
                    vec![FailValueRequirement::SchemaTypeEvenNull(
                        "object".to_owned(),
                    )],
                )],
                ..ContractPathSchemaEvidence::default()
            },
        )]),
        Vec::new(),
    );
    let observed = generate_values_schema(ValuesSchemaInput::new(&signals, &ReplicaDestination));
    let mut expected = schema("{{ $_ := .Values.accounts }}");
    expected["allOf"] = json!([{"additionalProperties": {}, "properties": {"accounts": {
        "anyOf": [
            {"type": "array", "items": {"type": "object"}},
            {"type": "object", "additionalProperties": {"type": "object"}},
            {"type": "integer", "maximum": 0},
            {"type": "null"}
        ]
    }}}]);
    sim_assert_eq!(have: &observed, want: &expected);
    let validator = jsonschema::validator_for(&observed)?;
    for value in [json!(0), json!(-1), json!([]), json!({}), Value::Null] {
        assert!(
            validator.is_valid(&json!({"accounts": value})),
            "empty iteration: {value}"
        );
    }
    for value in [json!(2), json!([7]), json!({"a": 7})] {
        assert!(
            !validator.is_valid(&json!({"accounts": value})),
            "member access aborts: {value}"
        );
    }
    Ok(())
}

#[test]
fn yaml_serialization_keeps_decoded_numeric_placement() -> eyre::Result<()> {
    // Helm decodes the emitted YAML, so serialized integer 3 is still a valid replica count.
    let observed = schema(indoc! {"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          replicas: {{ toYaml .Values.replicas }}
    "});
    // The existing bounded preimage retains a marker escape and abstains on empty containers.
    // These admissions remain until their placement producers have a parity-proven replacement.
    let expected = serialized_integer_preimage()?;
    let validator = jsonschema::validator_for(&observed)?;
    assert!(validator.is_valid(&json!({"replicas": 3})));
    for invalid in [
        json!(3.5),
        json!("3"),
        json!(false),
        json!({"x": 1}),
        json!([1]),
    ] {
        assert!(
            !validator.is_valid(&json!({"replicas": invalid})),
            "decoded YAML cannot fill an integer destination: {invalid}"
        );
    }
    sim_assert_eq!(have: &observed, want: &expected);
    Ok(())
}

fn serialized_integer_preimage() -> eyre::Result<Value> {
    let required = |leaf| json!({"type": "object", "properties": {"replicas": leaf}, "required": ["replicas"]});
    let truthy = json!({"anyOf": [
        {"const": true}, {"type": "number", "not": {"const": 0}},
        {"type": "string", "minLength": 1}, {"type": "array", "minItems": 1}, {"type": "object", "minProperties": 1}
    ]});
    let marker_free = json!({"anyOf": [
        {"type": ["null", "boolean", "number"]},
        {"type": "string", "not": {"pattern": "<no value>"}},
        {"type": "array", "items": {"$ref": "#/$defs/helm-marker-free"}},
        {"type": "object", "propertyNames": {"not": {"pattern": "<no value>"}}, "additionalProperties": {"$ref": "#/$defs/helm-marker-free"}}
    ]});
    let mut expected = schema("{{ $_ := .Values.replicas }}");
    let expected_object = expected
        .as_object_mut()
        .ok_or_eyre("expected schema object")?;
    expected_object.insert("$defs".to_owned(), json!({
        "t": truthy, "helm-marker-free": marker_free,
        "helm-condition-0": {"anyOf": [{"type": "array"}, {"type": "object"}]},
        "helm-condition-1": {"not": {"$ref": "#/$defs/helm-condition-0"}},
        "helm-condition-2": {"anyOf": [
            {"anyOf": [{"not": required(json!({}))}, required(json!({"enum": [null]}))]},
            required(json!({"$ref": "#/$defs/helm-marker-free"}))
        ]},
        "helm-condition-3": {"allOf": [required(json!({"$ref": "#/$defs/t"})), required(json!({"type": "array"}))]},
        "helm-condition-4": {"allOf": [required(json!({"type": "object"})), required(json!({"type": "object", "minProperties": 1}))]},
        "helm-condition-5": {"anyOf": [{"$ref": "#/$defs/helm-condition-3"}, {"$ref": "#/$defs/helm-condition-4"}]}
    }));
    expected_object.insert("allOf".to_owned(), json!([{"if": {"allOf": [{"$ref": "#/$defs/helm-condition-2"}, {"$ref": "#/$defs/helm-condition-5"}]}, "then": false}]));
    let properties = expected_object
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .ok_or_eyre("expected properties object")?;
    properties.insert(
        "replicas".to_owned(),
        json!({"allOf": [{"if": {"$ref": "#/$defs/helm-condition-1"}, "then": {"anyOf": [
            {"not": {"$ref": "#/$defs/helm-marker-free"}}, {"type": "integer"}
        ]}}]}),
    );
    Ok(expected)
}

#[test]
fn a_total_transform_does_not_acquire_the_raw_string_requirement() -> eyre::Result<()> {
    let observed = schema(indoc! {"
        {{ if .Release.IsUpgrade }}
          {{ upper .Values.x }}
        {{ else }}
          {{ upper (toString .Values.x) }}
        {{ end }}
    "});
    let validator = jsonschema::validator_for(&observed)?;
    assert!(
        validator.is_valid(&json!({"x": 7})),
        "install formats the integer before upper consumes it"
    );
    sim_assert_eq!(have: serde_json::to_vec(&observed)?, want: serde_json::to_vec(&schema(indoc! {"
        {{ if .Release.IsUpgrade }}
          {{ upper .Values.x }}
        {{ else }}
          {{ upper (toString .Values.x) }}
        {{ end }}
    "}))?);
    Ok(())
}

#[test]
fn opaque_policy_uses_only_its_own_executed_guard() -> eyre::Result<()> {
    let context = SymbolicIrContext::new(&DefineIndex::new());
    let signals = context
        .generate_contract_ir(indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          config: |
            {{ if .Values.enabled }}value={{ .Values.value }}{{ end }}
    "})
        .finalize()
        .into_schema_signals();
    let values = helm_schema_gen::PreparedValuesDocuments::new(
        serde_yaml::from_str(indoc! {"
            enabled: true
            value: text
        "})?,
        serde_yaml::Value::Null,
    );
    let observed = generate_values_schema(
        ValuesSchemaInput::new(&signals, &ReplicaDestination).with_values_documents(&values),
    );
    let validator = jsonschema::validator_for(&observed)?;
    assert!(validator.is_valid(&json!({"enabled": false, "value": 7})));
    assert!(!validator.is_valid(&json!({"enabled": true, "value": 7})));
    assert!(validator.is_valid(&json!({"enabled": true, "value": "text"})));
    sim_assert_eq!(have: observed, want: json!({
        "$schema": "http://json-schema.org/draft-07/schema#", "type": "object", "additionalProperties": false,
        "$defs": {
            "t": {"anyOf": [{"const": true}, {"type": "number", "not": {"const": 0}}, {"type": "string", "minLength": 1}, {"type": "array", "minItems": 1}, {"type": "object", "minProperties": 1}]},
            "helm-condition-0": {"if": {"type": "object", "properties": {"enabled": {"$ref": "#/$defs/t"}}, "required": ["enabled"]}, "then": true, "else": false}
        },
        "allOf": [{"if": {"$ref": "#/$defs/helm-condition-0"}, "then": {"additionalProperties": {}, "properties": {"value": {"type": "string"}}}}],
        "properties": {"enabled": {}, "value": {}, "global": {}}
    }));
    Ok(())
}

#[test]
fn opaque_policy_preserves_unreplaced_object_and_list_item_lanes() -> eyre::Result<()> {
    for default in [json!({"name": "text"}), json!(["text"])] {
        let path = ValuesPath::parse("config");
        let mut entry = ContractPathSchemaEvidence {
            is_referenced_value_path: true,
            ..ContractPathSchemaEvidence::default()
        };
        entry.facts.has_non_control_use = true;
        let mut expected_entry = entry.clone();
        if default.is_array() {
            expected_entry
                .requirement_implications
                .push(ContractRequirementImplication::new(
                    Vec::new(),
                    ContractRequirementTarget::Value,
                    vec![FailValueRequirement::SchemaTypeEvenNull("array".to_owned())],
                ));
        }
        let expected_signals = ContractSchemaSignals::new(
            BTreeMap::from([(path.clone(), expected_entry)]),
            Vec::new(),
        );
        let mut opaque = entry;
        opaque.opaque_consumption_guards = Some(vec![Vec::new()]);
        let observed_signals =
            ContractSchemaSignals::new(BTreeMap::from([(path, opaque)]), Vec::new());
        let values = helm_schema_gen::PreparedValuesDocuments::new(
            serde_yaml::to_value(json!({"config": default}))?,
            serde_yaml::Value::Null,
        );
        let observed = generate_values_schema(
            ValuesSchemaInput::new(&observed_signals, &ReplicaDestination)
                .with_values_documents(&values),
        );
        let expected = generate_values_schema(
            ValuesSchemaInput::new(&expected_signals, &ReplicaDestination)
                .with_values_documents(&values),
        );
        sim_assert_eq!(have: &observed, want: &expected);
        let validator = jsonschema::validator_for(&observed)?;
        assert!(!validator.is_valid(&json!({"config": 7})));
        if default.is_array() {
            assert!(!validator.is_valid(&json!({"config": [7]})));
            assert!(validator.is_valid(&json!({"config": ["text"]})));
        }
    }
    Ok(())
}

#[test]
fn an_unexported_opaque_guard_keeps_the_declared_scalar_lane() -> eyre::Result<()> {
    let path = ValuesPath::parse("config");
    let mut entry = ContractPathSchemaEvidence {
        is_referenced_value_path: true,
        ..ContractPathSchemaEvidence::default()
    };
    entry.facts.has_non_control_use = true;
    let expected_signals =
        ContractSchemaSignals::new(BTreeMap::from([(path.clone(), entry.clone())]), Vec::new());
    let mut opaque = entry;
    opaque.opaque_consumption_guards = Some(Vec::new());
    let observed_signals = ContractSchemaSignals::new(BTreeMap::from([(path, opaque)]), Vec::new());
    let values = helm_schema_gen::PreparedValuesDocuments::new(
        serde_yaml::to_value(json!({"config": "text"}))?,
        serde_yaml::Value::Null,
    );
    let observed = generate_values_schema(
        ValuesSchemaInput::new(&observed_signals, &ReplicaDestination)
            .with_values_documents(&values),
    );
    let expected = generate_values_schema(
        ValuesSchemaInput::new(&expected_signals, &ReplicaDestination)
            .with_values_documents(&values),
    );
    sim_assert_eq!(have: &observed, want: &expected);
    assert!(!jsonschema::validator_for(&observed)?.is_valid(&json!({"config": 7})));
    Ok(())
}

#[test]
fn complementary_opaque_map_arguments_emit_their_raw_object_requirement() -> eyre::Result<()> {
    let observed = schema(indoc! {"
        {{ if .Release.IsUpgrade }}
          {{ $_ := hasKey .Values.config \"enabled\" }}
        {{ else }}
          {{ $_ := hasKey .Values.config \"enabled\" }}
        {{ end }}
    "});
    let path = ValuesPath::parse("config");
    let signals = ContractSchemaSignals::new(
        BTreeMap::from([
            (
                path.clone(),
                ContractPathSchemaEvidence {
                    is_referenced_value_path: true,
                    requirement_implications: vec![ContractRequirementImplication::new(
                        Vec::new(),
                        ContractRequirementTarget::Value,
                        vec![FailValueRequirement::MemberHost {
                            handled_kinds: Vec::new(),
                            complete_domain: true,
                        }],
                    )],
                    ..ContractPathSchemaEvidence::default()
                },
            ),
            (
                ValuesPath::parse("config.enabled"),
                ContractPathSchemaEvidence {
                    is_referenced_value_path: true,
                    ..ContractPathSchemaEvidence::default()
                },
            ),
        ]),
        vec![vec![ConditionalGuard::Absent { path }]],
    );
    let expected = generate_values_schema(ValuesSchemaInput::new(&signals, &ReplicaDestination));
    sim_assert_eq!(have: &observed, want: &expected);
    let validator = jsonschema::validator_for(&observed)?;
    assert!(validator.is_valid(&json!({"config": {}})));
    for document in [json!({}), json!({"config": null}), json!({"config": 7})] {
        assert!(!validator.is_valid(&document));
    }
    Ok(())
}
