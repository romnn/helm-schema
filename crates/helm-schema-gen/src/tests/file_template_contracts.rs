use indoc::indoc;

use super::{parse_ir_with_files, schema_accepts_instance, schema_for_values_yaml};

/// A statically selected file evaluated by `tpl` executes its own member
/// accesses. Deleting the host of a field read aborts before `fromYaml` can
/// decode the rendered program (Cilium's Envoy bootstrap configuration).
#[test]
fn file_template_member_access_requires_its_source_host() {
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          bootstrap.json: |
            {{- tpl (.Files.Get "files/bootstrap.yaml") . | fromYaml | toJson | nindent 4 }}
    "#};
    let file = indoc! {r#"
        listeners:
        {{- if .Values.ipv6.enabled }}
          - address: "::1"
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        ipv6:
          enabled: true
    "};
    let schema = schema_for_values_yaml(
        parse_ir_with_files(source, &[("files/bootstrap.yaml", file)]),
        Some(values_yaml),
    );

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "ipv6": { "enabled": false } })
        ),
        "a present host renders: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({})),
        "deleting the member host aborts the file-backed tpl program: {schema}"
    );
}

/// The file's claim keeps the caller's decoded helper/local activation
/// predicate. It must reject only while the file-backed program executes,
/// not while a sibling state suppresses the whole `ConfigMap`.
#[test]
fn file_template_member_access_keeps_its_caller_activation() {
    let source = indoc! {r#"
        {{- $live := eq (include "file-live" .) "true" -}}
        {{- if and $live (not .Values.preflight.enabled) }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          bootstrap.json: |
            {{- tpl (.Files.Get "files/bootstrap.yaml") . | fromYaml | toJson | nindent 4 }}
        {{- end }}
    "#};
    let helpers = indoc! {r#"
        {{- define "file-live" -}}
        {{- if .Values.live -}}true{{- else -}}false{{- end -}}
        {{- end -}}
    "#};
    let file = indoc! {r#"
        listeners:
        {{- if .Values.ipv6.enabled }}
          - address: "::1"
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        live: true
        preflight:
          enabled: false
        ipv6:
          enabled: true
    "};
    let schema = schema_for_values_yaml(
        parse_ir_with_files(
            source,
            &[
                ("templates/_helpers.tpl", helpers),
                ("files/bootstrap.yaml", file),
            ],
        ),
        Some(values_yaml),
    );

    for (instance, want) in [
        (
            serde_json::json!({
                "live": true,
                "preflight": { "enabled": false }
            }),
            false,
        ),
        (
            serde_json::json!({
                "live": false,
                "preflight": { "enabled": false }
            }),
            true,
        ),
        (
            serde_json::json!({
                "live": true,
                "preflight": { "enabled": true }
            }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the file member host is required exactly while its caller is live: instance={instance}; schema={schema}"
        );
    }
}

/// Cilium's real Envoy helper selects an explicit Boolean when present and
/// otherwise derives the result from a version fallback. The file-backed
/// member claim must retain that complete helper dispatch rather than losing
/// the default arm at the helper-output equality.
#[test]
fn file_template_member_access_keeps_a_versioned_helper_activation() {
    let source = indoc! {r#"
        {{- $live := eq (include "file-live" .) "true" -}}
        {{- if and $live (not .Values.preflight.enabled) }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          bootstrap.json: |
            {{- tpl (.Files.Get "files/bootstrap.yaml") . | fromYaml | toJson | nindent 4 }}
        {{- end }}
    "#};
    let helpers = indoc! {r#"
        {{- define "file-live" -}}
        {{- if not .Values.l7Proxy -}}
          {{- false -}}
        {{- else if not (kindIs "invalid" .Values.envoy.enabled) -}}
          {{- .Values.envoy.enabled -}}
        {{- else if semverCompare ">=1.16" (default "1.16" .Values.upgradeCompatibility) -}}
          {{- true -}}
        {{- else -}}
          {{- false -}}
        {{- end -}}
        {{- end -}}
    "#};
    let file = indoc! {r#"
        listeners:
        {{- if .Values.ipv6.enabled }}
          - address: "::1"
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        l7Proxy: true
        envoy:
          enabled: null
        upgradeCompatibility: null
        preflight:
          enabled: false
        ipv6:
          enabled: true
    "};
    let ir = parse_ir_with_files(
        source,
        &[
            ("templates/_helpers.tpl", helpers),
            ("files/bootstrap.yaml", file),
        ],
    );
    let schema = schema_for_values_yaml(ir, Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({
                "l7Proxy": true,
                "envoy": {},
                "preflight": { "enabled": false }
            }),
            false,
            "the absent-value version fallback enables the file",
        ),
        (
            serde_json::json!({
                "l7Proxy": false,
                "envoy": { "enabled": null },
                "upgradeCompatibility": null,
                "preflight": { "enabled": false }
            }),
            true,
            "the outer helper arm disables the file",
        ),
        (
            serde_json::json!({
                "l7Proxy": true,
                "envoy": { "enabled": false },
                "upgradeCompatibility": null,
                "preflight": { "enabled": false }
            }),
            true,
            "the explicit helper value disables the file",
        ),
        (
            serde_json::json!({
                "l7Proxy": true,
                "envoy": { "enabled": null },
                "upgradeCompatibility": "1.15",
                "preflight": { "enabled": false }
            }),
            true,
            "the old-version helper arm disables the file",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the versioned helper scopes the file member host ({label}): \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}
