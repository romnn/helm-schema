use super::*;
use crate::resolve_policy::schema_covers_strict_plain_scalar_string;
use crate::schema_model::empty_schema;
use color_eyre::eyre::{self, OptionExt as _};
use test_util::prelude::sim_assert_eq;

#[test]
fn common_plain_string_proof_respects_one_of_exclusivity() {
    assert!(schema_covers_strict_plain_scalar_string(
        &serde_json::json!({
            "anyOf": [{ "type": "string" }, { "type": "null" }]
        })
    ));
    assert!(schema_covers_strict_plain_scalar_string(
        &serde_json::json!({
            "oneOf": [{ "type": "string" }, { "type": "integer" }]
        })
    ));
    assert!(!schema_covers_strict_plain_scalar_string(
        &serde_json::json!({
            "oneOf": [
                { "type": "string" },
                { "type": ["string", "null"] }
            ]
        })
    ));
    assert!(!schema_covers_strict_plain_scalar_string(
        &serde_json::json!({
            "type": "string",
            "pattern": "^fixed$"
        })
    ));
}

#[test]
fn overlapping_nullable_one_of_rejects_plain_null_spellings() -> eyre::Result<()> {
    let use_ = ProviderSchemaUse {
        value_path: "port".to_string(),
        path: YamlPath(vec!["spec".to_string()]),
        kind: ValueKind::Scalar,
        stringified: false,
        resource: ResourceRef::concrete("v1".to_string(), "Pod".to_string()),
        is_self_range_collection: false,
        source_null_tolerant: false,
        template_supplied_member_keys: BTreeSet::new(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: BTreeMap::new(),
        outer_guards: Vec::new(),
    };
    let schema = ResolvePolicy::provider_schema_for_value_use(
        &serde_json::json!({
            "oneOf": [
                { "type": ["string", "null"] },
                { "type": ["integer", "null"] },
            ]
        }),
        &use_,
    )
    .ok_or_eyre("scalar provider preimage")?;

    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!("&anchor")),
        "an anchor-only token reparses to null, which matches both oneOf arms: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!("null")),
        "an implicit null token matches both oneOf arms after rendering: {schema}"
    );
    assert!(
        schema_accepts_instance(&schema, &serde_json::json!("audit")),
        "an ordinary named port remains valid: {schema}"
    );
    assert!(
        schema_accepts_instance(&schema, &serde_json::json!(9878)),
        "an integer port remains valid: {schema}"
    );
    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({})),
        "a mapping still formats to an ordinary named-port string: {schema}"
    );

    Ok(())
}

#[test]
fn int_or_string_preimage_partitions_numeric_string_spellings() -> eyre::Result<()> {
    let use_ = ProviderSchemaUse {
        value_path: "port".to_string(),
        path: YamlPath(vec!["spec".to_string()]),
        kind: ValueKind::Scalar,
        stringified: false,
        resource: ResourceRef::concrete("v1".to_string(), "Pod".to_string()),
        is_self_range_collection: false,
        source_null_tolerant: false,
        template_supplied_member_keys: BTreeSet::new(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: BTreeMap::new(),
        outer_guards: Vec::new(),
    };
    let schema = ResolvePolicy::provider_schema_for_value_use(
        &serde_json::json!({
            "oneOf": [
                { "type": "string" },
                { "type": "integer" },
            ]
        }),
        &use_,
    )
    .ok_or_eyre("scalar provider preimage")?;

    for (value, label) in [
        (serde_json::json!("4317"), "an integer-token string"),
        (
            serde_json::json!("+_0x1f"),
            "a sign-and-underscore radix integer",
        ),
        (
            serde_json::json!("+_08"),
            "a sign-and-underscore integral float fallback",
        ),
        (serde_json::json!("http"), "an ordinary string"),
        (serde_json::json!(4317), "an integer"),
        (
            serde_json::json!({ "named": 1 }),
            "a safely formatted mapping",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &value),
            "IntOrString preimage must admit {label}: value={value}; schema={schema}"
        );
    }

    Ok(())
}

#[test]
fn plain_string_comment_preimage_tracks_the_parsed_prefix() -> eyre::Result<()> {
    let use_ = ProviderSchemaUse {
        value_path: "label".to_string(),
        path: YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
        kind: ValueKind::Scalar,
        stringified: false,
        resource: ResourceRef::concrete("v1".to_string(), "Pod".to_string()),
        is_self_range_collection: false,
        source_null_tolerant: false,
        template_supplied_member_keys: BTreeSet::new(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: BTreeMap::new(),
        outer_guards: Vec::new(),
    };
    let schema = ResolvePolicy::provider_schema_for_value_use(
        &serde_json::json!({ "type": "string" }),
        &use_,
    )
    .ok_or_eyre("scalar provider preimage")?;

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!("a #b")),
        "the parsed scalar is the ordinary string prefix: {schema}"
    );
    for value in ["true #b", "null #b"] {
        assert!(
            !schema_accepts_instance(&schema, &serde_json::json!(value)),
            "an implicit non-string prefix must stay outside the string preimage: \
             value={value}; schema={schema}"
        );
    }

    Ok(())
}

