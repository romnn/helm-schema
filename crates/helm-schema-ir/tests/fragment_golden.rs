//! Golden fragment dumps for the Stage-B interpreter micro-cases: branchy
//! mappings, ranges over values, partial scalars, helper splices, block
//! scalars, and opaque taint. Each expected dump is a reviewed semantic
//! statement, not a regenerated snapshot; update it only with a reasoned
//! change.

use helm_schema_ast::DefineIndex;
use helm_schema_ir::SymbolicIrContext;
use helm_schema_ir::fragment_eval::dump_document;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

fn assert_fragment_dump(source: &str, helpers: &str, expected: &str) {
    let mut idx = DefineIndex::new();
    if !helpers.is_empty() {
        idx.add_file_source("_helpers.tpl", helpers);
    }
    let document = SymbolicIrContext::new(&idx).eval_document_fragment(source);
    sim_assert_eq!(have: dump_document(&document), want: expected);
}

/// Branch alternatives merge under one mapping key; the second arm carries
/// the negation of the first arm's condition, and condition reads keep the
/// per-guard prefix rule.
#[test]
fn branchy_mapping_merges_guarded_entry_arms() {
    let source = indoc! {r#"
        metadata:
          name: static
          {{- if .Values.commonAnnotations }}
          annotations:
            checksum: fixed
          {{- else if .Values.legacyAnnotations }}
          annotations:
            legacy: "true"
          {{- end }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "metadata":
              when always:
                mapping:
                  key "name":
                    when always:
                      scalar [text{"static"}]
                  key "annotations":
                    when truthy(commonAnnotations):
                      mapping:
                        key "checksum":
                          when always:
                            scalar [text{"fixed"}]
                    when (!(truthy(commonAnnotations)) && truthy(legacyAnnotations)):
                      mapping:
                        key "legacy":
                          when always:
                            scalar [text{"\"true\""}]
        reads:
          commonAnnotations [truthy(commonAnnotations)]
          legacyAnnotations [not(commonAnnotations), truthy(legacyAnnotations)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// A range rendering scalar items splices the iterated list at the container
/// (plus per-item dot splices); a destructured range rendering templated
/// entries splices the source as a fragment, with the header read recorded.
#[test]
fn range_over_values_splices_list_and_mapping_sources() {
    let source = indoc! {r#"
        spec:
          args:
            {{- range .Values.extraArgs }}
            - {{ . | quote }}
            {{- end }}
          env:
            {{- range $key, $value := .Values.env }}
            {{ $key }}: {{ $value }}
            {{- end }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "spec":
              when always:
                mapping:
                  key "args":
                    when always:
                      sequence:
                        item:
                          when range(extraArgs):
                            splice extraArgs.* scalar
                    when range(extraArgs):
                      splice extraArgs scalar
                  key "env":
                    when range(env):
                      splice env fragment
                    when always:
                      mapping:
                        key dynamic []:
        reads:
          env [range(env)]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Partial scalars keep ordered parts: splices interleaved with literal
/// text, and a `default` fallback marks the primary path defaulted while its
/// fallback text stays a contribution part.
#[test]
fn partial_scalar_keeps_ordered_parts() {
    let source = indoc! {r#"
        metadata:
          name: {{ .Values.prefix }}-{{ .Values.suffix }}
          host: {{ .Values.host | default "localhost" }}:8080
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "metadata":
              when always:
                mapping:
                  key "name":
                    when always:
                      scalar [splice prefix partial text{"-"} splice suffix partial]
                  key "host":
                    when always:
                      scalar [splice host partial defaulted text{"localhost"} text{":8080"}]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Helper calls splice their memoized summary fragments: a scalar helper's
/// internal branch condition becomes the splice's arm condition, a
/// structured helper stays mapping entries, and helper-internal guard reads
/// surface pathlessly carrying their own decoded condition — the same
/// convention document-level condition reads always had.
#[test]
fn helper_splice_lowers_summary_branches_into_arms() {
    let helpers = indoc! {r#"
        {{- define "chart.fullname" -}}
        {{- if .Values.fullnameOverride -}}
        {{- .Values.fullnameOverride | trunc 63 -}}
        {{- else -}}
        {{- .Release.Name -}}
        {{- end -}}
        {{- end -}}
        {{- define "chart.labels" -}}
        app: {{ .Values.appName }}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        metadata:
          name: {{ include "chart.fullname" . }}
          labels: {{- include "chart.labels" . | nindent 4 }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "metadata":
              when always:
                mapping:
                  key "name":
                    when truthy(fullnameOverride):
                      splice fullnameOverride scalar
                  key "labels":
                    when always:
                      mapping:
                        key "app":
                          when always:
                            splice appName scalar
        reads:
          fullnameOverride [truthy(fullnameOverride)]
    "#};
    assert_fragment_dump(source, helpers, expected);
}

/// Block-scalar bodies are render-suppressed blobs: contained splices keep
/// influencing the text without sink-typing the entry's document position.
#[test]
fn block_scalar_body_is_render_suppressed() {
    let source = indoc! {r#"
        data:
          config.yaml: |
            port={{ .Values.port }}
            host=localhost
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key "config.yaml":
                    when always:
                      scalar suppressed [text{"    port="} splice port partial text{"\n    host=localhost"}]
    "#};
    assert_fragment_dump(source, "", expected);
}

/// Unknown calls carry their influence as opaque taint, and an inline
/// `{{ if }}…{{ end }}` region inside a scalar becomes guarded text arms.
#[test]
fn opaque_taint_and_inline_region_arms() {
    let source = indoc! {r#"
        metadata:
          name: {{ required "name is required" .Values.nameOverride }}
          kind: {{ if .Values.experimental }}Alpha{{ else }}Stable{{ end }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "metadata":
              when always:
                mapping:
                  key "name":
                    when always:
                      opaque taint={nameOverride}
                  key "kind":
                    when truthy(experimental):
                      scalar [text{"Alpha"}]
                    when !(truthy(experimental)):
                      scalar [text{"Stable"}]
        reads:
          experimental [truthy(experimental)]
    "#};
    assert_fragment_dump(source, "", expected);
}
