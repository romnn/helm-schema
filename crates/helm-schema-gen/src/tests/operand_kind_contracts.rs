use super::*;

/// F60: Go-template `eq`/`ne` against a STRING literal terminates on
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

/// F61: strict collection functions bind their operand domains: `merge` subjects
/// are maps, `concat` operands are lists, `len` needs a length-bearing value,
/// and `has` searches a list. The call itself does not skip Helm-empty operands.
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

    for instance in [
        serde_json::json!({ "overrides": { "a": 1 }, "defaults": { "b": 2 } }),
        serde_json::json!({ "extraEnv": [1], "env": [2] }),
        serde_json::json!({ "extraVolumes": ["v"] }),
        serde_json::json!({ "extraVolumes": "vols" }),
        serde_json::json!({ "collectors": ["certs"] }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the accepted operand kinds render: instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "overrides": "s" }),
        serde_json::json!({ "overrides": false }),
        serde_json::json!({ "overrides": 0 }),
        serde_json::json!({ "overrides": "" }),
        serde_json::json!({ "extraEnv": "s" }),
        serde_json::json!({ "extraEnv": false }),
        serde_json::json!({ "extraEnv": 0 }),
        serde_json::json!({ "extraEnv": "" }),
        serde_json::json!({ "extraVolumes": 7 }),
        serde_json::json!({ "collectors": 7 }),
        serde_json::json!({ "collectors": false }),
        serde_json::json!({ "collectors": "" }),
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
    let schema = schema_for_values_yaml(parse_ir(src), Some("healthPort: null\nsettings: {}\n"));

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

/// F66: an unconditional strict call evaluates Helm-falsy operands too.
/// Only structural control flow or fallback selection may make its domain conditional.
#[test]
fn unconditional_strict_call_rejects_falsy_wrong_kinds() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          merged: {{ merge .Values.config (dict) | toYaml | quote }}
    "#};
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

    for instance in [
        serde_json::json!({ "lengthBearing": "text" }),
        serde_json::json!({ "lengthBearing": { "key": "value" } }),
        serde_json::json!({ "ordered": ["a", "b"] }),
        serde_json::json!({ "mode": "active" }),
        serde_json::json!({ "integerLimit": 1 }),
        serde_json::json!({ "floatLimit": 1.5 }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "a runtime-compatible pipeline operand must validate: \
             instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "lengthBearing": 7 }),
        serde_json::json!({ "ordered": { "a": 1 } }),
        serde_json::json!({ "mode": { "active": true } }),
        serde_json::json!({ "integerLimit": 1.5 }),
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
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.enabled }}
          direct: {{ ternary "yes" "no" .Values.direct | quote }}
          pipeline: {{ .Values.pipeline | ternary "yes" "no" | quote }}
          computed-direct: {{ ternary "yes" "no" (eq .Values.mode "active") | quote }}
          computed-pipeline: {{ .Values.pipelineMode | eq "active" | ternary "yes" "no" | quote }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        enabled: true
        direct: true
        pipeline: false
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
                "mode": "active",
                "pipelineMode": "inactive"
            })
        ),
        "raw Boolean and computed Boolean selectors render in both call forms: {schema}"
    );
    for instance in [
        serde_json::json!({ "enabled": true, "direct": "true", "pipeline": false }),
        serde_json::json!({ "enabled": true, "direct": true, "pipeline": 1 }),
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
        Some("enabled: true\nkubeVersion: v1.30.0\npipelineVersion: v1.30.0\n"),
    );

    for version in ["v1.30.0", "1", "1.2", "01.002.0003-alpha.1+build.7"] {
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
    for version in ["garbage", "v", "1..2"] {
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
        Some("usePrimary: true\nprimaryVersion: v1.30.0\nfallbackVersion: v1.29.0\n"),
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

/// Structural guards, rather than operand truthiness, decide whether a
/// strict collection call executes. A `with` skips every Helm-empty value,
/// while a truthy wrong-kind value reaches `merge` and aborts rendering.
#[test]
fn guarded_collection_call_keeps_skipped_falsy_operands() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- with .Values.optional }}
          merged: {{ merge $.Values.optional (dict) | toYaml | quote }}
          {{- end }}
    "#};
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
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          defaulted: {{ merge (default (dict) .Values.defaulted) (dict) | toYaml | quote }}
          coalesced: {{ merge (coalesce .Values.first .Values.second (dict)) (dict) | toYaml | quote }}
    "#};
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
          {{ $name }}: {{ $dashboard.json | quote }}
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

/// F45: a strict string transform introduced by Sprig's regex family constrains
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

    for namespaces in [serde_json::json!("a,b"), serde_json::json!(["a", "b"])] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "namespaces": namespaces })),
            "join accepts and stringifies both source forms: {schema}"
        );
    }
}

/// F61 (join is total): sprig `join` stringifies non-list operands instead
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
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "join stringifies any operand: instance={instance}; schema={schema}"
        );
    }
}
