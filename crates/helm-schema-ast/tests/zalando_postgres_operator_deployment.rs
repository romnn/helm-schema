use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const EXPECTED_SEXPR: &str = r#"(Document
  (Mapping
    (Pair
      (Scalar "apiVersion")
      (Scalar "apps/v1"))
    (Pair
      (Scalar "kind")
      (Scalar "Deployment"))
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
          (HelmExpr "template \"postgres-operator.fullname\" ."))
        (Pair
          (Scalar "namespace")
          (HelmExpr ".Release.Namespace"))))
    (Pair
      (Scalar "spec")
      (Mapping
        (Pair
          (Scalar "replicas")
          (Scalar "1"))
        (Pair
          (Scalar "selector")
          (Mapping
            (Pair
              (Scalar "matchLabels")
              (Mapping
                (Pair
                  (Scalar "app.kubernetes.io/name")
                  (HelmExpr "template \"postgres-operator.name\" ."))
                (Pair
                  (Scalar "app.kubernetes.io/instance")
                  (HelmExpr ".Release.Name"))))))
        (Pair
          (Scalar "template")
          (Mapping
            (Pair
              (Scalar "metadata")
              (Mapping
                (Pair
                  (Scalar "annotations")))))))))
  (If "eq .Values.configTarget \"ConfigMap\""
    (then
      (Mapping
        (Pair
          (Scalar "checksum/config")
          (HelmExpr "include (print $.Template.BasePath \"/configmap.yaml\") . | sha256sum"))))
    (else
      (Mapping
        (Pair
          (Scalar "checksum/config")
          (HelmExpr "include (print $.Template.BasePath \"/operatorconfiguration.yaml\") . | sha256sum")))))
  (If ".Values.podAnnotations"
    (then
      (HelmExpr "toYaml .Values.podAnnotations | indent 8")))
  (Mapping
    (Pair
      (Scalar "labels")
      (Mapping
        (Pair
          (Scalar "app.kubernetes.io/name")
          (HelmExpr "template \"postgres-operator.name\" ."))
        (Pair
          (Scalar "app.kubernetes.io/instance")
          (HelmExpr ".Release.Name")))))
  (If ".Values.podLabels"
    (then
      (HelmExpr "toYaml .Values.podLabels | indent 8")))
  (Mapping
    (Pair
      (Scalar "spec")
      (Mapping
        (Pair
          (Scalar "serviceAccountName")
          (HelmExpr "include \"postgres-operator.serviceAccountName\" ."))
        (Pair
          (Scalar "containers")
          (Sequence
            (Mapping
              (Pair
                (Scalar "name")
                (HelmExpr ".Chart.Name"))
              (Pair
                (Scalar "image")
                (Scalar "{{.Values.image.registry}}/{{.Values.image.repository}}:{{.Values.image.tag}}"))
              (Pair
                (Scalar "imagePullPolicy")
                (HelmExpr ".Values.image.pullPolicy"))
              (Pair
                (Scalar "env"))))))))
  (If ".Values.enableJsonLogging"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "ENABLE_JSON_LOGGING"))
          (Pair
            (Scalar "value")
            (Scalar "true"))))))
  (If "eq .Values.configTarget \"ConfigMap\""
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "CONFIG_MAP_NAME"))
          (Pair
            (Scalar "value")
            (HelmExpr "template \"postgres-operator.fullname\" .")))))
    (else
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "POSTGRES_OPERATOR_CONFIGURATION_OBJECT"))
          (Pair
            (Scalar "value")
            (HelmExpr "template \"postgres-operator.fullname\" ."))))))
  (If ".Values.controllerID.create"
    (then
      (Sequence
        (Mapping
          (Pair
            (Scalar "name")
            (Scalar "CONTROLLER_ID"))
          (Pair
            (Scalar "value")
            (HelmExpr "template \"postgres-operator.controllerID\" ."))))))
  (If ".Values.extraEnvs"
    (then
      (HelmExpr "toYaml .Values.extraEnvs | indent 8")))
  (Mapping
    (Pair
      (Scalar "resources")))
  (HelmExpr "toYaml .Values.resources | indent 10")
  (Mapping
    (Pair
      (Scalar "securityContext")))
  (HelmExpr "toYaml .Values.securityContext | indent 10")
  (If ".Values.readinessProbe"
    (then
      (Mapping
        (Pair
          (Scalar "readinessProbe")
          (Mapping
            (Pair
              (Scalar "httpGet")
              (Mapping
                (Pair
                  (Scalar "path")
                  (Scalar "/readyz"))
                (Pair
                  (Scalar "port")
                  (HelmExpr ".Values.configLoggingRestApi.api_port"))))
            (Pair
              (Scalar "initialDelaySeconds")
              (HelmExpr ".Values.readinessProbe.initialDelaySeconds"))
            (Pair
              (Scalar "periodSeconds")
              (HelmExpr ".Values.readinessProbe.periodSeconds")))))))
  (If ".Values.imagePullSecrets"
    (then
      (Mapping
        (Pair
          (Scalar "imagePullSecrets")))
      (HelmExpr "toYaml .Values.imagePullSecrets | indent 8")))
  (Mapping
    (Pair
      (Scalar "affinity")))
  (HelmExpr "toYaml .Values.affinity | indent 8")
  (Mapping
    (Pair
      (Scalar "nodeSelector")))
  (HelmExpr "toYaml .Values.nodeSelector | indent 8")
  (Mapping
    (Pair
      (Scalar "tolerations")))
  (HelmExpr "toYaml .Values.tolerations | indent 8")
  (If ".Values.priorityClassName"
    (then
      (Mapping
        (Pair
          (Scalar "priorityClassName")
          (HelmExpr ".Values.priorityClassName"))))))"#;

#[test]
fn fused_rust_ast() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator/templates/deployment.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}

#[test]
fn tree_sitter_ast() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator/templates/deployment.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}
