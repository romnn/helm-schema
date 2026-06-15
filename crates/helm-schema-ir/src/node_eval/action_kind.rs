#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NodeActionKind {
    Text,
    Suppressed,
    Assignment,
    If,
    With,
    Range,
    Output,
    Descend,
}

pub(super) fn classify_node_action(node: tree_sitter::Node<'_>) -> NodeActionKind {
    match node.kind() {
        "text" | "yaml_no_injection_text" => NodeActionKind::Text,
        "define_action" | "block_action" => NodeActionKind::Suppressed,
        "variable_definition" | "assignment" => NodeActionKind::Assignment,
        "if_action" => NodeActionKind::If,
        "with_action" => NodeActionKind::With,
        "range_action" => NodeActionKind::Range,
        "template_action"
        | "dot"
        | "variable"
        | "field"
        | "chained_pipeline"
        | "parenthesized_pipeline"
        | "selector_expression"
        | "function_call"
        | "method_call" => NodeActionKind::Output,
        _ => NodeActionKind::Descend,
    }
}
