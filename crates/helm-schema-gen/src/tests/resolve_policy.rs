use super::*;
use crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME;
use crate::resolve_policy::{
    ConditionalSchemaAcceptanceMemo, schema_covers_strict_plain_scalar_string,
};
use color_eyre::eyre::{self, OptionExt as _};
use test_util::prelude::sim_assert_eq;

#[test]
fn conditional_schema_acceptance_memo_uses_complete_exact_keys() {
    let mut memo = ConditionalSchemaAcceptanceMemo::default();
    let string_schema = serde_json::json!({ "type": "string" });

    // Exact schema/instance repeats share an entry, while a different instance cannot.
    sim_assert_eq!(have: memo.accepts(&string_schema, &serde_json::json!("ok")), want: true);
    sim_assert_eq!(have: memo.entries.len(), want: 1);
    sim_assert_eq!(have: memo.accepts(&string_schema, &serde_json::json!("ok")), want: true);
    sim_assert_eq!(have: memo.entries.len(), want: 1);
    sim_assert_eq!(have: memo.accepts(&string_schema, &serde_json::json!(1)), want: false);
    sim_assert_eq!(have: memo.entries.len(), want: 2);

    // A Helm-truthy reference stores the complete injected definition document as its key.
    let truthy_schema = serde_json::json!({
        "$ref": format!("#/$defs/{HELM_TRUTHY_DEFINITION_NAME}")
    });
    sim_assert_eq!(have: memo.accepts(&truthy_schema, &serde_json::json!(true)), want: true);
    let wrapped_key_exists = memo.entries.keys().any(|(document, instance)| {
        instance == &serde_json::json!(true)
            && document
                .get("$defs")
                .and_then(Value::as_object)
                .is_some_and(|definitions| definitions.contains_key(HELM_TRUTHY_DEFINITION_NAME))
    });
    sim_assert_eq!(have: wrapped_key_exists, want: true);
}

#[test]
fn bounded_numeric_preimage_preserves_native_bounds_and_unknown_strings() -> eyre::Result<()> {
    let policy = ProviderValueUsePolicy::new(
        ValueKind::Scalar,
        false,
        false,
        BTreeSet::new(),
        None,
        BTreeMap::new(),
    );
    for bound in [
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
    ] {
        let original = serde_json::json!({"type": "integer", bound: 1});
        let actual = ResolvePolicy::provider_schema_for_value_use(&original, &policy)
            .ok_or_eyre("bounded numeric preimage")?;
        sim_assert_eq!(have: actual, want: serde_json::json!({
            "anyOf": [original, {"type": "string"}]
        }));
        let native = jsonschema::validator_for(&original)?;
        let projected = jsonschema::validator_for(&actual)?;
        for number in [0, 1, 4] {
            sim_assert_eq!(have: projected.is_valid(&serde_json::json!(number)), want: native.is_valid(&serde_json::json!(number)));
        }
        for spelling in ["4", "+4", "04", "0x4", "4.0", "0"] {
            assert!(projected.is_valid(&serde_json::json!(spelling)));
        }
    }
    Ok(())
}

