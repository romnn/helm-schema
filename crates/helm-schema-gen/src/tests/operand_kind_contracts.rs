use super::*;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

/// Go-template `eq`/`ne` against a STRING literal terminates on
/// incomparable operand kinds — maps, lists, and mismatched scalars abort
/// rendering while the schema accepted them (harbor `logLevel`, reloader
/// `reloadStrategy` shapes).
#[test]
fn string_literal_comparison_binds_operand_kind_contract() {
    let src = indoc! {r#"
        {{- $storageClass := default "" .Values.storageClass }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if eq .Values.logLevel "debug" }}
          verbosity: high
          {{- end }}
          {{- if ne .Values.reloadStrategy "env-vars" }}
          strategy: other
          {{- end }}
          {{- if eq $storageClass "-" }}
          storage-class: disabled
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        logLevel: ~
        reloadStrategy: ~
        storageClass: ''
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "logLevel": "debug" }),
        serde_json::json!({ "logLevel": "info" }),
        serde_json::json!({ "storageClass": "" }),
        serde_json::json!({ "storageClass": "-" }),
        serde_json::json!({}),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "strings compare fine and missing operands never reach eq: \
             instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "logLevel": { "a": 1 } }),
        serde_json::json!({ "logLevel": [1] }),
        serde_json::json!({ "reloadStrategy": 7 }),
        serde_json::json!({ "storageClass": 7 }),
        serde_json::json!({ "storageClass": true }),
        serde_json::json!({ "storageClass": { "a": 1 } }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "an incomparable operand kind aborts the comparison: \
             instance={instance}; schema={schema}"
        );
    }
}

/// strict collection functions bind their operand domains: `merge` subjects
/// are maps, `concat` operands are lists, `len` needs a length-bearing value,
/// and `has` searches a list. The call itself does not skip Helm-empty operands.
///
/// Nor does it skip a nil one, except at `has` — every other operand here
/// must be present, so the accepted documents carry all of them.
#[test]
fn collection_function_operands_bind_kind_contracts() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          merged: {{ merge .Values.overrides .Values.defaults | toYaml | quote }}
          combined: {{ concat .Values.extraEnv .Values.env | toYaml | quote }}
          count: "{{ len .Values.extraVolumes }}"
          {{- if has "certs" .Values.collectors }}
          certs: enabled
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        overrides: ~
        defaults: ~
        extraEnv: ~
        env: ~
        extraVolumes: ~
        collectors: ~
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let present = serde_json::json!({
        "overrides": {}, "defaults": {}, "extraEnv": [], "env": [], "extraVolumes": [],
    });
    let with_present = |overrides: serde_json::Value| {
        let mut instance = present.clone();
        for (key, value) in overrides.as_object().into_iter().flatten() {
            instance[key.as_str()] = value.clone();
        }
        instance
    };
    for instance in [
        with_present(serde_json::json!({ "overrides": { "a": 1 }, "defaults": { "b": 2 } })),
        with_present(serde_json::json!({ "extraEnv": [1], "env": [2] })),
        with_present(serde_json::json!({ "extraVolumes": ["v"] })),
        with_present(serde_json::json!({ "extraVolumes": "vols" })),
        // `has` answers false on a nil haystack, so the collector list is
        // the one operand this template may omit.
        with_present(serde_json::json!({ "collectors": ["certs"] })),
        present.clone(),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the accepted operand kinds render: instance={instance}; schema={schema}"
        );
    }
    for instance in [
        // Every operand but the `has` haystack aborts when nil.
        serde_json::json!({ "defaults": {}, "extraEnv": [], "env": [], "extraVolumes": [] }),
        serde_json::json!({ "overrides": {}, "extraEnv": [], "env": [], "extraVolumes": [] }),
        serde_json::json!({ "overrides": {}, "defaults": {}, "env": [], "extraVolumes": [] }),
        serde_json::json!({ "overrides": {}, "defaults": {}, "extraEnv": [], "extraVolumes": [] }),
        serde_json::json!({ "overrides": {}, "defaults": {}, "extraEnv": [], "env": [] }),
        with_present(serde_json::json!({ "overrides": "s" })),
        with_present(serde_json::json!({ "overrides": false })),
        with_present(serde_json::json!({ "overrides": 0 })),
        with_present(serde_json::json!({ "overrides": "" })),
        with_present(serde_json::json!({ "extraEnv": "s" })),
        with_present(serde_json::json!({ "extraEnv": false })),
        with_present(serde_json::json!({ "extraEnv": 0 })),
        with_present(serde_json::json!({ "extraEnv": "" })),
        with_present(serde_json::json!({ "extraVolumes": 7 })),
        with_present(serde_json::json!({ "collectors": 7 })),
        with_present(serde_json::json!({ "collectors": false })),
        with_present(serde_json::json!({ "collectors": "" })),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "an incompatible operand kind aborts the call: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A strict collection call constrains the constructed container operand, not
/// values that merely occupy its leaves.
#[test]
fn constructed_collection_operands_do_not_retype_leaf_values() {
    let src = indoc! {r#"
        {{- $handler := dict "port" .Values.healthPort }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          merged: {{ merge $handler .Values.settings | toYaml | quote }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            healthPort: null
            settings: {}
        "}),
    );

    for health_port in [
        serde_json::json!(5556),
        serde_json::json!("health"),
        serde_json::json!(false),
        serde_json::json!({ "named": true }),
    ] {
        let instance = serde_json::json!({ "healthPort": health_port, "settings": {} });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "dict accepts any leaf value before merge consumes the constructed object: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "healthPort": 5556, "settings": "bad" })
        ),
        "the raw merge operand still requires an object: {schema}"
    );
}

/// an unconditional strict call evaluates Helm-falsy operands too.
/// Only structural control flow or fallback selection may make its domain conditional.
#[test]
fn unconditional_strict_call_rejects_falsy_wrong_kinds() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          merged: {{ merge .Values.config (dict) | toYaml | quote }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("config: {}\n"));

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "config": { "a": 1 } })),
        "an object reaches merge successfully: {schema}"
    );
    for config in [
        serde_json::json!(false),
        serde_json::json!(0),
        serde_json::json!(""),
    ] {
        let instance = serde_json::json!({ "config": config });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "merge executes for falsy wrong kinds: instance={instance}; schema={schema}"
        );
    }
}

/// Runtime effects inside a derived range may shed the range guard only when
/// structural evaluation proves every result lane nonempty. The strict call
/// remains scoped by the surrounding chart guards.
#[test]
fn strict_call_in_nonempty_derived_range_keeps_outer_guards() {
    let src = indoc! {r#"
        {{- if and (eq .Values.rbac.create true) (not .Values.rbac.useExistingRole) -}}
        {{- range (ternary (join "," .Values.namespaces | split ",") (list "") (eq .Values.rbac.useClusterRole false)) }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if has "pods" $.Values.collectors }}
          pods: enabled
          {{- end }}
        {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        rbac:
          create: true
          useClusterRole: true
          useExistingRole: ""
        namespaces: ""
        collectors:
          - pods
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "rbac": { "create": true, "useClusterRole": true },
                "namespaces": "",
                "collectors": ["pods"]
            })
        ),
        "a list satisfies the live has call: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "rbac": { "create": true, "useClusterRole": true },
                "namespaces": "",
                "collectors": 7
            })
        ),
        "both derived range lanes execute has and reject an integer: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "rbac": { "create": false, "useClusterRole": true },
                "namespaces": "",
                "collectors": 7
            })
        ),
        "the disabled outer branch never evaluates has: {schema}"
    );
}

