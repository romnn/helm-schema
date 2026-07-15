use test_util::prelude::sim_assert_eq;

use super::*;

#[test]
fn self_guarded_fragment_object_keeps_exact_empty_object_placeholder() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: PersistentVolumeClaim
        spec:
          {{- with .Values.dataSource }}
          dataSource: {{- toYaml . | nindent 4 }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        dataSource: {}
    "};

    let ir = parse_ir(src);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    // A `with`-guarded `toYaml` fragment is TOTAL (F56): the declared-empty
    // off-state and every user-supplied shape stay valid.
    for instance in [
        serde_json::json!({ "dataSource": {} }),
        serde_json::json!({ "dataSource": { "name": "snap", "kind": "VolumeSnapshot" } }),
        serde_json::json!({}),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the off-state and arbitrary fragment shapes render: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn self_guarded_tplvalues_render_object_union_keeps_exact_empty_object_placeholder() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: PersistentVolumeClaim
        spec:
          {{- if .Values.persistence.dataSource }}
          dataSource: {{- include "common.tplvalues.render" (dict "value" .Values.persistence.dataSource "context" .) | nindent 4 }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        persistence:
          dataSource: {}
    "};
    let helpers = bitnami_tplvalues_helpers();

    let mut define_index = DefineIndex::new();
    define_index.add_file_source("helpers.tpl", helpers);
    let schema_signals = SymbolicIrContext::new(&define_index)
        .generate_contract_ir(src)
        .finalize()
        .into_schema_signals();
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider()).with_values_yaml(Some(values_yaml)),
    );
    let data_source = schema
        .pointer("/properties/persistence/properties/dataSource")
        .expect("persistence.dataSource present");

    any_of_variant_matching(data_source, |variant| {
        variant.get("type").and_then(Value::as_str) == Some("object")
            && variant.get("maxProperties").and_then(Value::as_u64) == Some(0)
    })
    .unwrap_or_else(|| {
        panic!(
            "exact empty object placeholder variant missing from helper-rendered object union: {data_source}",
        )
    });
}

#[test]
fn self_guarded_range_collection_keeps_exact_empty_object_placeholder() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: app
              image: busybox
              env:
              {{- range .Values.env }}
                - name: {{ .name }}
                  {{- if .valueFrom }}
                  valueFrom: {{- toYaml .valueFrom | nindent 20 }}
                  {{- else }}
                  value: {{ .value | quote }}
                  {{- end }}
              {{- end }}
    "#};
    let values_yaml = indoc! {"
        env: {}
    "};

    let ir = parse_ir(src);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));
    let env = schema.pointer("/properties/env").expect("env present");

    any_of_variant_matching(env, |variant| {
        variant.get("type").and_then(Value::as_str) == Some("object")
            && variant.get("maxProperties").and_then(Value::as_u64) == Some(0)
    })
    .unwrap_or_else(|| panic!("exact empty object off-state missing: {env}; ir={ir:?}",));

    any_of_variant_matching(env, |variant| {
        variant.get("type").and_then(Value::as_str) == Some("array")
    })
    .unwrap_or_else(|| panic!("non-empty array form missing: {env}"));
}

#[test]
fn guard_only_empty_map_default_stays_open_object() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
          {{- if .Values.config }}
          annotations:
            config-enabled: "true"
          {{- end }}
        spec:
          containers:
            - name: app
              image: busybox
    "#};
    let values_yaml = indoc! {"
        config: {}
    "};

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let config = schema
        .pointer("/properties/config")
        .expect("config present");
    sim_assert_eq!(
        have: config.get("type").and_then(Value::as_str),
        want: Some("object"),
        "guard-only empty-map default should keep the values.yaml object evidence, got {config}"
    );
    sim_assert_eq!(
        have: config
            .get("additionalProperties")
            .and_then(Value::as_object)
            .map(serde_json::Map::len),
        want: Some(0),
        "guard-only empty-map default should remain open, got {config}"
    );
    assert!(
        config.get("anyOf").is_none(),
        "guard-only empty-map default should not become an exact-empty-or-boolean union, got {config}"
    );
}

