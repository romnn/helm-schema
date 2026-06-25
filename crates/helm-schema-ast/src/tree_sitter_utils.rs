use crate::{TemplateExpr, parse_action_expressions};

pub fn children_with_field<'node>(
    node: tree_sitter::Node<'node>,
    field: &str,
) -> Vec<tree_sitter::Node<'node>> {
    let mut cursor = node.walk();
    node.children_by_field_name(field, &mut cursor)
        .filter(tree_sitter::Node::is_named)
        .collect()
}

pub fn parse_expr_text(text: &str) -> Vec<TemplateExpr> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        Vec::new()
    } else if trimmed.starts_with("{{") {
        parse_action_expressions(trimmed)
    } else {
        parse_action_expressions(&format!("{{{{ {trimmed} }}}}"))
    }
}

#[tracing::instrument(skip_all, fields(bytes = source.len()))]
pub fn parse_go_template(source: &str) -> Option<tree_sitter::Tree> {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return None;
    }
    parser.parse(source, None)
}

#[tracing::instrument(skip_all, fields(bytes = source.len()))]
pub fn parse_helm_template(source: &str) -> Option<tree_sitter::Tree> {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::helm_template::language());
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return None;
    }
    parser.parse(source, None)
}
