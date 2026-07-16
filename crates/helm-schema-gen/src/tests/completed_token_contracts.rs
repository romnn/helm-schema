//! F76: contracts of the COMPLETED YAML token a partial scalar assembles —
//! raw inputs that corrupt the assembled token abort rendering, while
//! totally-formatted embeddings tolerate any input kind.

use indoc::indoc;

use super::{parse_ir, schema_accepts_instance, schema_for_values_yaml};

/// A literal-prefixed splice (`--log-level={{ … }}`) embeds ANY rendered
/// value as argument text, so the `default "info"` fallback's string intent
/// must not close the branch against maps or lists.
#[test]
fn prefixed_argument_splice_keeps_fallback_typed_inputs_open() {
    let src = indoc! {r#"
        {{- if .Values.ctrl.create }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          args: --log-level={{ .Values.logLevel | default "info" }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some("ctrl:\n  create: true\nlogLevel: info\n"),
    );
    for instance in [
        serde_json::json!({ "logLevel": { "a": "b" } }),
        serde_json::json!({ "logLevel": ["a"] }),
        serde_json::json!({ "logLevel": "info" }),
        serde_json::json!({ "logLevel": false }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "an embedded splice totally formats every input: instance={instance}; schema={schema}"
        );
    }
}

/// A splice OPENING an unquoted token (`image: {{ .registry }}/…`) breaks
/// on a list value, whose rendering opens a flow sequence at the token
/// start; maps render as plain `map[…]` text and stay safe.
#[test]
fn token_initial_splice_excludes_lists() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          image: {{ .Values.tempo.registry }}/{{ .Values.tempo.repository }}:{{ .Values.tempo.tag }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some("tempo:\n  registry: docker.io\n  repository: grafana/tempo\n  tag: latest\n"),
    );
    for (instance, want) in [
        (serde_json::json!({ "tempo": { "registry": ["a"] } }), false),
        (
            serde_json::json!({ "tempo": { "registry": "docker.io" } }),
            true,
        ),
        (
            serde_json::json!({ "tempo": { "registry": { "a": "b" } } }),
            true,
        ),
        // The mid-token repository splice embeds after literal text.
        (
            serde_json::json!({ "tempo": { "repository": ["a"] } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "a token-initial list opens a flow sequence and breaks the token: \
             instance={instance}; schema={schema}"
        );
    }
}

/// The same token-initial contract holds inside container list items (the
/// sibling tag's `default` arm split must not hide the registry's position;
/// tempo's assembled image scalar).
#[test]
fn token_initial_splice_survives_sibling_default_arm_split() {
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: StatefulSet
        metadata:
          name: test
        spec:
          template:
            spec:
              containers:
              - args:
                - -config.file=/conf/tempo.yaml
                image: {{ .Values.tempo.registry }}/{{ .Values.tempo.repository }}:{{ .Values.tempo.tag | default .Chart.AppVersion }}
                name: tempo
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some("tempo:\n  registry: docker.io\n  repository: grafana/tempo\n  tag: latest\n"),
    );
    let instance = serde_json::json!({ "tempo": { "registry": ["a"] } });
    assert!(
        !schema_accepts_instance(&schema, &instance),
        "instance={instance}; schema={schema}"
    );
}

/// A splice inside MANUAL double quotes (`image: "{{ … }}/…"`) corrupts the
/// quoted token when the raw string contains `"` or `\` (zalando's manually
/// quoted image scalar); other input kinds format safely inside the quotes.
#[test]
fn double_quoted_splice_excludes_quote_and_backslash_strings() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          image: "{{ .Values.image.registry }}/{{ .Values.image.repository }}:{{ .Values.image.tag }}"
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some("image:\n  registry: ghcr.io\n  repository: op\n  tag: v1\n"),
    );
    for (instance, want) in [
        (
            serde_json::json!({ "image": { "registry": "bad\"quote" } }),
            false,
        ),
        (
            serde_json::json!({ "image": { "registry": "back\\slash" } }),
            false,
        ),
        (
            serde_json::json!({ "image": { "registry": "ghcr.io" } }),
            true,
        ),
        (serde_json::json!({ "image": { "registry": 7 } }), true),
        (
            serde_json::json!({ "image": { "tag": "no\"quotes\"allowed" } }),
            false,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "a raw quote corrupts the manually quoted token: instance={instance}; schema={schema}"
        );
    }
}
