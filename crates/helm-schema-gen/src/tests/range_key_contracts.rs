use test_util::prelude::sim_assert_eq;

use super::*;

/// F69: a collection and its member contract retain the complete outer guard.
/// Dead branches accept unrelated shapes; live branches constrain every
/// iterable lane after broad fragment/default alternatives are assembled.
#[test]
fn guarded_range_member_string_contract_stays_branch_scoped() {
    let src = indoc! {r#"
        {{- if .Values.enabled }}
        {{- with .Values.config }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $key, $value := .Values.templates }}
          {{ $key }}: |-
            {{- $value | nindent 4 }}
          {{- end }}
        {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        enabled: false
        config: {}
        templates: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let live_guard = serde_json::json!({
        "allOf": [helm_truthy_guard("enabled"), helm_truthy_guard("config")]
    });
    let mut properties = serde_json::Map::new();
    properties.insert(
        "config".to_string(),
        serde_json::json!({
            "anyOf": [
                {
                    "additionalProperties": false,
                    "maxProperties": 0,
                    "properties": {},
                    "type": "object",
                },
                { "additionalProperties": {}, "type": "object" },
                { "not": { "$ref": "#/$defs/helm-truthy" } },
            ]
        }),
    );
    properties.insert(
        "enabled".to_string(),
        serde_json::json!({
            "anyOf": [
                { "not": { "$ref": "#/$defs/helm-truthy" } },
                { "type": "boolean" },
            ]
        }),
    );
    properties.insert("templates".to_string(), serde_json::json!({}));
    let all_of = vec![serde_json::json!({
        "if": live_guard.clone(),
        "then": root_property_schema(
            "templates",
            serde_json::json!({
                "anyOf": [
                    { "items": { "type": "string" }, "type": "array" },
                    {
                        "additionalProperties": { "type": "string" },
                        "type": "object",
                    },
                    { "type": "null" },
                ]
            }),
        ),
    })];
    sim_assert_eq!(
        have: &schema,
        want: &expected_values_schema(properties, all_of, true)
    );

    for instance in [
        serde_json::json!({ "enabled": false, "config": {}, "templates": "audit" }),
        serde_json::json!({ "enabled": true, "config": {}, "templates": "audit" }),
        serde_json::json!({ "enabled": true, "config": { "route": "x" }, "templates": { "audit": "body" } }),
        serde_json::json!({ "enabled": true, "config": { "route": "x" }, "templates": ["body"] }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "dead consumers and live string members render: instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "enabled": true, "config": { "route": "x" }, "templates": "audit" }),
        serde_json::json!({ "enabled": true, "config": { "route": "x" }, "templates": { "audit": 7 } }),
        serde_json::json!({ "enabled": true, "config": { "route": "x" }, "templates": [7] }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a live non-iterable or non-string member aborts rendering: instance={instance}; schema={schema}"
        );
    }
}

/// F68: a string consumer on a two-variable range key distinguishes maps from
/// arrays. Empty arrays remain valid because no integer index reaches the
/// consumer.
#[test]
fn range_key_string_contract_preserves_only_the_empty_array_lane() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          keys: |-
            {{- range $key, $value := .Values.extraPorts }}
            {{ $key | lower }}
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("extraPorts: {}\n"));
    sim_assert_eq!(
        have: &schema,
        want: &expected_range_key_string_schema("extraPorts")
    );

    for instance in [
        serde_json::json!({ "extraPorts": { "syslog": 1514 } }),
        serde_json::json!({ "extraPorts": [] }),
        serde_json::json!({ "extraPorts": null }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "map keys are strings and empty collections execute no body: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "extraPorts": [1514] })),
        "a nonempty array sends an integer index to lower: {schema}"
    );
}

/// A helper called with the key must retain key provenance at its call
/// boundary; the current range member is a different runtime value.
#[test]
fn helper_string_contract_on_range_key_does_not_constrain_members() {
    let helpers = indoc! {r#"
        {{- define "normalize-key" -}}
        {{- . | lower -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          keys: |-
            {{- range $key, $value := .Values.extraPorts }}
            {{ include "normalize-key" $key }}
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("extraPorts: {}\n"),
    );
    sim_assert_eq!(
        have: &schema,
        want: &expected_range_key_string_schema("extraPorts")
    );

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "extraPorts": { "web": { "port": 8000 } } })
        ),
        "a string key contract must not retype its object member: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "extraPorts": [{ "port": 8000 }] })
        ),
        "a nonempty array sends its integer index through lower: {schema}"
    );
}

/// String predicates consume range keys even though their result is a
/// boolean rather than transformed text.
#[test]
fn range_key_string_predicate_constrains_the_array_lane() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          keys: |-
            {{- range $key, $value := .Values.extraPorts }}
            {{- if hasPrefix "sys" $key }}
            {{ $key }}
            {{- end }}
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("extraPorts: {}\n"));
    sim_assert_eq!(
        have: &schema,
        want: &expected_range_key_string_schema("extraPorts")
    );

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "extraPorts": { "syslog": 1514 } })
        ),
        "map keys satisfy hasPrefix: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "extraPorts": [1514] })),
        "an array index is not a string predicate operand: {schema}"
    );
}