/// Pipeline syntax appends its input as the final call operand, but the
/// runtime domain is otherwise identical to the corresponding direct call.
#[test]
fn pipeline_calls_bind_collection_and_comparison_domains() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          count: {{ .Values.lengthBearing | len | quote }}
          first: {{ .Values.ordered | first | quote }}
          reversed: {{ .Values.ordered | reverse | toYaml | quote }}
          {{- if .Values.mode | eq "active" }}
          mode: active
          {{- end }}
          {{- if .Values.integerLimit | eq 1 }}
          integer-limit: matched
          {{- end }}
          {{- if .Values.floatLimit | eq 1.5 }}
          float-limit: matched
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        lengthBearing: ~
        ordered: ~
        mode: ~
        integerLimit: ~
        floatLimit: ~
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    // A PIPED operand still reaches `len` and `first` through sprig's own
    // reflection, which faults on nil, so both keys must be present in
    // every document that renders. The comparison operands may be absent:
    // `eq` compares nil against anything.
    let present = serde_json::json!({ "lengthBearing": "text", "ordered": [] });
    let with_present = |overrides: serde_json::Value| {
        let mut instance = present.clone();
        for (key, value) in overrides.as_object().into_iter().flatten() {
            instance[key.as_str()] = value.clone();
        }
        instance
    };
    for instance in [
        with_present(serde_json::json!({ "lengthBearing": "text" })),
        with_present(serde_json::json!({ "lengthBearing": { "key": "value" } })),
        with_present(serde_json::json!({ "ordered": ["a", "b"] })),
        with_present(serde_json::json!({ "mode": "active" })),
        with_present(serde_json::json!({ "integerLimit": 1 })),
        with_present(serde_json::json!({ "floatLimit": 1.5 })),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "a runtime-compatible pipeline operand must validate: \
             instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "ordered": [] }),
        serde_json::json!({ "lengthBearing": "text" }),
        with_present(serde_json::json!({ "lengthBearing": 7 })),
        with_present(serde_json::json!({ "ordered": { "a": 1 } })),
        with_present(serde_json::json!({ "mode": { "active": true } })),
        with_present(serde_json::json!({ "integerLimit": 1.5 })),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "an incompatible pipeline operand aborts rendering: \
             instance={instance}; schema={schema}"
        );
    }
}

/// Sprig's `ternary` accepts arbitrary values for its two result arms, but
/// its selector is a strict Go `bool` in both direct and pipeline forms.
#[test]
fn ternary_selector_binds_boolean_operand_contract() {
    let src = indoc! {r#"
        {{- $local := .Values.local }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.enabled }}
          direct: {{ ternary "yes" "no" .Values.direct | quote }}
          pipeline: {{ .Values.pipeline | ternary "yes" "no" | quote }}
          local: {{ ternary "yes" "no" $local | quote }}
          computed-direct: {{ ternary "yes" "no" (eq .Values.mode "active") | quote }}
          computed-pipeline: {{ .Values.pipelineMode | eq "active" | ternary "yes" "no" | quote }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        enabled: true
        direct: true
        pipeline: false
        local: true
        mode: active
        pipelineMode: inactive
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "enabled": true,
                "direct": true,
                "pipeline": false,
                "local": true,
                "mode": "active",
                "pipelineMode": "inactive"
            })
        ),
        "raw Boolean and computed Boolean selectors render in both call forms: {schema}"
    );
    for instance in [
        serde_json::json!({ "enabled": true, "direct": "true", "pipeline": false }),
        serde_json::json!({ "enabled": true, "direct": true, "pipeline": 1 }),
        serde_json::json!({
            "enabled": true,
            "direct": true,
            "pipeline": false,
            "local": "true"
        }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a live non-Boolean selector aborts ternary: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "enabled": true,
                "direct": true,
                "pipeline": false,
                "mode": true,
                "pipelineMode": "inactive"
            })
        ),
        "the comparison constrains its string operand without retyping it as Boolean: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "enabled": false,
                "direct": "ignored",
                "pipeline": { "ignored": true },
                "mode": { "ignored": true },
                "pipelineMode": ["ignored"]
            })
        ),
        "the outer guard skips both strict calls: {schema}"
    );
}

/// KEDA selects a component `ServiceAccount` flag unless that value is absent,
/// in which case it falls back to the legacy shared flag.
#[test]
fn invalid_kind_ternary_scopes_each_provider_operand() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: test
        automountServiceAccountToken: {{ kindIs "invalid" .Values.component.automount | ternary .Values.fallback .Values.component.automount }}
    "#};
    let values_yaml = indoc! {"
        component:
          automount: true
        fallback: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "component": { "automount": true }, "fallback": "ignored" }),
        serde_json::json!({ "component": {}, "fallback": false }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the selected Boolean provider operand must validate: instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "component": { "automount": "wrong" }, "fallback": false }),
        serde_json::json!({ "component": {}, "fallback": "wrong" }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a selected non-Boolean provider operand must be rejected: instance={instance}; schema={schema}"
        );
    }
}

/// Go-template `or` returns the first truthy operand or its final operand;
/// downstream runtime contracts apply only to the value that was selected.
#[test]
fn short_circuit_value_selection_scopes_downstream_contracts() {
    let src = indoc! {r"
        {{- $selected := or .Values.primary .Values.fallback }}
        apiVersion: v1
        kind: Secret
        metadata:
          name: test
        data:
          selected: {{ $selected | b64enc | quote }}
          nested: {{ or .Values.ready (b64enc .Values.payload) | quote }}
    "};
    let values_yaml = indoc! {"
        primary: ''
        fallback: fallback
        ready: false
        payload: payload
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({
            "primary": "primary",
            "fallback": { "ignored": true },
            "ready": true,
            "payload": { "ignored": true }
        }),
        serde_json::json!({
            "primary": "",
            "fallback": "fallback",
            "ready": false,
            "payload": "payload"
        }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "inactive later operands must not inherit downstream contracts: \
             instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({
            "primary": "",
            "fallback": { "selected": true },
            "ready": true,
            "payload": "payload"
        }),
        serde_json::json!({
            "primary": { "selected": true },
            "fallback": "ignored",
            "ready": true,
            "payload": "payload"
        }),
        serde_json::json!({
            "primary": "primary",
            "fallback": "fallback",
            "ready": false,
            "payload": { "executed": true }
        }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "the selected or executed strict operand must satisfy b64enc: \
             instance={instance}; schema={schema}"
        );
    }
}

/// Type dispatch after `or` partitions the selected candidate, not every
/// source path that could have supplied the local.
#[test]
fn short_circuit_type_dispatch_preserves_candidate_partitions() {
    let src = indoc! {r#"
        {{- $selected := or .Values.primary .Values.fallback }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if $selected }}
          selected: |-
            {{- $type := typeOf $selected }}
            {{- if eq $type "string" }}
            {{ tpl $selected . | nindent 4 }}
            {{- else }}
            {{ toYaml $selected | nindent 4 }}
            {{- end }}
          {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            primary: default
            fallback: {}
        "}),
    );

    for instance in [
        serde_json::json!({ "primary": "text", "fallback": { "ignored": true } }),
        serde_json::json!({ "primary": { "selected": true }, "fallback": "ignored" }),
        serde_json::json!({ "primary": {}, "fallback": "text" }),
        serde_json::json!({ "primary": {}, "fallback": { "selected": true } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "string and structured shapes render for whichever candidate is selected: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A literal `index` into a values-backed list terminates unless that
/// position exists. The precondition follows ordinary control flow, while
/// eagerly evaluated function arguments cannot hide it.
#[test]
fn literal_index_records_guarded_cardinality_preconditions() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.enabled }}
          guarded: {{ index .Values.items 1 | quote }}
          {{- end }}
          eager: {{ default "fallback" (index .Values.eager 0) | quote }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            enabled: true
            items: [a, b]
            eager: [a]
        "}),
    );

    for instance in [
        serde_json::json!({ "enabled": true, "items": ["a", "b"], "eager": ["a"] }),
        serde_json::json!({ "enabled": false, "items": [], "eager": ["a"] }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "every executed index has an existing position: instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "enabled": true, "items": ["a"], "eager": ["a"] }),
        serde_json::json!({ "enabled": false, "items": [], "eager": [] }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "an executed out-of-range index must be rejected: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn split_index_projects_separator_cardinality_to_the_text_source() {
    let src = indoc! {r#"
        {{- if .Values.enabled }}
        {{- $address := toString .Values.address }}
        {{- $parts := regexSplit ":" $address -1 }}
        {{- $port := index $parts 1 }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          port: {{ $port | quote }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            enabled: true
            address: 0.0.0.0:9153
        "}),
    );

    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "enabled": true, "address": "9153" })
        ),
        "the second split item requires one separator: {schema}"
    );
    for instance in [
        serde_json::json!({ "enabled": true, "address": "0.0.0.0:9153" }),
        serde_json::json!({ "enabled": false, "address": "9153" }),
        serde_json::json!({ "enabled": true, "address": { "rendered": "conservatively" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the split precondition must keep its guard and total-conversion preimage: instance={instance}; schema={schema}"
        );
    }
}

/// Certificate helpers require list-shaped SAN arguments whose every member
/// is a Go string. List-preserving transforms keep that member contract on
/// each values-backed source list.
#[test]
fn certificate_signatures_constrain_nested_list_members() {
    let src = indoc! {r#"
        {{- if .Values.enabled }}
        {{- $ips := concat (list "127.0.0.1") .Values.extraIps }}
        {{- $dns := prepend .Values.extraDns "service.local" }}
        {{- $cert := genSelfSignedCert "service" $ips $dns 365 }}
        apiVersion: v1
        kind: Secret
        metadata:
          name: test
        stringData:
          marker: generated
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            enabled: true
            extraIps: []
            extraDns: []
        "}),
    );

    for instance in [
        serde_json::json!({
            "enabled": true,
            "extraIps": ["10.0.0.7"],
            "extraDns": ["audit.example"]
        }),
        serde_json::json!({
            "enabled": false,
            "extraIps": [7],
            "extraDns": [{ "ignored": true }]
        }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "valid SANs and values behind a dead guard must validate: \
             instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "enabled": true, "extraIps": [7], "extraDns": [] }),
        serde_json::json!({
            "enabled": true,
            "extraIps": [],
            "extraDns": [{ "invalid": true }]
        }),
        serde_json::json!({ "enabled": true, "extraIps": "10.0.0.7", "extraDns": [] }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a live certificate call rejects the wrong outer or member kind: \
             instance={instance}; schema={schema}"
        );
    }
}

/// `semverCompare` first parses its version operand, so the accepted string
/// domain is narrower than an arbitrary Go string under the live call guard.
#[test]
fn semver_parser_binds_lexical_operand_contract() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.enabled }}
          {{- if semverCompare ">=1.20.0" .Values.kubeVersion }}
          modern: "true"
          {{- end }}
          {{- if .Values.pipelineVersion | semverCompare ">=1.20.0" }}
          pipeline-modern: "true"
          {{- end }}
          {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            enabled: true
            kubeVersion: v1.30.0
            pipelineVersion: v1.30.0
        "}),
    );

    for version in [
        "v1.30.0",
        "1",
        "1.2",
        "01.002.0003-alpha.1+build.7",
        // Prerelease numeric identifiers without leading zeros parse
        "3.1.0-rc.1",
        "1.2.3-0",
    ] {
        let instance = serde_json::json!({
            "enabled": true,
            "kubeVersion": version,
            "pipelineVersion": version
        });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "Masterminds semver accepts the loose version spelling: \
             instance={instance}; schema={schema}"
        );
    }
    for version in [
        "garbage",
        "v",
        "1..2",
        // Masterminds rejects a leading zero in a NUMERIC prerelease
        // identifier (airflow's `airflowVersion: 3.1.0-01`)
        "3.1.0-01",
        "1.2.3-alpha.01",
        // A 21-digit core component certainly overflows `ParseUint`'s
        // uint64 and aborts the parser.
        "111111111111111111111.0.0",
    ] {
        let instance = serde_json::json!({
            "enabled": true,
            "kubeVersion": version,
            "pipelineVersion": version
        });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a live lexically invalid version aborts semverCompare: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "enabled": false,
                "kubeVersion": "garbage",
                "pipelineVersion": "garbage"
            })
        ),
        "the disabled outer branch never invokes the parser: {schema}"
    );
}