/// The temporal chart declares `imagePullSecrets: {}` and splices it whole
/// (`with` + `toYaml`) into a Kubernetes LIST position. The shipped empty-map
/// off-state AND the real list form must both validate; the luup3 gate caught
/// a round-2 state where the list typing squeezed out the declared default.
#[test]
fn with_guarded_whole_splice_accepts_empty_map_default_and_list_form() {
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: repro
        spec:
          template:
            spec:
              {{- with .Values.imagePullSecrets }}
              imagePullSecrets:
              {{- toYaml . | nindent 8 }}
              {{- end }}
              containers:
                - name: app
                  image: busybox
    "#};
    let values_yaml = indoc! {"
        imagePullSecrets: {}
    "};

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "imagePullSecrets": {} })),
        "the declared empty-map off-state must stay accepted: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "imagePullSecrets": [{ "name": "regcred" }] })
        ),
        "the rendered list form must stay accepted: {schema}"
    );
}

/// An UNDECLARED map the chart itself iterates (istiod's `env` has no
/// values.yaml default) is user-populated; a typed member guard must not
/// close it.
#[test]
fn undeclared_self_ranged_map_stays_open() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: repro
        data:
          {{- if not .Values.env.FORCE }}
          forced: "no"
          {{- end }}
          {{- range $key, $val := .Values.env }}
          {{ $key }}: {{ $val | quote }}
          {{- end }}
    "#};

    let schema = schema_for_values_yaml(parse_ir(src), None);
    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "env": { "ANY_KEY": "x" } })),
        "user-populated entries of an undeclared ranged map must stay accepted: {schema}"
    );
}

/// A declared-empty map spliced whole through `toYaml` (cert-manager's
/// `config`) is user-populated even when guard reads probe typed members;
/// the open arm of its off-state union hosts the members without closing.
#[test]
fn serialized_empty_map_union_keeps_open_arm_for_members() {
    let src = indoc! {r#"
        {{- if .Values.config.apiVersion }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: versioned
        data:
          v: {{ .Values.config.apiVersion | quote }}
        {{- end }}
        ---
        {{- with .Values.config }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: repro
        data:
          config.yaml: |
        {{ toYaml .Values.config | indent 4 }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        config: {}
    "};

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    for instance in [
        serde_json::json!({ "config": {} }),
        serde_json::json!({ "config": { "userField": true } }),
        serde_json::json!({ "config": { "apiVersion": "controller.config/v1alpha1" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "serialized user-populated map must accept {instance}: {schema}"
        );
    }
}

/// A guard probing one literal member of a user-populated map (datadog's
/// `envDict.HELM_FORCE_RENDER` pattern) must not close the map: the map is
/// declared `{}` and consumed by a helper that ranges over its entries, so
/// arbitrary user keys stay accepted alongside the probed member.
#[test]
fn member_probe_keeps_helper_ranged_empty_map_open() {
    let helpers = indoc! {r#"
        {{- define "repro.entries" -}}
        {{- range $key, $value := . }}
        - name: {{ $key }}
          value: {{ $value | quote }}
        {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if not .Values.envDict.FORCE }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: guarded
        data:
          on: "true"
        {{- end }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: repro
        data:
          entries: |
        {{- include "repro.entries" .Values.envDict | indent 4 }}
    "#};
    let values_yaml = indoc! {"
        envDict: {}
    "};

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    let env_dict = schema
        .pointer("/properties/envDict")
        .expect("envDict present");
    assert!(
        env_dict.get("additionalProperties") != Some(&Value::Bool(false)),
        "member probe must not close the user-populated map, got {env_dict}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "envDict": { "USER_KEY": "value" } })
        ),
        "user-populated entries must stay accepted, got {env_dict}"
    );
}
