use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const EXPECTED_SEXPR: &str = r#"(Document
  (If ".Values.rbac.create"
    (then
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "rbac.authorization.k8s.io/v1"))
        (Pair
          (Scalar "kind")
          (Scalar "ClusterRoleBinding"))
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (HelmExpr "include \"postgres-operator.serviceAccountName\" ."))
            (Pair
              (Scalar "labels")
              (Mapping
                (Pair
                  (Scalar "app.kubernetes.io/name")
                  (HelmExpr "template \"postgres-operator.name\" ."))
                (Pair
                  (Scalar "helm.sh/chart")
                  (HelmExpr "template \"postgres-operator.chart\" ."))
                (Pair
                  (Scalar "app.kubernetes.io/managed-by")
                  (HelmExpr ".Release.Service"))
                (Pair
                  (Scalar "app.kubernetes.io/instance")
                  (HelmExpr ".Release.Name"))))))
        (Pair
          (Scalar "roleRef")
          (Mapping
            (Pair
              (Scalar "apiGroup")
              (Scalar "rbac.authorization.k8s.io"))
            (Pair
              (Scalar "kind")
              (Scalar "ClusterRole"))
            (Pair
              (Scalar "name")
              (HelmExpr "include \"postgres-operator.serviceAccountName\" ."))))
        (Pair
          (Scalar "subjects")
          (Sequence
            (Mapping
              (Pair
                (Scalar "kind")
                (Scalar "ServiceAccount"))
              (Pair
                (Scalar "name")
                (HelmExpr "include \"postgres-operator.serviceAccountName\" ."))
              (Pair
                (Scalar "namespace")
                (HelmExpr ".Release.Namespace")))))))))"#;

#[test]
fn fused_rust_ast() {
    let src = test_util::read_testdata(
        "charts/zalando-postgres-operator/templates/clusterrolebinding.yaml",
    );
    let ast = FusedRustParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}

#[test]
fn tree_sitter_ast() {
    let src = test_util::read_testdata(
        "charts/zalando-postgres-operator/templates/clusterrolebinding.yaml",
    );
    let ast = TreeSitterParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}