#[test]
fn bounded_numeric_preimage_preserves_native_one_of_exclusivity() -> eyre::Result<()> {
    for (original, stringified, projected) in [
        (
            serde_json::json!({"oneOf": [
                {"type": "integer", "minimum": 1}, {"type": "boolean"}
            ]}),
            false,
            serde_json::json!({"oneOf": [
                {"type": "integer", "minimum": 1},
                {"anyOf": [
                    {"type": "string", "pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)$"},
                    {"type": "boolean"}
                ]}
            ]}),
        ),
        (
            serde_json::json!({"oneOf": [
                {"type": "integer", "minimum": 1}, {"type": "integer", "maximum": 5}
            ]}),
            false,
            serde_json::json!({"oneOf": [
                {"type": "integer", "minimum": 1}, {"type": "integer", "maximum": 5}
            ]}),
        ),
        // The stringified entry reaches the preimage without the separate
        // scalar restriction that discards validation siblings of anyOf.
        (
            serde_json::json!({"type": "integer", "minimum": 1, "anyOf": [{"const": 4}, {"const": 7}]}),
            true,
            serde_json::json!({"type": "integer", "minimum": 1, "anyOf": [{"const": 4}, {"const": 7}]}),
        ),
        (
            serde_json::json!({"type": ["integer", "null"], "minimum": 1}),
            false,
            serde_json::json!({"anyOf": [
                {"type": "integer", "minimum": 1}, {"type": "null", "minimum": 1},
                {"type": "string", "pattern": "^&[A-Za-z0-9_-]+[ \\t]*(#.*)?$"},
                {"type": "string", "pattern": "^(~|null|Null|NULL)([ \\t]+#.*)?$"},
                {"type": "string", "pattern": "^[ \\t]*(#.*)?$"}
            ]}),
        ),
    ] {
        let policy = ProviderValueUsePolicy::new(
            ValueKind::Scalar,
            stringified,
            false,
            BTreeSet::new(),
            None,
            BTreeMap::new(),
        );
        let actual = ResolvePolicy::provider_schema_for_value_use(&original, &policy)
            .ok_or_eyre("bounded numeric composite preimage")?;
        sim_assert_eq!(have: actual, want: serde_json::json!({
            "anyOf": [projected, {"type": "string"}]
        }));
        let validator = jsonschema::validator_for(&actual)?;
        assert!(validator.is_valid(&serde_json::json!("true")));
        assert!(validator.is_valid(&serde_json::json!("4")));
        let native = jsonschema::validator_for(&original)?;
        for value in [
            serde_json::json!(0),
            serde_json::json!(4),
            serde_json::json!(7),
            serde_json::json!(true),
        ] {
            sim_assert_eq!(have: validator.is_valid(&value), want: native.is_valid(&value));
        }
    }
    Ok(())
}

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

fn expected_mapping_preimage() -> Value {
    serde_json::json!({
        "type": "object",
        "propertyNames": {"type": "string", "allOf": [
            {"not": {"pattern": ":[ \\t]|:$"}},
            {"not": {"pattern": "[ \\t]#"}},
            {"not": {"pattern": "[\\r\\n]"}}
        ]},
        "additionalProperties": {"anyOf": [
            {"type": "boolean"}, {"type": "integer"}, {"type": "null"}, {"type": "number"},
            {"type": "string", "allOf": [
                {"not": {"pattern": ":[ \\t]|:$"}},
                {"not": {"pattern": "[ \\t]#"}},
                {"not": {"pattern": "[\\r\\n]"}}
            ]},
            {"type": "array", "maxItems": 0}, {"type": "object", "maxProperties": 0}
        ]}
    })
}

fn expected_plain_string_preimage() -> Value {
    serde_json::json!({"anyOf": [
        expected_mapping_preimage(),
        {"type": "string", "allOf": [
            {"not": {"pattern": "^[!&*#{}\\[\\],|>@`%]"}},
            {"not": {"pattern": "^[-?:]([ \\t]|$)"}},
            {"not": {"pattern": ":[ \\t]|:$"}},
            {"not": {"pattern": "[ \\t]#"}},
            {"not": {"pattern": "[\\r\\n]"}},
            {"not": {"pattern": "^(|~|null|Null|NULL)$"}},
            {"not": {"pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)$"}},
            {"not": {"pattern": r"^([0-9][0-9_]{0,50}(\.[0-9_]{0,50})?([eE][+-]?[0-9]{1,2})?|[+-]_*[0-9][0-9_]{0,50}(\.[0-9_]{0,50})?([eE][+-]?[0-9]{1,2})?|[+-]_*\._*[0-9][0-9_]{0,50}([eE][+-]?[0-9]{1,2})?|\.[0-9]{1,50}([eE][+-]?[0-9]{1,2})?)$"}},
            {"not": {"pattern": r"^(([+-]_*)?(0|[1-9][0-9_]{0,17}|0[xX][0-9a-fA-F]{1,15}|0[bB][01]{1,62}|0[oO][0-7]{1,20}|0[0-7]{1,20})|[+-]_*0[0-7]{0,8}[89][0-9]{0,8})$"}},
            {"not": {"pattern": r"^([+-]?\.(inf|Inf|INF)|\.(nan|NaN|NAN))$"}}
        ]},
        {"type": "string", "allOf": [
            {"pattern": "^[A-Za-z_][A-Za-z0-9_.+/\\-]*[ \\t]+#"},
            {"not": {"pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)[ \\t]+#"}},
            {"not": {"pattern": "^(null|Null|NULL)[ \\t]+#"}}
        ]}
    ]})
}

