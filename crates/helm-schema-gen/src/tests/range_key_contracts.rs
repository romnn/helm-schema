use test_util::prelude::sim_assert_eq;

use super::*;

/// a collection and its member contract retain the complete outer guard.
/// Dead branches accept unrelated shapes; live branches constrain every
/// iterable lane after broad fragment/default alternatives are assembled.
#[test]
fn guarded_range_member_string_contract_stays_branch_scoped() {
    let src = indoc! {r"
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
    "};
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

/// a string consumer on a two-variable range key distinguishes maps from
/// arrays. Empty arrays remain valid because no integer index reaches the
/// consumer.
#[test]
fn range_key_string_contract_preserves_only_the_empty_array_lane() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          keys: |-
            {{- range $key, $value := .Values.extraPorts }}
            {{ $key | lower }}
            {{- end }}
    "};
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
    // The unconditional two-variable range still demands an iterable
    // collection even though the key operand infers no string contract.
    let all_of = vec![root_property_schema(
        "items",
        serde_json::json!({
            "anyOf": [
                { "type": "array" },
                { "type": "object" },
                { "type": "null" },
            ]
        }),
    )];
    sim_assert_eq!(
        have: &schema,
        want: &expected_values_schema(properties, all_of, false)
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
    let src = indoc! {r"
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
    "};
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
    let src = indoc! {r"
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
    "};
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
    // The `tls` descendant renders as a mapping KEY, which formats every
    // scalar (numeric keys stringify through YAML-to-JSON) while composite
    // values stay out of the key lane.
    let member = serde_json::json!({
        "additionalProperties": {},
        "properties": { "tls": { "type": ["boolean", "integer", "number", "string"] } },
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

/// signoz's `renderAdditionalEnv` walks a values-backed map through
/// `range keys . | sortAlpha` and reads the CURRENT member with
/// `pluck . $dict | first`, then dispatches on the member's Go type
/// (`printf "%T"`). Plucking the ranged key from the same map is a member
/// projection, so the map arm's `toYaml` splice at the `EnvVar` slot must
/// type arbitrary members while the scalar arm's quoted lane stays open.
#[test]
fn same_map_pluck_of_ranged_key_projects_member_identity() {
    let helpers = indoc! {r#"
        {{- define "test.renderEnv" -}}
        {{- $dict := . -}}
        {{- range keys . | sortAlpha }}
        {{- $val := pluck . $dict | first -}}
        {{- $key := upper . -}}
        {{- $valueType := printf "%T" $val -}}
        {{- if eq $valueType "map[string]interface {}" }}
        - name: {{ $key }}
        {{ toYaml $val | indent 2 -}}
        {{- else }}
        - name: {{ $key }}
          value: {{ $val | quote }}
        {{- end }}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              containers:
                - name: main
                  env:
                    {{- include "test.renderEnv" .Values.additionalEnvs | nindent 20 }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("additionalEnvs: {}\n"),
    );
    for (instance, want) in [
        // The map arm splices the member as EnvVar fields: a numeric
        // `value` violates the provider's string field.
        (
            serde_json::json!({ "additionalEnvs": { "AUDIT": { "value": 7 } } }),
            false,
        ),
        (
            serde_json::json!({ "additionalEnvs": { "AUDIT": { "value": "ok" } } }),
            true,
        ),
        // Scalar members render through the quoted `value:` lane.
        (
            serde_json::json!({ "additionalEnvs": { "AUDIT": 7 } }),
            true,
        ),
        (
            serde_json::json!({ "additionalEnvs": { "AUDIT": "ok" } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "same-map pluck member projection: instance={instance}; schema={schema}"
        );
    }
}

/// minio renders each `environment` range KEY at the `EnvVar` `name:` slot.
/// A list supplies integer keys, so a non-empty list renders `name: 0`
/// against the provider's string-only field; the map lane and the empty
/// list (zero iterations) stay open.
#[test]
fn range_key_at_string_slot_excludes_integer_key_lanes() {
    let src = indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              containers:
                - name: main
                  env:
                    {{- range $key, $val := .Values.environment }}
                    - name: {{ $key }}
                      value: {{ $val | quote }}
                    {{- end }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("environment: {}\n"));
    for (instance, want) in [
        (serde_json::json!({ "environment": ["audit"] }), false),
        (
            serde_json::json!({ "environment": { "AUDIT": "ok" } }),
            true,
        ),
        (serde_json::json!({ "environment": [] }), true),
        (serde_json::json!({}), true),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "range keys at a string-only slot exclude integer key lanes: \
             instance={instance}; schema={schema}"
        );
    }
}
/// The signoz variant of the same helper guards every render on a
/// case-folding dedup accumulator (`$processedKeys := dict` before the
/// range, `if not (hasKey $processedKeys $key)` inside it). The guard is
/// provably TRUE on the first iteration — the accumulator starts empty —
/// so with at most one member every iteration is the first and the member
/// typing binds under that size bound; larger maps stay open because an
/// earlier case-colliding key can SHADOW a member entirely.
#[test]
fn dedup_accumulator_binds_member_typing_to_singleton_maps() {
    let helpers = indoc! {r#"
        {{- define "test.renderEnv" -}}
        {{- $dict := . -}}
        {{- $processedKeys := dict -}}
        {{- range keys . | sortAlpha }}
        {{- $val := pluck . $dict | first -}}
        {{- $key := upper . -}}
        {{- if not (hasKey $processedKeys $key) }}
        {{- $processedKeys = merge $processedKeys (dict $key true) -}}
        {{- $valueType := printf "%T" $val -}}
        {{- if eq $valueType "map[string]interface {}" }}
        - name: {{ $key }}
        {{ toYaml $val | indent 2 -}}
        {{- else }}
        - name: {{ $key }}
          value: {{ $val | quote }}
        {{- end }}
        {{- end -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              containers:
                - name: main
                  env:
                    {{- include "test.renderEnv" .Values.additionalEnvs | nindent 20 }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("additionalEnvs: {}\n"),
    );
    for (instance, want) in [
        // A single member cannot be shadowed: it always renders, so the
        // provider's EnvVar shape binds it.
        (
            serde_json::json!({ "additionalEnvs": { "AUDIT": { "value": 7 } } }),
            false,
        ),
        (
            serde_json::json!({ "additionalEnvs": { "AUDIT": { "value": "ok" } } }),
            true,
        ),
        // Scalar members render through the quoted `value:` lane.
        (
            serde_json::json!({ "additionalEnvs": { "AUDIT": 7 } }),
            true,
        ),
        // With two or more members the dedup is relational: an earlier
        // case-colliding key shadows the invalid member, so the map stays
        // open.
        (
            serde_json::json!({ "additionalEnvs": {
                "AUDIT": { "value": "ok" }, "audit": { "value": 7 }
            } }),
            true,
        ),
        (serde_json::json!({ "additionalEnvs": {} }), true),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "dedup accumulator singleton lane: instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// A guard-scoped `omit` removes literal keys from a values-backed map
/// before the sink reads it (external-secrets' `OpenShift`
/// `adaptSecurityContext`): the removed keys' provider typing must not
/// bind where the omit may run, and comes back exactly where the omitting
/// arm certainly did not run (`adaptSecurityContext` not force/auto).
/// Keys the omit never touches keep their unconditional typing, and the
/// member-count render gate (`gt (keys . | len) 1`) scopes every arm.
#[test]
fn guard_scoped_omit_scopes_removed_member_typing() {
    let helpers = indoc! {r#"
        {{- define "test.renderSecurityContext" -}}
        {{- $adaptedContext := .securityContext -}}
        {{- if .context.Values.global.compatibility -}}
          {{- if .context.Values.global.compatibility.openshift -}}
            {{- if or (eq .context.Values.global.compatibility.openshift.adaptSecurityContext "force") (and (eq .context.Values.global.compatibility.openshift.adaptSecurityContext "auto") (include "test.isOpenShift" .context)) -}}
              {{- $adaptedContext = omit $adaptedContext "fsGroup" "runAsUser" "runAsGroup" -}}
            {{- end -}}
          {{- end -}}
        {{- end -}}
        {{- omit $adaptedContext "enabled" | toYaml -}}
        {{- end -}}
        {{- define "test.isOpenShift" -}}
        {{- if .Capabilities.APIVersions.Has "security.openshift.io/v1" -}}
        {{- true -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              containers:
                - name: main
                  {{- with .Values.securityContext }}
                  {{- if and (.enabled) (gt (keys . | len) 1) }}
                  securityContext:
                    {{- include "test.renderSecurityContext" (dict "securityContext" . "context" $) | nindent 20 }}
                  {{- end }}
                  {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(
            "securityContext:\n  enabled: true\nglobal:\n  compatibility:\n    openshift:\n      adaptSecurityContext: auto\n",
        ),
    );
    for (mode, member, value, enabled, want) in [
        // The omit certainly runs under "force" and may run under "auto"
        // (the OpenShift capability is cluster-dependent), so the removed
        // key's typing abstains there.
        ("force", "runAsUser", serde_json::json!("audit"), true, true),
        ("auto", "runAsUser", serde_json::json!("audit"), true, true),
        // With adaptation disabled the key certainly survives to the
        // provider slot.
        (
            "disabled",
            "runAsUser",
            serde_json::json!("audit"),
            true,
            false,
        ),
        ("disabled", "runAsUser", serde_json::json!(1000), true, true),
        // A disabled render gate never reaches the provider slot at all.
        (
            "disabled",
            "runAsUser",
            serde_json::json!("audit"),
            false,
            true,
        ),
        // Keys the omit never touches keep their typing in every mode.
        (
            "force",
            "runAsNonRoot",
            serde_json::json!("audit"),
            true,
            false,
        ),
        (
            "disabled",
            "runAsNonRoot",
            serde_json::json!("audit"),
            true,
            false,
        ),
    ] {
        let instance = serde_json::json!({
            "securityContext": { "enabled": enabled, "runAsNonRoot": true, member: value },
            "global": { "compatibility": { "openshift": { "adaptSecurityContext": mode } } },
        });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "guard-scoped omit member typing: mode={mode} member={member} value={value} \
             enabled={enabled}; want={want}; schema={schema}"
        );
    }
}

/// A `fail` inside `range .Values.ingress.extraPaths` firing on
/// `or (.backend.serviceName) (.backend.servicePort)` — behind
/// `eq (include "capabilities.ingress.apiVersion" .) "networking.k8s.io/v1"`,
/// a literal dispatch whose non-else arms are capability-defaulted
/// `semverCompare "<C"` bounds — lowers to a per-member field-falsy
/// requirement scoped by the flipped `>=C` kubeVersion patterns
/// (oauth2-proxy's legacy extraPaths gate). Without a pinned kubeVersion
/// the selection is cluster-dependent and the arm soundly abstains.
#[test]
fn capability_dispatch_scoped_member_field_fail_lowers() {
    let helpers = indoc! {r#"
        {{- define "capabilities.ingress.apiVersion" -}}
        {{- if semverCompare "<1.14-0" ( .Values.kubeVersion | default .Capabilities.KubeVersion.Version ) -}}
        {{- print "extensions/v1beta1" -}}
        {{- else if semverCompare "<1.19-0" ( .Values.kubeVersion | default .Capabilities.KubeVersion.Version ) -}}
        {{- print "networking.k8s.io/v1beta1" -}}
        {{- else -}}
        {{- print "networking.k8s.io/v1" -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if .Values.checkDeprecation }}
            {{- if eq ( include "capabilities.ingress.apiVersion" . ) "networking.k8s.io/v1" -}}
                {{- range .Values.ingress.extraPaths }}
                    {{- if or (.backend.serviceName) (.backend.servicePort) }}
                        {{ fail "Please update the format of your `ingress.extraPaths`" }}
                    {{- end }}
                {{- end }}
            {{- end }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("checkDeprecation: true\ningress:\n  extraPaths: []\n"),
    );
    for (instance, want) in [
        (
            serde_json::json!({ "checkDeprecation": true, "kubeVersion": "1.30.0", "ingress": { "extraPaths": [
                { "path": "/*", "backend": { "serviceName": "x" } }
            ] } }),
            false,
        ),
        (
            serde_json::json!({ "checkDeprecation": true, "kubeVersion": "1.30.0", "ingress": { "extraPaths": [
                { "path": "/*", "backend": { "servicePort": "y" } }
            ] } }),
            false,
        ),
        // The old api keeps the legacy format.
        (
            serde_json::json!({ "kubeVersion": "1.18.0", "ingress": { "extraPaths": [
                { "path": "/*", "backend": { "serviceName": "x" } }
            ] } }),
            true,
        ),
        // Without a pinned kubeVersion the capability default decides:
        // cluster-dependent, so the arm abstains.
        (
            serde_json::json!({ "ingress": { "extraPaths": [
                { "path": "/*", "backend": { "serviceName": "x" } }
            ] } }),
            true,
        ),
        (
            serde_json::json!({ "kubeVersion": "1.30.0", "ingress": { "extraPaths": [
                { "path": "/*", "backend": { "service": { "name": "x" } } }
            ] } }),
            true,
        ),
        (
            serde_json::json!({ "checkDeprecation": false, "kubeVersion": "1.30.0",
                "ingress": { "extraPaths": [
                    { "path": "/*", "backend": { "serviceName": "x" } }
                ] } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "capability-scoped member field fail: instance={instance}; want={want}; schema={schema}"
        );
    }
}
