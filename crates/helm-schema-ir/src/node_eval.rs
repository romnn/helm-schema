//! Go-template node classification shared by the fragment interpreter's
//! inline-region evaluation and the resource-identity helper walk: typed
//! actions with parsed headers/expressions, and the if-chain's
//! else-if (header, body) pairs.

use helm_schema_ast::{TemplateExpr, TemplateHeader, range_header_from_source};

use helm_schema_ast::parse_expr_text;

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
        | "method_call"
        // A bare literal action (`{{- true -}}`) renders static text; it is
        // output, not structure to descend into (redis' `createConfigmap`
        // gate spells its body this way).
        | "true"
        | "false"
        | "int_literal"
        | "float_literal"
        | "interpreted_string_literal"
        | "raw_string_literal"
        | "nil" => NodeAction::Output(parse_node_exprs(source, node)),
        _ => NodeAction::Descend,
    }
}

fn parse_node_exprs(source: &str, node: tree_sitter::Node<'_>) -> Option<Vec<TemplateExpr>> {
    node.utf8_text(source.as_bytes()).ok().map(parse_expr_text)
}

pub(crate) fn control_header(source: &str, node: tree_sitter::Node<'_>) -> Option<TemplateHeader> {
    let condition = node.child_by_field_name("condition").unwrap_or(node);
    condition
        .utf8_text(source.as_bytes())
        .ok()
        .map(|text| TemplateHeader::parse_control(text.trim().to_string()))
}

pub(crate) fn else_if_pairs<'node>(
    node: tree_sitter::Node<'node>,
    source: &str,
) -> Vec<(Option<TemplateHeader>, Vec<tree_sitter::Node<'node>>)> {
    let mut pairs = Vec::new();
    let mut seen_main_condition = false;
    let mut walker = node.walk();
    if !walker.goto_first_child() {
        return pairs;
    }

    loop {
        let child = walker.node();
        match walker.field_name() {
            Some("condition") => {
                if seen_main_condition {
                    pairs.push((control_header(source, child), Vec::new()));
                } else {
                    seen_main_condition = true;
                }
            }
            Some("option") => {
                if let Some((_condition, option_children)) = pairs.last_mut() {
                    option_children.push(child);
                }
            }
            _ => {}
        }
        if !walker.goto_next_sibling() {
            break;
        }
    }

    pairs
}
