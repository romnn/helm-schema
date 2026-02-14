use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const EXPECTED_SEXPR: &str = r#"(Document
  (If ".Values.podPriorityClassName.create"
    (then
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "scheduling.k8s.io/v1"))
        (Pair
          (Scalar "description")
          (Scalar "Use only for databases controlled by Postgres operator"))
        (Pair
          (Scalar "kind")
          (Scalar "PriorityClass"))
        (Pair
          (Scalar "metadata")
          (Mapping
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
                  (HelmExpr ".Release.Name"))))
            (Pair
              (Scalar "name")
              (HelmExpr "include \"postgres-pod.priorityClassName\" ."))
            (Pair
              (Scalar "namespace")
              (HelmExpr ".Release.Namespace"))))
        (Pair
          (Scalar "preemptionPolicy")
          (Scalar "PreemptLowerPriority"))
        (Pair
          (Scalar "globalDefault")
          (Scalar "false"))
        (Pair
          (Scalar "value")
          (HelmExpr ".Values.podPriorityClassName.priority"))))))"#;

#[test]
fn fused_rust_ast() {
    let src = test_util::read_testdata(
        "charts/zalando-postgres-operator/templates/postgres-pod-priority-class.yaml",
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
        "charts/zalando-postgres-operator/templates/postgres-pod-priority-class.yaml",
    );
    let ast = TreeSitterParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}
