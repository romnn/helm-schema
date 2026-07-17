//! Per-key shadowing of ordered `merge` layers at provider sinks: with
//! destination-first `merge preferred legacy`, a legacy member reaches the
//! rendered slot only where the preferred layer lacks that key.

use indoc::indoc;

use super::{parse_ir, schema_accepts_instance, schema_for_values_yaml};

/// The velero shape: the deprecated `securityContext` merges beneath
/// `podSecurityContext` into a Deployment's pod security context. A legacy
/// member is typed exactly where the preferred object does not supply it,
/// the preferred layer keeps its whole payload typing under its own
/// truthiness, and custom legacy keys stay open.
#[test]
fn shadowed_merge_layer_binds_members_only_where_unshadowed() {
    let src = indoc! {r#"
        {{- $ctx := merge (.Values.podSecurityContext | default dict) (.Values.securityContext | default dict) }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              {{- with $ctx }}
              securityContext:
                {{- toYaml . | nindent 8 }}
              {{- end }}
              containers:
                - name: main
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some("podSecurityContext: {}\nsecurityContext: {}\n"),
    );
    for (instance, want) in [
        // An active legacy member reaches the rendered slot and must type.
        (
            serde_json::json!({ "securityContext": { "runAsUser": { "bad": true } } }),
            false,
        ),
        (
            serde_json::json!({ "securityContext": { "runAsUser": 1000 } }),
            true,
        ),
        // The preferred layer supplies the key, so the same malformed
        // legacy member is shadowed and never rendered.
        (
            serde_json::json!({
                "podSecurityContext": { "runAsUser": 1000 },
                "securityContext": { "runAsUser": { "bad": true } }
            }),
            true,
        ),
        // The preferred layer's own members always win and always type.
        (
            serde_json::json!({ "podSecurityContext": { "runAsUser": { "bad": true } } }),
            false,
        ),
        (
            serde_json::json!({ "podSecurityContext": { "runAsUser": 1000 } }),
            true,
        ),
        // Keys outside the provider payload stay open.
        (
            serde_json::json!({ "securityContext": { "customExtra": "x" } }),
            true,
        ),
        (serde_json::json!({}), true),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "per-key merge shadowing scopes the legacy layer's typing: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// `mergeOverwrite` has the opposite precedence — later arguments win — so
/// the layer roles flip: the SECOND path becomes the preferred layer and
/// the first is typed only where the second lacks the key.
#[test]
fn merge_overwrite_reverses_layer_precedence() {
    let src = indoc! {r#"
        {{- $ctx := mergeOverwrite (.Values.legacy | default dict) (.Values.preferred | default dict) }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          template:
            spec:
              {{- with $ctx }}
              securityContext:
                {{- toYaml . | nindent 8 }}
              {{- end }}
              containers:
                - name: main
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("legacy: {}\npreferred: {}\n"));
    for (instance, want) in [
        (
            serde_json::json!({ "legacy": { "runAsUser": { "bad": true } } }),
            false,
        ),
        (
            serde_json::json!({
                "preferred": { "runAsUser": 1000 },
                "legacy": { "runAsUser": { "bad": true } }
            }),
            true,
        ),
        (
            serde_json::json!({ "preferred": { "runAsUser": { "bad": true } } }),
            false,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "mergeOverwrite flips which layer is shadowed: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}
