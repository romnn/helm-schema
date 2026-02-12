mod sexpr;

use indoc::indoc;

use sexpr::{assert_yaml_doc_matches_sexpr, assert_yaml_matches_sexpr};

#[test]
fn yaml_scalar_string_plain() {
    let src = indoc! {r"
        foo: bar
    "};
    let want = indoc! {r#"
        (map
          (entry
            (str :text "foo")
            (str :text "bar")
          )
        )
    "#};
    assert_yaml_doc_matches_sexpr(src, want);
}

#[test]
fn literal_unterminated_helm_open_is_plain_text() {
    let src = indoc! {r#"
        x: "{{"
    "#};

    let want = indoc! {r#"
        (map
          (entry
            (str :text "x")
            (str :text "{{")
          )
        )
    "#};

    assert_yaml_doc_matches_sexpr(src, want);
}

#[test]
fn skip_template_comment_block_then_yaml() {
    let src = indoc! {r"
        {{- /*
        some template comment
        */}}
        foo: bar
    "};

    let want = indoc! {r#"
        (map
          (entry
            (str :text "foo")
            (str :text "bar")
          )
        )
    "#};

    assert_yaml_doc_matches_sexpr(src, want);
}

#[test]
fn skip_multiline_action_at_line_start_then_yaml() {
    let src = indoc! {r"
        {{- if and
              .Values.enabled
              (eq 1 1)
        -}}
        foo: bar
        {{- end -}}
    "};

    let want = indoc! {r#"
        (map
          (entry
            (str :text "foo")
            (str :text "bar")
          )
        )
    "#};

    assert_yaml_doc_matches_sexpr(src, want);
}

#[test]
fn block_scalar_can_contain_template_lines_with_less_indentation() {
    let src = indoc! {r"
        script: |
          echo start
        {{- if .Values.enabled }}
          echo enabled
        {{- end }}
          echo done
    "};

    let want = indoc! {r#"
        (map
          (entry
            (str :text "script")
            (str :text "echo start\n{{- if .Values.enabled }}\necho enabled\n{{- end }}\necho done\n")
          )
        )
    "#};

    assert_yaml_doc_matches_sexpr(src, want);
}

#[test]
fn skip_fragment_injector_line_inside_mapping_body() {
    let src = indoc! {r#"
        labels:
          {{- include "x" . | nindent 2 }}
          app: test
    "#};

    let want = indoc! {r#"
        (map
          (entry
            (str :text "labels")
            (map
              (entry
                (str :text "app")
                (str :text "test")
              )
            )
          )
        )
    "#};

    assert_yaml_doc_matches_sexpr(src, want);
}

#[test]
fn malformed_range_dot_syntax_is_skipped_as_control_line() {
    let src = indoc! {r"
        {{- range.spec.ports }}
        foo: bar
    "};

    let want = indoc! {r#"
        (map
          (entry
            (str :text "foo")
            (str :text "bar")
          )
        )
    "#};

    assert_yaml_doc_matches_sexpr(src, want);
}

#[test]
fn helm_action_used_as_mapping_key() {
    let src = indoc! {r"
        {{ $key | quote }}: {{ $value | quote }}
    "};

    let want = indoc! {r#"
        (map
          (entry
            (str :text "{{ $key | quote }}")
            (str :text "{{ $value | quote }}")
          )
        )
    "#};

    assert_yaml_doc_matches_sexpr(src, want);
}

#[test]
fn flow_sequence_with_unquoted_helm_action_item() {
    let src = indoc! {r"
        items: [{{ .Values.a }}, 2]
    "};

    let want = indoc! {r#"
        (map
          (entry
            (str :text "items")
            (seq
              (str :text "{{ .Values.a }}")
              (int :text "2")
            )
          )
        )
    "#};

    assert_yaml_doc_matches_sexpr(src, want);
}

#[test]
fn yaml_scalar_with_inline_helm_action() {
    let src = indoc! {r#"
        name: {{ include "common.names.fullname" . }}
    "#};
    let want = indoc! {r#"
        (map
          (entry
            (str :text "name")
            (str :text "{{ include \"common.names.fullname\" . }}")
          )
        )
    "#};
    assert_yaml_doc_matches_sexpr(src, want);
}

#[test]
fn yaml_sequence_with_inline_helm_action() {
    let src = indoc! {r#"
        args:
          - "--name={{ include "x" . }}"
          - "--port=8080"
    "#};

    let want = indoc! {r#"
        (map
          (entry
            (str :text "args")
            (seq
              (str :text "--name={{ include \"x\" . }}")
              (str :text "--port=8080")
            )
          )
        )
    "#};

    assert_yaml_doc_matches_sexpr(src, want);
}

#[test]
fn skip_inline_value_fragment_with_nindent_allows_nested_mapping_value() {
    let src = indoc! {r#"
        metadata:
          labels: {{- include "x" . | nindent 4 }}
          name: foo
    "#};

    // Best-effort parse keeps structural YAML that exists literally in the file;
    // the `labels:` value is treated as empty/omitted by the loader, but the map
    // remains parsable and `name` is preserved.
    // NOTE: yaml-rust loads an omitted mapping value as YAML `null`.
    let want = indoc! {r#"
        (map
          (entry
            (str :text "metadata")
            (map
              (entry
                (str :text "labels")
                (null)
              )
              (entry
                (str :text "name")
                (str :text "foo")
              )
            )
          )
        )
    "#};

    assert_yaml_doc_matches_sexpr(src, want);
}

#[test]
fn parse_multi_document_stream() {
    let src = indoc! {r"
        a: 1
        ---
        b: 2
    "};

    let want = indoc! {r#"
        (stream
          (doc
            (map
              (entry (str :text "a") (int :text "1"))
            )
          )
          (doc
            (map
              (entry (str :text "b") (int :text "2"))
            )
          )
        )
    "#};

    assert_yaml_matches_sexpr(src, want);
}

#[test]
fn yaml_flow_sequence_contains_helm_action_text() {
    let src = indoc! {r#"
        items: ["{{ template "x" . }}", "y"]
    "#};

    let want = indoc! {r#"
        (map
          (entry
            (str :text "items")
            (seq
              (str :text "{{ template \"x\" . }}")
              (str :text "y")
            )
          )
        )
    "#};

    assert_yaml_doc_matches_sexpr(src, want);
}