/// `substr` slices a Go string subject, so non-string inputs abort
/// rendering. The subject is the LAST argument in both the direct and the
/// pipeline spelling; the leading start/end offsets are numeric.
#[test]
fn substr_binds_string_subject_contract() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if eq (substr 0 7 .Values.tag) "sha256:" }}
          digest: "true"
          {{- end }}
          piped: {{ .Values.ref | substr 0 7 }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            tag: v1.2.3
            ref: v1.2.3
        "}),
    );

    for instance in [
        serde_json::json!({ "tag": "sha256:abc", "ref": "v1.2.3" }),
        serde_json::json!({ "tag": "v1.2.3", "ref": "sha256:abc" }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "any string subject slices fine: instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "tag": { "bad": true } }),
        serde_json::json!({ "tag": [1] }),
        serde_json::json!({ "ref": { "bad": true } }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a non-string substr subject aborts rendering: instance={instance}; schema={schema}"
        );
    }
}

/// flux2 `template.image`: the substr subject contract survives a
/// helper called with a values sub-object whose body reads a RELATIVE
/// selector, and the derived substring output does not leak a later strict
/// parser's lexical domain back onto the raw input.
#[test]
fn substr_contract_projects_through_helper_arguments() {
    let helpers = indoc! {r#"
        {{- define "template.image" -}}
        {{- if eq (substr 0 7 .tag) "sha256:" -}}
        {{- printf "%s@%s" .image .tag -}}
        {{- else -}}
        {{- printf "%s:%s" .image .tag -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          image: {{ template "template.image" .Values.cli }}
          {{- if semverCompare ">=1.0.0" (substr 1 7 .Values.cli.tag) }}
          modern: "true"
          {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(indoc! {"
            cli:
              image: repo/cli
              tag: v1.2.3
        "}),
    );

    for instance in [
        serde_json::json!({ "cli": { "tag": "sha256:abc" } }),
        serde_json::json!({ "cli": { "tag": "v1.2.3" } }),
        // The semver parser sees the DERIVED substring, not the raw tag, so
        // the raw path keeps the full string domain.
        serde_json::json!({ "cli": { "tag": "not-a-semver" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "string tags reach substr through the helper argument: \
             instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "cli": { "tag": { "bad": true } } }),
        serde_json::json!({ "cli": { "tag": [1] } }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a non-string tag aborts substr inside the helper: \
             instance={instance}; schema={schema}"
        );
    }
}

/// an `if` arm that reassigns the parser's local to a literal sentinel
/// (`$version = "1.20.0"` under `eq $version "latest"`, datadog's cluster
/// agent check) replaces the raw value on that arm, so the lexical contract
/// fires only where the sentinel equality is false. The undecodable sibling
/// conjunct (`eq $length 1`) must not widen the exclusion: `¬E` alone is a
/// sound subset of `¬(A ∧ E)`.
#[test]
fn conditional_literal_reassignment_excludes_the_sentinel_from_the_parser_domain() {
    let src = indoc! {r#"
        {{- $version := .Values.image.tag | toString -}}
        {{- $length := len (split "." $version) -}}
        {{- if and (eq $length 1) (eq $version "latest") -}}
        {{- $version = "1.20.0" -}}
        {{- end -}}
        {{- if not (semverCompare ">=1.20.0-0" $version) -}}
        {{- fail "chart requires 1.20.0 or greater" -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            image:
              tag: 7.68.2
        "}),
    );

    for tag in ["latest", "1.26.0", "7.68.2-rc.1"] {
        let instance = serde_json::json!({ "image": { "tag": tag } });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the reassigned sentinel and ordinary versions render: \
             instance={instance}; schema={schema}"
        );
    }
    for tag in ["garbage", "latest-extra"] {
        let instance = serde_json::json!({ "image": { "tag": tag } });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a non-sentinel lexically invalid version still aborts the parser: \
             instance={instance}; schema={schema}"
        );
    }
}

/// when the reassigning arm's condition has no decodable equality
/// conjunct, the exclusion is unrepresentable and the parser contract must
/// abstain — rejecting raw inputs that the reassigned arm would have
/// replaced is unsound.
#[test]
fn conditional_reassignment_without_equality_sentinel_abstains() {
    let src = indoc! {r#"
        {{- $version := .Values.image.tag | toString -}}
        {{- if .Values.image.floating -}}
        {{- $version = "1.20.0" -}}
        {{- end -}}
        {{- if not (semverCompare ">=1.20.0-0" $version) -}}
        {{- fail "chart requires 1.20.0 or greater" -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            image:
              tag: 7.68.2
              floating: false
        "}),
    );

    let instance = serde_json::json!({ "image": { "tag": "garbage", "floating": true } });
    assert!(
        schema_accepts_instance(&schema, &instance),
        "the truthy-guard arm replaces the raw value, so its lexical domain abstains: \
         instance={instance}; schema={schema}"
    );
}

/// a `replace OLD NEW` stage with a literal OLD is the identity on raw
/// strings that do not contain OLD, so the parser's lexical domain applies
/// only to the untransformed arm; raw strings containing the stripped
/// sentinel are exempt.
#[test]
fn replace_chain_exempts_stripped_sentinels_from_the_parser_domain() {
    let src = indoc! {r#"
        {{- $version := .Values.image.tag | replace "latest-" "" | replace "master" "1.20.0" -}}
        {{- if not (semverCompare ">=1.20.0-0" $version) -}}
        {{- fail "unsupported version" -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            image:
              tag: 1.26.0
        "}),
    );

    for tag in ["latest-1.26.0", "master", "1.26.0"] {
        let instance = serde_json::json!({ "image": { "tag": tag } });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "sentinel-bearing and plain versions render: instance={instance}; schema={schema}"
        );
    }
    let instance = serde_json::json!({ "image": { "tag": "garbage" } });
    assert!(
        !schema_accepts_instance(&schema, &instance),
        "a raw string no replace touches must satisfy the parser: \
         instance={instance}; schema={schema}"
    );
}

/// `(split "@" tag)._0` consumes only the text before the first `@`,
/// so a digest-suffixed tag is exempt from the parser's lexical domain
/// while an untouched raw string still must parse.
#[test]
fn split_prefix_member_exempts_digest_suffixes_from_the_parser_domain() {
    let src = indoc! {r#"
        {{- $version := (split "@" .Values.image.tag)._0 -}}
        {{- if not (semverCompare ">=1.20.0-0" $version) -}}
        {{- fail "unsupported version" -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            image:
              tag: 1.26.0
        "}),
    );

    for tag in ["1.26.0@sha256:0123abcd", "1.26.0"] {
        let instance = serde_json::json!({ "image": { "tag": tag } });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "digest and plain forms render: instance={instance}; schema={schema}"
        );
    }
    let instance = serde_json::json!({ "image": { "tag": "garbage" } });
    assert!(
        !schema_accepts_instance(&schema, &instance),
        "an undelimited raw string must satisfy the parser: \
         instance={instance}; schema={schema}"
    );
}

/// a helper's replace/split chain keeps its escape-qualified identity
/// across the include boundary, so the consumer-side parser weakens its
/// pattern by the same tokens instead of projecting the final language onto
/// raw inputs (traefik's `traefik.proxyVersion`).
#[test]
fn helper_replace_chain_keeps_escape_qualified_identity_at_the_consumer() {
    let src = indoc! {r#"
        {{- $version := include "repro.proxyVersion" $ }}
        {{- if semverCompare "<1.0.0-0" $version }}
        {{- fail "unsupported version" -}}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
    "#};
    let helpers = indoc! {r#"
        {{- define "repro.proxyVersion" -}}
          {{- $version := (split "@" (default "9.9.9" $.Values.image.tag))._0 | replace "latest-" "" | replace "master" "9.9.9" }}
          {{- $version -}}
        {{- end -}}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(indoc! {"
            image:
              tag:
        "}),
    );

    for tag in [
        serde_json::json!("latest-9.8.7"),
        serde_json::json!("master"),
        serde_json::json!("9.8.7@sha256:0123abcd"),
        serde_json::json!("9.8.7"),
        serde_json::json!(null),
    ] {
        let instance = serde_json::json!({ "image": { "tag": tag } });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "transformed and defaulted tags render: instance={instance}; schema={schema}"
        );
    }
    for tag in ["latest", "audit"] {
        let instance = serde_json::json!({ "image": { "tag": tag } });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a raw string the chain leaves untouched must satisfy the parser: \
             instance={instance}; schema={schema}"
        );
    }
}

/// a later dynamic transform in the same chain produces text the
/// escape tokens cannot account for, so the parser must abstain for the
/// whole path instead of firing with the earlier stage's weakened pattern.
#[test]
fn dynamic_transform_after_escape_stage_makes_the_parser_abstain() {
    let src = indoc! {r#"
        {{- $version := .Values.image.tag | replace "latest-" "" | replace .Values.image.strip "" -}}
        {{- if not (semverCompare ">=1.20.0-0" $version) -}}
        {{- fail "unsupported version" -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            image:
              tag: 1.26.0
              strip: xyz
        "}),
    );

    let instance = serde_json::json!({ "image": { "tag": "garbage", "strip": "garbage" } });
    assert!(
        schema_accepts_instance(&schema, &instance),
        "a dynamic replace can rewrite any raw string, so the lexical domain abstains: \
         instance={instance}; schema={schema}"
    );
}

/// A local selected from several source paths does not identify which raw
/// path reaches the parser. Until that value remains branch-partitioned, the
/// lexical contract must abstain instead of constraining inactive candidates.
#[test]
fn semver_parser_abstains_across_unpartitioned_local_choices() {
    let src = indoc! {r#"
        {{- $version := ternary .Values.primaryVersion .Values.fallbackVersion .Values.usePrimary }}
        {{- if semverCompare ">=1.20.0" $version }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            usePrimary: true
            primaryVersion: v1.30.0
            fallbackVersion: v1.29.0
        "}),
    );

    for instance in [
        serde_json::json!({
            "usePrimary": true,
            "primaryVersion": "v1.30.0",
            "fallbackVersion": "garbage"
        }),
        serde_json::json!({
            "usePrimary": false,
            "primaryVersion": "garbage",
            "fallbackVersion": "v1.30.0"
        }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "an inactive candidate does not inherit the selected local's parser domain: \
             instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn ternary_selected_semver_local_keeps_candidate_partitions() {
    let src = indoc! {r#"
        {{- $version := ternary .Values.primaryVersion .Values.fallbackVersion .Values.usePrimary }}
        {{- if semverCompare ">=1.20.0" $version }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            usePrimary: true
            primaryVersion: v1.30.0
            fallbackVersion: v1.29.0
        "}),
    );

    for instance in [
        serde_json::json!({
            "usePrimary": true,
            "primaryVersion": "v1.30.0",
            "fallbackVersion": "garbage"
        }),
        serde_json::json!({
            "usePrimary": false,
            "primaryVersion": "garbage",
            "fallbackVersion": "v1.30.0"
        }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the inactive parser candidate stays unconstrained: instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({
            "usePrimary": true,
            "primaryVersion": "garbage",
            "fallbackVersion": "v1.30.0"
        }),
        serde_json::json!({
            "usePrimary": false,
            "primaryVersion": "v1.30.0",
            "fallbackVersion": "garbage"
        }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "the selected parser candidate must be lexical semver: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn duration_and_url_parsers_bind_lexical_domains() {
    let src = indoc! {r"
        {{- if .Values.enabled }}
        {{- $_ := mustDateModify .Values.duration (now) }}
        {{- $_ := urlParse .Values.url }}
        {{- end }}
    "};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            enabled: true
            duration: 30s
            url: https://example.com
        "}),
    );

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "enabled": true,
                "duration": "1h30m",
                "url": "https://example.com/a%20b"
            })
        ),
        "valid parser inputs render: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "enabled": true,
                "duration": "garbage",
                "url": "https://example.com"
            })
        ),
        "an invalid duration aborts the live call: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "enabled": true,
                "duration": "30s",
                "url": "http://%zz"
            })
        ),
        "an invalid URL escape aborts the live call: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "enabled": false,
                "duration": "garbage",
                "url": "http://%zz"
            })
        ),
        "the dead outer branch invokes neither parser: {schema}"
    );
}

#[test]
fn semver_parser_projects_through_identity_helper_output() {
    let src = indoc! {r#"
        {{- if semverCompare ">=1.20.0" (include "app.version" .) }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        {{- end }}
    "#};
    let helpers = indoc! {r#"
        {{- define "app.version" -}}
        {{- default .Capabilities.KubeVersion.Version .Values.kubeVersion -}}
        {{- end -}}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("kubeVersion: v1.30.0\n"),
    );

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "kubeVersion": "v1.30.0" })),
        "a valid helper-returned version renders: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "kubeVersion": "garbage" })),
        "the helper-returned raw value must satisfy semver syntax: {schema}"
    );
}

/// Structural guards, rather than operand truthiness, decide whether a
/// strict collection call executes. A `with` skips every Helm-empty value,
/// while a truthy wrong-kind value reaches `merge` and aborts rendering.
#[test]
fn guarded_collection_call_keeps_skipped_falsy_operands() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- with .Values.optional }}
          merged: {{ merge $.Values.optional (dict) | toYaml | quote }}
          {{- end }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("optional: {}\n"));

    for optional in [
        serde_json::json!(false),
        serde_json::json!(0),
        serde_json::json!(""),
    ] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "optional": optional })),
            "with skips Helm-empty operands before merge executes: {schema}"
        );
    }
    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "optional": { "a": 1 } })),
        "an object reaches merge successfully: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "optional": "bad" })),
        "a truthy string enters with and reaches merge: {schema}"
    );
}

