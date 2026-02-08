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
fn redis_prometheus_rule_yaml() {
    let src = indoc! {r#"
        {{- /*
        Copyright Broadcom, Inc. All Rights Reserved.
        SPDX-License-Identifier: APACHE-2.0
        */}}

        {{- if and .Values.metrics.enabled .Values.metrics.prometheusRule.enabled }}
        apiVersion: monitoring.coreos.com/v1
        kind: PrometheusRule
        metadata:
          name: {{ template "common.names.fullname" . }}
          namespace: {{ default (include "common.names.namespace" .) .Values.metrics.prometheusRule.namespace | quote }}
          labels: {{- include "common.labels.standard" ( dict "customLabels" .Values.commonLabels "context" $ ) | nindent 4 }}
            {{- if .Values.metrics.prometheusRule.additionalLabels }}
            {{- include "common.tplvalues.render" (dict "value" .Values.metrics.prometheusRule.additionalLabels "context" $) | nindent 4 }}
            {{- end }}
          {{- if .Values.commonAnnotations }}
          annotations: {{- include "common.tplvalues.render" ( dict "value" .Values.commonAnnotations "context" $ ) | nindent 4 }}
          {{- end }}
        spec:
          groups:
            - name: {{ include "common.names.fullname" . }}
              rules: {{- include "common.tplvalues.render" ( dict "value" .Values.metrics.prometheusRule.rules "context" $ ) | nindent 8 }}
        {{- end }}
    "#};

    let want = indoc! {r#"
        (doc
          (helm_comment :text "/*\nCopyright Broadcom, Inc. All Rights Reserved.\nSPDX-License-Identifier: APACHE-2.0\n*/")
          (if
            (cond :text "and .Values.metrics.enabled .Values.metrics.prometheusRule.enabled")
            (then
              (map
                (entry
                  (str :text "apiVersion")
                  (str :text "monitoring.coreos.com/v1")
                )
                (entry
                  (str :text "kind")
                  (str :text "PrometheusRule")
                )
                (entry
                  (str :text "metadata")
                  (map
                    (entry
                      (str :text "name")
                      (helm_expr :text "template \"common.names.fullname\" .")
                    )
                    (entry
                      (str :text "namespace")
                      (helm_expr :text "default (include \"common.names.namespace\" .) .Values.metrics.prometheusRule.namespace | quote")
                    )
                    (entry
                      (str :text "labels")
                      (helm_expr :text "include \"common.labels.standard\" ( dict \"customLabels\" .Values.commonLabels \"context\" $ ) | nindent 4")
                    )
                  )
                )
              )
              (if
                (cond :text ".Values.metrics.prometheusRule.additionalLabels")
                (then
                  (helm_expr :text "include \"common.tplvalues.render\" (dict \"value\" .Values.metrics.prometheusRule.additionalLabels \"context\" $) | nindent 4")
                )
                (else)
              )
              (if
                (cond :text ".Values.commonAnnotations")
                (then
                  (map
                    (entry
                      (str :text "annotations")
                      (helm_expr :text "include \"common.tplvalues.render\" ( dict \"value\" .Values.commonAnnotations \"context\" $ ) | nindent 4")
                    )
                  )
                )
                (else)
              )
              (map
                (entry
                  (str :text "spec")
                  (map
                    (entry
                      (str :text "groups")
                      (seq
                        (map
                          (entry
                            (str :text "name")
                            (helm_expr :text "include \"common.names.fullname\" .")
                          )
                          (entry
                            (str :text "rules")
                            (helm_expr :text "include \"common.tplvalues.render\" ( dict \"value\" .Values.metrics.prometheusRule.rules \"context\" $ ) | nindent 8")
                          )
                        )
                      )
                    )
                  )
                )
              )
            )
            (else)
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
                  (helm_expr :text ".name")
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
                  (helm_expr :text "template \"common.capabilities.networkPolicy.apiVersion\" .")
                )
              )
            )
            (else)
          )
        )
    "#};

    assert_fused_matches_sexpr(src, want);
}
