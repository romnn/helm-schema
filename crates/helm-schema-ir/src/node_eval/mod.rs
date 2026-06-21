mod action;
mod control_flow;
mod effects;
mod runtime;

use crate::tree_sitter_utils::children_with_field;

use action::{NodeAction, node_action};
pub(crate) use effects::{
    NodeActionEffectSink, activate_condition_alternative_guards, activate_if_condition_plan,
    activate_range_action_plan, activate_with_condition_plan,
};
pub(crate) use runtime::{AssignmentObservation, NodeEvalRuntime};

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
        NodeAction::If => {
            control_flow::eval_if_node(runtime, node);
        }
        NodeAction::With => {
            control_flow::eval_with_node(runtime, node);
        }
        NodeAction::Range => {
            control_flow::eval_range_node(runtime, node);
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
