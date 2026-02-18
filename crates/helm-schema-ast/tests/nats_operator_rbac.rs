use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const EXPECTED_SEXPR: &str = r#"(Document
  (If ".Values.rbacEnabled"
    (then
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "rbac.authorization.k8s.io/v1"))
        (Pair
          (Scalar "kind")
          (Scalar "ClusterRole"))
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (Scalar "nats-io-nats-operator-crd"))))
        (Pair
          (Scalar "rules")
          (Sequence
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "apiextensions.k8s.io")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "customresourcedefinitions")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "get")
                  (Scalar "list")
                  (Scalar "create")
                  (Scalar "update")
                  (Scalar "watch"))))
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "nats.io")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "natsclusters")
                  (Scalar "natsserviceroles")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "*"))))
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "pods")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "create")
                  (Scalar "watch")
                  (Scalar "get")
                  (Scalar "patch")
                  (Scalar "update")
                  (Scalar "delete")
                  (Scalar "list")))))))
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
              (Scalar "nats-io-nats-operator-crd-binding"))))
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
              (Scalar "nats-io-nats-operator-crd"))))
        (Pair
          (Scalar "subjects")
          (Sequence
            (Mapping
              (Pair
                (Scalar "kind")
                (Scalar "ServiceAccount"))
              (Pair
                (Scalar "name")
                (Scalar "nats-operator"))
              (Pair
                (Scalar "namespace")
                (HelmExpr ".Release.Namespace"))))))
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "rbac.authorization.k8s.io/v1")))
      (If ".Values.clusterScoped"
        (then
          (Mapping
            (Pair
              (Scalar "kind")
              (Scalar "ClusterRole"))))
        (else
          (Mapping
            (Pair
              (Scalar "kind")
              (Scalar "Role")))))
      (Mapping
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (Scalar "nats-io-nats-operator"))))
        (Pair
          (Scalar "rules")
          (Sequence
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "pods")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "create")
                  (Scalar "watch")
                  (Scalar "get")
                  (Scalar "patch")
                  (Scalar "update")
                  (Scalar "delete")
                  (Scalar "list"))))
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "services")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "create")
                  (Scalar "watch")
                  (Scalar "get")
                  (Scalar "patch")
                  (Scalar "update")
                  (Scalar "delete")
                  (Scalar "list"))))
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "secrets")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "create")
                  (Scalar "watch")
                  (Scalar "get")
                  (Scalar "update")
                  (Scalar "delete")
                  (Scalar "list"))))
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "pods/exec")
                  (Scalar "pods/log")
                  (Scalar "serviceaccounts/token")
                  (Scalar "events")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "*"))))
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "namespaces")
                  (Scalar "serviceaccounts")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "list")
                  (Scalar "get")
                  (Scalar "watch"))))
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "endpoints")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "create")
                  (Scalar "watch")
                  (Scalar "get")
                  (Scalar "update")
                  (Scalar "delete")
                  (Scalar "list")))))))
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "rbac.authorization.k8s.io/v1")))
      (If ".Values.clusterScoped"
        (then
          (Mapping
            (Pair
              (Scalar "kind")
              (Scalar "ClusterRoleBinding"))))
        (else
          (Mapping
            (Pair
              (Scalar "kind")
              (Scalar "RoleBinding")))))
      (Mapping
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (Scalar "nats-io-nats-operator-binding"))))
        (Pair
          (Scalar "roleRef")
          (Mapping
            (Pair
              (Scalar "apiGroup")
              (Scalar "rbac.authorization.k8s.io")))))
      (If ".Values.clusterScoped"
        (then
          (Mapping
            (Pair
              (Scalar "kind")
              (Scalar "ClusterRole"))))
        (else
          (Mapping
            (Pair
              (Scalar "kind")
              (Scalar "Role"))))))))

"#;

#[test]
fn fused_rust_ast() {
    let src = test_util::read_testdata("charts/nats-operator/templates/rbac.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}

#[test]
fn tree_sitter_ast() {
    let src = test_util::read_testdata("charts/nats-operator/templates/rbac.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}
