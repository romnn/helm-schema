use super::*;

fn strict_provider() -> Chain {
    Chain::new(vec![Box::new(
        KubernetesJsonSchemaProvider::new("v1.29.0-standalone-strict")
            .with_cache_dir(super::bundle_cache_dir())
            .with_allow_download(false),
    )])
}

/// bitnami-redis master: a values-selected `kind:` crossed with a
/// helper-resolved apiVersion partitions the strategy slot per kind. The
/// Deployment arm places `spec.strategy` (no `rollingUpdate.partition`),
/// every other arm places `spec.updateStrategy` (no `maxSurge`), and the
/// provider projection must follow the selected partition instead of
/// blending the kinds.
#[test]
fn values_selected_kind_partitions_strategy_provider_projection() {
    let helpers = indoc! {r#"
        {{- define "common.capabilities.statefulset.apiVersion" -}}
        {{- print "apps/v1" -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: {{ include "common.capabilities.statefulset.apiVersion" . }}
        kind: {{ .Values.master.kind }}
        metadata:
          name: test
        spec:
          {{- if not (eq .Values.master.kind "DaemonSet") }}
          replicas: {{ .Values.master.count }}
          {{- end }}
          {{- if (eq .Values.master.kind "StatefulSet") }}
          serviceName: test-headless
          {{- end }}
          {{- if .Values.master.updateStrategy }}
          {{- if (eq .Values.master.kind "Deployment") }}
          strategy: {{- toYaml .Values.master.updateStrategy | nindent 4 }}
          {{- else }}
          updateStrategy: {{- toYaml .Values.master.updateStrategy | nindent 4 }}
          {{- end }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        master:
          kind: StatefulSet
          count: 1
          updateStrategy:
            type: RollingUpdate
    "};
    let signals = schema_signals_for(parse_ir_with_helpers(src, helpers));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &strict_provider()).with_values_yaml(Some(values_yaml)),
    );

    for instance in [
        serde_json::json!({ "master": { "kind": "Deployment", "updateStrategy": { "rollingUpdate": { "maxSurge": "25%" } } } }),
        serde_json::json!({ "master": { "kind": "StatefulSet", "updateStrategy": { "rollingUpdate": { "partition": 1 } } } }),
        serde_json::json!({ "master": { "kind": "StatefulSet", "updateStrategy": { "type": "RollingUpdate" } } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the strategy field set matching the selected kind renders and validates: \
             instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "master": { "kind": "Deployment", "updateStrategy": { "rollingUpdate": { "partition": 1 } } } }),
        serde_json::json!({ "master": { "kind": "StatefulSet", "updateStrategy": { "rollingUpdate": { "maxSurge": "25%" } } } }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a strategy field from the OTHER kind's schema is rejected on this partition: \
             instance={instance}; schema={schema}"
        );
    }
}

/// nfs-subdir-external-provisioner: `maxUnavailable` flows through
/// `default 1` into a PodDisruptionBudget's int-or-string slot, so the
/// declared integer default documents intent without narrowing away the
/// provider-accepted percentage string.
#[test]
fn pdb_int_or_string_survives_declared_integer_default() {
    let helpers = indoc! {r#"
        {{- define "pdb.apiVersion" -}}
        {{- if semverCompare ">=1.21-0" .Capabilities.KubeVersion.GitVersion -}}
        {{- print "policy/v1" -}}
        {{- else -}}
        {{- print "policy/v1beta1" -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if .Values.podDisruptionBudget.enabled }}
        apiVersion: {{ template "pdb.apiVersion" . }}
        kind: PodDisruptionBudget
        metadata:
          name: test
        spec:
          maxUnavailable: {{ .Values.podDisruptionBudget.maxUnavailable | default 1 }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        podDisruptionBudget:
          enabled: false
          maxUnavailable: 1
    "};
    let signals = schema_signals_for(parse_ir_with_helpers(src, helpers));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &strict_provider()).with_values_yaml(Some(values_yaml)),
    );

    for instance in [
        serde_json::json!({ "podDisruptionBudget": { "enabled": true, "maxUnavailable": "50%" } }),
        serde_json::json!({ "podDisruptionBudget": { "enabled": true, "maxUnavailable": 1 } }),
        serde_json::json!({ "podDisruptionBudget": { "enabled": true } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the provider slot accepts int-or-string and the default covers absence: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "podDisruptionBudget": { "enabled": true, "maxUnavailable": { "a": 1 } }
            })
        ),
        "a live mapping is neither integer nor string: {schema}"
    );
}

/// loki gateways: `hostUsers` renders ONLY where `kindIs "bool"` says so —
/// every other kind is silently omitted and the chart still renders — so
/// the declared default's string intent must not close the path against
/// maps: the self dispatch proves the complement never reaches the sink.
#[test]
fn self_kind_dispatch_keeps_complement_kinds_open() {
    let helpers = indoc! {r#"
        {{- define "test.kubeVersion" -}}
        {{- .Capabilities.KubeVersion.Version -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          {{- if and (semverCompare ">=1.33-0" (include "test.kubeVersion" .)) (kindIs "bool" .Values.hostUsers) }}
          hostUsers: {{ .Values.hostUsers }}
          {{- end }}
          containers:
            - name: main
    "#};
    let signals = schema_signals_for(parse_ir_with_helpers(src, helpers));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &strict_provider())
            .with_values_yaml(Some("hostUsers: nil\n")),
    );
    assert!(
        schema
            .get("properties")
            .and_then(|properties| properties.get("hostUsers"))
            .is_some(),
        "the dispatched path stays a referenced property: {schema}"
    );
    for instance in [
        serde_json::json!({ "hostUsers": { "a": 1 } }),
        serde_json::json!({ "hostUsers": true }),
        serde_json::json!({ "hostUsers": "nil" }),
        serde_json::json!({ "hostUsers": 7 }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "a kind outside the dispatch is omitted, not rejected: \
             instance={instance}; schema={schema}"
        );
    }
}

/// vault's affinity helper: `typeOf` dispatch selects `tpl` for strings
/// and `toYaml` for everything else, so structured affinity values are
/// chart-handled and must validate against the provider slot instead of
/// being rejected as non-strings.
#[test]
fn type_of_dispatch_keeps_serialized_arm_structured() {
    let helpers = indoc! {r#"
        {{- define "test.affinity" -}}
          {{- if .Values.affinity }}
      affinity:
        {{ $tp := typeOf .Values.affinity }}
        {{- if eq $tp "string" }}
          {{- tpl .Values.affinity . | nindent 8 | trim }}
        {{- else }}
          {{- toYaml .Values.affinity | nindent 8 }}
        {{- end }}
          {{- end }}
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
              {{ template "test.affinity" . }}
              containers:
                - name: main
    "#};
    let signals = schema_signals_for(parse_ir_with_helpers(src, helpers));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &strict_provider())
            .with_values_yaml(Some("affinity: {}\n")),
    );
    for (instance, want) in [
        (
            serde_json::json!({ "affinity": { "nodeAffinity": {
                "requiredDuringSchedulingIgnoredDuringExecution": { "nodeSelectorTerms": [] }
            } } }),
            true,
        ),
        (serde_json::json!({ "affinity": "nodeAffinity: {}" }), true),
        (
            serde_json::json!({ "affinity": { "nodeAffinity": 7 } }),
            false,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the toYaml arm keeps provider typing for structured values: \
             instance={instance}; schema={schema}"
        );
    }
}
