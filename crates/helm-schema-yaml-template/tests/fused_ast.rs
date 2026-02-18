use std::str::FromStr;

use indoc::indoc;
use test_util::sexpr::SExpr;
use yaml_rust::FusedNode;

#[allow(clippy::too_many_lines)]
fn fused_to_sexpr(node: &FusedNode) -> SExpr {
    match node {
        FusedNode::Stream { items } => SExpr::Node {
            kind: "stream".to_string(),
            children: items.iter().map(fused_to_sexpr).collect(),
        },
        FusedNode::Document { items } => SExpr::Node {
            kind: "doc".to_string(),
            children: items.iter().map(fused_to_sexpr).collect(),
        },
        FusedNode::Mapping { items } => {
            if items.is_empty() {
                SExpr::Leaf {
                    kind: "map".to_string(),
                    text: None,
                }
            } else {
                SExpr::Node {
                    kind: "map".to_string(),
                    children: items.iter().map(fused_to_sexpr).collect(),
                }
            }
        }
        FusedNode::Pair { key, value } => {
            let mut children = Vec::with_capacity(2);
            children.push(fused_to_sexpr(key));
            if let Some(v) = value {
                children.push(fused_to_sexpr(v));
            } else {
                children.push(SExpr::Empty);
            }
            SExpr::Node {
                kind: "entry".to_string(),
                children,
            }
        }
        FusedNode::Sequence { items } => {
            if items.is_empty() {
                SExpr::Leaf {
                    kind: "seq".to_string(),
                    text: None,
                }
            } else {
                SExpr::Node {
                    kind: "seq".to_string(),
                    children: items.iter().map(fused_to_sexpr).collect(),
                }
            }
        }
        FusedNode::Item { value } => {
            if let Some(v) = value {
                fused_to_sexpr(v)
            } else {
                SExpr::Empty
            }
        }
        FusedNode::Scalar { kind, text } => SExpr::Leaf {
            kind: kind.clone(),
            text: Some(text.clone()),
        },
        FusedNode::HelmExpr { text } => SExpr::Leaf {
            kind: "helm_expr".to_string(),
            text: Some(text.clone()),
        },
        FusedNode::HelmComment { text } => SExpr::Leaf {
            kind: "helm_comment".to_string(),
            text: Some(text.clone()),
        },
        FusedNode::If {
            cond,
            then_branch,
            else_branch,
        } => SExpr::Node {
            kind: "if".to_string(),
            children: vec![
                SExpr::Leaf {
                    kind: "cond".to_string(),
                    text: Some(cond.clone()),
                },
                if then_branch.is_empty() {
                    SExpr::Leaf {
                        kind: "then".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "then".to_string(),
                        children: then_branch.iter().map(fused_to_sexpr).collect(),
                    }
                },
                if else_branch.is_empty() {
                    SExpr::Leaf {
                        kind: "else".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "else".to_string(),
                        children: else_branch.iter().map(fused_to_sexpr).collect(),
                    }
                },
            ],
        },
        FusedNode::Range {
            header,
            body,
            else_branch,
        } => SExpr::Node {
            kind: "range".to_string(),
            children: vec![
                SExpr::Leaf {
                    kind: "header".to_string(),
                    text: Some(header.clone()),
                },
                if body.is_empty() {
                    SExpr::Leaf {
                        kind: "body".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "body".to_string(),
                        children: body.iter().map(fused_to_sexpr).collect(),
                    }
                },
                if else_branch.is_empty() {
                    SExpr::Leaf {
                        kind: "else".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "else".to_string(),
                        children: else_branch.iter().map(fused_to_sexpr).collect(),
                    }
                },
            ],
        },
        FusedNode::With {
            header,
            body,
            else_branch,
        } => SExpr::Node {
            kind: "with".to_string(),
            children: vec![
                SExpr::Leaf {
                    kind: "header".to_string(),
                    text: Some(header.clone()),
                },
                if body.is_empty() {
                    SExpr::Leaf {
                        kind: "body".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "body".to_string(),
                        children: body.iter().map(fused_to_sexpr).collect(),
                    }
                },
                if else_branch.is_empty() {
                    SExpr::Leaf {
                        kind: "else".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "else".to_string(),
                        children: else_branch.iter().map(fused_to_sexpr).collect(),
                    }
                },
            ],
        },
        FusedNode::Define { header, body } => SExpr::Node {
            kind: "define".to_string(),
            children: vec![
                SExpr::Leaf {
                    kind: "header".to_string(),
                    text: Some(header.clone()),
                },
                if body.is_empty() {
                    SExpr::Leaf {
                        kind: "body".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "body".to_string(),
                        children: body.iter().map(fused_to_sexpr).collect(),
                    }
                },
            ],
        },
        FusedNode::Block { header, body } => SExpr::Node {
            kind: "block".to_string(),
            children: vec![
                SExpr::Leaf {
                    kind: "header".to_string(),
                    text: Some(header.clone()),
                },
                if body.is_empty() {
                    SExpr::Leaf {
                        kind: "body".to_string(),
                        text: None,
                    }
                } else {
                    SExpr::Node {
                        kind: "body".to_string(),
                        children: body.iter().map(fused_to_sexpr).collect(),
                    }
                },
            ],
        },
        FusedNode::Unknown {
            kind,
            text,
            children,
        } => {
            let mut out_children = Vec::new();
            if let Some(text) = text {
                out_children.push(SExpr::Leaf {
                    kind: "text".to_string(),
                    text: Some(text.clone()),
                });
            }
            out_children.extend(children.iter().map(fused_to_sexpr));
            SExpr::Node {
                kind: kind.clone(),
                children: out_children,
            }
        }
    }
}

fn assert_fused_matches_sexpr(src: &str, want: &str) {
    let have = yaml_rust::parse_fused_yaml_helm(src).expect("parse fused");
    let have = fused_to_sexpr(&have);
    let want = SExpr::from_str(want).expect("parse expected sexpr");
    similar_asserts::assert_eq!(have, want);
}

#[test]
fn if_else_end_with_yaml_branches() {
    let src = indoc! {r"
        {{- if .Values.enabled }}
        foo: bar
        {{- else }}
        {}
        {{- end }}
    "};

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
#[allow(clippy::too_many_lines)]
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
    let src = indoc! {r"
        {{- if .A }}
        foo: 1
        {{- else if .B }}
        foo: 2
        {{- else }}
        foo: 3
        {{- end }}
    "};

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
    let src = indoc! {r"
        {{- range .Values.items }}
        name: {{ .name }}
        {{- else }}
        {}
        {{- end }}
    "};

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
