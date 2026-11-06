//! Wrapper alternatives for chart-authored values-program conventions
//! (nats' `$tplYaml`): when a chart routes its values tree through an
//! engine that replaces singleton `{KEY: PROGRAM}` maps with rendered
//! program results, every value-position node accepts the wrapper beside
//! its ordinary domain.

use indoc::indoc;

use super::{
    parse_ir, parse_ir_with_files, parse_ir_with_helpers, schema_accepts_instance,
    schema_for_values_yaml,
};

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
    {{- $values := get (include "test.tplValues" (dict "doc" .Values "ctx" $) | fromJson) "doc" }}
    {{- $_ := set . "Values" $values }}
    apiVersion: v1
    kind: Service
    metadata:
      name: test
    spec:
      selector:
        app: test
      ports:
        - port: {{ .Values.port }}
"#};

/// A detected engine adds the singleton-wrapper alternative at value
/// nodes: the wrapper's program must be a string, exactly one sentinel key
/// forms a wrapper, and non-wrapper maps keep failing the integer provider
/// slot.
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

#[test]
fn program_wrappers_do_not_rewrite_truthiness_definitions() {
    let source = indoc! {r#"
        {{- $values := get (include "test.tplValues" (dict "doc" .Values "ctx" $) | fromJson) "doc" }}
        {{- $_ := set . "Values" $values }}
        {{- if .Values.port }}
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          selector:
            app: test
          ports:
            - port: {{ .Values.port }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(source, ENGINE_HELPERS),
        Some("port: 4222\n"),
    );

    for (instance, want) in [
        (serde_json::json!({ "port": { "$tplYaml": "4333" } }), true),
        (serde_json::json!({ "port": { "$tplYaml": true } }), false),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "truthiness tests must see the raw wrapper map while the value schema validates its program: \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// An engine with a replace AND a spread sentinel: nats' own recursive
/// `tplYaml` engine (verbatim from the corpus chart), which walks the whole
/// values tree, replaces singleton `$tplYaml` maps with their decoded
/// program result and splices `$tplYamlSpread` results into the parent
/// collection, failing when the result kind cannot spread there. The engine
/// runs before the manifest reads `.Values`, so the labels render the
/// rewritten tree.
const SPREAD_ENGINE_FILES: [(&str, &str); 2] = [
    (
        "templates/_tplYaml.tpl",
        include_str!("../../../../testdata/charts/nats/templates/_tplYaml.tpl"),
    ),
    (
        "templates/_jsonpatch.tpl",
        include_str!("../../../../testdata/charts/nats/templates/_jsonpatch.tpl"),
    ),
];

const SPREAD_ENGINE_SRC: &str = indoc! {r#"
    {{- $values := get (include "tplYaml" (dict "doc" .Values "ctx" $) | fromJson) "doc" }}
    {{- $_ := set . "Values" $values }}
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: test
      {{- with .Values.labels }}
      labels: {{ toYaml . | nindent 4 }}
      {{- end }}
"#};

/// The engine substitutes wrapper maps before consumers read the tree, so
/// a raw singleton sentinel map must never ride a node's ordinary object
/// domain: its program constraint is the only lane it may take. A static
/// replace program whose decoded kind certainly cannot inhabit the node
/// rejects; a spread program whose decoded kind cannot spread into the
/// parent map rejects; dynamic programs stay open.
///
/// Every verdict is adjudicated with Helm 4.2.3 and the `ConfigMap` destination
/// schema over the coalesced document (`transfer/receipts/wrapper_program_*-r2`).
/// One accepted verdict is a documented uncertainty (ADR 0013): the dynamic
/// replace program `{{ .Release.Name }}` renders a string label set, which the
/// destination rejects, but a template program's result is not decided
/// statically.
///
/// Kept red on the current model, as at the line base: the interpreter does not
/// carry the values tree through the engine's recursive `tplYamlItr` patch
/// walk and `jsonpatch` application, so the rewritten `.Values` is opaque. The
/// schema then has no `labels` node, admits every root member, and emits
/// neither the wrapper alternatives nor the replace/spread program
/// constraints. Missing mechanism: executing the recursive values rewrite
/// (patch list construction and application) as ordinary value flow.
#[test]
fn wrapper_program_results_must_be_compatible_with_node_and_parent() {
    let schema = schema_for_values_yaml(
        parse_ir_with_files(SPREAD_ENGINE_SRC, &SPREAD_ENGINE_FILES),
        Some("labels: {}\n"),
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
        (serde_json::json!({ "labels": {} }), true),
        (serde_json::json!({}), true),
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
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          port: {{ .Values.port }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("port: 4222\n"));
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "port": { "$tplYaml": "4333" } })
        ),
        "without an engine the map is ordinary input, and its safe Go-formatted text renders: schema={schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "port": { "$tplYaml": "a: b" } })
        ),
        "ordinary maps still obey the plain-scalar text grammar: schema={schema}"
    );
}
