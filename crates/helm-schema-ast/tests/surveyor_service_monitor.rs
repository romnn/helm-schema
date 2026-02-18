use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const EXPECTED_SEXPR_FUSED: &str = r#"(Document
  (If ".Values.serviceMonitor.enabled"
    (then
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "monitoring.coreos.com/v1"))
        (Pair
          (Scalar "kind")
          (Scalar "ServiceMonitor"))
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (HelmExpr "template \"surveyor.fullname\" ."))
            (Pair
              (Scalar "labels")
              (HelmExpr "include \"surveyor.labels\" . | nindent 4")))))
      (With ".Values.serviceMonitor.labels"
        (body
          (HelmExpr "toYaml . | nindent 4")))
      (With ".Values.serviceMonitor.annotations"
        (body
          (Mapping
            (Pair
              (Scalar "annotations")
              (HelmExpr "toYaml . | nindent 4")))))
      (Mapping
        (Pair
          (Scalar "spec")
          (Mapping
            (Pair
              (Scalar "endpoints")
              (Sequence
                (Mapping
                  (Pair
                    (Scalar "port")
                    (Scalar "http"))
                  (Pair
                    (Scalar "path")
                    (Scalar "/metrics"))))))))
      (If ".Values.serviceMonitor.interval"
        (then
          (Mapping
            (Pair
              (Scalar "interval")
              (HelmExpr ".Values.serviceMonitor.interval")))))
      (If ".Values.serviceMonitor.scrapeTimeout"
        (then
          (Mapping
            (Pair
              (Scalar "scrapeTimeout")
              (HelmExpr ".Values.serviceMonitor.scrapeTimeout")))))
      (With ".Values.serviceMonitor.relabelings"
        (body
          (Mapping
            (Pair
              (Scalar "relabelings")))
          (HelmExpr "toYaml . | nindent 4")))
      (With ".Values.serviceMonitor.metricRelabelings"
        (body
          (Mapping
            (Pair
              (Scalar "metricRelabelings")))
          (HelmExpr "toYaml . | nindent 4")))
      (Mapping
        (Pair
          (Scalar "selector")
          (Mapping
            (Pair
              (Scalar "matchLabels")
              (HelmExpr "include \"surveyor.selectorLabels\" . | nindent 6"))))))))"#;

const EXPECTED_SEXPR_TREE: &str = r#"(Document
  (If ".Values.serviceMonitor.enabled"
    (then
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "monitoring.coreos.com/v1"))
        (Pair
          (Scalar "kind")
          (Scalar "ServiceMonitor"))
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (HelmExpr "template \"surveyor.fullname\" ."))
            (Pair
              (Scalar "labels")
              (HelmExpr "include \"surveyor.labels\" . | nindent 4")))))
      (With ".Values.serviceMonitor.labels"
        (body
          (HelmExpr "toYaml . | nindent 4")))
      (With ".Values.serviceMonitor.annotations"
        (body
          (Mapping
            (Pair
              (Scalar "annotations")
              (HelmExpr "toYaml . | nindent 4")))))
      (Mapping
        (Pair
          (Scalar "spec")
          (Mapping
            (Pair
              (Scalar "endpoints")
              (Sequence
                (Mapping
                  (Pair
                    (Scalar "port")
                    (Scalar "http"))
                  (Pair
                    (Scalar "path")
                    (Scalar "/metrics"))))))))
      (If ".Values.serviceMonitor.interval"
        (then
          (Mapping
            (Pair
              (Scalar "interval")
              (HelmExpr ".Values.serviceMonitor.interval")))))
      (If ".Values.serviceMonitor.scrapeTimeout"
        (then
          (Mapping
            (Pair
              (Scalar "scrapeTimeout")
              (HelmExpr ".Values.serviceMonitor.scrapeTimeout")))))
      (With ".Values.serviceMonitor.relabelings"
        (body
          (Mapping
            (Pair
              (Scalar "relabelings")))
          (HelmExpr "toYaml . | nindent 4")))
      (With ".Values.serviceMonitor.metricRelabelings"
        (body
          (Mapping
            (Pair
              (Scalar "metricRelabelings")))
          (HelmExpr "toYaml . | nindent 4")))
      (Mapping
        (Pair
          (Scalar "selector")
          (Mapping
            (Pair
              (Scalar "matchLabels")
              (HelmExpr "include \"surveyor.selectorLabels\" . | nindent 6"))))))))"#;

#[test]
fn fused_rust_ast() {
    let src = test_util::read_testdata("charts/surveyor/templates/serviceMonitor.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR_FUSED.trim_end());
}

#[test]
fn tree_sitter_ast() {
    let src = test_util::read_testdata("charts/surveyor/templates/serviceMonitor.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");

    if std::env::var("AST_DUMP").is_ok() {
        eprintln!("{}", ast.to_sexpr());
    }

    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR_TREE.trim_end());
}