/// `default` and `coalesce` select only truthy source candidates, so their
/// raw paths carry conditional operand contracts even though an
/// unconditional `merge` consumes the selected result.
#[test]
fn fallback_selected_collection_operands_are_truthy_scoped() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          defaulted: {{ merge (default (dict) .Values.defaulted) (dict) | toYaml | quote }}
          coalesced: {{ merge (coalesce .Values.first .Values.second (dict)) (dict) | toYaml | quote }}
    "};
    let values_yaml = indoc! {"
        defaulted: {}
        first: {}
        second: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for defaulted in [
        serde_json::json!(false),
        serde_json::json!(0),
        serde_json::json!(""),
        serde_json::json!({ "a": 1 }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "defaulted": defaulted })),
            "default replaces empty operands and passes objects through: {schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "defaulted": "bad" })),
        "a truthy string remains selected and reaches merge: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "first": false, "second": { "a": 1 } })
        ),
        "coalesce skips the empty first candidate: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "first": false, "second": "bad" })
        ),
        "the first truthy coalesce candidate must satisfy merge: {schema}"
    );
}

/// Collection helpers with different output shapes still bind their input
/// domains: `hasKey`, `pick`, `keys`, and `values` require objects, while
/// `mustUniq` requires an array.
#[test]
fn additional_collection_function_catalogs_bind_operand_domains() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if hasKey .Values.labels "app" }}
          has-app: "true"
          {{- end }}
          picked: {{ pick .Values.options "app" | toYaml | quote }}
          unique: {{ mustUniq .Values.items | toYaml | quote }}
          keys: {{ keys .Values.keyed | join "," | quote }}
          values: {{ .Values.valued | values | toYaml | quote }}
    "#};
    let values_yaml = indoc! {"
        labels: {}
        options: {}
        items: []
        keyed: {}
        valued: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "labels": { "app": "test" },
                "options": { "app": "test" },
                "items": ["a", "a"],
                "keyed": { "app": "test" },
                "valued": { "app": "test" }
            })
        ),
        "the catalogued operand domains render: {schema}"
    );
    for instance in [
        serde_json::json!({ "labels": false }),
        serde_json::json!({ "labels": "bad" }),
        serde_json::json!({ "options": 0 }),
        serde_json::json!({ "options": [] }),
        serde_json::json!({ "items": "" }),
        serde_json::json!({ "items": { "a": 1 } }),
        serde_json::json!({ "keyed": [] }),
        serde_json::json!({ "valued": ["test"] }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "the call executes and rejects its wrong-kind operand: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn nested_range_has_key_binds_inner_members() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $provider, $dashboards := .Values.dashboards }}
          {{- range $name, $dashboard := $dashboards }}
          {{- if hasKey $dashboard "json" }}
          {{ $name }}: present
          {{- end }}
          {{- end }}
          {{- end }}
    "#};
    let ir = parse_ir(src);
    let schema = schema_for_values_yaml(ir, Some("dashboards: {}\n"));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "dashboards": { "default": { "audit": { "json": "{}" } } } })
        ),
        "object dashboard members satisfy hasKey: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "dashboards": { "default": { "audit": 7 } } })
        ),
        "hasKey rejects scalar dashboard members: {schema}"
    );
}

