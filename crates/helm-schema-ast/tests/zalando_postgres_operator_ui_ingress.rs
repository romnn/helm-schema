use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const EXPECTED_SEXPR: &str = r#"(Document
  (If ".Values.ingress.enabled"
    (then
      (HelmExpr "$fullName := include \"postgres-operator-ui.fullname\" .")
      (HelmExpr "$svcPort := .Values.service.port")
      (If "semverCompare \">=1.19-0\" .Capabilities.KubeVersion.GitVersion"
        (then
          (Mapping
            (Pair
              (Scalar "apiVersion")
              (Scalar "networking.k8s.io/v1"))))
        (else
          (If "semverCompare \">=1.14-0\" .Capabilities.KubeVersion.GitVersion"
            (then
              (Mapping
                (Pair
                  (Scalar "apiVersion")
                  (Scalar "networking.k8s.io/v1beta1"))))
            (else
              (Mapping
                (Pair
                  (Scalar "apiVersion")
                  (Scalar "extensions/v1beta1")))))))
      (Mapping
        (Pair
          (Scalar "kind")
          (Scalar "Ingress"))
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (HelmExpr "$fullName"))
            (Pair
              (Scalar "namespace")
              (HelmExpr ".Release.Namespace"))
            (Pair
              (Scalar "labels")
              (Mapping
                (Pair
                  (Scalar "app.kubernetes.io/name")
                  (HelmExpr "template \"postgres-operator-ui.name\" ."))
                (Pair
                  (Scalar "helm.sh/chart")
                  (HelmExpr "template \"postgres-operator-ui.chart\" ."))
                (Pair
                  (Scalar "app.kubernetes.io/managed-by")
                  (HelmExpr ".Release.Service"))
                (Pair
                  (Scalar "app.kubernetes.io/instance")
                  (HelmExpr ".Release.Name")))))))
      (With ".Values.ingress.annotations"
        (body
          (Mapping
            (Pair
              (Scalar "annotations")
              (HelmExpr "toYaml . | nindent 4")))))
      (Mapping
        (Pair
          (Scalar "spec")))
      (If ".Values.ingress.ingressClassName"
        (then
          (Mapping
            (Pair
              (Scalar "ingressClassName")
              (HelmExpr ".Values.ingress.ingressClassName")))))
      (If ".Values.ingress.tls"
        (then
          (Mapping
            (Pair
              (Scalar "tls")))
          (Range ".Values.ingress.tls"
            (body
              (Sequence
                (Mapping
                  (Pair
                    (Scalar "hosts"))))
              (Range ".hosts"
                (body
                  (Sequence
                    (HelmExpr ". | quote"))))
              (Mapping
                (Pair
                  (Scalar "secretName")
                  (HelmExpr ".secretName")))))))
      (Mapping
        (Pair
          (Scalar "rules")))
      (Range ".Values.ingress.hosts"
        (body
          (Sequence
            (Mapping
              (Pair
                (Scalar "host")
                (HelmExpr ".host | quote"))
              (Pair
                (Scalar "http")
                (Mapping
                  (Pair
                    (Scalar "paths"))))))
          (Range ".paths"
            (body
              (Sequence
                (Mapping
                  (Pair
                    (Scalar "path")
                    (HelmExpr "."))))
              (If "semverCompare \">=1.19-0\" $.Capabilities.KubeVersion.GitVersion"
                (then
                  (Mapping
                    (Pair
                      (Scalar "pathType")
                      (Scalar "Prefix"))
                    (Pair
                      (Scalar "backend")
                      (Mapping
                        (Pair
                          (Scalar "service")
                          (Mapping
                            (Pair
                              (Scalar "name")
                              (HelmExpr "$fullName"))
                            (Pair
                              (Scalar "port")
                              (Mapping
                                (Pair
                                  (Scalar "number")
                                  (HelmExpr "$svcPort"))))))))))
                (else
                  (Mapping
                    (Pair
                      (Scalar "backend")
                      (Mapping
                        (Pair
                          (Scalar "serviceName")
                          (HelmExpr "$fullName"))
                        (Pair
                          (Scalar "servicePort")
                          (HelmExpr "$svcPort"))))))))))))))
"#;

#[test]
fn fused_rust_ast() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator-ui/templates/ingress.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}

#[test]
fn tree_sitter_ast() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator-ui/templates/ingress.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}
