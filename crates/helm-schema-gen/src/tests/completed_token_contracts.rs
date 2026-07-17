//! contracts of the COMPLETED YAML token a partial scalar assembles —
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
/// quoted token when the raw string is not valid double-quoted YAML content
/// (zalando's manually quoted image scalar). Valid escape sequences such as
/// `\"` and `\\` render, and other input kinds format safely inside the
/// quotes.
#[test]
fn double_quoted_splice_excludes_invalid_quoted_content() {
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
        // Unescaped quote breaks the token
        (
            serde_json::json!({ "image": { "registry": "bad\"quote" } }),
            false,
        ),
        // Lone backslash starts an invalid escape
        (
            serde_json::json!({ "image": { "registry": "back\\slash" } }),
            false,
        ),
        // Dangling trailing backslash
        (
            serde_json::json!({ "image": { "registry": "trail\\" } }),
            false,
        ),
        // Escaped quote and doubled backslash are valid YAML escapes
        (
            serde_json::json!({ "image": { "registry": "esc\\\"ok" } }),
            true,
        ),
        (
            serde_json::json!({ "image": { "registry": "esc\\\\ok" } }),
            true,
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
            "only invalid double-quoted content corrupts the token: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A splice inside MANUAL single quotes breaks on any apostrophe that is
/// not doubled — `''` is the only escape in single-quoted YAML (cilium's
/// `envoy.log.defaultLevel`, kube-state-metrics' `prometheusScrape`).
#[test]
fn single_quoted_splice_excludes_undoubled_apostrophes() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          level: 'trace|debug|{{ .Values.defaultLevel }}'
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("defaultLevel: info\n"));
    for (instance, want) in [
        (serde_json::json!({ "defaultLevel": "a'b" }), false),
        (serde_json::json!({ "defaultLevel": "a''b" }), true),
        (serde_json::json!({ "defaultLevel": "info" }), true),
        (serde_json::json!({ "defaultLevel": 7 }), true),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "an undoubled apostrophe corrupts the single-quoted token: \
             instance={instance}; schema={schema}"
        );
    }
}

/// The quote context survives literal flow-content text, so a splice inside
/// a flow-style quoted item (`[ "prefix.{{ … }}" ]`) carries the same
/// double-quoted contract as a whole quoted token (cilium's clustermesh
/// hostname list).
#[test]
fn flow_style_quoted_splice_keeps_the_quoted_contract() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          hosts: [ "clustermesh.apiserver.{{ .Values.domain }}" ]
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("domain: mesh.cilium.io\n"));
    for (instance, want) in [
        (serde_json::json!({ "domain": "a\"b" }), false),
        (serde_json::json!({ "domain": "mesh.cilium.io" }), true),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "flow-content quoting keeps the double-quoted contract: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A `toYaml … | indent N` splice in the VALUE slot of a same-line mapping
/// entry renders its first line directly after `key: `, so an object or
/// list member opens block structure mid-line and breaks the document
/// (coredns' `{{ .filename }}: {{ toYaml .contents | indent 4 }}` zone
/// files); scalars (and multi-line strings, which serialize as block
/// scalars with their own indicator) stay valid.
#[test]
fn same_line_yaml_serialized_value_rejects_structured_members() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range .Values.zoneFiles }}
          {{ .filename }}: {{ toYaml .contents | indent 4 }}
          {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("zoneFiles: []\n"));

    for (instance, want) in [
        (
            serde_json::json!({ "zoneFiles": [{ "filename": "db.local", "contents": "zone data" }] }),
            true,
        ),
        (
            serde_json::json!({ "zoneFiles": [{ "filename": "db.local", "contents": "multi\nline" }] }),
            true,
        ),
        (
            serde_json::json!({ "zoneFiles": [{ "filename": "db.local", "contents": { "a": 1 } }] }),
            false,
        ),
        (
            serde_json::json!({ "zoneFiles": [{ "filename": "db.local", "contents": ["a"] }] }),
            false,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "structured members break the same-line serialized slot: \
             instance={instance}; schema={schema}"
        );
    }
}
