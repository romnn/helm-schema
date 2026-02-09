mod common;

use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

// Both parsers produce identical AST for the networkpolicy template.

const EXPECTED_SEXPR: &str = r#"(Document
  (HelmComment "/*\nCopyright Broadcom, Inc. All Rights Reserved.\nSPDX-License-Identifier: APACHE-2.0\n*/")
  (If ".Values.networkPolicy.enabled"
    (then
      (Mapping
        (Pair
          (Scalar "kind")
          (Scalar "NetworkPolicy"))
        (Pair
          (Scalar "apiVersion")
          (HelmExpr "template \"common.capabilities.networkPolicy.apiVersion\" ."))
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (HelmExpr "template \"common.names.fullname\" ."))
            (Pair
              (Scalar "namespace")
              (HelmExpr "include \"common.names.namespace\" . | quote"))
            (Pair
              (Scalar "labels")
              (HelmExpr "include \"common.labels.standard\" ( dict \"customLabels\" .Values.commonLabels \"context\" $ ) | nindent 4")))))
      (If ".Values.commonAnnotations"
        (then
          (Mapping
            (Pair
              (Scalar "annotations")
              (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.commonAnnotations \"context\" $ ) | nindent 4")))))
      (Mapping
        (Pair
          (Scalar "spec")
          (Mapping
            (Pair
              (Scalar "podSelector")
              (Mapping
                (Pair
                  (Scalar "matchLabels")
                  (HelmExpr "include \"common.labels.matchLabels\" ( dict \"customLabels\" .Values.commonLabels \"context\" $ ) | nindent 6"))))
            (Pair
              (Scalar "policyTypes")
              (Sequence
                (Scalar "Ingress")
                (Scalar "Egress"))))))
      (If ".Values.networkPolicy.allowExternalEgress"
        (then
          (Mapping
            (Pair
              (Scalar "egress")
              (Sequence
                (Mapping)))))
        (else
          (Mapping
            (Pair
              (Scalar "egress")))
          (If "eq .Values.architecture \"replication\""
            (then
              (Sequence
                (Mapping
                  (Pair
                    (Scalar "ports")
                    (Sequence
                      (Mapping
                        (Pair
                          (Scalar "port")
                          (Scalar "53"))
                        (Pair
                          (Scalar "protocol")
                          (Scalar "UDP"))))))
                (Mapping
                  (Pair
                    (Scalar "ports")
                    (Sequence
                      (Mapping
                        (Pair
                          (Scalar "port")
                          (HelmExpr ".Values.master.containerPorts.redis")))))))
              (If ".Values.sentinel.enabled"
                (then
                  (Sequence
                    (Mapping
                      (Pair
                        (Scalar "port")
                        (HelmExpr ".Values.sentinel.containerPorts.sentinel"))))))
              (Mapping
                (Pair
                  (Scalar "to")
                  (Sequence
                    (Mapping
                      (Pair
                        (Scalar "podSelector")
                        (Mapping
                          (Pair
                            (Scalar "matchLabels")
                            (HelmExpr "include \"common.labels.matchLabels\" ( dict \"customLabels\" .Values.commonLabels \"context\" $ ) | nindent 14"))))))))))
          (If ".Values.networkPolicy.extraEgress"
            (then
              (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.networkPolicy.extraEgress \"context\" $ ) | nindent 4")))))
      (Mapping
        (Pair
          (Scalar "ingress")
          (Sequence
            (Mapping
              (Pair
                (Scalar "ports")
                (Sequence
                  (Mapping
                    (Pair
                      (Scalar "port")
                      (HelmExpr ".Values.master.containerPorts.redis")))))))))
      (If ".Values.sentinel.enabled"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "port")
                (HelmExpr ".Values.sentinel.containerPorts.sentinel"))))))
      (If "not .Values.networkPolicy.allowExternal"
        (then
          (Mapping
            (Pair
              (Scalar "from")
              (Sequence
                (Mapping
                  (Pair
                    (Scalar "podSelector")
                    (Mapping
                      (Pair
                        (Scalar "matchLabels")
                        (Mapping
                          (Pair
                            (Scalar "{{ template \"common.names.fullname\" . }}-client")
                            (Scalar "true")))))))
                (Mapping
                  (Pair
                    (Scalar "podSelector")
                    (Mapping
                      (Pair
                        (Scalar "matchLabels")
                        (HelmExpr "include \"common.labels.matchLabels\" ( dict \"customLabels\" .Values.commonLabels \"context\" $ ) | nindent 14"))))))))
          (If "or .Values.networkPolicy.ingressNSMatchLabels .Values.networkPolicy.ingressNSPodMatchLabels"
            (then
              (Sequence
                (Mapping
                  (Pair
                    (Scalar "namespaceSelector")
                    (Mapping
                      (Pair
                        (Scalar "matchLabels"))))))
              (If ".Values.networkPolicy.ingressNSMatchLabels"
                (then
                  (Range "$key, $value := .Values.networkPolicy.ingressNSMatchLabels"
                    (body
                      (Mapping
                        (Pair
                          (HelmExpr "$key | quote")
                          (HelmExpr "$value | quote"))))))
                (else
                  (Mapping)))
              (If ".Values.networkPolicy.ingressNSPodMatchLabels"
                (then
                  (Mapping
                    (Pair
                      (Scalar "podSelector")
                      (Mapping
                        (Pair
                          (Scalar "matchLabels")))))
                  (Range "$key, $value := .Values.networkPolicy.ingressNSPodMatchLabels"
                    (body
                      (Mapping
                        (Pair
                          (HelmExpr "$key | quote")
                          (HelmExpr "$value | quote")))))))))))
      (If ".Values.metrics.enabled"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "ports")
                (Sequence
                  (Mapping
                    (Pair
                      (Scalar "port")
                      (HelmExpr ".Values.metrics.containerPorts.http")))))))
          (If "not .Values.networkPolicy.metrics.allowExternal"
            (then
              (Mapping
                (Pair
                  (Scalar "from")))
              (If "or .Values.networkPolicy.metrics.ingressNSMatchLabels .Values.networkPolicy.metrics.ingressNSPodMatchLabels"
                (then
                  (Sequence
                    (Mapping
                      (Pair
                        (Scalar "namespaceSelector")
                        (Mapping
                          (Pair
                            (Scalar "matchLabels"))))))
                  (If ".Values.networkPolicy.metrics.ingressNSMatchLabels"
                    (then
                      (Range "$key, $value := .Values.networkPolicy.metrics.ingressNSMatchLabels"
                        (body
                          (Mapping
                            (Pair
                              (HelmExpr "$key | quote")
                              (HelmExpr "$value | quote"))))))
                    (else
                      (Mapping)))
                  (If ".Values.networkPolicy.metrics.ingressNSPodMatchLabels"
                    (then
                      (Mapping
                        (Pair
                          (Scalar "podSelector")
                          (Mapping
                            (Pair
                              (Scalar "matchLabels")))))
                      (Range "$key, $value := .Values.networkPolicy.metrics.ingressNSPodMatchLabels"
                        (body
                          (Mapping
                            (Pair
                              (HelmExpr "$key | quote")
                              (HelmExpr "$value | quote")))))))))))))
      (If ".Values.networkPolicy.extraIngress"
        (then
          (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.networkPolicy.extraIngress \"context\" $ ) | nindent 4"))))))"#;

#[test]
fn fused_rust_ast() {
    let src = common::networkpolicy_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR);
}

#[test]
fn tree_sitter_ast() {
    let src = common::networkpolicy_src();
    let ast = TreeSitterParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR);
}
