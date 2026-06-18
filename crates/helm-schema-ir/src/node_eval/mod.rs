mod action_kind;
mod control_flow;
mod effects;
mod runtime;

use crate::template_expr_cache::parse_expr_text;
use crate::tree_sitter_utils::children_with_field;

use action_kind::{NodeActionKind, classify_node_action};

pub(crate) use effects::NodeActionEffectSink;
pub(crate) use runtime::NodeEvalRuntime;

pub(crate) fn eval_node<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    runtime.enter_node(node);

    match classify_node_action(node) {
        NodeActionKind::Text => {
            runtime.ingest_text_up_to(node.end_byte());
        }
        NodeActionKind::Suppressed => {}
        NodeActionKind::Assignment => {
            control_flow::eval_assignment_node(runtime, node);
        }
        NodeActionKind::If => {
            control_flow::eval_if_node(runtime, node);
        }
        NodeActionKind::With => {
            control_flow::eval_with_node(runtime, node);
        }
        NodeActionKind::Range => {
            control_flow::eval_range_node(runtime, node);
        }
        NodeActionKind::Output => {
            if let Ok(text) = node.utf8_text(runtime.source().as_bytes()) {
                let exprs = parse_expr_text(text);
                runtime.handle_output_node(node, &exprs);
            }
        }
        NodeActionKind::Descend => {
            eval_children(runtime, node);
        }
    }
}

pub(crate) fn eval_template_body<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    if matches!(node.kind(), "define_action" | "block_action") {
        for child in children_with_field(node, "body") {
            eval_node(runtime, child);
        }
    } else {
        eval_node(runtime, node);
    }
}

pub(super) fn eval_children<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        eval_node(runtime, child);
    }
}
