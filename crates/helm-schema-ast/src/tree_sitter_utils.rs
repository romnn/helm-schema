use crate::{TemplateExpr, parse_action_expressions};

/// Returns named children occupying a tree-sitter field.
pub fn children_with_field<'node>(
    node: tree_sitter::Node<'node>,
    field: &str,
) -> Vec<tree_sitter::Node<'node>> {
    let mut cursor = node.walk();
    node.children_by_field_name(field, &mut cursor)
        .filter(tree_sitter::Node::is_named)
        .collect()
}

/// Parses expression text with or without surrounding action delimiters.
#[must_use]
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

/// The Go-template tree parse is owned by `helm-schema-syntax` (this crate
/// layers the typed expression AST on top of the same trees).
pub use helm_schema_syntax::parse_go_template;

#[tracing::instrument(skip_all, fields(bytes = source.len()))]
/// Parses a source file with the fused Helm-template grammar.
pub fn parse_helm_template(source: &str) -> Option<tree_sitter::Tree> {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::helm_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    parser.parse(source, None)
}
