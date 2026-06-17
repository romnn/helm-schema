use crate::{
    DefineIndex, HelmAst, HelmParser, TemplateExpr, TreeSitterParser, contains_template_action,
};

// ===========================================================================
// Simple template
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
fn tree_sitter_ast_simple() {
    let src = "{{- if .Values.enabled }}\nfoo: bar\n{{- end }}\n";
    let ast = TreeSitterParser.parse(src).expect("parse");
    similar_asserts::assert_eq!(ast.to_sexpr(), SIMPLE_EXPECTED_SEXPR);
}

#[test]
fn tree_sitter_ast_control_flow_headers_are_typed() {
    let src =
        "{{- if .Values.enabled }}{{- range $i, $v := include \"items\" . }}x{{- end }}{{- end }}";
    let ast = TreeSitterParser.parse(src).expect("parse");
    let HelmAst::Document { items } = ast else {
        panic!("expected document root");
    };
    let [
        HelmAst::If {
            condition,
            then_branch,
            ..
        },
    ] = items.as_slice()
    else {
        panic!("expected one top-level if node");
    };
    assert_eq!(condition.raw(), ".Values.enabled");
    assert_eq!(
        condition.expr(),
        &TemplateExpr::Field(vec!["Values".to_string(), "enabled".to_string()])
    );

    let [HelmAst::Range { header, .. }] = then_branch.as_slice() else {
        panic!("expected nested range node");
    };
    assert_eq!(header.raw(), "$i, $v := include \"items\" .");
    let mut saw_include = false;
    header.expr().walk(|expr| {
        if let TemplateExpr::Call { function, args } = expr
            && function == "include"
            && matches!(args.first(), Some(TemplateExpr::Literal(lit)) if lit.as_string() == Some("items"))
        {
            saw_include = true;
        }
    });
    assert!(saw_include, "expected typed range header include call");
}

#[test]
fn template_action_detection_finds_inline_output_action() {
    let src = "metadata:\n  name: {{ .Values.name }}\n";

    assert!(contains_template_action(src).expect("parse template source"));
}

#[test]
fn template_action_detection_accepts_literal_yaml_comments() {
    let src = "# comment\nmetadata:\n  name: demo\n";

    assert!(!contains_template_action(src).expect("parse template source"));
}

// ===========================================================================
// DefineIndex
// ===========================================================================

#[test]
fn define_index_from_helpers() {
    let helpers = test_util::read_testdata("charts/bitnami-redis/templates/_helpers.tpl");

    let mut idx_ts = DefineIndex::new();
    idx_ts
        .add_source(&TreeSitterParser, &helpers)
        .expect("ts define index");

    let expected_defines = ["redis.image", "redis.sentinel.image", "redis.metrics.image"];
    for name in expected_defines {
        assert!(
            idx_ts.get(name).is_some(),
            "ts define index should find '{name}'"
        );
    }
}
