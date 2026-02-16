use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const EXPECTED_SEXPR: &str = r#"(Document
  (If ".Values.autoscaling.enabled"
    (then
      (Mapping
        (Pair
          (Scalar "apiVersion")
          (Scalar "autoscaling/v2beta1"))
        (Pair
          (Scalar "kind")
          (Scalar "HorizontalPodAutoscaler"))
        (Pair
          (Scalar "metadata")
          (Mapping
            (Pair
              (Scalar "name")
              (HelmExpr "include \"surveyor.fullname\" ."))
            (Pair
              (Scalar "labels")
              (HelmExpr "include \"surveyor.labels\" . | nindent 4"))))
        (Pair
          (Scalar "spec")
          (Mapping
            (Pair
              (Scalar "scaleTargetRef")
              (Mapping
                (Pair
                  (Scalar "apiVersion")
                  (Scalar "apps/v1"))
                (Pair
                  (Scalar "kind")
                  (Scalar "Deployment"))
                (Pair
                  (Scalar "name")
                  (HelmExpr "include \"surveyor.fullname\" ."))))
            (Pair
              (Scalar "minReplicas")
              (HelmExpr ".Values.autoscaling.minReplicas"))
            (Pair
              (Scalar "maxReplicas")
              (HelmExpr ".Values.autoscaling.maxReplicas"))
            (Pair
              (Scalar "metrics")))))
      (If ".Values.autoscaling.targetCPUUtilizationPercentage"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "type")
                (Scalar "Resource"))
              (Pair
                (Scalar "resource")
                (Mapping
                  (Pair
                    (Scalar "name")
                    (Scalar "cpu"))
                  (Pair
                    (Scalar "targetAverageUtilization")
                    (HelmExpr ".Values.autoscaling.targetCPUUtilizationPercentage"))))))))
      (If ".Values.autoscaling.targetMemoryUtilizationPercentage"
        (then
          (Sequence
            (Mapping
              (Pair
                (Scalar "type")
                (Scalar "Resource"))
              (Pair
                (Scalar "resource")
                (Mapping
                  (Pair
                    (Scalar "name")
                    (Scalar "memory"))
                  (Pair
                    (Scalar "targetAverageUtilization")
                    (HelmExpr ".Values.autoscaling.targetMemoryUtilizationPercentage")))))))))))"#;

#[test]
fn fused_rust_ast() {
    let src = test_util::read_testdata("charts/surveyor/templates/hpa.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR);
}

#[test]
fn tree_sitter_ast() {
    let src = test_util::read_testdata("charts/surveyor/templates/hpa.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}
