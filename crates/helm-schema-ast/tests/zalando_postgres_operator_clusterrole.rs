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
          (Scalar "ClusterRole"))
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
          (Scalar "rules")
          (Sequence
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "acid.zalan.do")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "postgresqls")
                  (Scalar "postgresqls/status")
                  (Scalar "operatorconfigurations")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "create")
                  (Scalar "delete")
                  (Scalar "deletecollection")
                  (Scalar "get")
                  (Scalar "list")
                  (Scalar "patch")
                  (Scalar "update")
                  (Scalar "watch"))))
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "acid.zalan.do")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "postgresteams")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "get")
                  (Scalar "list")
                  (Scalar "watch")))))))
      (If ".Values.enableStreams"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "zalando.org")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "fabriceventstreams")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "create")
                  (Scalar "delete")
                  (Scalar "deletecollection")
                  (Scalar "get")
                  (Scalar "list")
                  (Scalar "patch")
                  (Scalar "update")
                  (Scalar "watch")))))))
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
              (Scalar "get")))))
      (If "toString .Values.configGeneral.enable_crd_registration | eq \"true\""
        (then
          (Sequence
            (Scalar "create")
            (Scalar "patch")
            (Scalar "update"))))
      (Sequence
        (Mapping
          (Pair
            (Scalar "apiGroups")
            (Sequence
              (Scalar "")))
          (Pair
            (Scalar "resources")
            (Sequence
              (Scalar "events")))
          (Pair
            (Scalar "verbs")
            (Sequence
              (Scalar "create")
              (Scalar "get")
              (Scalar "list")
              (Scalar "patch")
              (Scalar "update")
              (Scalar "watch")))))
      (If "toString .Values.configGeneral.kubernetes_use_configmaps | eq \"true\""
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "configmaps")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "create")
                  (Scalar "delete")
                  (Scalar "deletecollection")
                  (Scalar "get")
                  (Scalar "list")
                  (Scalar "patch")
                  (Scalar "update")
                  (Scalar "watch"))))))
        (else
          (Sequence
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "configmaps")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "get"))))
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
                  (Scalar "delete")
                  (Scalar "deletecollection")
                  (Scalar "get")
                  (Scalar "list")
                  (Scalar "patch")
                  (Scalar "update")
                  (Scalar "watch")))))))
      (Sequence
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
              (Scalar "delete")
              (Scalar "get")
              (Scalar "patch")
              (Scalar "update"))))
        (Mapping
          (Pair
            (Scalar "apiGroups")
            (Sequence
              (Scalar "")))
          (Pair
            (Scalar "resources")
            (Sequence
              (Scalar "nodes")))
          (Pair
            (Scalar "verbs")
            (Sequence
              (Scalar "get")
              (Scalar "list")
              (Scalar "watch"))))
        (Mapping
          (Pair
            (Scalar "apiGroups")
            (Sequence
              (Scalar "")))
          (Pair
            (Scalar "resources")
            (Sequence
              (Scalar "persistentvolumeclaims")))
          (Pair
            (Scalar "verbs")
            (Sequence
              (Scalar "delete")
              (Scalar "get")
              (Scalar "list")
              (Scalar "patch")))))
      (If "or (toString .Values.configKubernetes.storage_resize_mode | eq \"pvc\") (toString .Values.configKubernetes.storage_resize_mode | eq \"mixed\")"
        (then
          (Sequence
            (Scalar "update"))))
      (Sequence
        (Mapping
          (Pair
            (Scalar "apiGroups")
            (Sequence
              (Scalar "")))
          (Pair
            (Scalar "resources")
            (Sequence
              (Scalar "persistentvolumes")))
          (Pair
            (Scalar "verbs")
            (Sequence
              (Scalar "get")
              (Scalar "list")))))
      (If "toString .Values.configKubernetes.storage_resize_mode | eq \"ebs\""
        (then
          (Sequence
            (Scalar "update"))))
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
              (Scalar "delete")
              (Scalar "get")
              (Scalar "list")
              (Scalar "patch")
              (Scalar "update")
              (Scalar "watch"))))
        (Mapping
          (Pair
            (Scalar "apiGroups")
            (Sequence
              (Scalar "")))
          (Pair
            (Scalar "resources")
            (Sequence
              (Scalar "pods/exec")))
          (Pair
            (Scalar "verbs")
            (Sequence
              (Scalar "create"))))
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
              (Scalar "delete")
              (Scalar "get")
              (Scalar "patch")
              (Scalar "update"))))
        (Mapping
          (Pair
            (Scalar "apiGroups")
            (Sequence
              (Scalar "apps")))
          (Pair
            (Scalar "resources")
            (Sequence
              (Scalar "statefulsets")
              (Scalar "deployments")))
          (Pair
            (Scalar "verbs")
            (Sequence
              (Scalar "create")
              (Scalar "delete")
              (Scalar "get")
              (Scalar "list")
              (Scalar "patch")
              (Scalar "update"))))
        (Mapping
          (Pair
            (Scalar "apiGroups")
            (Sequence
              (Scalar "batch")))
          (Pair
            (Scalar "resources")
            (Sequence
              (Scalar "cronjobs")))
          (Pair
            (Scalar "verbs")
            (Sequence
              (Scalar "create")
              (Scalar "delete")
              (Scalar "get")
              (Scalar "list")
              (Scalar "patch")
              (Scalar "update"))))
        (Mapping
          (Pair
            (Scalar "apiGroups")
            (Sequence
              (Scalar "")))
          (Pair
            (Scalar "resources")
            (Sequence
              (Scalar "namespaces")))
          (Pair
            (Scalar "verbs")
            (Sequence
              (Scalar "get"))))
        (Mapping
          (Pair
            (Scalar "apiGroups")
            (Sequence
              (Scalar "policy")))
          (Pair
            (Scalar "resources")
            (Sequence
              (Scalar "poddisruptionbudgets")))
          (Pair
            (Scalar "verbs")
            (Sequence
              (Scalar "create")
              (Scalar "delete")
              (Scalar "get"))))
        (Mapping
          (Pair
            (Scalar "apiGroups")
            (Sequence
              (Scalar "")))
          (Pair
            (Scalar "resources")
            (Sequence
              (Scalar "serviceaccounts")))
          (Pair
            (Scalar "verbs")
            (Sequence
              (Scalar "get")
              (Scalar "create"))))
        (Mapping
          (Pair
            (Scalar "apiGroups")
            (Sequence
              (Scalar "rbac.authorization.k8s.io")))
          (Pair
            (Scalar "resources")
            (Sequence
              (Scalar "rolebindings")))
          (Pair
            (Scalar "verbs")
            (Sequence
              (Scalar "get")
              (Scalar "create")))))
      (If "toString .Values.configKubernetes.spilo_privileged | eq \"true\""
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "apiGroups")
                (Sequence
                  (Scalar "extensions")))
              (Pair
                (Scalar "resources")
                (Sequence
                  (Scalar "podsecuritypolicies")))
              (Pair
                (Scalar "resourceNames")
                (Sequence
                  (Scalar "privileged")))
              (Pair
                (Scalar "verbs")
                (Sequence
                  (Scalar "use"))))))))))
"#;

#[test]
fn fused_rust_ast() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator/templates/clusterrole.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}

#[test]
fn tree_sitter_ast() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator/templates/clusterrole.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}
