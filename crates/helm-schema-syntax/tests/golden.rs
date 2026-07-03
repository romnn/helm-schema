//! Golden CST dumps for the layout micro-cases: partial scalars, block
//! scalars containing actions, flow collections, `---` markers inside and
//! outside branches, mapping-key actions, comment lines with actions, ranges
//! with destructured variables, define blocks, and the Opaque fallback for
//! ill-nested inline regions.

use helm_schema_syntax::TemplatedDocument;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

fn assert_dump(source: &str, expected: &str) {
    let document = TemplatedDocument::parse(source);
    sim_assert_eq!(have: document.dump(), want: expected);
}

#[test]
fn scalars_partial_scalars_and_mapping_key_actions() {
    let source = indoc! {r##"
        metadata:
          name: {{ .Values.name }}
          addr: {{ .Values.a }}:{{ .Values.b }}
          "{{ .Values.key }}": literal
        {{ .Values.key }}: v
    "##};
    let expected = indoc! {r##"
        document 0 [0..129)
        entry [0..9) open indent=0 key="metadata"
          entry [12..36) open indent=2 key="name" value="⟦{{ .Values.name }}⟧"
          entry [39..76) open indent=2 key="addr" value="⟦{{ .Values.a }}⟧:⟦{{ .Values.b }}⟧"
          entry [79..107) closed indent=2 key="\"{{ .Values.key }}\"" value="literal"
          output [108..125) "{{ .Values.key }}"
          opaque action-line-text [125..128) ": v"
    "##};
    assert_dump(source, expected);
}

#[test]
fn block_scalar_bodies_suppress_actions_and_comments() {
    let source = indoc! {r##"
        data:
          conf: |
            a={{ .Values.a }}
            # not a yaml comment
          next: {{ .Values.c }}
        resources: {limits: {cpu: 1}}
        # comment {{ .Values.d }}
    "##};
    let expected = indoc! {r##"
        document 0 [0..143)
        entry [0..5) open indent=0 key="data"
          entry [8..15) block indent=2 key="conf" block-header=[14..15) body=[16..62) suppressed-holes=1
          entry [65..86) open indent=2 key="next" value="⟦{{ .Values.c }}⟧"
        entry [87..116) closed indent=0 key="resources" value="{limits: {cpu: 1}}"
        comment [117..142) "# comment ⟦{{ .Values.d }}⟧"
    "##};
    assert_dump(source, expected);
}

#[test]
fn document_markers_inside_and_outside_branches() {
    let source = indoc! {r##"
        kind: A
        ---
        {{- if .Values.b }}
        kind: B
        ---
        kind: C
        {{- end }}
    "##};
    let expected = indoc! {r##"
        document 0 [0..8)
        document 1 [12..40)
        document 2 [44..63)
        entry [0..7) closed indent=0 key="kind" value="A"
        scalar [8..11) indent=0 "---"
        control if [12..62)
          branch [12..31) "{{- if .Values.b }}"
            entry [32..39) closed indent=0 key="kind" value="B"
            scalar [40..43) indent=0 "---"
            entry [44..51) closed indent=0 key="kind" value="C"
    "##};
    assert_dump(source, expected);
}

#[test]
fn range_with_destructured_variables_and_item_ranges() {
    let source = indoc! {r##"
        env:
        {{- range $key, $value := .Values.env }}
          {{ $key }}: {{ $value | quote }}
        {{- end }}
        items:
        {{- range .Values.list }}
          - name: {{ .name }}
        {{- end }}
    "##};
    // The second range is conservatively flagged: the sequence item opened in
    // its body is still open (never popped) when `{{ end }}` arrives, so it
    // escapes the branch body.
    let expected = indoc! {r##"
        document 0 [0..158)
        entry [0..4) open indent=0 key="env"
          control range [5..91)
            branch [5..45) "{{- range $key, $value := .Values.env }}"
              output [48..58) "{{ $key }}"
              opaque action-line-text [58..59) ":"
              output [60..80) "{{ $value | quote }}"
        entry [92..98) open indent=0 key="items"
          control range [99..157) ill-nested
            branch [99..124) "{{- range .Values.list }}"
          item [127..146) indent=2
            entry [129..146) open indent=4 key="name" value="⟦{{ .name }}⟧"
    "##};
    assert_dump(source, expected);
}

#[test]
fn define_blocks_with_fragment_output() {
    let source = indoc! {r##"
        {{- define "chart.labels" -}}
        app: {{ .Values.app }}
        {{- end }}
        {{- define "chart.body" }}
        spec: {{- toYaml .Values.spec | nindent 2 }}
        {{- end }}
    "##};
    let expected = indoc! {r##"
        document 0 [0..147)
        control define [0..63) ill-nested
          branch [0..29) "{{- define \"chart.labels\" -}}"
        entry [30..52) open indent=0 key="app" value="⟦{{ .Values.app }}⟧"
        control define [64..146) ill-nested
          branch [64..90) "{{- define \"chart.body\" }}"
        entry [91..135) open indent=0 key="spec" value="⟦{{- toYaml .Values.spec | nindent 2 }}⟧"
    "##};
    assert_dump(source, expected);
}

#[test]
fn inline_region_degrades_to_opaque_and_open_entries_cross_end() {
    let source = indoc! {r##"
        kind: {{ if .Values.x }}A{{ else }}B{{ end }}
        {{- if .Values.ann }}
        annotations:
        {{- end }}
          extra: {{ .Values.extra }}
    "##};
    // The scalar-inline `if` keeps its raw span as one opaque node; the
    // `annotations:` entry stays open across `{{ end }}` (layout is decided
    // by lines alone), so the later `extra` entry still nests under it.
    let expected = indoc! {r##"
        document 0 [0..121)
        opaque inline-region [6..45) "{{ if .Values.x }}A{{ else }}B{{ end }}"
        entry [0..45) open indent=0 key="kind" value="⟦{{ if .Values.x }}⟧A⟦{{ else }}⟧B⟦{{ end }}⟧"
        control if [46..91) ill-nested
          branch [46..67) "{{- if .Values.ann }}"
        entry [68..80) open indent=0 key="annotations"
          entry [94..120) open indent=2 key="extra" value="⟦{{ .Values.extra }}⟧"
    "##};
    assert_dump(source, expected);
}

#[test]
fn sequence_item_fragment_slot_and_flow_collection_lines() {
    let source = indoc! {r##"
        containers:
          - name: app
            {{- toYaml .Values.extra | nindent 4 }}
        resources: [
          {key: {{ .Values.k }}},
        ]
    "##};
    // Multi-line flow collections keep the line model's literal treatment:
    // continuation lines parse as ordinary layout lines.
    let expected = indoc! {r##"
        document 0 [0..111)
        entry [0..11) open indent=0 key="containers"
          item [14..25) indent=2
            entry [16..25) closed indent=4 key="name" value="app"
            output [30..69) "{{- toYaml .Values.extra | nindent 4 }}"
        entry [70..82) closed indent=0 key="resources" value="["
        entry [85..108) open indent=2 key="{key" value="⟦{{ .Values.k }}⟧},"
        scalar [109..110) indent=0 "]"
    "##};
    assert_dump(source, expected);
}

#[test]
fn block_scalar_body_continues_across_control_lines() {
    let source = indoc! {r##"
        conf: |
          a=1
        {{- if .Values.x }}
          b={{ .Values.b }}
        {{- end }}
        after: {{ .Values.c }}
    "##};
    let expected = indoc! {r##"
        document 0 [0..88)
        entry [0..7) block indent=0 key="conf" block-header=[6..7) body=[8..53) suppressed-holes=2
          control if [14..64)
            branch [14..33) "{{- if .Values.x }}"
        entry [65..87) open indent=0 key="after" value="⟦{{ .Values.c }}⟧"
    "##};
    assert_dump(source, expected);
}
