use helm_schema_ast::{FusedRustParser, HelmParser, TreeSitterParser};

const EXPECTED_SEXPR: &str = r#"(Document
  (HelmExpr "include \"nats.defaultValues\" .")
  (With ".Values.service"
    (body
      (If ".enabled"
        (then
          (HelmExpr "include \"nats.loadMergePatch\" (merge (dict \"file\" \"service.yaml\" \"ctx\" $) .)"))))))"#;

#[test]
fn fused_rust_ast() {
    let src = test_util::read_testdata("charts/nats/templates/service.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}

#[test]
fn tree_sitter_ast() {
    let src = test_util::read_testdata("charts/nats/templates/service.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    similar_asserts::assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}
