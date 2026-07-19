use super::*;

/// cilium `upgradeCompatibility`: a strict parser applied to
/// `default LITERAL .Values.path` only ever sees the raw value on the truthy
/// arm. Every Helm-empty input (`false`, `0`, `""`, `{}`, `[]`, `null`)
/// selects the literal fallback and renders, so the raw path must stay open
/// for the whole Helm-falsy set while truthy inputs keep the parser domain.
#[test]
fn default_literal_fallback_keeps_helm_empty_inputs_open_for_parsers() {
    let src = indoc! {r#"
        {{- if semverCompare ">=1.8" (default "1.8" .Values.upgradeCompatibility) }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), None);

    for instance in [
        serde_json::json!({}),
        serde_json::json!({ "upgradeCompatibility": null }),
        serde_json::json!({ "upgradeCompatibility": false }),
        serde_json::json!({ "upgradeCompatibility": 0 }),
        serde_json::json!({ "upgradeCompatibility": "" }),
        serde_json::json!({ "upgradeCompatibility": {} }),
        serde_json::json!({ "upgradeCompatibility": [] }),
        serde_json::json!({ "upgradeCompatibility": "1.14" }),
        serde_json::json!({ "upgradeCompatibility": "v1.2.3-rc.1" }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "a Helm-empty input selects the fallback and never reaches the parser: \
             instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "upgradeCompatibility": "garbage" }),
        serde_json::json!({ "upgradeCompatibility": { "a": 1 } }),
        serde_json::json!({ "upgradeCompatibility": [1] }),
        serde_json::json!({ "upgradeCompatibility": true }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a truthy non-semver input survives selection and aborts semverCompare: \
             instance={instance}; schema={schema}"
        );
    }
}

/// cloudnative-pg `nameOverride`: the fullname helpers select
/// `default .Chart.Name .Values.nameOverride` before `trunc`/`contains`, so
/// the string contract binds only truthy raw values even when the consuming
/// template is guarded by an unrelated liveness switch.
#[test]
fn helper_default_fallback_keeps_helm_empty_inputs_open_for_string_consumers() {
    let helpers = indoc! {r#"
        {{- define "chart.name" -}}
        {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
        {{- end }}
        {{- define "chart.fullname" -}}
        {{- $name := default .Chart.Name .Values.nameOverride }}
        {{- if contains $name .Release.Name }}
        {{- .Release.Name | trunc 63 | trimSuffix "-" }}
        {{- else }}
        {{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
        {{- end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        {{- if .Values.config.create }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ include "chart.fullname" . }}
          labels:
            app: {{ include "chart.name" . }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("nameOverride: \"\"\nconfig:\n  create: true\n"),
    );

    for instance in [
        serde_json::json!({}),
        serde_json::json!({ "nameOverride": null }),
        serde_json::json!({ "nameOverride": false }),
        serde_json::json!({ "nameOverride": "" }),
        serde_json::json!({ "nameOverride": {} }),
        serde_json::json!({ "nameOverride": "custom-name" }),
        serde_json::json!({ "nameOverride": false, "config": { "create": true } }),
        serde_json::json!({ "nameOverride": {}, "config": { "create": true } }),
        serde_json::json!({ "nameOverride": "custom", "config": { "create": true } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "Helm-empty overrides take the chart-name fallback and render: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "nameOverride": { "a": 1 }, "config": { "create": true } })
        ),
        "a live truthy map survives selection and aborts trunc: {schema}"
    );
}

/// cloudnative-pg `namespaceOverride`: a raw value read only inside its
/// own truthy `if` arm is skipped entirely for every Helm-falsy input, so a
/// downstream string contract must not exclude those inputs.
#[test]
fn truthy_guarded_read_keeps_helm_falsy_inputs_open() {
    let helpers = indoc! {r#"
        {{- define "chart.namespace" -}}
        {{- if .Values.namespaceOverride -}}
        {{- .Values.namespaceOverride -}}
        {{- else -}}
        {{- .Release.Namespace -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if .Values.rbac.create }}
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: test
          namespace: {{ include "chart.namespace" . }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("namespaceOverride: \"\"\nrbac:\n  create: true\n"),
    );

    for instance in [
        serde_json::json!({ "namespaceOverride": false }),
        serde_json::json!({ "namespaceOverride": "" }),
        serde_json::json!({ "namespaceOverride": {} }),
        serde_json::json!({ "namespaceOverride": "custom-ns" }),
        serde_json::json!({ "namespaceOverride": false, "rbac": { "create": true } }),
        serde_json::json!({ "namespaceOverride": "ns", "rbac": { "create": true } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "falsy overrides skip the truthy arm and render the release namespace: \
             instance={instance}; schema={schema}"
        );
    }
}
/// cloudnative-pg, reduced: the fullname helper's `default`-selected
/// string contract stays truthy-scoped even when the consuming template is
/// gated by an unrelated liveness switch, so Helm-empty overrides render.
#[test]
fn liveness_gated_helper_keeps_helm_empty_fallback_inputs_open() {
    let helpers = indoc! {r#"
        {{- define "cloudnative-pg.name" -}}
        {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        {{- if .Values.config.create }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config-name
        data:
          chartname: {{ include "cloudnative-pg.name" . }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(
            "nameOverride: \"\"\nfullnameOverride: \"\"\nnamespaceOverride: \"\"\nconfig:\n  create: true\n",
        ),
    );
    for instance in [
        serde_json::json!({ "nameOverride": false, "config": { "create": true } }),
        serde_json::json!({ "nameOverride": {}, "config": { "create": true } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "helm-empty stays open: instance={instance}; schema={schema}"
        );
    }
}

/// datadog's otel gateway image helper replaces a FALSY tag with the
/// agent-version fallback (another source path) before its `semverCompare`
/// checks, so only truthy raw values reach the parser: an empty or null
/// tag renders through the fallback while a truthy non-version aborts.
#[test]
fn falsy_reassignment_to_another_source_scopes_the_parser_to_truthy_values() {
    let helpers = indoc! {r#"
        {{- define "repro.agentVersion" -}}
        {{- $version := .Values.agents.tag | toString | trimSuffix "-jmx" -}}
        {{- if eq $version "latest" -}}
        {{- $version = "7.80.1" -}}
        {{- end -}}
        {{- $version -}}
        {{- end -}}

        {{- define "repro.gatewayImage" -}}
          {{- $imageTag := .Values.gw.tag -}}
          {{- if not $imageTag -}}
            {{- $imageTag = include "repro.agentVersion" . -}}
          {{- end -}}
          {{- $imageTag = $imageTag | toString -}}
          {{- if semverCompare "<7.67.0" $imageTag -}}
            {{- fail "agent versions before 7.67.0 are not supported" -}}
          {{- end -}}
          {{- $imageTag -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if .Values.gw.enabled }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: gw
        spec:
          template:
            spec:
              containers:
                - name: gw
                  image: "agent:{{ include "repro.gatewayImage" . }}"
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        gw:
          enabled: false
          tag: ""
        agents:
          tag: "7.80.1"
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    for (tag, want) in [
        (serde_json::json!(""), true),
        (serde_json::json!(null), true),
        (serde_json::json!("7.70.0"), true),
        (serde_json::json!("junk"), false),
    ] {
        let instance = serde_json::json!({
            "gw": { "enabled": true, "tag": tag },
            "agents": { "tag": "7.80.1" },
        });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "falsy-reassignment parser scoping: instance={instance}; schema={schema}"
        );
    }
}