/// A range key used in a non-string argument position does not acquire a
/// string contract merely because another argument is a string.
#[test]
fn non_string_range_key_operand_does_not_infer_a_string_contract() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          output: |-
            {{- range $key, $value := .Values.items }}
            {{ trunc $key "hello" }}
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("items: []\n"));
    let mut properties = serde_json::Map::new();
    properties.insert(
        "items".to_string(),
        serde_json::json!({
            "anyOf": [
                { "items": {}, "type": "array" },
                { "type": "array" },
                { "type": "null" },
                { "type": "object" },
            ]
        }),
    );
    sim_assert_eq!(
        have: &schema,
        want: &expected_values_schema(properties, Vec::new(), false)
    );

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "items": ["value"] })),
        "array indices satisfy trunc's numeric width operand: {schema}"
    );
}

/// A derived occurrence of a range key must not hide a separate raw string
/// occurrence in the same call.
#[test]
fn raw_range_key_occurrence_survives_a_derived_sibling() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          output: |-
            {{- range $key, $value := .Values.items }}
            {{ replace ($key | quote) $key "x" }}
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("items: {}\n"));
    sim_assert_eq!(
        have: &schema,
        want: &expected_range_key_string_schema("items")
    );

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "items": { "key": 1 } })),
        "the raw map key is a valid replace operand: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "items": [1] })),
        "quoting one occurrence does not stringify the separate raw array index: {schema}"
    );
}

/// Named range variables inside a block scalar retain member identity, so
/// their strict consumers constrain collection members just like structural
/// YAML holes do.
#[test]
fn block_scalar_range_variable_projects_its_string_contract() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          plugins.txt: |-
            {{- if .Values.enabled }}
            {{- if .Values.plugins }}
            {{- range $plugin := .Values.plugins }}
              {{- $plugin | nindent 4 }}
            {{- end }}
            {{- end }}
            {{- end }}
    "#};
    let values_yaml = "enabled: false\nplugins: ~\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "enabled": true, "plugins": ["git:latest"] }),
        serde_json::json!({ "enabled": false, "plugins": [7] }),
        serde_json::json!({ "enabled": true, "plugins": false }),
        serde_json::json!({ "enabled": true, "plugins": 0 }),
        serde_json::json!({ "enabled": true, "plugins": -1 }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "string members render and a disabled consumer imposes no member type: instance={instance}; schema={schema}"
        );
    }
    for plugins in [serde_json::json!([7]), serde_json::json!(2)] {
        let instance = serde_json::json!({ "enabled": true, "plugins": plugins });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a live non-string member reaches nindent: instance={instance}; schema={schema}"
        );
    }
}

/// Field access on ranged members requires object members whenever the
/// outer branch executes, without narrowing the same collection while the
/// branch is disabled.
#[test]
fn guarded_ranged_member_access_constrains_collection_lanes() {
    let src = indoc! {r#"
        {{- if .Values.enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range .Values.accounts }}
          {{ .tls }}: enabled
          {{- end }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("enabled: false\naccounts: ~\n"));
    let mut properties = serde_json::Map::new();
    properties.insert("accounts".to_string(), serde_json::json!({}));
    properties.insert(
        "enabled".to_string(),
        serde_json::json!({ "type": "boolean" }),
    );
    let guard = helm_truthy_guard("enabled");
    // The member-host implication already contains the complete iterable
    // domain, so a second broad range-domain conditional would be redundant.
    // Retaining the `tls` descendant in each collection lane documents the
    // structural member read without requiring that optional key.
    let member = serde_json::json!({
        "additionalProperties": {},
        "properties": { "tls": {} },
        "type": "object",
    });
    let all_of = vec![serde_json::json!({
        "if": guard,
        "then": root_property_schema(
            "accounts",
            serde_json::json!({
                "anyOf": [
                    { "items": member.clone(), "type": "array" },
                    {
                        "additionalProperties": member,
                        "type": "object",
                    },
                    { "maximum": 0, "type": "integer" },
                    { "type": "null" },
                ]
            }),
        ),
    })];
    sim_assert_eq!(
        have: &schema,
        want: &expected_values_schema(properties, all_of, true)
    );

    for instance in [
        serde_json::json!({ "enabled": false, "accounts": [7] }),
        serde_json::json!({ "enabled": true, "accounts": [{ "tls": "on" }] }),
        serde_json::json!({ "enabled": true, "accounts": { "A": { "tls": "on" } } }),
        serde_json::json!({ "enabled": true, "accounts": 0 }),
        serde_json::json!({ "enabled": true, "accounts": -1 }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "dead access or live object members render: instance={instance}; schema={schema}"
        );
    }
    for accounts in [
        serde_json::json!([7]),
        serde_json::json!({ "A": 7 }),
        serde_json::json!(2),
    ] {
        let instance = serde_json::json!({ "enabled": true, "accounts": accounts });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a live scalar member cannot host field access: instance={instance}; schema={schema}"
        );
    }
}
