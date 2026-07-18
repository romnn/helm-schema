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

/// Member selection over `mergeOverwrite` layers keeps the layered
/// precedence (mergo recurses into nested maps with the same override
/// order), so a `pick`ed member of the merged dict still resolves each
/// layer's path — kyverno's `featuresOverride.logging` reaches the helper's
/// member reads instead of vanishing behind the base layer. The base
/// layer's declared-map typing stays: the scalar-base-fully-shadowed lane
/// is a documented declared-default policy limitation.
#[test]
fn merged_member_projection_reaches_both_layers() {
    let src = indoc! {r#"
        {{- $picked := pick (mergeOverwrite (deepCopy .Values.features) .Values.ctrl.featuresOverride) "logging" }}
        {{- $flags := list -}}
        {{- with $picked.logging -}}
          {{- $flags = append $flags (print "--loggingFormat=" .format) -}}
          {{- $flags = append $flags (print "--v=" .verbosity) -}}
        {{- end -}}
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
            - name: test
              image: busybox
              args:
                {{- range $flags }}
                - {{ . }}
                {{- end }}
    "#};
    let values = "features:\n  logging:\n    format: text\n    verbosity: 2\nctrl:\n  featuresOverride: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values));
    for (instance, want, label) in [
        (
            serde_json::json!({ "features": { "logging": { "format": "json", "verbosity": 4 } } }),
            true,
            "map base logging",
        ),
        (
            serde_json::json!({
                "ctrl": { "featuresOverride": { "logging": { "format": "json", "verbosity": 4 } } }
            }),
            true,
            "map override logging",
        ),
        (
            serde_json::json!({ "features": { "logging": 5 } }),
            false,
            "scalar base unshadowed",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "merged member projection {label}: instance={instance}; schema={schema}"
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
