use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const EXPECTED_SEXPR: &str = r#"(Document
  (If "and .Values.prometheus.enabled (not .Values.prometheus.podmonitor.enabled)"
    (then
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "v1"))
        (Pair
          (Scalar "kind")
          (Scalar "Service"))
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (HelmExpr "template \"cert-manager.fullname\" ."))
            (Pair
              (Scalar "namespace")
              (HelmExpr "include \"cert-manager.namespace\" .")))))
      (With ".Values.serviceAnnotations"
        (body
          (Mapping
            (Pair
              (Scalar "annotations")))
          (HelmExpr "toYaml . | indent 4")))
      (Mapping
        (Pair
          (Scalar "labels")
          (Mapping
            (Pair
              (Scalar "app")
              (HelmExpr "include \"cert-manager.name\" ."))
            (Pair
              (Scalar "app.kubernetes.io/name")
              (HelmExpr "include \"cert-manager.name\" ."))
            (Pair
              (Scalar "app.kubernetes.io/instance")
              (HelmExpr ".Release.Name"))
            (Pair
              (Scalar "app.kubernetes.io/component")
              (Scalar "controller")))))
      (HelmExpr "include \"labels\" . | nindent 4")
      (With ".Values.serviceLabels"
        (body
          (HelmExpr "toYaml . | nindent 4")))
      (Mapping
        (Pair
          (Scalar "spec")
          (Mapping
            (Pair
              (Scalar "type")
              (Scalar "ClusterIP")))))
      (If ".Values.serviceIPFamilyPolicy"
        (then
          (Mapping
            (Pair
              (Scalar "ipFamilyPolicy")
              (HelmExpr ".Values.serviceIPFamilyPolicy")))))
      (If ".Values.serviceIPFamilies"
        (then
          (Mapping
            (Pair
              (Scalar "ipFamilies")
              (HelmExpr ".Values.serviceIPFamilies | toYaml | nindent 2")))))
      (Mapping
        (Pair
          (Scalar "ports")
          (Sequence
            (Mapping
              (Pair
                (Scalar "protocol")
                (Scalar "TCP"))
              (Pair
                (Scalar "port")
                (Scalar "9402"))
              (Pair
                (Scalar "name")
                (Scalar "tcp-prometheus-servicemonitor"))
              (Pair
                (Scalar "targetPort")
                (HelmExpr ".Values.prometheus.servicemonitor.targetPort")))))
        (Pair
          (Scalar "selector")
          (Mapping
            (Pair
              (Scalar "app.kubernetes.io/name")
              (HelmExpr "include \"cert-manager.name\" ."))
            (Pair
              (Scalar "app.kubernetes.io/instance")
              (HelmExpr ".Release.Name"))
            (Pair
              (Scalar "app.kubernetes.io/component")
              (Scalar "controller"))))))))"#;

#[test]
fn fused_rust_ast() {
    let src = test_util::read_testdata("charts/cert-manager/templates/service.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}

#[test]
fn tree_sitter_ast() {
    let src = test_util::read_testdata("charts/cert-manager/templates/service.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}
