use helm_schema_ast::{
    mapping_key_text_refs_range_key_variable, range_body_mapping_entry_indent_from_source,
};
use test_util::prelude::sim_assert_eq;

#[test]
fn templated_mapping_key_text_refs_range_key_variable() {
    assert!(mapping_key_text_refs_range_key_variable(
        "{{- $key | nindent 2 }}: {{ tpl (toString $value) $ | quote }}",
        "key",
    ));
}

#[test]
fn destructured_range_mapping_entry_indent_uses_body_key_indent() {
    let source = r#"
data:
{{- range $key, $value := .Values.controller.config }}
  {{- $key | nindent 2 }}: {{ tpl (toString $value) $ | quote }}
{{- end }}
        "#;
    let tree = parse_go_template(source);
    let range = find_kind(tree.root_node(), "range_action").expect("range action");

    sim_assert_eq!(
        have: range_body_mapping_entry_indent_from_source(range, source),
        want: Some(2)
    );
}

fn parse_go_template(source: &str) -> tree_sitter::Tree {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("set go template language");
    parser.parse(source, None).expect("parse go template")
}

fn find_kind<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_kind(child, kind) {
            return Some(found);
        }
    }
    None
}