/// Control actions embedded in YAML block scalars evaluate their calls even
/// though the action itself has no rendered output row.
#[test]
fn block_scalar_condition_absorbs_comparison_contracts() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          config: |
            {{- if eq .Values.logLevel "debug" }}
            verbosity=high
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("logLevel: info\n"));

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "logLevel": "debug" })),
        "a string operand compares successfully: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "logLevel": { "a": 1 } })),
        "the block-scalar action still executes eq: {schema}"
    );
}

/// a strict string transform introduced by Sprig's regex family constrains
/// its dynamic subject even when a later total stringifier renders the
/// result.
#[test]
fn must_regex_replace_all_literal_binds_string_subject() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          tag: {{ mustRegexReplaceAllLiteral "[^a-z]" .Values.tag "-" | quote }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("tag: latest\n"));

    for tag in [serde_json::json!(""), serde_json::json!("v1")] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "tag": tag })),
            "string subjects render: {schema}"
        );
    }
    for tag in [
        serde_json::json!(false),
        serde_json::json!(7),
        serde_json::json!(["v1"]),
        serde_json::json!({ "tag": "v1" }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &serde_json::json!({ "tag": tag })),
            "the regex transform requires a Go string: {schema}"
        );
    }
}

/// Effects from calls inside a derived range header execute before the
/// range binds its item. `join` therefore erases the raw collection shape
/// even though the header itself renders no value.
#[test]
fn derived_range_header_absorbs_join_shape_erasure() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          namespaces: |-
            {{- range splitList "," (join "," .Values.namespaces) }}
            {{ . }}
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("namespaces: \"\"\n"));

    for namespaces in [
        serde_json::json!("a,b"),
        serde_json::json!(["a", "b"]),
        serde_json::json!({ "a": "b" }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "namespaces": namespaces })),
            "join accepts and stringifies both source forms: {schema}"
        );
    }
}

/// join is total: sprig `join` stringifies non-list operands instead
/// of failing, so a `join | split` chain accepts BOTH the comma-string and
/// the list form (kube-state-metrics `namespaces` shape).
#[test]
fn join_accepts_lists_and_strings_alike() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          namespaces: {{ join "," .Values.namespaces | quote }}
    "#};
    let values_yaml = "namespaces: \"\"\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "namespaces": "a,b" }),
        serde_json::json!({ "namespaces": ["a", "b"] }),
        serde_json::json!({ "namespaces": { "a": "b" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "join stringifies any operand: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn helper_range_header_keeps_join_conversion_boundary() {
    let helpers = indoc! {r#"
        {{- define "namespaces" -}}
        {{- $items := list -}}
        {{- if .Values.enabled -}}
          {{- if .Values.namespaces -}}
            {{- range $namespace := join "," .Values.namespaces | split "," -}}
              {{- $items = append $items (tpl $namespace $) -}}
            {{- end -}}
          {{- end -}}
        {{- end -}}
        {{ mustToJson $items }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- range include "namespaces" . | fromJsonArray }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(indoc! {"
            enabled: true
            namespaces: []
        "}),
    );

    for namespaces in [
        serde_json::json!(["default"]),
        serde_json::json!({ "audit": "namespace" }),
    ] {
        let instance = serde_json::json!({ "enabled": true, "namespaces": namespaces });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the helper's join formats either input shape: instance={instance}; schema={schema}"
        );
    }
}

/// A decoded helper list carries its exact nonempty predicate into the range
/// body, so provider rows stay dormant when the helper returns `[]`.
#[test]
fn helper_json_array_range_scopes_body_rows_by_payload_liveness() {
    let helpers = indoc! {r#"
        {{- define "namespaces" -}}
        {{- $items := list -}}
        {{- if and .Values.rbac.create .Values.roleName -}}
          {{- if .Values.namespaces -}}
            {{- range $namespace := join "," .Values.namespaces | split "," -}}
              {{- $items = append $items (tpl $namespace $) -}}
            {{- end -}}
          {{- end -}}
        {{- end -}}
        {{ mustToJson $items }}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        {{- range include "namespaces" . | fromJsonArray }}
        apiVersion: rbac.authorization.k8s.io/v1
        kind: RoleBinding
        metadata:
          name: test
          namespace: {{ . }}
        roleRef:
          apiGroup: rbac.authorization.k8s.io
          kind: ClusterRole
          name: {{ $.Values.roleName }}
        subjects: []
        {{- end }}
    "#};
    let finalized = parse_ir_with_helpers(source, helpers).finalize();
    let use_ = finalized
        .uses()
        .iter()
        .find(|use_| {
            use_.source_expr == "roleName"
                && use_.path == YamlPath(vec!["roleRef".to_string(), "name".to_string()])
        })
        .expect("roleRef name provider row");

    sim_assert_eq!(
        have: &use_.condition,
        want: &helm_schema_core::GuardDnf::from_guards([
            Guard::Truthy {
                path: "namespaces".to_string(),
            },
            Guard::Truthy {
                path: "rbac.create".to_string(),
            },
            Guard::Truthy {
                path: "roleName".to_string(),
            },
        ])
    );
}

#[test]
fn range_key_prefix_scopes_member_contract_to_matching_keys() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $key, $value := .Values.config }}
          {{- if hasPrefix "strict" $key }}
          {{ $key }}: |-
            {{ $value | trim }}
          {{- end }}
          {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("config: {}\n"));

    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "config": { "strictAudit": { "bad": true } } })
        ),
        "a matching map entry reaches trim and must be a string: {schema}"
    );
    for instance in [
        serde_json::json!({ "config": { "strictAudit": "policy" } }),
        serde_json::json!({ "config": { "unrelated": { "bad": true } } }),
        serde_json::json!({ "config": [] }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "only matching keys execute the value contract: instance={instance}; schema={schema}"
        );
    }
}