#[test]
fn plain_probe_port_preserves_provider_one_of_semantics() {
    let src = indoc! {"
        apiVersion: v1
        kind: Pod
        metadata:
          name: probe
        spec:
          containers:
            - name: probe
              image: probe
              readinessProbe:
                httpGet:
                  port: {{ .Values.port }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("port: 9878\n"));

    sim_assert_eq!(have: schema, want: plain_probe_port_expected_schema());
}

#[expect(
    clippy::too_many_lines,
    reason = "the complete expected schema is clearest as one literal"
)]
fn plain_probe_port_expected_schema() -> Value {
    serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "allOf": [
                {
                    "additionalProperties": {},
                    "properties": {
                        "port": {
                            "not": { "type": "null" }
                        }
                    }
                },
                {
                    "required": ["port"],
                    "type": "object"
                }
            ],
            "properties": {
                "port": {
                    "oneOf": [
                        {
                            "anyOf": [
                                {
                                    "additionalProperties": {
                                        "anyOf": [
                                            { "type": "boolean" },
                                            { "type": "integer" },
                                            { "type": "null" },
                                            { "type": "number" },
                                            {
                                                "allOf": [
                                                    { "not": { "pattern": ":[ \\t]|:$" } },
                                                    { "not": { "pattern": "[ \\t]#" } },
                                                    { "not": { "pattern": "[\\r\\n]" } }
                                                ],
                                                "type": "string"
                                            },
                                            { "maxItems": 0, "type": "array" },
                                            { "maxProperties": 0, "type": "object" }
                                        ]
                                    },
                                    "propertyNames": {
                                        "allOf": [
                                            { "not": { "pattern": ":[ \\t]|:$" } },
                                            { "not": { "pattern": "[ \\t]#" } },
                                            { "not": { "pattern": "[\\r\\n]" } }
                                        ],
                                        "type": "string"
                                    },
                                    "type": "object"
                                },
                                {
                                    "allOf": [
                                        { "not": { "pattern": "^[!&*#{}\\[\\],|>@`%]" } },
                                        { "not": { "pattern": "^[-?:]([ \\t]|$)" } },
                                        { "not": { "pattern": ":[ \\t]|:$" } },
                                        { "not": { "pattern": "[ \\t]#" } },
                                        { "not": { "pattern": "[\\r\\n]" } },
                                        { "not": { "pattern": "^(|~|null|Null|NULL)$" } },
                                        {
                                            "not": {
                                                "pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)$"
                                            }
                                        },
                                        {
                                            "not": {
                                                "pattern": "^([0-9][0-9_]{0,50}(\\.[0-9_]{0,50})?([eE][+-]?[0-9]{1,2})?|[+-]_*[0-9][0-9_]{0,50}(\\.[0-9_]{0,50})?([eE][+-]?[0-9]{1,2})?|[+-]_*\\._*[0-9][0-9_]{0,50}([eE][+-]?[0-9]{1,2})?|\\.[0-9]{1,50}([eE][+-]?[0-9]{1,2})?)$"
                                            }
                                        },
                                        {
                                            "not": {
                                                "pattern": "^(([+-]_*)?(0|[1-9][0-9_]{0,17}|0[xX][0-9a-fA-F]{1,15}|0[bB][01]{1,62}|0[oO][0-7]{1,20}|0[0-7]{1,20})|[+-]_*0[0-7]{0,8}[89][0-9]{0,8})$"
                                            }
                                        },
                                        {
                                            "not": {
                                                "pattern": "^([+-]?\\.(inf|Inf|INF)|\\.(nan|NaN|NAN))$"
                                            }
                                        }
                                    ],
                                    "type": "string"
                                },
                                {
                                    "allOf": [
                                        {
                                            "pattern": "^[A-Za-z_][A-Za-z0-9_.+/\\-]*[ \\t]+#"
                                        },
                                        {
                                            "not": {
                                                "pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)[ \\t]+#"
                                            }
                                        },
                                        {
                                            "not": {
                                                "pattern": "^(null|Null|NULL)[ \\t]+#"
                                            }
                                        }
                                    ],
                                    "type": "string"
                                }
                            ]
                        },
                        {
                            "anyOf": [
                                {
                                    "pattern": "^(([+-]_*)?(0|[1-9][0-9_]{0,17}|0[xX][0-9a-fA-F]{1,15}|0[bB][01]{1,62}|0[oO][0-7]{1,20}|0[0-7]{1,20})|[+-]_*0[0-7]{0,8}[89][0-9]{0,8})$",
                                    "type": "string"
                                },
                                { "type": "integer" }
                            ]
                        }
                    ]
                }
            },
            "type": "object"
    })
}

