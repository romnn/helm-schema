use helm_schema_ast::TemplateExpr;

use crate::template_expr_cache::parse_expr_text;

#[derive(Clone, Debug)]
pub(crate) enum NodeAction {
    Text,
    Suppressed,
    Assignment(Option<Vec<TemplateExpr>>),
    If,
    With,
    Range,
    Output(Option<Vec<TemplateExpr>>),
    Descend,
}

pub(crate) fn node_action(source: &str, node: tree_sitter::Node<'_>) -> NodeAction {
    match node.kind() {
        "text" | "yaml_no_injection_text" => NodeAction::Text,
        "define_action" | "block_action" => NodeAction::Suppressed,
        "variable_definition" | "assignment" => {
            NodeAction::Assignment(parse_node_exprs(source, node))
        }
        "if_action" => NodeAction::If,
        "with_action" => NodeAction::With,
        "range_action" => NodeAction::Range,
        "template_action"
        | "dot"
        | "variable"
        | "field"
        | "chained_pipeline"
        | "parenthesized_pipeline"
        | "selector_expression"
        | "function_call"
        | "method_call" => NodeAction::Output(parse_node_exprs(source, node)),
        _ => NodeAction::Descend,
    }
}

fn parse_node_exprs(source: &str, node: tree_sitter::Node<'_>) -> Option<Vec<TemplateExpr>> {
    node.utf8_text(source.as_bytes()).ok().map(parse_expr_text)
}