/// bitnami-redis master: the standalone/replication partition guard
/// sits under an outer `gt (int64 .Values.master.count) 0` header. The
/// coercing comparison cannot lower exactly, but a raw positive integer (or
/// an absent key whose declared default is one) provably satisfies it, so
/// the ternary's Boolean operand contract keeps firing there instead of
/// vanishing with the whole capture. (The scale-to-zero acceptance side —
/// `master.count: 0` keeps a non-Boolean input valid — is pinned on the
/// real chart in `chart_reaudit`, where the base stays open through the
/// chart's other serialized uses.)
#[test]
fn int_cast_guard_keeps_strict_operand_contract_alive() {
    let src = indoc! {r#"
        {{- if gt (int64 .Values.master.count) 0 -}}
        {{- if or (not (eq .Values.architecture "replication")) (not .Values.sentinel.enabled) }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: m
        data:
          allow: {{ ternary "no" "yes" .Values.auth.enabled | quote }}
        {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        master:
          count: 1
        architecture: replication
        sentinel:
          enabled: false
        auth:
          enabled: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    // Instances compose over the declared defaults: `master`, `sentinel`
    // and `auth` are navigated on every render, so a document dropping them
    // is the null-deleted state helm aborts on.
    for overrides in [
        serde_json::json!({ "architecture": "standalone", "auth": { "enabled": true } }),
        serde_json::json!({ "auth": { "enabled": true } }),
        serde_json::json!({ "auth": { "enabled": false } }),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance),
            "boolean operands and dead partitions render: instance={instance}; schema={schema}"
        );
    }
    // The coalesced document carries the declared `master.count: 1`; a
    // missing count was null-deleted and coerces to 0, closing the outer
    // gate.
    for instance in [
        serde_json::json!({
            "master": { "count": 1 },
            "architecture": "replication",
            "auth": { "enabled": "true" }
        }),
        serde_json::json!({
            "master": { "count": 1 },
            "architecture": "standalone",
            "auth": { "enabled": "true" }
        }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a live ternary condition requires a Go bool: instance={instance}; schema={schema}"
        );
    }
}

/// cluster-autoscaler `expanderPriorities`: a `kindIs "string"` arm
/// inside a block scalar proves the chart handles the raw-string form even
/// when the surrounding liveness header carries an undecodable `include`
/// condition — the dispatch alternative widens the declared-map base
/// instead of vanishing with the unlowerable guard set.
#[test]
fn type_dispatch_survives_approximate_liveness_guard() {
    let src = indoc! {r#"
        {{- if and (.Values.expanderPriorities) (include "ca.enabled" .) }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: x
        data:
          priorities: |-
        {{- if kindIs "string" .Values.expanderPriorities }}
        {{ .Values.expanderPriorities | indent 4 }}
        {{- else }}
        {{- range $k,$v := .Values.expanderPriorities }}
            {{ $k | int }}:
              {{- toYaml $v | nindent 6 }}
        {{- end -}}
        {{- end -}}
        {{- end }}
    "#};
    let helpers = indoc! {r#"
        {{- define "ca.enabled" -}}
        {{- if .Values.enabled -}}
        true
        {{- end -}}
        {{- end -}}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(indoc! {"
            expanderPriorities: {}
            enabled: true
        "}),
    );
    for instance in [
        serde_json::json!({ "expanderPriorities": indoc! {"
            10:
              - .*"} }),
        serde_json::json!({ "expanderPriorities": { "10": [".*"] } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "both dispatch arms render: instance={instance}; schema={schema}"
        );
    }
}

/// prometheus `server.remoteWrite`: the URL of every ranged member
/// reaches `tpl`, a strict string consumer, even though the consuming
/// include sits inside a foreign `range` selected by `eq $key
/// "prometheus.yml"` — the key-equality lowers to a has-key condition
/// instead of abstaining with the foreign iteration.
#[test]
fn ranged_member_field_contract_survives_foreign_key_selected_range() {
    let helpers = indoc! {r#"
        {{- define "prometheus.server.remoteWrite" -}}
        {{- $remoteWrites := list }}
        {{- range $remoteWrite := .Values.server.remoteWrite }}
          {{- $remoteWrites = tpl $remoteWrite.url $ | set $remoteWrite "url" | append $remoteWrites }}
        {{- end -}}
        {{ toYaml $remoteWrites }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: x
        data:
          {{- $root := . -}}
          {{- range $key, $value := .Values.serverFiles }}
          {{ $key }}: |
          {{- if eq $key "prometheus.yml" }}
          {{- if $root.Values.server.remoteWrite }}
          {{- include "prometheus.server.remoteWrite" $root | nindent 4 }}
          {{- end }}
          {{- end }}
          {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(indoc! {"
            server:
              remoteWrite: []
            serverFiles:
              prometheus.yml: {}
        "}),
    );
    for instance in [
        serde_json::json!({ "server": { "remoteWrite": [{ "url": "http://x" }] } }),
        serde_json::json!({ "server": { "remoteWrite": [] } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "string urls render through tpl: instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({
            "serverFiles": { "prometheus.yml": {} },
            "server": { "remoteWrite": [{ "url": 7 }] },
        }),
        serde_json::json!({
            "serverFiles": { "prometheus.yml": {} },
            "server": { "remoteWrite": [{}] },
        }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "tpl requires a string url on every member: instance={instance}; schema={schema}"
        );
    }
}

/// A digest strip through `regexReplaceAll "@.*$" … ""` and a `trimPrefix
/// "v"` are each the identity on raw strings not containing their token,
/// so the parser exempts `@`-suffixed and `v`-prefixed spellings while an
/// untouched non-version still terminates (cilium's Hubble UI tag check).
#[test]
fn regex_strip_and_trim_prefix_carry_the_parser_preimage() {
    let src = indoc! {r#"
        {{- if regexReplaceAll "@.*$" .Values.ui.backend.tag "" | trimPrefix "v" | semverCompare "<0.9.0" }}
        {{- fail "requires >=v0.9.0" }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            ui:
              backend:
                tag: v0.13.5
        "}),
    );

    for (instance, want) in [
        (
            serde_json::json!({ "ui": { "backend": { "tag": "v0.13.5" } } }),
            true,
        ),
        (
            serde_json::json!({ "ui": { "backend": { "tag": "v0.13.5@sha256:abc" } } }),
            true,
        ),
        (
            serde_json::json!({ "ui": { "backend": { "tag": "0.13.5" } } }),
            true,
        ),
        (
            serde_json::json!({ "ui": { "backend": { "tag": "garbage" } } }),
            false,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the transform chain exempts its tokens and constrains the rest: \
             instance={instance}; schema={schema}"
        );
    }
}

/// Go's `eq`/`ne` compare `nil` against anything, so a MISSING or null
/// member is a valid comparison operand (cilium's optional
/// `clusters[].enabled` defaulting through `ne $cluster.enabled false`);
/// only a present value of a different basic kind aborts.
#[test]
fn comparison_operands_accept_missing_and_null_members() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $name, $cluster := .Values.clusters }}
          {{- if ne $cluster.enabled false }}
          {{ $name }}: live
          {{- end }}
          {{- end }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("clusters: {}\n"));

    for (instance, want) in [
        // The member omitted entirely evaluates nil and stays truthy
        (
            serde_json::json!({ "clusters": { "a": { "ips": ["1.1.1.1"] } } }),
            true,
        ),
        (
            serde_json::json!({ "clusters": { "a": { "enabled": null } } }),
            true,
        ),
        (
            serde_json::json!({ "clusters": { "a": { "enabled": true } } }),
            true,
        ),
        (
            serde_json::json!({ "clusters": { "a": { "enabled": false } } }),
            true,
        ),
        (
            serde_json::json!({ "clusters": { "a": { "enabled": "audit" } } }),
            false,
        ),
        (
            serde_json::json!({ "clusters": { "a": { "enabled": 7 } } }),
            false,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "nil compares while a present wrong kind aborts: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A `default`-selected capability fallback keeps Helm-falsy raw overrides
/// renderable — directly and through a helper return: only a truthy
/// override reaches `semverCompare`'s parser (harbor's
/// `harbor.ingress.kubeVersion` helper feeding its ingress apiVersion
/// switch).
#[test]
fn helper_returned_default_keeps_falsy_parser_operands_open() {
    let helpers = indoc! {r#"
        {{- define "repro.kubeVersion" -}}
        {{- default .Capabilities.KubeVersion.Version .Values.kubeVersionOverride -}}
        {{- end -}}
    "#};
    let direct = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if semverCompare "<1.14-0" (default .Capabilities.KubeVersion.Version .Values.kubeVersionOverride) }}
          mode: legacy
          {{- else }}
          mode: modern
          {{- end }}
    "#};
    let via_helper = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if semverCompare "<1.14-0" (include "repro.kubeVersion" .) }}
          mode: legacy
          {{- else }}
          mode: modern
          {{- end }}
    "#};
    for src in [direct, via_helper] {
        let schema = schema_for_values_yaml(
            parse_ir_with_helpers(src, helpers),
            Some("kubeVersionOverride: \"\"\n"),
        );

        for (instance, want) in [
            // Helm-falsy overrides select the capability fallback and render
            (serde_json::json!({ "kubeVersionOverride": false }), true),
            (serde_json::json!({ "kubeVersionOverride": {} }), true),
            (serde_json::json!({ "kubeVersionOverride": [] }), true),
            (
                serde_json::json!({ "kubeVersionOverride": "v1.30.1" }),
                true,
            ),
            // A truthy non-semver string reaches the parser and aborts
            (
                serde_json::json!({ "kubeVersionOverride": "garbage" }),
                false,
            ),
        ] {
            assert!(
                schema_accepts_instance(&schema, &instance) == want,
                "only a truthy override reaches the semver parser: \
                 instance={instance}; schema={schema}"
            );
        }
    }
}

/// `regexMatch "64$" (typeOf x)` matches Go's `int64`/`float64` spellings
/// and nothing else, so the guard emits the field only for numeric values
/// and OMITS every other kind (sealed-secrets' `PodDisruptionBudget`
/// `pdb.minAvailable`/`pdb.maxUnavailable` arms). The derived predicate
/// must stay a type dispatch: the omitted complement renders and stays
/// open.
#[test]
fn regex_match_over_type_of_dispatches_numeric_kinds() {
    let src = indoc! {r#"
        apiVersion: policy/v1
        kind: PodDisruptionBudget
        metadata:
          name: test
        spec:
          {{- if regexMatch "64$" (typeOf .Values.pdb.minAvailable) }}
          minAvailable: {{ .Values.pdb.minAvailable }}
          {{- end }}
          {{- if regexMatch "64$" (typeOf .Values.pdb.maxUnavailable) }}
          maxUnavailable: {{ .Values.pdb.maxUnavailable }}
          {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {r#"
            pdb:
              minAvailable: 1
              maxUnavailable: ""
        "#}),
    );

    for (instance, want) in [
        // Non-numeric kinds fail the typeOf regex, so the guard omits the
        // field and rendering succeeds
        (
            serde_json::json!({ "pdb": { "minAvailable": "audit" } }),
            true,
        ),
        (serde_json::json!({ "pdb": { "minAvailable": 2 } }), true),
        (serde_json::json!({ "pdb": { "maxUnavailable": 1 } }), true),
        (
            serde_json::json!({ "pdb": { "maxUnavailable": "50%" } }),
            true,
        ),
        (
            serde_json::json!({ "pdb": { "maxUnavailable": [1] } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the typeOf regex dispatches kinds instead of hard-typing: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A chart value spelled as a JSON integer can reach Helm as `float64`
/// through a values file or `int64` through `--set`. A direct
/// `typeIs "int64"` test therefore cannot make its live branch a
/// schema-level consequence of `type: integer`.
#[test]
fn numeric_type_is_spelling_does_not_claim_transport_provenance() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if typeIs "int64" .Values.value }}
          encoded: {{ b64enc .Values.payload }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        value: 1
        payload: 7
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let instance = composed_instance(values_yaml, serde_json::json!({}));

    assert!(
        schema_accepts_instance(&schema, &instance),
        "values-file decoding can make the int64 branch dormant: \
         instance={instance}; schema={schema}"
    );
}

/// A `semverCompare` outer guard on a direct values path lowers to an exact
/// version-range pattern arm, so a `tpl` string contract inside the guarded
/// branch binds exactly when the comparison holds instead of abstaining
/// wholesale (airflow's webserver Deployment guards
/// `tpl .Values.config.webserver.base_url` behind
/// `semverCompare "<3.0.0" .Values.airflowVersion`).
#[test]
fn semver_guarded_string_contract_binds_conditionally() {
    let src = indoc! {r#"
        {{- if and .Values.webserver.enabled (semverCompare "<3.0.0" .Values.airflowVersion) }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          probe: {{ tpl .Values.baseUrl . }}
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        webserver:
          enabled: true
        airflowVersion: "3.2.2"
        baseUrl: "http://placeholder"
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    // Cases compose over the declared defaults: `.Values.webserver.enabled`
    // is navigated on every render.
    for (overrides, want) in [
        // Live version branch: the tpl subject must be a string.
        (
            serde_json::json!({
                "webserver": { "enabled": true },
                "airflowVersion": "2.11.0",
                "baseUrl": { "a": "b" }
            }),
            false,
        ),
        (
            serde_json::json!({
                "webserver": { "enabled": true },
                "airflowVersion": "v2.9",
                "baseUrl": { "a": "b" }
            }),
            false,
        ),
        (
            serde_json::json!({ "airflowVersion": "2.11.0", "baseUrl": "http://live" }),
            true,
        ),
        // Dead version branch (explicit and via the chart default): the
        // contract must not leak out of its guard.
        (
            serde_json::json!({ "airflowVersion": "3.2.2", "baseUrl": { "a": "b" } }),
            true,
        ),
        (serde_json::json!({ "baseUrl": { "a": "b" } }), true),
        // A prerelease version matches no bare comparator, so the guarded
        // branch is dead there too.
        (
            serde_json::json!({ "airflowVersion": "2.5.0-rc1", "baseUrl": { "a": "b" } }),
            true,
        ),
        // The faithful sibling conjunct still gates the arm.
        (
            serde_json::json!({
                "airflowVersion": "2.11.0",
                "webserver": { "enabled": false },
                "baseUrl": { "a": "b" }
            }),
            true,
        ),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the semver comparator arm scopes the tpl string contract: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// A boolean flag local carries no values-path identity, so an
/// `and $flag (hasKey …)` short circuit used to gate its `hasKey` capture on
/// an approximate stand-in — which dropped the enclosing guard and left the
/// operand claim unconditional. A statically-true flag contributes no guard
/// of its own, so the capture keeps exactly the ambient one.
#[test]
fn statically_true_flag_operands_keep_short_circuit_operand_contracts() {
    let src = indoc! {r#"
        {{- $shouldContinue := true }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.assert }}
          {{- if and $shouldContinue (hasKey .Values.cfg "a") }}
          key: {{ index .Values.cfg "a" }}
          {{- end }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        assert: true
        cfg:
          a: value
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (overrides, want) in [
        (serde_json::json!({ "cfg": { "a": "x" } }), true),
        // `hasKey` on a present non-map aborts under the live assertion.
        (serde_json::json!({ "cfg": 7 }), false),
        (serde_json::json!({ "cfg": "text" }), false),
        // The ambient guard survives: the operand claim is not unconditional.
        (serde_json::json!({ "assert": false, "cfg": 7 }), true),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the flag-guarded hasKey operand claim keeps its ambient guard: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// A branch that turns an initially-true local off leaves it truthy only in
/// the branch complement. The join must retain that condition instead of
/// unioning the untouched entry `true` back into an unconditional fact.
#[test]
fn kill_switch_reassignment_scopes_later_short_circuit_operands() {
    let src = indoc! {r#"
        {{- $shouldContinue := hasKey .Values.gate "enabled" }}
        {{- if .Values.stop }}
        {{- $shouldContinue = false }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if $shouldContinue }}
          encoded: {{ b64enc .Values.payload }}
          {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), None);

    for (instance, want) in [
        (
            serde_json::json!({
                "stop": false,
                "gate": { "enabled": true },
                "payload": 7
            }),
            false,
        ),
        (
            serde_json::json!({
                "stop": true,
                "gate": { "enabled": true },
                "payload": 7
            }),
            true,
        ),
        (
            serde_json::json!({ "stop": false, "gate": {}, "payload": 7 }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the b64enc call executes only before the kill switch: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// The unrolled traversal shape behind grafana's leaked-secret assertion:
/// each iteration tests `hasKey` on the host it stepped into last, so every
/// INTERMEDIATE host must be a map wherever it is present. The kill-switch
/// flag is what carries the earlier iterations' presence tests, and only its
/// exact decode lets the deeper hosts bind at all.
#[test]
fn unrolled_traversal_binds_every_intermediate_host_kind() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- $sensitive := list (dict "path" (list "database" "password")) }}
          {{- if .Values.assert }}
          {{- range $_, $secret := $sensitive }}
            {{- $currentMap := $.Values.cfg }}
            {{- $shouldContinue := true }}
            {{- range $index, $elem := $secret.path }}
              {{- if and $shouldContinue (hasKey $currentMap $elem) }}
                {{- if eq (len $secret.path) (add1 $index) }}
                  {{- if not (regexMatch "^expanded$" (index $currentMap $elem)) }}
                    {{- fail "leaked secret" }}
                  {{- end }}
                {{- else }}
                  {{- $currentMap = index $currentMap $elem }}
                {{- end }}
              {{- else }}
                {{- $shouldContinue = false }}
              {{- end }}
            {{- end }}
          {{- end }}
          {{- end }}
          rendered: "yes"
    "#};
    let values_yaml = indoc! {"
        assert: true
        cfg:
          database:
            password: expanded
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (overrides, want) in [
        (serde_json::json!({}), true),
        // A present scalar host aborts the walk's next `hasKey`.
        (serde_json::json!({ "cfg": { "database": 7 } }), false),
        (serde_json::json!({ "cfg": { "database": "text" } }), false),
        // The leaf claim still binds under the walk.
        (
            serde_json::json!({ "cfg": { "database": { "password": "plain" } } }),
            false,
        ),
        // Dormant assertion: the walk never runs.
        (
            serde_json::json!({ "assert": false, "cfg": { "database": 7 } }),
            true,
        ),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the unrolled walk binds each intermediate host: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// A typed Go parameter aborts on a NIL argument. `text/template` hands a
/// missing key to an argument position as a valid `interface{}` holding
/// nil, which is assignable to no declared parameter type, so the call
/// answers "wrong type for value; expected map[string]interface {}; got
/// interface {}" (cilium's `hasKey .Values.k8s …`, velero's
/// `hasKey .Values.configMaps …`, the `set` chain in nats'
/// `nats.defaultValues`). The truthy⇒kind capture beside this one can never
/// say it: absence is Helm-falsy, so its own guard excuses exactly the
/// state that aborts.
///
/// A PIPED nil is different — `evalPipeline` unwraps the interface, so the
/// operand arrives INVALID and `validateType` substitutes the parameter's
/// zero value whenever the type can be nil.
#[test]
fn map_parameter_calls_require_a_present_operand() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          probed: "{{ hasKey .Values.k8s "requireIPv4PodCIDR" }}"
          picked: {{ pick .Values.settings "a" | toYaml | quote }}
          piped: {{ .Values.optional | keys | toYaml | quote }}
    "#};
    let values_yaml = indoc! {"
        k8s: {}
        settings: {}
        optional: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (instance, want) in [
        (
            serde_json::json!({ "k8s": {}, "settings": {}, "optional": {} }),
            true,
        ),
        // The piped operand tolerates nil: `keys` receives an empty map.
        (serde_json::json!({ "k8s": {}, "settings": {} }), true),
        (
            serde_json::json!({ "k8s": {}, "settings": {}, "optional": null }),
            true,
        ),
        (serde_json::json!({ "settings": {}, "optional": {} }), false),
        (
            serde_json::json!({ "k8s": null, "settings": {}, "optional": {} }),
            false,
        ),
        (serde_json::json!({ "k8s": {}, "optional": {} }), false),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "a map parameter aborts on a nil ARGUMENT and tolerates a piped one: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// Sprig's reflection-backed helpers and Go's own `index`/`len` fault on a
/// nil operand in EITHER form: `len` answers "len of nil pointer", `index`
/// answers "index of untyped nil", and `deepCopy` walks the operand with
/// `copystructure`, which faults on a zero value (oauth2-proxy's
/// `len .Values.extraVolumes`, cilium's `index .Values.extraConfig …`,
/// airflow's `deepCopy .Values.airflowPodAnnotations`).
#[test]
fn reflection_backed_calls_require_a_present_operand() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          count: "{{ len .Values.extraVolumes }}"
          flag: "{{ index .Values.extraConfig "unsafe" }}"
          copied: {{ deepCopy .Values.podAnnotations | toYaml | quote }}
    "#};
    let values_yaml = indoc! {"
        extraVolumes: []
        extraConfig: {}
        podAnnotations: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let full = serde_json::json!({
        "extraVolumes": [], "extraConfig": {}, "podAnnotations": {},
    });
    assert!(
        schema_accepts_instance(&schema, &full),
        "the present operands render: schema={schema}"
    );
    for instance in [
        serde_json::json!({ "extraConfig": {}, "podAnnotations": {} }),
        serde_json::json!({ "extraVolumes": [], "podAnnotations": {} }),
        serde_json::json!({ "extraVolumes": [], "extraConfig": {} }),
        serde_json::json!({
            "extraVolumes": [], "extraConfig": {}, "podAnnotations": null,
        }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a reflection-backed call faults on a nil operand: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A derived range can retain one values-backed member domain while a
/// literal item makes the range live independently of that source. Runtime
/// effects in the body follow the range's exact execution condition, not the
/// raw source path's truthiness (Airflow's synthesized default worker set).
#[test]
fn reflection_call_in_synthesized_range_uses_range_execution_condition() {
    let src = indoc! {r#"
        {{- $workerSets := .Values.workers.celery.sets | default list -}}
        {{- if .Values.workers.celery.enableDefault -}}
          {{- $workerSets = concat (list (dict "name" "default")) $workerSets -}}
        {{- end -}}
        {{- range $workerSet := $workerSets -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ $workerSet.name }}
        data:
          copied: {{ deepCopy $.Values.podAnnotations | toYaml | quote }}
        {{- end -}}
    "#};
    let values_yaml = indoc! {"
        podAnnotations: {}
        workers:
          celery:
            enableDefault: true
            sets: []
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({
                "podAnnotations": {},
                "workers": { "celery": { "enableDefault": true, "sets": [] } },
            }),
            true,
            "the present operand renders in the synthesized iteration",
        ),
        (
            serde_json::json!({
                "workers": { "celery": { "enableDefault": true, "sets": [] } },
            }),
            false,
            "the synthesized item executes deepCopy even when sets is empty",
        ),
        (
            serde_json::json!({
                "workers": { "celery": { "enableDefault": false, "sets": [] } },
            }),
            true,
            "an empty derived range never evaluates deepCopy",
        ),
        (
            serde_json::json!({
                "workers": {
                    "celery": {
                        "enableDefault": false,
                        "sets": [{ "name": "custom" }],
                    },
                },
            }),
            false,
            "a values-backed item also evaluates deepCopy",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "derived-range reflection presence ({label}): \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// A `bool` parameter cannot hold nil, so BOTH forms abort: the argument
/// form with "wrong type for value; expected bool" (nack's
/// `ternary "Role" "ClusterRole" .Values.namespaced`) and the piped form
/// with "invalid value; expected bool" (external-dns' piped `ternary`).
#[test]
fn non_nilable_parameters_require_a_present_operand_in_either_form() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          kind: {{ ternary "Role" "ClusterRole" .Values.namespaced }}
          binding: {{ .Values.scoped | ternary "RoleBinding" "ClusterRoleBinding" }}
    "#};
    let values_yaml = indoc! {"
        namespaced: false
        scoped: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let full = serde_json::json!({ "namespaced": true, "scoped": false });
    assert!(
        schema_accepts_instance(&schema, &full),
        "the present operands render: schema={schema}"
    );
    for instance in [
        serde_json::json!({ "scoped": false }),
        serde_json::json!({ "namespaced": true }),
        serde_json::json!({ "namespaced": true, "scoped": null }),
        serde_json::json!({ "namespaced": null, "scoped": false }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a non-nilable parameter aborts on a nil operand: \
             instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn live_unset_requires_a_present_object_operand() {
    let src = indoc! {r#"
        {{- if semverCompare "<1.30.0" (printf "%d.%d.0" (semver .Capabilities.KubeVersion.Version).Major (semver .Capabilities.KubeVersion.Version).Minor) -}}
          {{- $_ := unset .Values.context "appArmorProfile" -}}
        {{- end -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          context: {{ toYaml .Values.context | quote }}
    "#};
    let values_yaml = indoc! {"
        context:
          appArmorProfile: RuntimeDefault
    "};
    let schema = schema_for_values_yaml(
        parse_ir_with_kubernetes_version(src, "1.29.0"),
        Some(values_yaml),
    );

    sim_assert_eq!(
        have: schema,
        want: serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "allOf": [
                {
                    "additionalProperties": {},
                    "properties": {
                        "context": { "type": "object" }
                    }
                },
                {
                    "if": {
                        "anyOf": [
                            {
                                "not": {
                                    "properties": { "context": {} },
                                    "required": ["context"],
                                    "type": "object"
                                }
                            },
                            {
                                "properties": {
                                    "context": { "enum": [null] }
                                },
                                "required": ["context"],
                                "type": "object"
                            }
                        ]
                    },
                    "then": false
                }
            ],
            "properties": {
                "context": {}
            },
            "type": "object"
        })
    );
}
