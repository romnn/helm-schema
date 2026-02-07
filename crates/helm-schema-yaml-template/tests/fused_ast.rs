mod sexpr;

use indoc::indoc;

use sexpr::assert_fused_matches_sexpr;

#[test]
fn if_else_end_with_yaml_branches() {
    let src = indoc! {r#"
        {{- if .Values.enabled }}
        foo: bar
        {{- else }}
        {}
        {{- end }}
    "#};

    let want = indoc! {r#"
        (doc
          (if
            (cond :text ".Values.enabled")
            (then
              (map
                (entry
                  (str :text "foo")
                  (str :text "bar")
                )
              )
            )
            (else
              (map)
            )
          )
        )
    "#};

    assert_fused_matches_sexpr(src, want);
}

#[test]
fn else_if_chain_is_nested_if_in_else_branch() {
    let src = indoc! {r#"
        {{- if .A }}
        foo: 1
        {{- else if .B }}
        foo: 2
        {{- else }}
        foo: 3
        {{- end }}
    "#};

    let want = indoc! {r#"
        (doc
          (if
            (cond :text ".A")
            (then
              (map
                (entry
                  (str :text "foo")
                  (int :text "1")
                )
              )
            )
            (else
              (if
                (cond :text ".B")
                (then
                  (map
                    (entry
                      (str :text "foo")
                      (int :text "2")
                    )
                  )
                )
                (else
                  (map
                    (entry
                      (str :text "foo")
                      (int :text "3")
                    )
                  )
                )
              )
            )
          )
        )
    "#};

    assert_fused_matches_sexpr(src, want);
}

#[test]
fn range_else_end_wraps_yaml_body_and_else_branch() {
    let src = indoc! {r#"
        {{- range .Values.items }}
        name: {{ .name }}
        {{- else }}
        {}
        {{- end }}
    "#};

    let want = indoc! {r#"
        (doc
          (range
            (header :text ".Values.items")
            (body
              (map
                (entry
                  (str :text "name")
                  (str :text "{{ .name }}")
                )
              )
            )
            (else
              (map)
            )
          )
        )
    "#};

    assert_fused_matches_sexpr(src, want);
}

#[test]
fn if_block_can_contain_inline_template_values_in_yaml() {
    let src = indoc! {r#"
        {{- if .Values.networkPolicy.enabled }}
        kind: NetworkPolicy
        apiVersion: {{ template "common.capabilities.networkPolicy.apiVersion" . }}
        {{- end }}
    "#};

    let want = indoc! {r#"
        (doc
          (if
            (cond :text ".Values.networkPolicy.enabled")
            (then
              (map
                (entry
                  (str :text "kind")
                  (str :text "NetworkPolicy")
                )
                (entry
                  (str :text "apiVersion")
                  (str :text "{{ template \"common.capabilities.networkPolicy.apiVersion\" . }}")
                )
              )
            )
            (else)
          )
        )
    "#};

    assert_fused_matches_sexpr(src, want);
}
