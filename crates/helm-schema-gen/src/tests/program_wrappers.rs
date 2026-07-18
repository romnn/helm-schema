//! Wrapper alternatives for chart-authored values-program conventions
//! (nats' `$tplYaml`): when a chart routes its values tree through an
//! engine that replaces singleton `{KEY: PROGRAM}` maps with rendered
//! program results, every value-position node accepts the wrapper beside
//! its ordinary domain.

use indoc::indoc;

use super::{parse_ir, parse_ir_with_helpers, schema_accepts_instance, schema_for_values_yaml};

const ENGINE_HELPERS: &str = indoc! {r#"
    {{- define "test.tplValues" -}}
    {{- $doc := .doc -}}
    {{- if and (eq (kindOf $doc) "map") (eq (len $doc) 1) (hasKey $doc "$tplYaml") -}}
    {{- $tpl := get $doc "$tplYaml" -}}
    {{- toJson (dict "doc" (fromYaml (tpl $tpl .ctx))) -}}
    {{- else -}}
    {{- toJson (dict "doc" $doc) -}}
    {{- end -}}
    {{- end -}}
"#};

const ENGINE_SRC: &str = indoc! {r#"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: test
    data:
      port: {{ .Values.port }}
    {{- $values := get (include "test.tplValues" (dict "doc" .Values "ctx" $) | fromJson) "doc" }}
    {{- $_ := set . "Values" $values }}
"#};

/// A detected engine adds the singleton-wrapper alternative at value
/// nodes: the wrapper's program must be a string, exactly one sentinel key
/// forms a wrapper, and non-wrapper maps keep failing the node's ordinary
/// domain.
#[test]
fn detected_engine_accepts_program_wrappers_at_value_nodes() {
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(ENGINE_SRC, ENGINE_HELPERS),
        Some("port: 4222\n"),
    );
    for (instance, want) in [
        (
            serde_json::json!({ "port": { "$tplYaml": "{{ add 4000 333 }}" } }),
            true,
        ),
        (serde_json::json!({ "port": { "$tplYaml": "4333" } }), true),
        // The program must be a string: `tpl` errors on other kinds.
        (serde_json::json!({ "port": { "$tplYaml": true } }), false),
        (
            serde_json::json!({ "port": { "$tplYaml": { "$tplYaml": "1" } } }),
            false,
        ),
        // A two-key map is not a wrapper and fails the scalar node.
        (
            serde_json::json!({ "port": { "$tplYaml": "1", "x": 2 } }),
            false,
        ),
        (serde_json::json!({ "port": 4222 }), true),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "wrapper alternatives ride detected engine conventions: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// An engine with a replace AND a spread sentinel: the spread key's
/// `hasKey` test guards a `fail` (the root-object refusal), which
/// classifies it structurally. Program constraints then differ per key —
/// the replace program must be able to inhabit the node, the spread
/// program must be able to spread into the parent collection's kind.
const SPREAD_ENGINE_HELPERS: &str = indoc! {r#"
    {{- define "test.tplValues" -}}
    {{- $doc := .doc -}}
    {{- if and (eq (kindOf $doc) "map") (eq (len $doc) 1) (or (hasKey $doc "$tplYaml") (hasKey $doc "$tplYamlSpread")) -}}
    {{- $tpl := get $doc "$tplYaml" -}}
    {{- if hasKey $doc "$tplYamlSpread" -}}
    {{- if eq .path "/" -}}
    {{- fail "cannot spread on the root object" -}}
    {{- end -}}
    {{- $tpl = get $doc "$tplYamlSpread" -}}
    {{- end -}}
    {{- toJson (dict "doc" (fromYaml (tpl $tpl .ctx))) -}}
    {{- else -}}
    {{- toJson (dict "doc" $doc) -}}
    {{- end -}}
    {{- end -}}
"#};

const SPREAD_ENGINE_SRC: &str = indoc! {r#"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: test
      labels: {{ .Values.labels | toYaml | nindent 4 }}
    data:
      port: {{ .Values.port }}
    {{- $values := get (include "test.tplValues" (dict "doc" .Values "ctx" $ "path" "/") | fromJson) "doc" }}
    {{- $_ := set . "Values" $values }}
"#};

/// The engine substitutes wrapper maps before consumers read the tree, so
/// a raw singleton sentinel map must never ride a node's ordinary object
/// domain: its program constraint is the only lane it may take. A static
/// replace program whose decoded kind certainly cannot inhabit the node
/// rejects; a spread program whose decoded kind cannot spread into the
/// parent map rejects; dynamic programs stay open.
#[test]
fn wrapper_program_results_must_be_compatible_with_node_and_parent() {
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(SPREAD_ENGINE_SRC, SPREAD_ENGINE_HELPERS),
        Some("port: 4222\nlabels:\n  app: test\n"),
    );
    for (instance, want) in [
        // Replace programs at the object-typed node: the decoded result
        // replaces the node, so scalar and list decodings cannot inhabit
        // it even though the wrapper map itself is an object.
        (
            serde_json::json!({ "labels": { "$tplYaml": "true" } }),
            false,
        ),
        (
            serde_json::json!({ "labels": { "$tplYaml": "4333" } }),
            false,
        ),
        (
            serde_json::json!({ "labels": { "$tplYaml": "[a]" } }),
            false,
        ),
        (
            serde_json::json!({ "labels": { "$tplYaml": "app: x" } }),
            true,
        ),
        (
            serde_json::json!({ "labels": { "$tplYaml": "{{ .Release.Name }}" } }),
            true,
        ),
        // Spread programs at a map-member edge: only a map (or a null
        // no-op) can spread into the parent map.
        (
            serde_json::json!({ "labels": { "$tplYamlSpread": "[]" } }),
            false,
        ),
        (
            serde_json::json!({ "labels": { "$tplYamlSpread": "- a" } }),
            false,
        ),
        (
            serde_json::json!({ "labels": { "$tplYamlSpread": "audit" } }),
            false,
        ),
        (
            serde_json::json!({ "labels": { "$tplYamlSpread": "{a: 1}" } }),
            true,
        ),
        (
            serde_json::json!({ "labels": { "$tplYamlSpread": "null" } }),
            true,
        ),
        (
            serde_json::json!({ "labels": { "$tplYamlSpread": "{{ .Values.x }}" } }),
            true,
        ),
        // Ordinary maps — including two-key maps carrying a sentinel key —
        // keep the node's ordinary domain.
        (serde_json::json!({ "labels": { "app": "x" } }), true),
        (
            serde_json::json!({ "labels": { "$tplYaml": "true", "x": "y" } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "wrapper result compatibility: instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// Without an engine the same wrapper map is an ordinary object and fails
/// the scalar node: the alternative exists only for charts that actually
/// route their values through a wrapper engine.
#[test]
fn without_an_engine_wrapper_maps_stay_ordinary_objects() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          port: {{ .Values.port }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("port: 4222\n"));
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "port": { "$tplYaml": "4333" } })
        ),
        "no engine, no wrapper alternative: schema={schema}"
    );
}
