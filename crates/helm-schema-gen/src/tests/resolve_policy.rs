use super::*;

#[test]
fn pathless_conditional_target_does_not_own_descendant_defaults() {
    let mut contract = ContractIr::from_contract_uses(vec![ContractUse {
        source_expr: String::new(),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
            path: "enabled".to_string(),
        }]),
        resource: Some(ResourceRef::concrete(
            "v1".to_string(),
            "ConfigMap".to_string(),
        )),
        provenance: Vec::new(),
        has_string_contract: false,
        template_supplied_member_keys: BTreeSet::new(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: BTreeMap::new(),
        digest: false,
        merge_operand: false,
    }]);
    contract.push_pathless_dependency_fragment("dependency");
    let values_yaml = indoc! {"
        enabled: false
        dependency:
          nested: 7
    "};
    let schema = schema_for_values_yaml(contract, Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "enabled": false,
                "dependency": { "nested": 7 }
            })
        ),
        "dependency descendants remain valid in the conditional target's off state: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "enabled": false,
                "dependency": { "nested": "not the declared integer" }
            })
        ),
        "an empty root target must not suppress dependency descendant typing: {schema}"
    );
}

#[test]
fn declared_scalar_default_survives_active_conjunctive_branch() {
    let unsafe_plain_scalar = serde_json::json!({
        "allOf": [
            {
                "not": {
                    "pattern": "^(|~|null|Null|NULL)$"
                }
            },
            {
                "not": {
                    "pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)$"
                }
            }
        ],
        "type": "string"
    });
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "mode": { "type": "string" },
            "locations": { "type": "array" }
        },
        "allOf": [{
            "if": {
                "properties": { "mode": { "const": "enabled" } },
                "required": ["mode"]
            },
            "then": {
                "properties": {
                    "locations": {
                        "items": {
                            "properties": {
                                "provider": unsafe_plain_scalar
                            },
                            "type": "object"
                        },
                        "type": "array"
                    }
                }
            }
        }]
    });
    let declared = serde_json::json!({
        "mode": "enabled",
        "locations": [{ "provider": "" }]
    });
    let schema = preserve_declared_default_in_schema(schema, &declared);

    assert!(
        schema_accepts_instance(&schema, &declared),
        "the exact chart-authored empty default should survive: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "mode": "enabled",
                "locations": [{ "provider": "true" }]
            })
        ),
        "preserving the exact default must not widen the lexical domain: {schema}"
    );
}

/// A `range`d map whose members are validated by a shared member schema
/// (`additionalProperties`/`items`) still preserves each member's declared
/// empty scalar default, including through the `anyOf` array | object | null
/// member projection and a nullable-sink `anyOf` wrapper on the leaf.
#[test]
fn declared_empty_default_survives_ranged_map_member_projection() {
    let nullable_unsafe_plain_scalar = serde_json::json!({
        "anyOf": [
            {
                "allOf": [
                    {
                        "not": {
                            "pattern": "^(|~|null|Null|NULL)$"
                        }
                    },
                    {
                        "not": {
                            "pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)$"
                        }
                    }
                ],
                "type": "string"
            },
            { "type": "null" }
        ]
    });
    let member = serde_json::json!({
        "type": "object",
        "properties": { "secretName": nullable_unsafe_plain_scalar }
    });
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "databases": {
                "anyOf": [
                    { "type": "object", "additionalProperties": member },
                    { "type": "array", "items": member },
                    { "type": "null" }
                ]
            }
        }
    });
    let declared = serde_json::json!({
        "databases": { "airtype": { "secretName": "" } }
    });
    let schema = preserve_declared_default_in_schema(schema, &declared);

    assert!(
        schema_accepts_instance(&schema, &declared),
        "the declared empty member default must survive the member projection: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "databases": { "airtype": { "secretName": "true" } } })
        ),
        "preserving the exact empty default must not widen the lexical domain: {schema}"
    );
}

#[test]
fn declared_default_does_not_weaken_terminal_false_branch() {
    let schema = serde_json::json!({
        "type": "object",
        "allOf": [false]
    });
    let declared = serde_json::json!({ "enabled": true });
    let schema = preserve_declared_default_in_schema(schema, &declared);

    assert!(
        !schema_accepts_instance(&schema, &declared),
        "a terminal validator must not be bypassed by default preservation: {schema}"
    );
}
