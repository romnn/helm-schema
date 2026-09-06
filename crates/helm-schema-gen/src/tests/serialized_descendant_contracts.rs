use super::*;
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::ContractPathSchemaEvidence;
use test_util::prelude::sim_assert_eq;

fn signals_with_serialized_parent(child: ContractPathSchemaEvidence) -> ContractSchemaSignals {
    ContractSchemaSignals::new(
        BTreeMap::from([
            (
                helm_schema_core::ValuesPath::parse("image"),
                ContractPathSchemaEvidence {
                    is_referenced_value_path: true,
                    facts: ContractValuePathFacts {
                        has_referenced_descendants: true,
                        has_non_control_use: true,
                        used_as_serialized: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
            (
                helm_schema_core::ValuesPath::parse("image.repository"),
                child,
            ),
        ]),
        Vec::new(),
    )
}

fn serialization_only_schema() -> Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": { "image": { "additionalProperties": {} } }
    })
}

#[test]
fn descendant_contract_preserves_serialized_string_and_object_lanes() -> eyre::Result<()> {
    let host = serde_json::json!({"anyOf": [
        {"type": "string"}, {"type": "object", "additionalProperties": true}
    ]});
    for (node, expected_host) in [
        (
            SchemaNode::foreign(host.clone()),
            serde_json::json!({
                "anyOf": [{"type": "string"}, {"type": "object", "additionalProperties": true}],
                "properties": {"repository": {"type": "string"}}
            }),
        ),
        (
            SchemaNode::Foreign(host.clone()),
            serde_json::json!({"allOf": [host, {
                "additionalProperties": {},
                "properties": {"repository": {"type": "string"}}
            }]}),
        ),
    ] {
        let mut document = crate::schema_tree::SchemaDocument::new_root_object();
        document.insert_values_path_schema(&helm_schema_core::ValuesPath::parse("image"), node);
        document.conjoin_literal_path_schema(
            &["image".to_string(), "repository".to_string()],
            SchemaNode::type_named("string"),
        );
        let actual = document.into_value();
        let validator = jsonschema::validator_for(&actual)?;
        for string in ["", "4", "templated"] {
            assert!(validator.is_valid(&serde_json::json!({"image": string})));
        }
        assert!(!validator.is_valid(&serde_json::json!({"image": {"repository": false}})));
        assert!(validator.is_valid(&serde_json::json!({"image": {"repository": "valid"}})));
        sim_assert_eq!(have: actual, want: serde_json::json!({
            "type": "object", "additionalProperties": false,
            "properties": {"image": expected_host}
        }));
    }
    Ok(())
}

#[test]
fn independent_string_contract_intersects_boolean_provider_preimage() -> eyre::Result<()> {
    #[derive(Debug)]
    struct BooleanProvider;

    impl ResourceSchemaOracle for BooleanProvider {
        fn schema_fragment_for_use(
            &self,
            _use: &ProviderSchemaUse,
        ) -> Option<ProviderSchemaFragment> {
            Some(ProviderSchemaFragment::new(
                serde_json::json!({"type": "boolean"}),
            ))
        }
    }

    // A plain manifest boolean slot accepts boolean tokens, but a separate
    // contains operand requires the original value to be a string.
    let signals = signals_with_serialized_parent(ContractPathSchemaEvidence {
        is_referenced_value_path: true,
        facts: ContractValuePathFacts {
            has_string_contract: true,
            has_non_self_guarded_string_contract: true,
            has_non_control_use: true,
            ..Default::default()
        },
        type_hints: BTreeSet::from(["string".to_string()]),
        provider_schema_uses: vec![ProviderSchemaUse {
            value_path: helm_schema_core::ValuesPath::parse("image.repository"),
            path: YamlPath(vec!["spec".to_string(), "enabled".to_string()]),
            kind: ValueKind::Scalar,
            stringified: false,
            resource: ResourceRef::concrete("example.io/v1".to_string(), "Example".to_string()),
            is_self_range_collection: false,
            source_null_tolerant: false,
            template_supplied_member_keys: BTreeSet::new(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            nil_omitting: false,
            omitted_members: BTreeMap::new(),
            outer_guards: Vec::new(),
        }],
        ..Default::default()
    });
    let actual = generate_values_schema(ValuesSchemaInput::new(&signals, &BooleanProvider));
    sim_assert_eq!(have: actual, want: serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object", "additionalProperties": false,
        "properties": {"image": {
            "additionalProperties": {},
            "properties": {"repository": {"allOf": [
                {"anyOf": [
                    {"type": "string", "pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)$"},
                    {"type": "boolean"}
                ]},
                {"type": "string"}
            ]}}
        }}
    }));
    let validator = jsonschema::validator_for(&actual)?;
    assert!(!validator.is_valid(&serde_json::json!({"image": {"repository": true}})));
    assert!(!validator.is_valid(&serde_json::json!({"image": {"repository": "arbitrary"}})));
    assert!(validator.is_valid(&serde_json::json!({"image": {"repository": "true"}})));
    Ok(())
}

