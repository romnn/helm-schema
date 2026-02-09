mod common;

use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const EXPECTED_SEXPR: &str = r#"(Document
  (HelmComment "/*\nCopyright Broadcom, Inc. All Rights Reserved.\nSPDX-License-Identifier: APACHE-2.0\n*/")
  (If "and .Values.metrics.enabled .Values.metrics.prometheusRule.enabled"
    (then
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "monitoring.coreos.com/v1"))
        (Pair
          (Scalar "kind")
          (Scalar "PrometheusRule"))
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (HelmExpr "template \"common.names.fullname\" ."))
            (Pair
              (Scalar "namespace")
              (HelmExpr "default (include \"common.names.namespace\" .) .Values.metrics.prometheusRule.namespace | quote"))
            (Pair
              (Scalar "labels")
              (HelmExpr "include \"common.labels.standard\" ( dict \"customLabels\" .Values.commonLabels \"context\" $ ) | nindent 4")))))
      (If ".Values.metrics.prometheusRule.additionalLabels"
        (then
          (HelmExpr "include \"common.tplvalues.render\" (dict \"value\" .Values.metrics.prometheusRule.additionalLabels \"context\" $) | nindent 4")))
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
              (Scalar "groups")
              (Sequence
                (Mapping
                  (Pair
                    (Scalar "name")
                    (HelmExpr "include \"common.names.fullname\" ."))
                  (Pair
                    (Scalar "rules")
                    (HelmExpr "include \"common.tplvalues.render\" ( dict \"value\" .Values.metrics.prometheusRule.rules \"context\" $ ) | nindent 8")))))))))))
"#;

#[test]
fn fused_rust_ast() {
    let src = common::prometheusrule_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(ast.to_sexpr(), EXPECTED_SEXPR.trim_end());
}

#[test]
fn tree_sitter_ast() {
    let src = common::prometheusrule_src();
    let ast = TreeSitterParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(ast.to_sexpr(), EXPECTED_SEXPR.trim_end());
}
