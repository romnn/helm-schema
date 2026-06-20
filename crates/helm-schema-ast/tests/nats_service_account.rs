use helm_schema_ast::{HelmParser, TreeSitterParser};
use test_util::prelude::sim_assert_eq;

const EXPECTED_SEXPR: &str = r#"(Document
  (HelmExpr "include \"nats.defaultValues\" .")
  (With ".Values.serviceAccount"
    (body
      (If ".enabled"
        (then
          (HelmExpr "include \"nats.loadMergePatch\" (merge (dict \"file\" \"service-account.yaml\" \"ctx\" $) .)"))))))"#;

#[test]
fn tree_sitter_ast() {
    let src = test_util::read_testdata("charts/nats/templates/service-account.yaml");
    let ast = TreeSitterParser.parse(&src).expect("parse");
    sim_assert_eq!(have: ast.to_sexpr(), want: EXPECTED_SEXPR.trim_end());
}
