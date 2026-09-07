use color_eyre::eyre;
use helm_schema_core::{ConditionalGuard, ValuesDefaultSource, ValuesPath};
use indoc::indoc;
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;

#[test]
fn coalesced_dependency_defaults_do_not_change_input_conditions() -> eyre::Result<()> {
    let source = indoc! {r"
        {{ .Values.grp.enabled | quote }}
        {{ .Values.kid.grp.enabled | quote }}
    "};
    let defaults = indoc! {"
        grp: {enabled: true}
        kid:
          grp: {enabled: true}
    "};
    let mut contract = parse_ir(source);
    contract.push_pathless_dependency_fragment("kid");
    let signals = contract.into_schema_signals();
    let declared = serde_yaml::from_str(defaults)?;
    let documents = PreparedValuesDocuments::new(declared, serde_yaml::from_str(defaults)?);
    let actual = generate_values_schema(
        ValuesSchemaInput::new(&signals, &provider()).with_values_documents(&documents),
    );
    let expected = expected_dependency_hosts_schema();

    // Both chart scopes reject the missing host in the already-coalesced document.
    sim_assert_eq!(have: &actual, want: &expected);
    let absent = signals.terminal_clauses().iter().any(|guards| {
        guards
            == &[ConditionalGuard::Absent {
                path: ValuesPath::parse("kid.grp"),
            }]
    });
    eyre::ensure!(
        absent,
        "the analyzer must retain the dependency host obligation"
    );
    for (input, accepted) in [
        (json!({"grp": {}, "kid": {"grp": {}}}), true),
        (json!({"grp": {}, "kid": {}}), false),
        (json!({"kid": {"grp": {}}}), false),
        (json!({"grp": {}, "kid": {"grp": null}}), false),
    ] {
        sim_assert_eq!(have: schema_accepts_instance(&actual, &input), want: accepted);
    }
    Ok(())
}

#[test]
fn coalesced_deleted_dependency_guard_is_falsy() -> eyre::Result<()> {
    let source = indoc! {r"
        {{- if .Values.kid.flag }}
        {{ .Values.kid.grp.enabled | quote }}
        {{- end }}
    "};
    let defaults = indoc! {"
        kid:
          flag: true
          grp: {enabled: true}
    "};
    let mut contract = parse_ir(source);
    contract.push_pathless_dependency_fragment("kid");
    let signals = contract.into_schema_signals();
    let documents = PreparedValuesDocuments::new(
        serde_yaml::from_str(defaults)?,
        serde_yaml::from_str(defaults)?,
    );
    let actual = generate_values_schema(
        ValuesSchemaInput::new(&signals, &provider()).with_values_documents(&documents),
    );
    let expected = expected_dependency_guard_schema();

    sim_assert_eq!(have: &actual, want: &expected);
    for (input, accepted) in [
        (json!({"kid": {"grp": "unused"}}), true),
        (json!({"kid": {"flag": false, "grp": "unused"}}), true),
        (json!({"kid": {"flag": true}}), false),
        (json!({"kid": {"flag": true, "grp": {}}}), true),
    ] {
        sim_assert_eq!(have: schema_accepts_instance(&actual, &input), want: accepted);
    }
    Ok(())
}

fn expected_dependency_hosts_schema() -> Value {
    let missing_group = json!({
        "anyOf": [
            {"not": {"properties": {"grp": {}}, "required": ["grp"], "type": "object"}},
            {"properties": {"grp": {"enum": [null]}}, "required": ["grp"], "type": "object"}
        ]
    });
    let dependency_missing_group = json!({"allOf": [{"type": "object"}, missing_group]});
    let group = json!({
        "additionalProperties": {},
        "properties": {"enabled": {}},
        "type": "object"
    });
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {"additionalProperties": {}, "properties": {"kid": {"type": ["null", "object"]}}},
            {"if": missing_group, "then": false},
            {
                "if": {"allOf": [
                    {
                        "properties": {"kid": dependency_missing_group},
                        "required": ["kid"],
                        "type": "object"
                    },
                    {"anyOf": [
                        {"not": {"properties": {"kid": {}}, "required": ["kid"], "type": "object"}},
                        {"properties": {"kid": {"enum": [null]}}, "required": ["kid"], "type": "object"}
                    ]}
                ]},
                "then": false
            }
        ],
        "properties": {
            "grp": group,
            "kid": {
                "additionalProperties": {},
                "allOf": [{"if": dependency_missing_group, "then": false}],
                "properties": {"grp": group},
                "type": "object"
            }
        },
        "type": "object"
    })
}

fn expected_dependency_guard_schema() -> Value {
    let flag_truthy = json!({
        "properties": {"flag": {"$ref": "#/$defs/t"}},
        "required": ["flag"],
        "type": "object"
    });
    json!({
        "$defs": {"t": {"anyOf": [
            {"const": true},
            {"not": {"const": 0}, "type": "number"},
            {"minLength": 1, "type": "string"},
            {"minItems": 1, "type": "array"},
            {"minProperties": 1, "type": "object"}
        ]}},
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "if": {"properties": {"kid": flag_truthy}, "required": ["kid"], "type": "object"},
                "then": {
                    "additionalProperties": {},
                    "properties": {"kid": {
                        "additionalProperties": {},
                        "properties": {"grp": {"type": "object"}}
                    }}
                }
            },
            {"additionalProperties": {}, "properties": {"kid": {"type": ["null", "object"]}}}
        ],
        "properties": {"kid": {
            "additionalProperties": {},
            "allOf": [{
                "if": {"allOf": [
                    flag_truthy,
                    {"allOf": [
                        {"type": "object"},
                        {"anyOf": [
                            {"not": {"properties": {"grp": {}}, "required": ["grp"], "type": "object"}},
                            {"properties": {"grp": {"enum": [null]}}, "required": ["grp"], "type": "object"}
                        ]}
                    ]}
                ]},
                "then": false
            }],
            "properties": {
                "flag": {"type": "boolean"},
                "grp": {"additionalProperties": {}, "properties": {"enabled": {}}}
            },
            "type": "object"
        }},
        "type": "object"
    })
}

#[test]
fn coalesced_runtime_hint_origin_requires_a_source_fact() -> eyre::Result<()> {
    let declarations = serde_yaml::from_str(indoc! {"
        kid:
          flag: true
        _defaults:
          restored: true
    "})?;
    let absent = crate::condition_encoding::RuntimeDefaultHints::from_sources(
        &declarations,
        &BTreeSet::new(),
    );
    sim_assert_eq!(have: absent.contains_path(&ValuesPath::parse("kid.flag")), want: false);
    sim_assert_eq!(have: absent.contains_path(&ValuesPath::parse("restored")), want: false);

    let runtime = crate::condition_encoding::RuntimeDefaultHints::from_sources(
        &declarations,
        &BTreeSet::from([ValuesDefaultSource {
            target_path: ValuesPath::default(),
            source_path: ValuesPath::parse("_defaults"),
        }]),
    );
    sim_assert_eq!(have: runtime.contains_path(&ValuesPath::parse("kid.flag")), want: false);
    sim_assert_eq!(have: runtime.contains_path(&ValuesPath::parse("restored")), want: true);
    sim_assert_eq!(
        have: runtime.evaluate_guard_hints(&[ConditionalGuard::Truthy { path: ValuesPath::parse("restored") }]),
        want: Some(true)
    );
    Ok(())
}