#[test]
fn serialized_ancestor_preserves_independent_descendant_string_contract() {
    // Presence requirements are absent so this isolates the strict descendant
    // obligation from nil handling and chart defaults.
    let signals = signals_with_serialized_parent(ContractPathSchemaEvidence {
        is_referenced_value_path: true,
        facts: ContractValuePathFacts {
            has_string_contract: true,
            has_non_self_guarded_string_contract: true,
            has_non_control_use: true,
            ..Default::default()
        },
        type_hints: BTreeSet::from(["string".to_string()]),
        ..Default::default()
    });
    let actual = generate_values_schema(ValuesSchemaInput::new(&signals, &NoopProvider));
    sim_assert_eq!(have: actual, want: serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "image": {
                "additionalProperties": {},
                "properties": { "repository": { "type": "string" } }
            }
        }
    }));
}

#[test]
fn bounded_numeric_preimage_keeps_unknown_strings_beneath_serialized_parent() -> eyre::Result<()> {
    #[derive(Debug)]
    struct BoundedProvider;

    impl ResourceSchemaOracle for BoundedProvider {
        fn schema_fragment_for_use(
            &self,
            _use: &ProviderSchemaUse,
        ) -> Option<ProviderSchemaFragment> {
            Some(ProviderSchemaFragment::new(
                serde_json::json!({"type": "integer", "minimum": 1}),
            ))
        }
    }

    let signals = signals_with_serialized_parent(ContractPathSchemaEvidence {
        is_referenced_value_path: true,
        facts: ContractValuePathFacts {
            has_string_contract: true,
            has_non_self_guarded_string_contract: true,
            has_non_control_use: true,
            ..Default::default()
        },
        type_hints: BTreeSet::from(["string".to_string()]),
        provider_schema_uses: vec![ProviderSchemaUse {
            value_path: helm_schema_core::ValuesPath::parse("image.repository"),
            path: YamlPath(vec!["spec".to_string(), "count".to_string()]),
            kind: ValueKind::Scalar,
            stringified: false,
            resource: ResourceRef::concrete(
                "example.io/v1".to_string(),
                "NumericWorkload".to_string(),
            ),
            is_self_range_collection: false,
            source_null_tolerant: false,
            template_supplied_member_keys: BTreeSet::new(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            nil_omitting: false,
            omitted_members: BTreeMap::new(),
            outer_guards: Vec::new(),
        }],
        ..Default::default()
    });
    let actual = generate_values_schema(ValuesSchemaInput::new(&signals, &BoundedProvider));
    let validator = jsonschema::validator_for(&actual)?;
    assert!(validator.is_valid(&serde_json::json!({"image": {"repository": "4"}})));
    assert!(!validator.is_valid(&serde_json::json!({"image": {"repository": 4}})));
    // Numeric bounds cannot validate the parsed meaning of a string in Draft 7.
    // The zero spelling is an acknowledged unknown, not a proved valid input.
    assert!(validator.is_valid(&serde_json::json!({"image": {"repository": "0"}})));
    sim_assert_eq!(have: actual, want: serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object", "additionalProperties": false,
        "properties": {"image": {
            "additionalProperties": {},
            "properties": {"repository": {"allOf": [
                {"anyOf": [{"type": "integer", "minimum": 1}, {"type": "string"}]},
                {"type": "string"}
            ]}}
        }}
    }));
    Ok(())
}

#[test]
fn guarded_descendant_hint_does_not_impose_a_serialized_parent_shape() {
    let signals = signals_with_serialized_parent(ContractPathSchemaEvidence {
        is_referenced_value_path: true,
        guarded_type_hints: BTreeSet::from(["string".to_string()]),
        ..Default::default()
    });
    let actual = generate_values_schema(ValuesSchemaInput::new(&signals, &NoopProvider));
    sim_assert_eq!(have: actual, want: serialization_only_schema());
}

#[test]
fn fallback_descendant_hint_does_not_impose_a_serialized_parent_shape() {
    let signals = signals_with_serialized_parent(ContractPathSchemaEvidence {
        is_referenced_value_path: true,
        fallback_type_hints: BTreeSet::from(["string".to_string()]),
        ..Default::default()
    });
    let actual = generate_values_schema(ValuesSchemaInput::new(&signals, &NoopProvider));
    sim_assert_eq!(have: actual, want: serialization_only_schema());
}

#[test]
fn declared_descendant_default_does_not_impose_a_serialized_parent_shape() {
    let signals = signals_with_serialized_parent(ContractPathSchemaEvidence {
        is_referenced_value_path: true,
        ..Default::default()
    });
    let defaults = prepared_values_documents(Some("image: {repository: default-repository}"));
    let actual = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider).with_values_documents(&defaults),
    );
    sim_assert_eq!(have: actual, want: serialization_only_schema());
}

