mod action;
mod control_flow;
mod effects;
mod runtime;

use helm_schema_ast::children_with_field;

pub(crate) use action::{NodeAction, control_header, node_action};
pub(crate) use control_flow::else_if_pairs;
pub(crate) use effects::{NodeActionEffectSink, push_predicate_contract_guards};
pub(crate) use runtime::{BranchOutcome, NodeEvalRuntime};

pub(crate) fn eval_node<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    runtime.enter_node(node);

    match node_action(runtime.source(), node) {
        NodeAction::Text => {}
        NodeAction::Suppressed => {}
        NodeAction::Assignment(exprs) => {
            control_flow::eval_assignment_node(runtime, node, exprs.as_deref());
        }
        NodeAction::If(header) => {
            control_flow::eval_if_node(runtime, node, header.as_ref());
        }
        NodeAction::With(header) => {
            control_flow::eval_with_node(runtime, node, header.as_ref());
        }
        NodeAction::Range(header) => {
            control_flow::eval_range_node(runtime, node, header.as_ref());
        }
        NodeAction::Output(exprs) => {
            if let Some(exprs) = exprs {
                runtime.handle_output_node(node, &exprs);
            }
        }
        NodeAction::Descend => {
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
