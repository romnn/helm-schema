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

/// Rebinding the root evaluates its receiver before a later direct selector lookup.
#[test]
fn rebound_root_selector_preserves_receiver_and_leaf_boundaries() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          original: {{ hasKey $.Values.original "key" | quote }}
          {{- $ = .Values.rebound }}
          rebound: {{ hasKey $.child "key" | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let mut properties = serde_json::Map::new();
    properties.insert(
        "original".to_string(),
        serde_json::json!({ "allOf": [{}, { "type": "object" }] }),
    );
    properties.insert(
        "rebound".to_string(),
        serde_json::json!({
            "additionalProperties": {},
            "properties": { "child": {} },
        }),
    );
    let receiver_is_present = serde_json::json!({ "not": { "anyOf": [
        { "not": {
            "properties": { "rebound": {} },
            "required": ["rebound"],
            "type": "object",
        } },
        {
            "properties": { "rebound": { "enum": [null] } },
            "required": ["rebound"],
            "type": "object",
        },
    ] } });
    let expected = expected_values_schema(
        properties,
        vec![
            serde_json::json!({
                "if": receiver_is_present.clone(),
                "then": { "allOf": [
                    root_property_schema(
                        "rebound",
                        serde_json::json!({
                            "additionalProperties": {},
                            "properties": { "child": { "type": "object" } },
                        }),
                    ),
                    root_property_schema("rebound", serde_json::json!({ "type": "object" })),
                ] },
            }),
            serde_json::json!({
                "if": { "anyOf": [
                    { "not": {
                        "properties": { "original": {} },
                        "required": ["original"],
                        "type": "object",
                    } },
                    {
                        "properties": { "original": { "enum": [null] } },
                        "required": ["original"],
                        "type": "object",
                    },
                ] },
                "then": false,
            }),
            serde_json::json!({
                "if": { "allOf": [
                    { "anyOf": [
                        { "not": {
                            "properties": { "rebound": {
                                "properties": { "child": {} },
                                "required": ["child"],
                                "type": "object",
                            } },
                            "required": ["rebound"],
                            "type": "object",
                        } },
                        {
                            "properties": { "rebound": {
                                "properties": { "child": { "enum": [null] } },
                                "required": ["child"],
                                "type": "object",
                            } },
                            "required": ["rebound"],
                            "type": "object",
                        },
                    ] },
                    receiver_is_present,
                ] },
                "then": false,
            }),
        ],
        false,
    );

    sim_assert_eq!(have: &schema, want: &expected);

    for (instance, want) in [
        (serde_json::json!({}), false),
        (serde_json::json!({ "original": {} }), true),
        (serde_json::json!({ "original": null }), false),
        (serde_json::json!({ "original": "wrong" }), false),
        (serde_json::json!({ "original": {}, "rebound": null }), true),
        (
            serde_json::json!({ "original": {}, "rebound": "wrong" }),
            false,
        ),
        (
            serde_json::json!({ "original": {}, "rebound": { "marker": 1 } }),
            false,
        ),
        (
            serde_json::json!({ "original": {}, "rebound": { "child": null } }),
            false,
        ),
        (
            serde_json::json!({ "original": {}, "rebound": { "child": "wrong" } }),
            false,
        ),
        (
            serde_json::json!({ "original": {}, "rebound": { "child": {} } }),
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

/// A named pipeline binding preserves the same evaluated-receiver boundary as root rebinding.
#[test]
fn named_local_selector_preserves_receiver_and_leaf_boundaries() {
    let src = indoc! {r#"
        {{- $cfg := .Values.cfg }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ hasKey $cfg.db "key" | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let receiver_is_present = serde_json::json!({ "not": { "anyOf": [
        { "not": {
            "properties": { "cfg": {} },
            "required": ["cfg"],
            "type": "object",
        } },
        {
            "properties": { "cfg": { "enum": [null] } },
            "required": ["cfg"],
            "type": "object",
        },
    ] } });
    let expected = expected_values_schema(
        serde_json::Map::from_iter([(
            "cfg".to_string(),
            serde_json::json!({
                "additionalProperties": {},
                "properties": { "db": {} },
            }),
        )]),
        vec![
            serde_json::json!({
                "if": receiver_is_present.clone(),
                "then": { "allOf": [
                    root_property_schema(
                        "cfg",
                        serde_json::json!({
                            "additionalProperties": {},
                            "properties": { "db": { "type": "object" } },
                        }),
                    ),
                    root_property_schema("cfg", serde_json::json!({ "type": "object" })),
                ] },
            }),
            serde_json::json!({
                "if": { "allOf": [
                    { "anyOf": [
                        { "not": {
                            "properties": { "cfg": {
                                "properties": { "db": {} },
                                "required": ["db"],
                                "type": "object",
                            } },
                            "required": ["cfg"],
                            "type": "object",
                        } },
                        {
                            "properties": { "cfg": {
                                "properties": { "db": { "enum": [null] } },
                                "required": ["db"],
                                "type": "object",
                            } },
                            "required": ["cfg"],
                            "type": "object",
                        },
                    ] },
                    receiver_is_present,
                ] },
                "then": false,
            }),
        ],
        false,
    );

    sim_assert_eq!(have: &schema, want: &expected);

    for (instance, want) in [
        (serde_json::json!({}), true),
        (serde_json::json!({ "cfg": null }), true),
        (serde_json::json!({ "cfg": "wrong" }), false),
        (serde_json::json!({ "cfg": {} }), false),
        (serde_json::json!({ "cfg": { "db": null } }), false),
        (serde_json::json!({ "cfg": { "db": "wrong" } }), false),
        (serde_json::json!({ "cfg": { "db": {} } }), true),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

/// A helper's dot is the evaluated call argument, including when that value is nil.
#[test]
fn helper_dot_map_argument_tolerates_nil_but_rejects_concrete_non_maps() {
    let helpers = indoc! {r#"
        {{- define "test.sortedKeys" -}}
        {{- range keys . | sortAlpha -}}
        {{ . }}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          keys: {{ include "test.sortedKeys" .Values.subject | quote }}
    "#};
    let schema = schema_for(parse_ir_with_helpers(src, helpers));
    let expected = expected_values_schema(
        serde_json::Map::from_iter([("subject".to_string(), serde_json::json!({}))]),
        vec![root_property_schema(
            "subject",
            serde_json::json!({ "type": ["null", "object"] }),
        )],
        false,
    );

    sim_assert_eq!(have: &schema, want: &expected);

    for (instance, want) in [
        (serde_json::json!({}), true),
        (serde_json::json!({ "subject": null }), true),
        (serde_json::json!({ "subject": {} }), true),
        (serde_json::json!({ "subject": { "key": "value" } }), true),
        (serde_json::json!({ "subject": "wrong" }), false),
        (serde_json::json!({ "subject": [] }), false),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

#[test]
fn helper_dot_selector_tolerates_nil_receiver_but_not_nil_leaf() {
    let helpers = indoc! {r#"
        {{- define "test.child" -}}
        {{- hasKey .child "key" -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ include "test.child" .Values.subject | quote }}
    "#};
    let schema = schema_for(parse_ir_with_helpers(src, helpers));
    let receiver_is_present = serde_json::json!({ "not": { "anyOf": [
        { "not": {
            "properties": { "subject": {} },
            "required": ["subject"],
            "type": "object",
        } },
        {
            "properties": { "subject": { "enum": [null] } },
            "required": ["subject"],
            "type": "object",
        },
    ] } });
    let expected = expected_values_schema(
        serde_json::Map::from_iter([(
            "subject".to_string(),
            serde_json::json!({
                "additionalProperties": {},
                "properties": { "child": {} },
            }),
        )]),
        vec![
            serde_json::json!({
                "if": receiver_is_present.clone(),
                "then": { "allOf": [
                    root_property_schema(
                        "subject",
                        serde_json::json!({
                            "additionalProperties": {},
                            "properties": { "child": { "type": "object" } },
                        }),
                    ),
                    root_property_schema("subject", serde_json::json!({ "type": "object" })),
                ] },
            }),
            serde_json::json!({
                "if": { "allOf": [
                    { "anyOf": [
                        { "not": {
                            "properties": { "subject": {
                                "properties": { "child": {} },
                                "required": ["child"],
                                "type": "object",
                            } },
                            "required": ["subject"],
                            "type": "object",
                        } },
                        {
                            "properties": { "subject": {
                                "properties": { "child": { "enum": [null] } },
                                "required": ["child"],
                                "type": "object",
                            } },
                            "required": ["subject"],
                            "type": "object",
                        },
                    ] },
                    receiver_is_present,
                ] },
                "then": false,
            }),
        ],
        false,
    );

    sim_assert_eq!(have: &schema, want: &expected);

    for (instance, want) in [
        (serde_json::json!({}), true),
        (serde_json::json!({ "subject": null }), true),
        (serde_json::json!({ "subject": "wrong" }), false),
        (serde_json::json!({ "subject": {} }), false),
        (serde_json::json!({ "subject": { "child": null } }), false),
        (
            serde_json::json!({ "subject": { "child": "wrong" } }),
            false,
        ),
        (serde_json::json!({ "subject": { "child": {} } }), true),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

#[test]
fn exact_list_range_binding_retains_direct_member_access() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $cfg := list .Values.cfg }}
          result: {{ hasKey $cfg.child "key" | quote }}
          {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));
    let absent_or_null_cfg = serde_json::json!({ "anyOf": [
        { "not": {
            "properties": { "cfg": {} },
            "required": ["cfg"],
            "type": "object",
        } },
        {
            "properties": { "cfg": { "enum": [null] } },
            "required": ["cfg"],
            "type": "object",
        },
    ] });
    let absent_or_null_child = serde_json::json!({ "anyOf": [
        { "not": {
            "properties": { "cfg": {
                "properties": { "child": {} },
                "required": ["child"],
                "type": "object",
            } },
            "required": ["cfg"],
            "type": "object",
        } },
        {
            "properties": { "cfg": {
                "properties": { "child": { "enum": [null] } },
                "required": ["child"],
                "type": "object",
            } },
            "required": ["cfg"],
            "type": "object",
        },
    ] });
    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            { "if": absent_or_null_cfg.clone(), "then": false },
            {
                "if": { "allOf": [absent_or_null_child, absent_or_null_cfg] },
                "then": false,
            },
        ],
        "properties": {
            "cfg": {
                "additionalProperties": {},
                "allOf": [{
                    "if": { "anyOf": [
                        { "not": {
                            "properties": { "child": {} },
                            "required": ["child"],
                            "type": "object",
                        } },
                        {
                            "properties": { "child": { "enum": [null] } },
                            "required": ["child"],
                            "type": "object",
                        },
                    ] },
                    "then": false,
                }],
                "properties": {
                    "child": { "allOf": [{}, { "type": "object" }] },
                },
                "type": "object",
            },
        },
        "type": "object",
    });

    sim_assert_eq!(have: &schema, want: &expected);

    for (instance, want) in [
        (serde_json::json!({}), false),
        (serde_json::json!({ "cfg": null }), false),
        (serde_json::json!({ "cfg": "wrong" }), false),
        (serde_json::json!({ "cfg": {} }), false),
        (serde_json::json!({ "cfg": { "child": null } }), false),
        (serde_json::json!({ "cfg": { "child": "wrong" } }), false),
        (serde_json::json!({ "cfg": { "child": {} } }), true),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

#[test]
fn helper_range_binding_retains_direct_member_access() {
    let helpers = indoc! {r#"
        {{- define "test.items" -}}
        {{- range $cfg := .Values.items -}}
        {{- hasKey $cfg.child "key" | quote -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ include "test.items" . }}
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some("items: []\n"));
    let expected = schema_for_values_yaml(
        parse_ir(indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: test
            data:
              {{- range $cfg := .Values.items }}
              result: {{ hasKey $cfg.child "key" | quote }}
              {{- end }}
        "#}),
        Some("items: []\n"),
    );

    sim_assert_eq!(have: &schema, want: &expected);

    for (instance, want) in [
        (serde_json::json!({}), true),
        (serde_json::json!({ "items": null }), true),
        (serde_json::json!({ "items": [] }), true),
        (serde_json::json!({ "items": [null] }), false),
        (serde_json::json!({ "items": [{}] }), false),
        (serde_json::json!({ "items": [{ "child": null }] }), false),
        (
            serde_json::json!({ "items": [{ "child": "wrong" }] }),
            false,
        ),
        (serde_json::json!({ "items": [{ "child": {} }] }), true),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

#[test]
fn branch_root_reassignment_joins_value_and_mode_per_arm() {
    let src = indoc! {r#"
        {{- if .Values.rebind }}
        {{- $ = .Values.probe }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ hasKey $.child "key" | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let rebind_is_truthy = serde_json::json!({
        "properties": { "rebind": { "$ref": "#/$defs/t" } },
        "required": ["rebind"],
        "type": "object",
    });
    let receiver_is_present = serde_json::json!({ "not": { "anyOf": [
        { "not": {
            "properties": { "probe": {} },
            "required": ["probe"],
            "type": "object",
        } },
        {
            "properties": { "probe": { "enum": [null] } },
            "required": ["probe"],
            "type": "object",
        },
    ] } });
    let expected = expected_values_schema(
        serde_json::Map::from_iter([
            (
                "probe".to_string(),
                serde_json::json!({
                    "additionalProperties": {},
                    "properties": { "child": {} },
                }),
            ),
            ("rebind".to_string(), serde_json::json!({})),
        ]),
        vec![
            serde_json::json!({
                "if": { "allOf": [rebind_is_truthy.clone(), receiver_is_present.clone()] },
                "then": { "allOf": [
                    root_property_schema(
                        "probe",
                        serde_json::json!({
                            "additionalProperties": {},
                            "properties": { "child": { "type": "object" } },
                        }),
                    ),
                    root_property_schema("probe", serde_json::json!({ "type": "object" })),
                ] },
            }),
            serde_json::json!({
                "if": { "allOf": [
                    rebind_is_truthy,
                    { "anyOf": [
                        { "not": {
                            "properties": { "probe": {
                                "properties": { "child": {} },
                                "required": ["child"],
                                "type": "object",
                            } },
                            "required": ["probe"],
                            "type": "object",
                        } },
                        {
                            "properties": { "probe": {
                                "properties": { "child": { "enum": [null] } },
                                "required": ["child"],
                                "type": "object",
                            } },
                            "required": ["probe"],
                            "type": "object",
                        },
                    ] },
                    receiver_is_present,
                ] },
                "then": false,
            }),
        ],
        true,
    );

    sim_assert_eq!(have: &schema, want: &expected);

    for (instance, want) in [
        (serde_json::json!({ "rebind": true }), true),
        (serde_json::json!({ "rebind": true, "probe": null }), true),
        (
            serde_json::json!({ "rebind": true, "probe": "wrong" }),
            false,
        ),
        (serde_json::json!({ "rebind": true, "probe": {} }), false),
        (
            serde_json::json!({ "rebind": true, "probe": { "child": null } }),
            false,
        ),
        (
            serde_json::json!({ "rebind": true, "probe": { "child": "wrong" } }),
            false,
        ),
        (
            serde_json::json!({ "rebind": true, "probe": { "child": {} } }),
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

#[test]
fn with_join_keeps_direct_range_member_conditional_on_fallthrough() {
    let helpers = indoc! {r#"
        {{- define "test.itemsWithOverride" -}}
        {{- range $cfg := .Values.items -}}
        {{- with $.Values.override -}}
        {{- $cfg = . -}}
        {{- end -}}
        {{- keys $cfg | join "," -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ include "test.itemsWithOverride" . | quote }}
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some("items: []\n"));
    let explicit_helpers = indoc! {r#"
        {{- define "test.itemsWithoutJoin" -}}
        {{- range $cfg := .Values.items -}}
        {{- if $.Values.override -}}
        {{- keys ($.Values.override) | join "," -}}
        {{- else -}}
        {{- keys $cfg | join "," -}}
        {{- end -}}
        {{- end -}}
        {{- end -}}
    "#};
    let explicit_src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ include "test.itemsWithoutJoin" . | quote }}
    "#};
    let expected = schema_for_values_yaml(
        parse_ir_with_helpers(explicit_src, explicit_helpers),
        Some("items: []\n"),
    );

    sim_assert_eq!(have: &schema, want: &expected);

    for (instance, want) in [
        (
            serde_json::json!({ "items": [null], "override": { "a": 1 } }),
            true,
        ),
        (serde_json::json!({ "items": [null] }), false),
        (
            serde_json::json!({ "items": [{}], "override": { "a": 1 } }),
            true,
        ),
        (serde_json::json!({ "items": [{}] }), true),
        (
            serde_json::json!({ "items": [{}], "override": "wrong" }),
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

#[test]
fn pristine_root_values_selector_keeps_host_contract_in_document_and_helper() {
    let document = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ $.Values.a.b | quote }}
    "#};
    let helpers = indoc! {r#"
        {{- define "test.rootValue" -}}
        {{- $.Values.a.b | quote -}}
        {{- end -}}
    "#};
    let helper = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ include "test.rootValue" . }}
    "#};
    let document_schema = schema_for(parse_ir(document));
    let helper_schema = schema_for(parse_ir_with_helpers(helper, helpers));

    sim_assert_eq!(have: &helper_schema, want: &document_schema);
    for (instance, want) in [
        (serde_json::json!({}), false),
        (serde_json::json!({ "a": null }), false),
        (serde_json::json!({ "a": "wrong" }), false),
        (serde_json::json!({ "a": {} }), true),
        (serde_json::json!({ "a": { "b": null } }), true),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&document_schema, &instance),
            want: want,
            "document instance={instance}; schema={document_schema}",
        );
        sim_assert_eq!(
            have: schema_accepts_instance(&helper_schema, &instance),
            want: want,
            "helper instance={instance}; schema={helper_schema}",
        );
    }
}

#[test]
fn exact_range_assignment_keeps_the_last_written_direct_member() {
    let empty = indoc! {r#"
        {{- $cfg := dict -}}
        {{- range $cfg = list -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ keys $cfg | join "," | quote }}
    "#};
    let empty_reference = indoc! {r#"
        {{- $cfg := list -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ keys $cfg | join "," | quote }}
    "#};
    let empty_schema = schema_for(parse_ir(empty));
    let empty_expected = schema_for(parse_ir(empty_reference));
    sim_assert_eq!(have: &empty_schema, want: &empty_expected);

    let one = indoc! {r#"
        {{- $cfg := dict -}}
        {{- range $cfg = list .Values.only -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ keys $cfg | join "," | quote }}
    "#};
    let one_reference = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ keys .Values.only | join "," | quote }}
    "#};
    let one_schema = schema_for(parse_ir(one));
    let one_expected = schema_for(parse_ir(one_reference));
    sim_assert_eq!(have: &one_schema, want: &one_expected);
    for (instance, want) in [
        (serde_json::json!({ "only": {} }), true),
        (serde_json::json!({ "only": "wrong" }), false),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&one_schema, &instance),
            want: want,
            "one-item instance={instance}; schema={one_schema}",
        );
    }

    let src = indoc! {r#"
        {{- $cfg := dict -}}
        {{- range $cfg = list .Values.first .Values.last -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ keys $cfg | join "," | quote }}
    "#};
    let direct = indoc! {r#"
        {{- $ignored := .Values.first -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ keys .Values.last | join "," | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(direct));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({ "first": "wrong", "last": {} }), true),
        (serde_json::json!({ "first": {}, "last": "wrong" }), false),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

#[test]
fn symbolic_range_exit_keeps_exact_zero_and_widens_only_the_changed_binding() {
    let src = indoc! {r#"
        {{- range $stable := list .Values.stable -}}
        {{- $cfg := $.Values.seed -}}
        {{- range $cfg = $.Values.items -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          cfg: {{ keys $cfg | join "," | quote }}
          stable: {{ keys $stable | join "," | quote }}
        {{- end -}}
    "#};
    let reference = indoc! {r#"
        {{- range $stable := list .Values.stable -}}
        {{- $ignored := $.Values.seed -}}
        {{- range $.Values.items -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if not $.Values.items }}
          {{- $empty := $.Values.items }}
          cfg: {{ keys $empty | join "," | quote }}
          {{- end }}
          stable: {{ keys $stable | join "," | quote }}
        {{- end -}}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    for (instance, want) in [
        (serde_json::json!({ "items": {}, "stable": {} }), true),
        (serde_json::json!({ "items": [], "stable": {} }), false),
        (serde_json::json!({ "items": [null], "stable": {} }), true),
        (
            serde_json::json!({ "items": [null, {}], "stable": {} }),
            true,
        ),
        (serde_json::json!({ "items": {}, "stable": null }), false),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
    sim_assert_eq!(have: &schema, want: &expected);
}

#[test]
fn conditional_local_string_consumer_uses_only_the_selected_binding_leaf() {
    let src = indoc! {r#"
        {{- $value := .Values.a -}}
        {{- if .Values.useB -}}
        {{- $value = .Values.b -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          encoded: {{ b64enc $value | quote }}
    "#};
    let reference = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.useB }}
          encoded: {{ b64enc .Values.b | quote }}
          {{- else }}
          encoded: {{ b64enc .Values.a | quote }}
          {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({ "useB": true, "a": { "k": 1 }, "b": "beta" }),
            true,
        ),
        (
            serde_json::json!({ "useB": true, "a": "alpha", "b": { "k": 1 } }),
            false,
        ),
        (
            serde_json::json!({ "useB": false, "a": "alpha", "b": { "k": 1 } }),
            true,
        ),
        (
            serde_json::json!({ "useB": false, "a": { "k": 1 }, "b": "beta" }),
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

/// A strict consumer preserves presence for every selected `and` operand.
#[test]
fn selected_and_operand_keeps_its_strict_nil_boundary() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ and .Values.left .Values.right | ternary "yes" "no" | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let mut properties = serde_json::Map::new();
    properties.insert("left".to_string(), serde_json::json!({}));
    properties.insert("right".to_string(), serde_json::json!({}));
    let left_truthy = helm_truthy_guard("left");
    let expected = expected_values_schema(
        properties,
        vec![
            serde_json::json!({
                "if": { "not": left_truthy.clone() },
                "then": root_property_schema(
                    "left",
                    serde_json::json!({ "type": "boolean" }),
                ),
            }),
            serde_json::json!({
                "if": left_truthy.clone(),
                "then": root_property_schema(
                    "right",
                    serde_json::json!({ "type": "boolean" }),
                ),
            }),
            serde_json::json!({
                "if": { "allOf": [
                    left_truthy,
                    { "anyOf": [
                        { "not": {
                            "properties": { "right": {} },
                            "required": ["right"],
                            "type": "object",
                        } },
                        {
                            "properties": { "right": { "enum": [null] } },
                            "required": ["right"],
                            "type": "object",
                        },
                    ] },
                ] },
                "then": false,
            }),
            serde_json::json!({
                "if": { "anyOf": [
                    { "not": {
                        "properties": { "left": {} },
                        "required": ["left"],
                        "type": "object",
                    } },
                    {
                        "properties": { "left": { "enum": [null] } },
                        "required": ["left"],
                        "type": "object",
                    },
                ] },
                "then": false,
            }),
        ],
        true,
    );

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({}), false),
        (serde_json::json!({ "left": null }), false),
        (serde_json::json!({ "left": false }), true),
        (serde_json::json!({ "left": true }), false),
        (serde_json::json!({ "left": true, "right": null }), false),
        (serde_json::json!({ "left": true, "right": false }), true),
        (serde_json::json!({ "left": "truthy", "right": true }), true),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

/// An outer short circuit cannot hide a strict nil boundary in its else arm.
#[test]
fn strict_operand_in_short_circuit_else_keeps_its_nil_boundary() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: '{{- if and .Values.subject (not (empty .Values.other)) -}}
                     fixed
                   {{- else -}}
                     {{ .Values.subject | ternary "yes" "no" }}
                   {{- end }}'
    "#};
    let schema = schema_for(parse_ir(src));
    let mut properties = serde_json::Map::new();
    properties.insert("other".to_string(), serde_json::json!({}));
    properties.insert("subject".to_string(), serde_json::json!({}));
    let expected = expected_values_schema(
        properties,
        vec![
            serde_json::json!({
                "if": { "not": { "allOf": [
                    helm_truthy_guard("other"),
                    helm_truthy_guard("subject"),
                ] } },
                "then": root_property_schema(
                    "subject",
                    serde_json::json!({ "type": "boolean" }),
                ),
            }),
            serde_json::json!({
                "if": { "anyOf": [
                    { "not": {
                        "properties": { "subject": {} },
                        "required": ["subject"],
                        "type": "object",
                    } },
                    {
                        "properties": { "subject": { "enum": [null] } },
                        "required": ["subject"],
                        "type": "object",
                    },
                ] },
                "then": false,
            }),
        ],
        true,
    );

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({}), false),
        (serde_json::json!({ "subject": null }), false),
        (serde_json::json!({ "subject": false }), true),
        (
            serde_json::json!({ "subject": "truthy", "other": false }),
            false,
        ),
        (
            serde_json::json!({ "subject": "truthy", "other": true }),
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

/// A helper summary preserves the strict nil boundary from its selected else arm.
#[test]
fn helper_short_circuit_else_keeps_its_strict_nil_boundary() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ include "selected.strict" . | quote }}
    "#};
    let helpers = indoc! {r#"
        {{- define "selected.strict" -}}
        {{- if and .Values.subject (not (empty .Values.other)) -}}
        {{ (default false .Values.demo) | ternary true ((and .Values.authEnabled (or (eq .Values.authType "oidc") (eq .Values.authType "dex"))) | ternary true false) }}
        {{- else -}}
        {{ .Values.subject | ternary "true" ((default false .Values.demo) | ternary "true" .Values.authEnabled) }}
        {{- end -}}
        {{- end -}}
    "#};
    let inline = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: '{{- if and .Values.subject (not (empty .Values.other)) -}}
                     {{ (default false .Values.demo) | ternary true ((and .Values.authEnabled (or (eq .Values.authType "oidc") (eq .Values.authType "dex"))) | ternary true false) }}
                   {{- else -}}
                     {{ .Values.subject | ternary "true" ((default false .Values.demo) | ternary "true" .Values.authEnabled) }}
                   {{- end }}'
    "#};
    let schema = schema_for(parse_ir_with_helpers(src, helpers));
    let expected = schema_for(parse_ir(inline));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({}), false),
        (serde_json::json!({ "subject": null }), false),
        (serde_json::json!({ "subject": false }), true),
        (
            serde_json::json!({ "subject": "truthy", "other": false }),
            false,
        ),
        (
            serde_json::json!({ "subject": "truthy", "other": true }),
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

/// `tpl` constrains only the local value selected by its binding decision.
#[test]
fn conditional_local_tpl_uses_only_the_selected_binding_leaf() {
    let src = indoc! {r#"
        {{- $value := .Values.a -}}
        {{- if .Values.useB -}}
        {{- $value = .Values.b -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          rendered: {{ tpl $value . | quote }}
    "#};
    let reference = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.useB }}
          rendered: {{ tpl .Values.b . | quote }}
          {{- else }}
          rendered: {{ tpl .Values.a . | quote }}
          {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);

    for (instance, want) in [
        (
            serde_json::json!({ "useB": true, "a": {}, "b": "beta" }),
            true,
        ),
        (
            serde_json::json!({ "useB": true, "a": "alpha", "b": {} }),
            false,
        ),
        (
            serde_json::json!({ "useB": false, "a": "alpha", "b": {} }),
            true,
        ),
        (
            serde_json::json!({ "useB": false, "a": {}, "b": "beta" }),
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

/// Escaped arm nodes remain beside an output-empty control in a shared container.
#[test]
fn deferred_nested_tpl_retains_its_immediate_truthiness_gate() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          nested.yaml: |-
            {{- include "nested.deferred" . | nindent 4 }}
    "#};
    let helpers = indoc! {r#"
        {{- define "nested.deferred" -}}
        env:
        - envVar:
        {{- if .Values.direct }}
            key: DIRECT
        {{- else }}
            key: {{ required "key is required" .Values.key }}
        {{- if .Values.url }}
            value: {{ tpl .Values.url . }}
        {{- else }}
            value: fallback
        {{- end }}
            after: fixed
        {{- end }}
        {{- end -}}
    "#};
    let schema = schema_for(parse_ir_with_helpers(src, helpers));
    let mut properties = serde_json::Map::new();
    for path in ["direct", "key", "url"] {
        properties.insert(path.to_string(), serde_json::json!({}));
    }
    let missing_key = navigated_host_condition(&["key"]);
    let expected = expected_values_schema(
        properties,
        vec![
            serde_json::json!({
                "if": { "allOf": [
                    helm_truthy_guard("url"),
                    { "not": helm_truthy_guard("direct") },
                ] },
                "then": { "allOf": [
                    root_property_schema("url", serde_json::json!({
                        "anyOf": [
                            {
                                "allOf": plain_token_exclusions(true),
                                "type": "string",
                            },
                            { "not": { "type": "string" } },
                            { "pattern": "\\{\\{", "type": "string" },
                        ],
                    })),
                    root_property_schema(
                        "url",
                        serde_json::json!({ "type": ["null", "string"] }),
                    ),
                ] },
            }),
            serde_json::json!({
                "if": { "allOf": [
                    { "not": helm_truthy_guard("direct") },
                    { "anyOf": [
                        {
                            "properties": { "key": { "enum": [""] } },
                            "required": ["key"],
                            "type": "object",
                        },
                        missing_key.clone(),
                        missing_key,
                    ] },
                ] },
                "then": false,
            }),
        ],
        true,
    );

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({ "direct": false, "key": "URL" }), true),
        (serde_json::json!({ "direct": false }), false),
        (
            serde_json::json!({ "direct": false, "key": "URL", "url": "value" }),
            true,
        ),
        (
            serde_json::json!({ "direct": false, "key": "URL", "url": {} }),
            true,
        ),
        (
            serde_json::json!({ "direct": false, "key": "URL", "url": { "x": 1 } }),
            false,
        ),
        (
            serde_json::json!({ "direct": true, "url": { "x": 1 } }),
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

/// A known non-empty opaque result replaces the local's prior value.
#[test]
fn nonempty_opaque_reassignment_does_not_make_a_later_fail_unconditional() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          generated: {{ include "unknown.reassignment" . | quote }}
    "#};
    let helpers = indoc! {r#"
        {{- define "unknown.reassignment" -}}
        {{- $value := "" -}}
        {{- $value = uuidv4 -}}
        {{- if not $value -}}
        {{- fail "uuidv4 returned an empty value" -}}
        {{- end -}}
        {{- $value -}}
        {{- end -}}
    "#};
    let schema = schema_for(parse_ir_with_helpers(src, helpers));
    let expected = expected_values_schema(serde_json::Map::new(), Vec::new(), false);

    sim_assert_eq!(have: &schema, want: &expected);
    sim_assert_eq!(have: schema_accepts_instance(&schema, &serde_json::json!({})), want: true);
}

/// An unresolved range reassignment keeps its unknown alternative at the next iteration.
#[test]
fn unresolved_range_reassignment_does_not_select_one_known_operand() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          password: {{ include "lookup.value" (dict "key" "auth.password" "context" $) | quote }}
    "#};
    let helpers = indoc! {r#"
        {{- define "lookup.value" -}}
        {{- $segments := splitList "." .key -}}
        {{- $value := "" -}}
        {{- $latest := $.context.Values -}}
        {{- range $segments -}}
        {{- if not $latest -}}
        {{- fail "path does not exist" -}}
        {{- end -}}
        {{- $value = index $latest . -}}
        {{- $latest = $value -}}
        {{- end -}}
        {{- printf "%v" (default "" $value) -}}
        {{- end -}}
    "#};
    let reference_helpers = indoc! {r#"
        {{- define "lookup.value" -}}
        {{- $value := index $.context.Values.auth "password" -}}
        {{- if not $.context.Values.auth -}}
        {{- fail "path does not exist" -}}
        {{- end -}}
        {{- printf "%v" (default "" $value) -}}
        {{- end -}}
    "#};
    let schema = schema_for(parse_ir_with_helpers(src, helpers));
    let expected = schema_for(parse_ir_with_helpers(src, reference_helpers));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({}), false),
        (serde_json::json!({ "auth": {} }), false),
        (serde_json::json!({ "auth": { "password": "" } }), true),
        (serde_json::json!({ "auth": { "password": null } }), true),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

/// An output-empty control scopes sibling entries inside its mapping parent.
#[test]
fn empty_control_inside_mapping_keeps_required_in_its_true_arm() {
    let src = indoc! {r#"
        {{- if .Values.enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          before: fixed
          {{- if .Values.force }}
          password: {{ required "password is required" .Values.password | quote }}
          {{- else }}
          password: generated
          {{- end }}
          after: fixed
        {{- end }}
    "#};
    let reference = indoc! {r#"
        {{- if and .Values.enabled .Values.force }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          before: fixed
          password: {{ required "password is required" .Values.password | quote }}
          after: fixed
        {{- else if .Values.enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          before: fixed
          password: generated
          after: fixed
        {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({ "enabled": false, "force": true }), true),
        (serde_json::json!({ "enabled": true, "force": false }), true),
        (serde_json::json!({ "enabled": true, "force": true }), false),
        (
            serde_json::json!({ "enabled": true, "force": true, "password": null }),
            false,
        ),
        (
            serde_json::json!({ "enabled": true, "force": true, "password": "" }),
            false,
        ),
        (
            serde_json::json!({ "enabled": true, "force": true, "password": "secret" }),
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

/// Specialized string consumers preserve the selected local leaf.
#[test]
fn conditional_local_specialized_string_consumers_use_proven_leaves() {
    for expression in [
        "$value | sha256sum",
        "splitList \",\" $value | first",
        "$value | split \",\" | get \"_0\"",
        "$value | contains \"x\"",
        "trimPrefix \"x\" $value",
        "$value | fromYaml | toJson",
        "printf $value \"x\"",
    ] {
        let src = indoc! {r#"
            {{- $value := .Values.a -}}
            {{- if .Values.useB -}}
            {{- $value = .Values.b -}}
            {{- end -}}
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: test
            data:
              rendered: {{ __EXPRESSION__ | quote }}
        "#}
        .replace("__EXPRESSION__", expression);
        let reference = indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: test
            data:
              {{- if .Values.useB }}
              rendered: {{ __TRUE_EXPRESSION__ | quote }}
              {{- else }}
              rendered: {{ __FALSE_EXPRESSION__ | quote }}
              {{- end }}
        "#}
        .replace(
            "__TRUE_EXPRESSION__",
            &expression.replace("$value", ".Values.b"),
        )
        .replace(
            "__FALSE_EXPRESSION__",
            &expression.replace("$value", ".Values.a"),
        );

        sim_assert_eq!(
            have: schema_for(parse_ir(&src)),
            want: schema_for(parse_ir(&reference)),
            "expression={expression}",
        );
    }
}

/// A selected trim affix preserves the chosen local leaf's string contract.
#[test]
fn conditional_local_trim_affix_uses_proven_leaves() {
    let src = indoc! {r#"
        {{- $value := .Values.a -}}
        {{- if .Values.useB -}}
        {{- $value = .Values.b -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          rendered: {{ trimPrefix $value .Values.subject | quote }}
    "#};
    let reference = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          subject: {{ trim .Values.subject | quote }}
          {{- if .Values.useB }}
          affix: {{ trim .Values.b | quote }}
          {{- else }}
          affix: {{ trim .Values.a | quote }}
          {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));

    sim_assert_eq!(have: &schema, want: &schema_for(parse_ir(&reference)));

    for (instance, want) in [
        (
            serde_json::json!({ "useB": true, "a": {}, "b": "-ok", "subject": "-value" }),
            true,
        ),
        (
            serde_json::json!({ "useB": true, "a": "-ok", "b": {}, "subject": "-value" }),
            false,
        ),
        (
            serde_json::json!({ "useB": false, "a": "-ok", "b": {}, "subject": "-value" }),
            true,
        ),
        (
            serde_json::json!({ "useB": false, "a": {}, "b": "-ok", "subject": "-value" }),
            false,
        ),
        (
            serde_json::json!({ "useB": true, "a": "-ok", "b": "-ok", "subject": {} }),
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

#[test]
fn conditional_local_default_metadata_stays_owned_by_the_selected_leaf() {
    let src = indoc! {r#"
        {{- $value := dict -}}
        {{- if .Values.chooseP -}}
        {{- $value = .Values.p | default .Values.q -}}
        {{- else -}}
        {{- $value = .Values.q | default .Values.p -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          selected: {{ keys $value | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.chooseP }}
          selected: {{ keys (.Values.p | default .Values.q) | join "," | quote }}
          {{- else }}
          selected: {{ keys (.Values.q | default .Values.p) | join "," | quote }}
          {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({ "chooseP": true, "p": { "k": 1 }, "q": "fallback" }),
            true,
        ),
        (
            serde_json::json!({ "chooseP": false, "p": "fallback", "q": { "k": 1 } }),
            true,
        ),
        (
            serde_json::json!({ "chooseP": true, "p": "wrong", "q": { "k": 1 } }),
            false,
        ),
        (
            serde_json::json!({ "chooseP": false, "p": { "k": 1 }, "q": "wrong" }),
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

#[test]
fn with_header_declaration_shadows_only_inside_the_region() {
    let src = indoc! {r#"
        {{- $cfg := .Values.a -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- with $cfg := .Values.b }}
          inside: {{ . | quote }}
          {{- else }}
          inside: {{ keys $cfg | join "," | quote }}
          {{- end }}
          result: {{ keys $cfg | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        {{- $cfg := .Values.a -}}
        {{- $header := .Values.b -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- with $header }}
          inside: {{ . | quote }}
          {{- else }}
          inside: {{ keys $header | join "," | quote }}
          {{- end }}
          result: {{ keys $cfg | join "," | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({ "a": {}, "b": "inside" }), true),
        (serde_json::json!({ "a": {}, "b": {} }), true),
        (serde_json::json!({ "a": {}, "b": false }), false),
        (serde_json::json!({ "a": "wrong", "b": "inside" }), false),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

#[test]
fn if_header_declaration_shadows_both_arms_and_restores_the_outer_slot() {
    let src = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if $cfg := .Values.after }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- else }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        {{- $cfg := .Values.before -}}
        {{- $header := .Values.after -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if $header }}
          selected: {{ keys $header | join "," | quote }}
          {{- else }}
          selected: {{ keys $header | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({ "before": "wrong", "after": {} }), false),
        (serde_json::json!({ "before": {}, "after": "wrong" }), false),
        (serde_json::json!({ "before": {}, "after": {} }), true),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

#[test]
fn if_header_assignment_updates_the_slot_before_both_arms() {
    let src = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if $cfg = .Values.after }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- else }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        {{- $unused := .Values.before -}}
        {{- $cfg := .Values.after -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if $cfg }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- else }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({ "before": "wrong", "after": {} }), true),
        (serde_json::json!({ "before": {}, "after": "wrong" }), false),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

#[test]
fn if_header_assignment_persists_when_a_falsy_value_skips_the_only_arm() {
    let src = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if $cfg = .Values.after }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        {{- $unused := .Values.before -}}
        {{- $cfg := .Values.after -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if $cfg }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({ "before": "wrong", "after": {} }), true),
        (serde_json::json!({ "before": {}, "after": "wrong" }), false),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

#[test]
fn with_header_assignment_updates_the_outer_slot_before_both_arms() {
    let src = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- with $cfg = .Values.override }}
          inside: {{ keys $cfg | join "," | quote }}
          {{- else }}
          inside: {{ keys $cfg | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        {{- $unused := .Values.before -}}
        {{- $cfg := .Values.override -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- with $cfg }}
          inside: {{ keys $cfg | join "," | quote }}
          {{- else }}
          inside: {{ keys $cfg | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({ "before": "wrong", "override": {} }),
            true,
        ),
        (
            serde_json::json!({ "before": {}, "override": "wrong" }),
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

#[test]
fn with_header_assignment_persists_when_a_falsy_value_skips_the_only_arm() {
    let src = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- with $cfg = .Values.after }}
          inside: {{ keys $cfg | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        {{- $unused := .Values.before -}}
        {{- $cfg := .Values.after -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- with $cfg }}
          inside: {{ keys $cfg | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({ "before": "wrong", "after": {} }), true),
        (serde_json::json!({ "before": {}, "after": "wrong" }), false),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

#[test]
fn if_header_assignment_updates_scalar_dispatch_on_the_falsy_fallthrough() {
    let src = indoc! {r#"
        {{- $mode := "a" -}}
        {{- if $mode = .Values.mode }}{{- end -}}
        {{- if eq $mode "a" }}{{- fail "a is reserved" }}{{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
    "#};
    let reference = indoc! {r#"
        {{- $mode := .Values.mode -}}
        {{- if eq $mode "a" }}{{- fail "a is reserved" }}{{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({ "mode": "" }), true),
        (serde_json::json!({ "mode": "b" }), true),
        (serde_json::json!({ "mode": "a" }), false),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
}

#[test]
fn if_header_assignment_updates_truthiness_on_the_falsy_fallthrough() {
    let src = indoc! {r#"
        {{- $flag := true -}}
        {{- if $flag = .Values.flag }}{{- $flag = false }}{{- end -}}
        {{- if $flag }}{{- fail "flag remained truthy" }}{{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
    "#};
    let reference = indoc! {r#"
        {{- $flag := .Values.flag -}}
        {{- if $flag }}{{- $flag = false }}{{- end -}}
        {{- if $flag }}{{- fail "flag remained truthy" }}{{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for instance in [
        serde_json::json!({}),
        serde_json::json!({ "flag": false }),
        serde_json::json!({ "flag": true }),
        serde_json::json!({ "flag": "truthy" }),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: true,
            "instance={instance}; schema={schema}",
        );
    }
}

#[test]
fn later_if_header_assignment_updates_the_final_else_and_post_region_state() {
    let src = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.first }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- else if $cfg = .Values.after }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- else }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.first }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- else }}
          {{- $cfg = .Values.after }}
          {{- if $cfg }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- else }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- end }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({ "first": true, "before": {}, "after": "wrong" }),
            true,
        ),
        (
            serde_json::json!({ "first": true, "before": "wrong", "after": {} }),
            false,
        ),
        (
            serde_json::json!({ "first": false, "before": "wrong", "after": {} }),
            true,
        ),
        (
            serde_json::json!({ "first": false, "before": {}, "after": "wrong" }),
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

#[test]
fn later_if_header_declaration_reaches_its_else_and_restores_after_the_region() {
    let src = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.first }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- else if $cfg := .Values.after }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- else }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = serde_json::json!({
        "$defs": {
            "t": {
                "anyOf": [
                    { "const": true },
                    { "not": { "const": 0 }, "type": "number" },
                    { "minLength": 1, "type": "string" },
                    { "minItems": 1, "type": "array" },
                    { "minProperties": 1, "type": "object" }
                ]
            }
        },
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "if": {
                    "not": {
                        "properties": { "first": { "$ref": "#/$defs/t" } },
                        "required": ["first"],
                        "type": "object"
                    }
                },
                "then": {
                    "additionalProperties": {},
                    "properties": { "after": { "type": ["null", "object"] } }
                }
            },
            {
                "additionalProperties": {},
                "properties": { "before": { "type": ["null", "object"] } }
            }
        ],
        "properties": { "after": {}, "before": {}, "first": {} },
        "type": "object"
    });

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({ "first": true, "before": {}, "after": "wrong" }),
            true,
        ),
        (
            serde_json::json!({ "first": false, "before": {}, "after": {} }),
            true,
        ),
        (
            serde_json::json!({ "first": false, "before": {}, "after": "wrong" }),
            false,
        ),
        (
            serde_json::json!({ "first": false, "before": "wrong", "after": {} }),
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

#[test]
fn conditioned_binding_survives_a_with_header_assignment() {
    let src = indoc! {r#"
        {{- $chosen := .Values.a -}}
        {{- if .Values.useB -}}
        {{- $chosen = .Values.b -}}
        {{- end -}}
        {{- $slot := dict -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- with $slot = $chosen }}
          selected: {{ hasKey $slot "key" | quote }}
          {{- if not (hasKey $slot "key") }}{{ fail "key is required" }}{{ end }}
          {{- end }}
    "#};
    let reference = indoc! {r#"
        {{- $chosen := .Values.a -}}
        {{- if .Values.useB -}}
        {{- $chosen = .Values.b -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- with $chosen }}
          selected: {{ hasKey $chosen "key" | quote }}
          {{- if not (hasKey $chosen "key") }}{{ fail "key is required" }}{{ end }}
          {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({ "useB": false, "a": { "key": 1 }, "b": "wrong" }),
            true,
        ),
        (
            serde_json::json!({ "useB": false, "a": { "other": 1 }, "b": { "key": 1 } }),
            false,
        ),
        (
            serde_json::json!({ "useB": true, "a": "wrong", "b": { "key": 1 } }),
            true,
        ),
        (
            serde_json::json!({ "useB": true, "a": { "key": 1 }, "b": { "other": 1 } }),
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

#[test]
fn range_header_declaration_binds_the_iterable_in_else_and_restores_the_outer_slot() {
    let src = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $cfg := .Values.items }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- else }}
          selected: {{ keys $cfg | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        {{- $cfg := .Values.before -}}
        {{- $iterable := .Values.items -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $item := .Values.items }}
          selected: {{ keys $item | join "," | quote }}
          {{- else }}
          selected: {{ keys $iterable | join "," | quote }}
          {{- end }}
          after: {{ keys $cfg | join "," | quote }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    for (instance, want) in [
        (serde_json::json!({ "before": {}, "items": [] }), false),
        (serde_json::json!({ "before": {}, "items": [{}] }), true),
        (
            serde_json::json!({ "before": "wrong", "items": [{}] }),
            false,
        ),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }
    sim_assert_eq!(have: &schema, want: &expected);
}

#[test]
fn approximate_break_keeps_later_exact_and_symbolic_range_consumers_guarded() {
    let exact = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
          annotations:
            inputs: {{ printf "%v:%v" .Values.first .Values.last | quote }}
        spec:
          containers:
          - name: main
            image: busybox
        {{- range $item := list .Values.first .Values.last }}
        {{- if eq (tpl $.Values.stop $) "yes" }}{{- break }}{{- end }}
          - name: {{ required "name is required" $item.name }}
            image: busybox
        {{- end }}
    "#};
    let exact_reference = exact.replace(r#"required "name is required" $item.name"#, r#""fixed""#);
    let exact_schema = schema_for(parse_ir(exact));
    let exact_expected = schema_for(parse_ir(&exact_reference));

    sim_assert_eq!(have: &exact_schema, want: &exact_expected);
    sim_assert_eq!(
        have: schema_accepts_instance(
            &exact_schema,
            &serde_json::json!({ "stop": "yes", "first": {}, "last": {} }),
        ),
        want: true,
        "schema={exact_schema}",
    );

    let symbolic = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
          - name: main
            image: busybox
        {{- range $item := .Values.items }}
        {{- if eq (tpl $.Values.stop $) "yes" }}{{- break }}{{- end }}
          - name: {{ required "name is required" $item.name }}
            image: busybox
        {{- end }}
    "#};
    let symbolic_reference =
        symbolic.replace(r#"required "name is required" $item.name"#, r#""fixed""#);
    let symbolic_schema = schema_for(parse_ir(symbolic));
    let symbolic_expected = schema_for(parse_ir(&symbolic_reference));

    sim_assert_eq!(have: &symbolic_schema, want: &symbolic_expected);
    sim_assert_eq!(
        have: schema_accepts_instance(
            &symbolic_schema,
            &serde_json::json!({ "stop": "yes", "items": [{}] }),
        ),
        want: true,
        "schema={symbolic_schema}",
    );
}

/// Escaped and direct branch segments retain their shared source order.
#[test]
fn escaped_batch_and_direct_nodes_keep_assignment_order() {
    let src = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          parent:
        {{- if .Values.enabled }}
            selected: {{ keys $cfg | join "," | quote }}
        {{- $cfg = .Values.after }}
          direct: {{ keys $cfg | join "," | quote }}
        {{- end }}
    "#};
    let reference = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
        {{- if .Values.enabled }}
          parent:
            selected: {{ keys $cfg | join "," | quote }}
        {{- $cfg = .Values.after }}
          direct: {{ keys $cfg | join "," | quote }}
        {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({ "enabled": true, "before": {}, "after": {} }),
            true,
        ),
        (
            serde_json::json!({ "enabled": true, "before": "wrong", "after": {} }),
            false,
        ),
        (
            serde_json::json!({ "enabled": true, "before": {}, "after": "wrong" }),
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

#[test]
fn escaped_batch_after_an_unconditional_break_does_not_execute() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          parent:
        {{- range $item := list .Values.item }}
        {{- break }}
            selected: {{ required "name is required" $item.name | quote }}
        {{- end }}
    "#};
    let reference = src.replace(
        r#"required "name is required" $item.name"#,
        "toString $item.name",
    );
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(&reference));

    sim_assert_eq!(have: &schema, want: &expected);
    sim_assert_eq!(
        have: schema_accepts_instance(&schema, &serde_json::json!({ "item": {} })),
        want: true,
    );
}

#[test]
fn exact_range_continue_guards_a_post_control_escaped_batch() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
        {{- range .Values.containers }}
        {{- if .enabled }}
          - name: {{ .name }}
        {{- else }}
        {{- continue }}
        {{- end }}
            image: {{ required "image is required" .image }}
        {{- end }}
    "#};
    let reference = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
        {{- range .Values.containers }}
        {{- if .enabled }}
          - name: {{ .name }}
            image: {{ required "image is required" .image }}
        {{- end }}
        {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({
                "containers": [
                    { "enabled": true, "name": "a", "image": "x" },
                    { "enabled": false, "name": "b" },
                ],
            }),
            true,
        ),
        (
            serde_json::json!({
                "containers": [{ "enabled": true, "name": "a" }],
            }),
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

#[test]
fn exact_range_break_joins_the_complete_first_iteration_exit() {
    let src = indoc! {r#"
        {{- $cfg := dict -}}
        {{- range $index, $value := list .Values.first .Values.last -}}
        {{- $cfg = $value -}}
        {{- if $.Values.stop -}}{{- break -}}{{- end -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          selected: {{ keys $cfg | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.stop }}
          {{- $selected := .Values.first }}
          selected: {{ keys $selected | join "," | quote }}
          {{- else }}
          {{- $selected := .Values.last }}
          selected: {{ keys $selected | join "," | quote }}
          {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({ "stop": true, "first": {}, "last": "wrong" }),
            true,
        ),
        (
            serde_json::json!({ "stop": true, "first": "wrong", "last": {} }),
            false,
        ),
        (
            serde_json::json!({ "stop": false, "first": "wrong", "last": {} }),
            true,
        ),
        (
            serde_json::json!({ "stop": false, "first": {}, "last": "wrong" }),
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

#[test]
fn exact_range_continue_joins_the_complete_last_iteration_exit() {
    let src = indoc! {r#"
        {{- $cfg := dict -}}
        {{- range $value := list .Values.last -}}
        {{- $cfg = $value -}}
        {{- if $.Values.skipLast -}}{{- continue -}}{{- end -}}
        {{- $cfg = $.Values.after -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          selected: {{ keys $cfg | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.skipLast }}
          {{- $selected := .Values.last }}
          selected: {{ keys $selected | join "," | quote }}
          {{- else }}
          {{- $selected := .Values.after }}
          selected: {{ keys $selected | join "," | quote }}
          {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({ "skipLast": true, "last": {}, "after": "wrong" }),
            true,
        ),
        (
            serde_json::json!({ "skipLast": true, "last": "wrong", "after": {} }),
            false,
        ),
        (
            serde_json::json!({ "skipLast": false, "last": "wrong", "after": {} }),
            true,
        ),
        (
            serde_json::json!({ "skipLast": false, "last": {}, "after": "wrong" }),
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

#[test]
fn exact_range_alternatives_retain_their_structural_selector() {
    let src = indoc! {r#"
        {{- $cfg := dict -}}
        {{- $items := ternary (list .Values.a) (list .Values.b) .Values.pickA -}}
        {{- range $cfg = $items -}}{{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          selected: {{ keys $cfg | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        {{- $checked := ternary 0 0 .Values.pickA -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.pickA }}
          selected: {{ keys .Values.a | join "," | quote }}
          {{- else }}
          selected: {{ keys .Values.b | join "," | quote }}
          {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({ "pickA": true, "a": {}, "b": "wrong" }),
            true,
        ),
        (
            serde_json::json!({ "pickA": true, "a": "wrong", "b": {} }),
            false,
        ),
        (
            serde_json::json!({ "pickA": false, "a": "wrong", "b": {} }),
            true,
        ),
        (
            serde_json::json!({ "pickA": false, "a": {}, "b": "wrong" }),
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

/// A guarded traversal retains both the advanced and untouched binding exits.
#[test]
fn guarded_traversal_retains_ancestor_and_child_binding_exits() {
    let src = indoc! {r#"
        {{- $x := .Values.a -}}
        {{- if hasKey $x "child" -}}
        {{- $x = index $x "child" -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          result: {{ keys $x.target | join "," | quote }}
    "#};
    let reference = indoc! {r#"
        {{- $x := .Values.a -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if hasKey $x "child" }}
          {{- $selected := index $x "child" }}
          result: {{ keys $selected.target | join "," | quote }}
          {{- else }}
          result: {{ keys $x.target | join "," | quote }}
          {{- end }}
    "#};
    let schema = schema_for(parse_ir(src));
    let expected = schema_for(parse_ir(reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (serde_json::json!({ "a": { "target": "wrong" } }), false),
        (serde_json::json!({ "a": { "target": {} } }), true),
        (
            serde_json::json!({
                "a": { "target": "wrong", "child": { "target": {} } }
            }),
            true,
        ),
        (
            serde_json::json!({
                "a": { "target": {}, "child": { "target": "wrong" } }
            }),
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

/// Later-arm placement preserves assignment order across deferred and direct output.
#[test]
fn later_arm_deferred_and_direct_nodes_keep_assignment_order() {
    let read_before_assignment = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          zero:
        {{- if .Values.enabled }}
          first:
        {{- else }}
            selected: {{ keys $cfg | join "," | quote }}
        {{- $cfg = .Values.after }}
          direct: {{ keys $cfg | join "," | quote }}
        {{- end }}
    "#};
    let read_before_reference = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
        {{- if .Values.enabled }}
          first:
        {{- else }}
          selected: {{ keys $cfg | join "," | quote }}
        {{- $cfg = .Values.after }}
          direct: {{ keys $cfg | join "," | quote }}
        {{- end }}
    "#};
    let schema = schema_for(parse_ir(read_before_assignment));
    let expected = schema_for(parse_ir(read_before_reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({ "enabled": false, "before": {}, "after": {} }),
            true,
        ),
        (
            serde_json::json!({ "enabled": false, "before": "wrong", "after": {} }),
            false,
        ),
        (
            serde_json::json!({ "enabled": false, "before": {}, "after": "wrong" }),
            false,
        ),
    ] {
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "instance={instance}; schema={schema}",
        );
    }

    let assignment_before_read = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          zero:
        {{- if .Values.enabled }}
          first:
        {{- else }}
        {{- $cfg = .Values.after }}
            selected: {{ keys $cfg | join "," | quote }}
          direct: {{ keys $cfg | join "," | quote }}
        {{- end }}
    "#};
    let assignment_before_reference = indoc! {r#"
        {{- $cfg := .Values.before -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
        {{- if .Values.enabled }}
          first:
        {{- else }}
        {{- $cfg = .Values.after }}
          selected: {{ keys $cfg | join "," | quote }}
          direct: {{ keys $cfg | join "," | quote }}
        {{- end }}
    "#};
    let schema = schema_for(parse_ir(assignment_before_read));
    let expected = schema_for(parse_ir(assignment_before_reference));

    sim_assert_eq!(have: &schema, want: &expected);
    for (instance, want) in [
        (
            serde_json::json!({ "enabled": false, "before": "wrong", "after": {} }),
            true,
        ),
        (
            serde_json::json!({ "enabled": false, "before": {}, "after": "wrong" }),
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