#[test]
fn bounded_numeric_preimage_counts_typeless_siblings_for_mapping_inputs() -> eyre::Result<()> {
    let policy = ProviderValueUsePolicy::new(
        ValueKind::Scalar,
        false,
        false,
        BTreeSet::new(),
        None,
        BTreeMap::new(),
    );
    for (other, accepts_map) in [
        (serde_json::json!({}), false),
        (serde_json::json!(true), false),
        (serde_json::json!(false), true),
    ] {
        let original = serde_json::json!({"oneOf": [
            {"type": "integer", "minimum": 1}, {"type": "string"}, other
        ]});
        let actual = ResolvePolicy::provider_schema_for_value_use(&original, &policy)
            .ok_or_eyre("complete non-string preimage")?;
        let validator = jsonschema::validator_for(&actual)?;
        sim_assert_eq!(have: validator.is_valid(&serde_json::json!({"a": "b"})), want: accepts_map);
        sim_assert_eq!(have: actual, want: serde_json::json!({"anyOf": [
            {"oneOf": [{"type": "integer", "minimum": 1}, expected_plain_string_preimage(), other]},
            {"type": "string"}
        ]}));
    }
    Ok(())
}

#[test]
fn bounded_numeric_preimage_preserves_mapping_inputs_of_string_siblings() -> eyre::Result<()> {
    let mut bounded_string = expected_plain_string_preimage();
    bounded_string["anyOf"][1]["minimum"] = serde_json::json!(1);
    let policy = ProviderValueUsePolicy::new(
        ValueKind::Scalar,
        false,
        false,
        BTreeSet::new(),
        None,
        BTreeMap::new(),
    );
    for (original, projected) in [
        (
            serde_json::json!({"anyOf": [{"type": "integer", "minimum": 1}, {"type": "string"}]}),
            serde_json::json!({"anyOf": [{"type": "integer", "minimum": 1}, expected_plain_string_preimage()]}),
        ),
        (
            serde_json::json!({"oneOf": [{"type": "integer", "minimum": 1}, {"type": "string"}]}),
            serde_json::json!({"oneOf": [{"type": "integer", "minimum": 1}, expected_plain_string_preimage()]}),
        ),
        (
            serde_json::json!({"type": ["integer", "string"], "minimum": 1}),
            serde_json::json!({"anyOf": [
                bounded_string["anyOf"][0], bounded_string["anyOf"][1], bounded_string["anyOf"][2],
                {"type": "integer", "minimum": 1}
            ]}),
        ),
    ] {
        let actual = ResolvePolicy::provider_schema_for_value_use(&original, &policy)
            .ok_or_eyre("numeric/string preimage")?;
        let validator = jsonschema::validator_for(&actual)?;
        assert!(validator.is_valid(&serde_json::json!({"a": "b"})));
        sim_assert_eq!(have: actual, want: serde_json::json!({
            "anyOf": [projected, {"type": "string"}]
        }));
        let strict = crate::merge::intersect_schema_list(vec![
            actual,
            serde_json::json!({"type": "string"}),
        ]);
        let strict_validator = jsonschema::validator_for(&strict)?;
        assert!(!strict_validator.is_valid(&serde_json::json!({"a": "b"})));
        assert!(strict_validator.is_valid(&serde_json::json!("4")));
    }
    Ok(())
}

#[test]
fn bounded_numeric_preimage_keeps_non_string_composition_exclusive() -> eyre::Result<()> {
    let policy = ProviderValueUsePolicy::new(
        ValueKind::Scalar,
        false,
        false,
        BTreeSet::new(),
        None,
        BTreeMap::new(),
    );
    let string = expected_plain_string_preimage();
    for (original, projected, accepts_mapping) in [
        (
            serde_json::json!({"oneOf": [{"type": "integer", "minimum": 1}, {"type": "string"}, {"type": "string"}]}),
            serde_json::json!({"oneOf": [{"type": "integer", "minimum": 1}, string, string]}),
            false,
        ),
        (
            serde_json::json!({"oneOf": [{"type": "integer", "minimum": 1}, {"anyOf": [{"type": "string"}, {"type": "string"}]}]}),
            serde_json::json!({"oneOf": [{"type": "integer", "minimum": 1}, {"anyOf": [string, string]}]}),
            true,
        ),
        (
            serde_json::json!({"anyOf": [{"type": "integer", "minimum": 1}, {"oneOf": [{"type": "string"}, {"type": "string"}]}]}),
            serde_json::json!({"anyOf": [{"type": "integer", "minimum": 1}, {"oneOf": [string, string]}]}),
            false,
        ),
    ] {
        let actual = ResolvePolicy::provider_schema_for_value_use(&original, &policy)
            .ok_or_eyre("composed non-string preimage")?;
        let validator = jsonschema::validator_for(&actual)?;
        sim_assert_eq!(have: validator.is_valid(&serde_json::json!({"a": "b"})), want: accepts_mapping);
        sim_assert_eq!(have: actual, want: serde_json::json!({
            "anyOf": [projected, {"type": "string"}]
        }));
    }
    Ok(())
}