#[test]
fn branch_only_type_hint_keeps_declared_shape_until_base_classification() {
    let resolved = ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts::default(),
            ValuesYamlPathFacts::default(),
        ),
        provider_schema: empty_schema(),
        values_yaml_schema: serde_json::json!({ "type": "boolean" }),
        guard_predicate_schema: empty_schema(),
        type_hint_schema: empty_schema(),
        guarded_type_hint_schema: serde_json::json!({ "type": "string" }),
        fallback_type_hint_schema: empty_schema(),
    });

    sim_assert_eq!(
        have: resolved,
        want: serde_json::json!({
            "anyOf": [
                { "type": "boolean" },
                { "type": "string" },
            ]
        })
    );
}

#[test]
fn branch_only_string_hint_widens_restricted_string_provider_domain() {
    let resolved = ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts::default(),
            ValuesYamlPathFacts::default(),
        ),
        provider_schema: serde_json::json!({
            "type": "string",
            "pattern": "^restricted$"
        }),
        values_yaml_schema: empty_schema(),
        guard_predicate_schema: empty_schema(),
        type_hint_schema: empty_schema(),
        guarded_type_hint_schema: serde_json::json!({ "type": "string" }),
        fallback_type_hint_schema: empty_schema(),
    });

    sim_assert_eq!(
        have: resolved,
        want: serde_json::json!({
            "anyOf": [
                {
                    "pattern": "^restricted$",
                    "type": "string"
                },
                { "type": "string" },
            ]
        })
    );
}

#[test]
fn common_plain_string_survives_all_provider_evidence_merges() {
    let resolved = ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts::default(),
            ValuesYamlPathFacts::default(),
        ),
        provider_schema: serde_json::json!({
            "anyOf": [
                {
                    "type": "string",
                    "allOf": [{ "not": { "pattern": "^[!&*#{}\\[\\],|>@`%]" } }]
                },
                {
                    "type": "string",
                    "allOf": [{
                        "not": {
                            "pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)$"
                        }
                    }]
                },
                {
                    "type": "string",
                    "pattern": "^[A-Za-z_][A-Za-z0-9_.+/\\-]*[ \\t]+#"
                },
                { "type": "null" },
            ]
        }),
        values_yaml_schema: serde_json::json!({
            "type": "array",
            "items": {},
        }),
        guard_predicate_schema: empty_schema(),
        type_hint_schema: serde_json::json!({ "type": "string" }),
        guarded_type_hint_schema: empty_schema(),
        fallback_type_hint_schema: empty_schema(),
    });

    sim_assert_eq!(
        have: schema_covers_strict_plain_scalar_string(&resolved),
        want: true,
    );
}

#[test]
fn dependency_default_refill_accepts_null_without_parent_consumer() {
    let provider_schema = serde_json::json!({
        "additionalProperties": { "type": "string" },
        "type": "object",
    });
    let resolved = ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts {
                has_render_use: true,
                ..ContractValuePathFacts::default()
            },
            ValuesYamlPathFacts {
                has_dependency_default: true,
                ..ValuesYamlPathFacts::default()
            },
        ),
        provider_schema: provider_schema.clone(),
        values_yaml_schema: empty_schema(),
        guard_predicate_schema: empty_schema(),
        type_hint_schema: empty_schema(),
        guarded_type_hint_schema: empty_schema(),
        fallback_type_hint_schema: empty_schema(),
    });

    sim_assert_eq!(
        have: resolved,
        want: serde_json::json!({
            "anyOf": [
                provider_schema,
                { "type": "null" },
            ],
        })
    );

    let parent_consumed = ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts {
                has_render_use: true,
                has_unconditional_render_use: true,
                ..ContractValuePathFacts::default()
            },
            ValuesYamlPathFacts {
                has_dependency_default: true,
                ..ValuesYamlPathFacts::default()
            },
        ),
        provider_schema: provider_schema.clone(),
        values_yaml_schema: empty_schema(),
        guard_predicate_schema: empty_schema(),
        type_hint_schema: empty_schema(),
        guarded_type_hint_schema: empty_schema(),
        fallback_type_hint_schema: empty_schema(),
    });
    sim_assert_eq!(have: parent_consumed, want: provider_schema);

    let dependency_root = ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts {
                has_render_use: true,
                accepted_dependency_values_root_fragment: true,
                ..ContractValuePathFacts::default()
            },
            ValuesYamlPathFacts {
                has_dependency_default: true,
                ..ValuesYamlPathFacts::default()
            },
        ),
        provider_schema: provider_schema.clone(),
        values_yaml_schema: empty_schema(),
        guard_predicate_schema: empty_schema(),
        type_hint_schema: empty_schema(),
        guarded_type_hint_schema: empty_schema(),
        fallback_type_hint_schema: empty_schema(),
    });
    sim_assert_eq!(have: dependency_root, want: provider_schema);
}

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
        stringified: false,
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
