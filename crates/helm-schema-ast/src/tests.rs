use crate::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};

// ===========================================================================
// simple template — both parsers produce identical AST
// ===========================================================================

const SIMPLE_EXPECTED_SEXPR: &str = "\
(Document
  (If \".Values.enabled\"
    (then
      (Mapping
        (Pair
          (Scalar \"foo\")
          (Scalar \"bar\"))))))";

#[test]
fn fused_rust_ast_simple() {
    let src = "{{- if .Values.enabled }}\nfoo: bar\n{{- end }}\n";
    let ast = FusedRustParser.parse(src).expect("parse");
    similar_asserts::assert_eq!(ast.to_sexpr(), SIMPLE_EXPECTED_SEXPR);
}

#[test]
fn tree_sitter_ast_simple() {
    let src = "{{- if .Values.enabled }}\nfoo: bar\n{{- end }}\n";
    let ast = TreeSitterParser.parse(src).expect("parse");
    similar_asserts::assert_eq!(ast.to_sexpr(), SIMPLE_EXPECTED_SEXPR);
}

// ===========================================================================
// DefineIndex — both parsers collect the same named template definitions
// ===========================================================================

#[test]
fn define_index_from_helpers() {
    let helpers = test_util::read_testdata("charts/bitnami-redis/templates/_helpers.tpl");

    let mut idx_rust = DefineIndex::new();
    idx_rust
        .add_source(&FusedRustParser, &helpers)
        .expect("rust define index");

    let mut idx_ts = DefineIndex::new();
    idx_ts
        .add_source(&TreeSitterParser, &helpers)
        .expect("ts define index");

    let expected_defines = ["redis.image", "redis.sentinel.image", "redis.metrics.image"];
    for name in expected_defines {
        assert!(
            idx_rust.get(name).is_some(),
            "rust define index should find '{name}'"
        );
        assert!(
            idx_ts.get(name).is_some(),
            "ts define index should find '{name}'"
        );
    }
}