#[test]
fn overlapping_nullable_one_of_rejects_plain_null_spellings() -> eyre::Result<()> {
    let policy = ProviderValueUsePolicy::new(
        ValueKind::Scalar,
        false,
        false,
        BTreeSet::new(),
        None,
        BTreeMap::new(),
    );
    let schema = ResolvePolicy::provider_schema_for_value_use(
        &serde_json::json!({
            "oneOf": [
                { "type": ["string", "null"] },
                { "type": ["integer", "null"] },
            ]
        }),
        &policy,
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
fn bounded_numeric_preimage_inherits_only_proven_numeric_parent_type() -> eyre::Result<()> {
    let policy = ProviderValueUsePolicy::new(
        ValueKind::Scalar,
        false,
        false,
        BTreeSet::new(),
        None,
        BTreeMap::new(),
    );
    let original = serde_json::json!({"type": "integer", "oneOf": [
        {"minimum": 1, "maximum": 10}, {"minimum": 100}
    ]});
    let actual = ResolvePolicy::provider_schema_for_value_use(&original, &policy)
        .ok_or_eyre("inherited numeric preimage")?;
    let validator = jsonschema::validator_for(&actual)?;
    assert!(validator.is_valid(&serde_json::json!("4")));
    sim_assert_eq!(have: actual, want: serde_json::json!({
        "anyOf": [original, {"type": "string"}]
    }));
    let native = jsonschema::validator_for(&original)?;
    for value in [0, 4, 50, 101] {
        sim_assert_eq!(have: validator.is_valid(&serde_json::json!(value)), want: native.is_valid(&serde_json::json!(value)));
    }
    let string_parent = serde_json::json!({"type": "string", "oneOf": [
        {"minimum": 1, "maximum": 10}, {"minimum": 100}
    ]});
    let actual_string = ResolvePolicy::provider_schema_for_value_use(&string_parent, &policy)
        .ok_or_eyre("string parent preimage")?;
    sim_assert_eq!(have: actual_string, want: string_parent);
    assert!(!jsonschema::validator_for(&actual_string)?.is_valid(&serde_json::json!("4")));
    Ok(())
}

#[test]
fn int_or_string_preimage_partitions_numeric_string_spellings() -> eyre::Result<()> {
    let policy = ProviderValueUsePolicy::new(
        ValueKind::Scalar,
        false,
        false,
        BTreeSet::new(),
        None,
        BTreeMap::new(),
    );
    let schema = ResolvePolicy::provider_schema_for_value_use(
        &serde_json::json!({
            "oneOf": [
                { "type": "string" },
                { "type": "integer" },
            ]
        }),
        &policy,
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
    let policy = ProviderValueUsePolicy::new(
        ValueKind::Scalar,
        false,
        false,
        BTreeSet::new(),
        None,
        BTreeMap::new(),
    );
    let schema = ResolvePolicy::provider_schema_for_value_use(
        &serde_json::json!({ "type": "string" }),
        &policy,
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
            "required": ["port"],
            "type": "object"
    })
}

#[test]
fn branch_only_type_hint_keeps_declared_shape_until_base_classification() {
    let resolved = ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs::Complete {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts::default(),
            ValuesYamlPathFacts::default(),
        ),
        provider_schema: SchemaNode::empty(),
        values_yaml_schema: SchemaNode::from_value(serde_json::json!({ "type": "boolean" })),
        guard_predicate_schema: SchemaNode::empty(),
        type_hint_schema: SchemaNode::empty(),
        guarded_type_hint_schema: SchemaNode::from_value(serde_json::json!({ "type": "string" })),
        fallback_type_hint_schema: SchemaNode::empty(),
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
    let resolved = ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs::Complete {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts::default(),
            ValuesYamlPathFacts::default(),
        ),
        provider_schema: SchemaNode::from_value(serde_json::json!({
            "type": "string",
            "pattern": "^restricted$"
        })),
        values_yaml_schema: SchemaNode::empty(),
        guard_predicate_schema: SchemaNode::empty(),
        type_hint_schema: SchemaNode::empty(),
        guarded_type_hint_schema: SchemaNode::from_value(serde_json::json!({ "type": "string" })),
        fallback_type_hint_schema: SchemaNode::empty(),
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
    let resolved = ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs::Complete {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts::default(),
            ValuesYamlPathFacts::default(),
        ),
        provider_schema: SchemaNode::from_value(serde_json::json!({
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
        })),
        values_yaml_schema: SchemaNode::from_value(serde_json::json!({
            "type": "array",
            "items": {},
        })),
        guard_predicate_schema: SchemaNode::empty(),
        type_hint_schema: SchemaNode::from_value(serde_json::json!({ "type": "string" })),
        guarded_type_hint_schema: SchemaNode::empty(),
        fallback_type_hint_schema: SchemaNode::empty(),
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
    let resolved = ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs::Complete {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts {
                has_render_use: true,
                all_render_uses_self_guarded: helm_schema_core::AllUses::new(false),
                all_render_uses_falsy_tolerant: helm_schema_core::AllUses::new(false),
                ..ContractValuePathFacts::default()
            },
            ValuesYamlPathFacts {
                has_dependency_default: true,
                ..ValuesYamlPathFacts::default()
            },
        ),
        provider_schema: SchemaNode::from_value(provider_schema.clone()),
        values_yaml_schema: SchemaNode::empty(),
        guard_predicate_schema: SchemaNode::empty(),
        type_hint_schema: SchemaNode::empty(),
        guarded_type_hint_schema: SchemaNode::empty(),
        fallback_type_hint_schema: SchemaNode::empty(),
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

    let parent_consumed =
        ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs::Complete {
            facts: ValuePathSchemaFacts::new(
                ContractValuePathFacts {
                    has_render_use: true,
                    has_unconditional_render_use: true,
                    all_render_uses_self_guarded: helm_schema_core::AllUses::new(false),
                    all_render_uses_falsy_tolerant: helm_schema_core::AllUses::new(false),
                    ..ContractValuePathFacts::default()
                },
                ValuesYamlPathFacts {
                    has_dependency_default: true,
                    ..ValuesYamlPathFacts::default()
                },
            ),
            provider_schema: SchemaNode::from_value(provider_schema.clone()),
            values_yaml_schema: SchemaNode::empty(),
            guard_predicate_schema: SchemaNode::empty(),
            type_hint_schema: SchemaNode::empty(),
            guarded_type_hint_schema: SchemaNode::empty(),
            fallback_type_hint_schema: SchemaNode::empty(),
        });
    sim_assert_eq!(have: parent_consumed, want: provider_schema);

    let dependency_root =
        ResolvePolicy::resolve_schema_for_value_path(ValuePathSchemaInputs::Complete {
            facts: ValuePathSchemaFacts::new(
                ContractValuePathFacts {
                    has_render_use: true,
                    accepted_dependency_values_root_fragment: true,
                    all_render_uses_self_guarded: helm_schema_core::AllUses::new(false),
                    all_render_uses_falsy_tolerant: helm_schema_core::AllUses::new(false),
                    ..ContractValuePathFacts::default()
                },
                ValuesYamlPathFacts {
                    has_dependency_default: true,
                    ..ValuesYamlPathFacts::default()
                },
            ),
            provider_schema: SchemaNode::from_value(provider_schema.clone()),
            values_yaml_schema: SchemaNode::empty(),
            guard_predicate_schema: SchemaNode::empty(),
            type_hint_schema: SchemaNode::empty(),
            guarded_type_hint_schema: SchemaNode::empty(),
            fallback_type_hint_schema: SchemaNode::empty(),
        });
    sim_assert_eq!(have: dependency_root, want: provider_schema);
}

#[test]
fn pathless_conditional_target_does_not_own_descendant_defaults() {
    let mut contract = ContractIr::from_contract_uses(vec![ContractUse {
        source_expr: helm_schema_core::ValuesPath::parse(""),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("enabled"),
        }]),
        resource: Some(ResourceRef::concrete(
            "v1".to_string(),
            "ConfigMap".to_string(),
        )),
        provenance: Vec::new(),
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