#[test]
fn conditional_descendant_preserves_serialized_object_and_array_alternatives() -> eyre::Result<()> {
    let signals = signals_with_serialized_parent(ContractPathSchemaEvidence {
        is_referenced_value_path: true,
        guarded_type_hints: BTreeSet::from(["string".to_string()]),
        ..Default::default()
    });
    let mut evidence = signals.schema_evidence_by_value_path().clone();
    evidence
        .get_mut(&helm_schema_core::ValuesPath::parse("image"))
        .ok_or_eyre("image evidence")?
        .type_hints = BTreeSet::from(["array".to_string(), "object".to_string()]);
    let signals = ContractSchemaSignals::new(evidence, Vec::new());
    let actual = generate_values_schema(ValuesSchemaInput::new(&signals, &NoopProvider));
    sim_assert_eq!(have: actual, want: serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {"image": {"anyOf": [{"type": "array"}, {"type": "object"}]}}
    }));
    Ok(())
}

#[test]
fn independent_contract_does_not_replace_a_stronger_member_constraint() -> eyre::Result<()> {
    let mut document = crate::schema_tree::SchemaDocument::new_root_object();
    let ancestor = serde_json::json!({
        "type": "object", "additionalProperties": false,
        "properties": {"repository": {"type": "integer"}}
    });
    sim_assert_eq!(have: document.insert_values_path_schema(
        &helm_schema_core::ValuesPath::parse("image"), SchemaNode::foreign(ancestor)), want: 0);
    document.conjoin_literal_path_schema(
        &["image".to_string(), "repository".to_string()],
        SchemaNode::type_named("string"),
    );
    let actual = document.into_value();
    sim_assert_eq!(have: actual, want: serde_json::json!({
        "type": "object", "additionalProperties": false,
        "properties": {"image": {
            "type": "object", "additionalProperties": false,
            "properties": {"repository": {"allOf": [{"type": "integer"}, {"type": "string"}]}}
        }}
    }));
    let validator = jsonschema::validator_for(&actual)?;
    assert!(!validator.is_valid(&serde_json::json!({"image": {"repository": 1}})));
    assert!(!validator.is_valid(&serde_json::json!({"image": {"repository": "one"}})));
    Ok(())
}

#[test]
fn independent_contract_does_not_open_a_closed_ancestor() -> eyre::Result<()> {
    let mut document = crate::schema_tree::SchemaDocument::new_root_object();
    sim_assert_eq!(have: document.insert_values_path_schema(
        &helm_schema_core::ValuesPath::parse("image"), SchemaNode::foreign(serde_json::json!({
            "type": "object", "additionalProperties": false
        }))), want: 0);
    document.conjoin_literal_path_schema(
        &["image".to_string(), "repository".to_string()],
        SchemaNode::type_named("string"),
    );
    let actual = document.into_value();
    sim_assert_eq!(have: actual, want: serde_json::json!({
        "type": "object", "additionalProperties": false,
        "properties": {"image": {"allOf": [
            {"type": "object", "additionalProperties": false},
            {"additionalProperties": {}, "properties": {"repository": {"type": "string"}}}
        ]}}
    }));
    let validator = jsonschema::validator_for(&actual)?;
    assert!(!validator.is_valid(&serde_json::json!({"image": {"repository": "one"}})));
    assert!(validator.is_valid(&serde_json::json!({"image": {}})));
    Ok(())
}

#[test]
fn independent_contract_preserves_closed_host_across_representations() -> eyre::Result<()> {
    let closed = serde_json::json!({
        "type": "object", "properties": {}, "additionalProperties": false
    });
    // Representation changes must not turn conjunction into permission to
    // introduce a property forbidden by the original host.
    for ancestor in [
        SchemaNode::foreign(closed.clone()),
        SchemaNode::Foreign(closed.clone()),
        SchemaNode::closed_object(),
    ] {
        sim_assert_eq!(have: ancestor.clone().into_value(), want: closed);
        let mut document = crate::schema_tree::SchemaDocument::new_root_object();
        document.insert_values_path_schema(&helm_schema_core::ValuesPath::parse("image"), ancestor);
        document.conjoin_literal_path_schema(
            &["image".to_string(), "repository".to_string()],
            SchemaNode::type_named("string"),
        );
        let actual = document.into_value();
        sim_assert_eq!(have: actual, want: serde_json::json!({
            "type": "object", "additionalProperties": false,
            "properties": {"image": {"allOf": [
                {"type": "object", "properties": {}, "additionalProperties": false},
                {"additionalProperties": {}, "properties": {"repository": {"type": "string"}}}
            ]}}
        }));
        let validator = jsonschema::validator_for(&actual)?;
        assert!(!validator.is_valid(&serde_json::json!({"image": {"repository": "one"}})));
        assert!(validator.is_valid(&serde_json::json!({"image": {}})));
    }
    Ok(())
}
