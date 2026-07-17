use super::*;

fn strict_provider() -> Chain {
    Chain::new(vec![Box::new(
        KubernetesJsonSchemaProvider::new("v1.29.0-standalone-strict").with_allow_download(true),
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
