use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use super::*;

/// A grouped map argument tolerates nil without widening its concrete kind domain.
#[test]
fn grouped_map_argument_keeps_nil_conversion_separate_from_path_identity() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          bare: {{ hasKey .Values.bare "key" | quote }}
          grouped: {{ hasKey (.Values.grouped) "key" | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let mut properties = serde_json::Map::new();
    properties.insert(
        "bare".to_string(),
        serde_json::json!({ "allOf": [{}, { "type": "object" }] }),
    );
    properties.insert("grouped".to_string(), serde_json::json!({}));
    let expected = expected_values_schema(
        properties,
        vec![
            root_property_schema("grouped", serde_json::json!({ "type": ["null", "object"] })),
            serde_json::json!({
                "if": { "anyOf": [
                    { "not": {
                        "properties": { "bare": {} },
                        "required": ["bare"],
                        "type": "object",
                    } },
                    {
                        "properties": { "bare": { "enum": [null] } },
                        "required": ["bare"],
                        "type": "object",
                    },
                ] },
                "then": false,
            }),
        ],
        false,
    );

    sim_assert_eq!(have: &schema, want: &expected);

    for (instance, want) in [
        (serde_json::json!({ "bare": {} }), true),
        (serde_json::json!({ "bare": null }), false),
        (serde_json::json!({}), false),
        (serde_json::json!({ "bare": "wrong" }), false),
        (serde_json::json!({ "bare": {}, "grouped": null }), true),
        (serde_json::json!({ "bare": {}, "grouped": {} }), true),
        (serde_json::json!({ "bare": {}, "grouped": "wrong" }), false),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

/// Grouping a selector receiver tolerates its nil result without grouping the final lookup.
#[test]
fn grouped_selector_argument_keeps_receiver_and_final_lookup_boundaries() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          nested: {{ hasKey (.Values.parent).subject "key" | quote }}
          whole: {{ hasKey ((.Values.whole).subject) "key" | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "if": { "not": { "anyOf": [
                    { "not": {
                        "properties": { "parent": {} },
                        "required": ["parent"],
                        "type": "object",
                    } },
                    {
                        "properties": { "parent": { "enum": [null] } },
                        "required": ["parent"],
                        "type": "object",
                    },
                ] } },
                "then": { "allOf": [
                    root_property_schema(
                        "parent",
                        serde_json::json!({
                            "additionalProperties": {},
                            "properties": { "subject": { "type": "object" } },
                        }),
                    ),
                    root_property_schema("parent", serde_json::json!({ "type": "object" })),
                ] },
            },
            root_property_schema(
                "whole",
                serde_json::json!({
                    "additionalProperties": {},
                    "properties": {
                        "subject": { "type": ["null", "object"] },
                    },
                }),
            ),
            {
                "if": { "not": { "anyOf": [
                    { "not": {
                        "properties": { "whole": {} },
                        "required": ["whole"],
                        "type": "object",
                    } },
                    {
                        "properties": { "whole": { "enum": [null] } },
                        "required": ["whole"],
                        "type": "object",
                    },
                ] } },
                "then": root_property_schema("whole", serde_json::json!({ "type": "object" })),
            },
            {
                "if": { "allOf": [
                    { "anyOf": [
                        { "not": {
                            "properties": { "parent": {
                                "properties": { "subject": {} },
                                "required": ["subject"],
                                "type": "object",
                            } },
                            "required": ["parent"],
                            "type": "object",
                        } },
                        {
                            "properties": { "parent": {
                                "properties": { "subject": { "enum": [null] } },
                                "required": ["subject"],
                                "type": "object",
                            } },
                            "required": ["parent"],
                            "type": "object",
                        },
                    ] },
                    { "not": { "anyOf": [
                        { "not": {
                            "properties": { "parent": {} },
                            "required": ["parent"],
                            "type": "object",
                        } },
                        {
                            "properties": { "parent": { "enum": [null] } },
                            "required": ["parent"],
                            "type": "object",
                        },
                    ] } },
                ] },
                "then": false,
            },
        ],
        "properties": {
            "parent": {
                "additionalProperties": {},
                "properties": { "subject": {} },
            },
            "whole": {
                "additionalProperties": {},
                "properties": { "subject": {} },
            },
        },
        "type": "object",
    });

    sim_assert_eq!(have: &schema, want: &expected);

    for (instance, want) in [
        (serde_json::json!({}), true),
        (serde_json::json!({ "parent": null, "whole": null }), true),
        (serde_json::json!({ "parent": { "marker": 1 } }), false),
        (
            serde_json::json!({ "parent": { "marker": 1, "subject": null } }),
            false,
        ),
        (
            serde_json::json!({ "parent": { "subject": "wrong" } }),
            false,
        ),
        (
            serde_json::json!({ "parent": { "subject": {} }, "whole": { "subject": "wrong" } }),
            false,
        ),
        (
            serde_json::json!({ "parent": { "subject": {} }, "whole": { "marker": 1 } }),
            true,
        ),
        (
            serde_json::json!({ "parent": { "subject": {} }, "whole": { "subject": null } }),
            true,
        ),
        (
            serde_json::json!({ "parent": { "subject": {} }, "whole": { "subject": {} } }),
            true,
        ),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

/// Range-member variables retain direct lookup nil behavior at map arguments.
#[test]
fn ranged_member_map_arguments_reject_nil_members_and_fields() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $member := .Values.direct }}
          direct: {{ hasKey $member "key" | quote }}
          {{- end }}
          {{- range $member := .Values.nested }}
          nested: {{ hasKey $member.child "key" | quote }}
          {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            direct: []
            nested: []
        "}),
    );

    for (instance, want) in [
        (serde_json::json!({ "direct": [{}], "nested": [] }), true),
        (serde_json::json!({ "direct": [null], "nested": [] }), false),
        (
            serde_json::json!({ "direct": ["wrong"], "nested": [] }),
            false,
        ),
        (
            serde_json::json!({ "direct": [], "nested": [{ "child": {} }] }),
            true,
        ),
        (serde_json::json!({ "direct": [], "nested": [null] }), false),
        (serde_json::json!({ "direct": [], "nested": [{}] }), false),
        (
            serde_json::json!({ "direct": [], "nested": [{ "child": null }] }),
            false,
        ),
        (
            serde_json::json!({ "direct": [], "nested": [{ "child": "wrong" }] }),
            false,
        ),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}
