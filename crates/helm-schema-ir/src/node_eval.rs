//! Go-template node classification shared by the fragment interpreter's
//! inline-region evaluation and the resource-identity helper walk: typed
//! actions with parsed headers/expressions, and the if-chain's
//! else-if (header, body) pairs.

use helm_schema_ast::{TemplateExpr, TemplateHeader, node_expressions, range_header_from_source};

#[derive(Clone, Debug)]
pub(crate) enum NodeAction {
    Text,
    Suppressed,
    Assignment(Vec<TemplateExpr>),
    If(Option<TemplateHeader>),
    With(Option<TemplateHeader>),
    Range(Option<TemplateHeader>),
    Output(Vec<TemplateExpr>),
    Descend,
}

pub(crate) fn node_action(source: &str, node: tree_sitter::Node<'_>) -> NodeAction {
    match node.kind() {
        "text" | "yaml_no_injection_text" => NodeAction::Text,
        "define_action" | "block_action" => NodeAction::Suppressed,
        "variable_definition" | "assignment" => {
            NodeAction::Assignment(node_expressions(node, source))
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
        | "nil" => NodeAction::Output(node_expressions(node, source)),
        _ => NodeAction::Descend,
    }
}

/// The typed header of a control node's condition; `None` when the
/// grammar recovered the action without one.
pub(crate) fn control_header(source: &str, node: tree_sitter::Node<'_>) -> Option<TemplateHeader> {
    node.child_by_field_name("condition")
        .map(|condition| TemplateHeader::from_node(condition, source))
}

pub(crate) fn control_headers(
    source: &str,
    node: tree_sitter::Node<'_>,
) -> Vec<Option<TemplateHeader>> {
    let mut headers = Vec::new();
    let mut walker = node.walk();
    if !walker.goto_first_child() {
        return headers;
    }
    loop {
        if walker.field_name() == Some("condition") {
            headers.push(Some(TemplateHeader::from_node(walker.node(), source)));
        }
        if !walker.goto_next_sibling() {
            break;
        }
    }
    headers
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
                    pairs.push((Some(TemplateHeader::from_node(child, source)), Vec::new()));
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
