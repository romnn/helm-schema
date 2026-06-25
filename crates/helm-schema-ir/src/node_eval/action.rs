use helm_schema_ast::{TemplateExpr, TemplateHeader};

use crate::fragment_range_scope::range_header_from_source;
use crate::tree_sitter_utils::parse_expr_text;

#[derive(Clone, Debug)]
pub(crate) enum NodeAction {
    Text,
    Suppressed,
    Assignment(Option<Vec<TemplateExpr>>),
    If(Option<TemplateHeader>),
    With(Option<TemplateHeader>),
    Range(Option<TemplateHeader>),
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
        "if_action" => NodeAction::If(control_header(source, node)),
        "with_action" => NodeAction::With(control_header(source, node)),
        "range_action" => NodeAction::Range(range_header_from_source(node, source)),
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

pub(super) fn control_header(source: &str, node: tree_sitter::Node<'_>) -> Option<TemplateHeader> {
    let condition = node.child_by_field_name("condition").unwrap_or(node);
    condition
        .utf8_text(source.as_bytes())
        .ok()
        .map(|text| TemplateHeader::parse_control(text.trim().to_string()))
}
