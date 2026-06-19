use helm_schema_ast::TemplateHeader;

use crate::condition_action_plan::ConditionActionPlan;
use crate::fragment_range_scope::range_header_from_source;
use crate::template_expr_cache::parse_expr_text;
use crate::tree_sitter_utils::children_with_field;

use super::effects::{
    apply_assignment_action_plan, apply_condition_alternative_guards, apply_if_condition_plan,
    apply_range_action_plan, apply_with_condition_plan,
};
use super::{NodeEvalRuntime, eval_children, eval_node};

pub(super) fn eval_assignment_node<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    if let Ok(text) = node.utf8_text(runtime.source().as_bytes()) {
        let exprs = parse_expr_text(text);
        if !runtime.apply_assignment_side_effects(&exprs) {
            let plan = runtime.plan_assignment_action(&exprs);
            apply_assignment_action_plan(runtime, plan);
        }
    }

    runtime.enter_no_output();
    eval_children(runtime, node);
    runtime.exit_no_output();
}

pub(super) fn eval_if_node<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    eval_condition_node(runtime, node, |runtime, header| {
        let plan = runtime.plan_if_condition(header);
        apply_if_condition_plan(runtime, plan.clone());
        plan
    });
}

pub(super) fn eval_with_node<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    eval_condition_node(runtime, node, |runtime, header| {
        let plan = runtime.plan_with_condition(header);
        apply_with_condition_plan(runtime, plan.clone());
        plan
    });
}

fn eval_condition_node<R, F>(runtime: &mut R, node: tree_sitter::Node<'_>, mut enter_consequence: F)
where
    R: NodeEvalRuntime,
    F: FnMut(&mut R, &TemplateHeader) -> ConditionActionPlan,
{
    let entry = runtime.scope_snapshot();
    let else_if_pairs = else_if_pairs(node);
    let alternatives = children_with_field(node, "alternative");

    let condition_plan = if let Some(condition) = node.child_by_field_name("condition")
        && let Ok(text) = condition.utf8_text(runtime.source().as_bytes())
    {
        let header = TemplateHeader::parse_control(text.trim().to_string());
        Some(enter_consequence(runtime, &header))
    } else {
        None
    };

    runtime.enter_local_scope();
    for child in children_with_field(node, "consequence") {
        eval_node(runtime, child);
    }
    runtime.exit_local_scope();
    let consequence_outcome = runtime.scope_snapshot();

    runtime.restore_scope(entry.clone());
    if let Some(plan) = &condition_plan {
        apply_condition_alternative_guards(runtime, plan);
    }
    let alternative_outcome = eval_condition_alternative_chain(
        runtime,
        &else_if_pairs,
        &alternatives,
        &mut enter_consequence,
    );

    runtime.restore_scope(entry.clone());
    runtime.join_branch_scopes(&entry, vec![consequence_outcome, alternative_outcome]);
}

fn else_if_pairs<'node>(
    node: tree_sitter::Node<'node>,
) -> Vec<(tree_sitter::Node<'node>, Vec<tree_sitter::Node<'node>>)> {
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
                    pairs.push((child, Vec::new()));
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

fn eval_condition_alternative_chain<R, F>(
    runtime: &mut R,
    else_if_pairs: &[(tree_sitter::Node<'_>, Vec<tree_sitter::Node<'_>>)],
    alternatives: &[tree_sitter::Node<'_>],
    enter_consequence: &mut F,
) -> R::ScopeSnapshot
where
    R: NodeEvalRuntime,
    F: FnMut(&mut R, &TemplateHeader) -> ConditionActionPlan,
{
    let Some(((condition, option_children), tail)) = else_if_pairs.split_first() else {
        runtime.enter_local_scope();
        for child in alternatives {
            eval_node(runtime, *child);
        }
        runtime.exit_local_scope();
        return runtime.scope_snapshot();
    };

    let entry = runtime.scope_snapshot();
    let condition_header = condition
        .utf8_text(runtime.source().as_bytes())
        .ok()
        .map(|text| TemplateHeader::parse_control(text.trim().to_string()));
    let condition_plan = condition_header
        .as_ref()
        .map(|header| enter_consequence(runtime, header));

    runtime.enter_local_scope();
    for child in option_children {
        eval_node(runtime, *child);
    }
    runtime.exit_local_scope();
    let consequence_outcome = runtime.scope_snapshot();

    runtime.restore_scope(entry.clone());
    if let Some(plan) = &condition_plan {
        apply_condition_alternative_guards(runtime, plan);
    }
    let alternative_outcome =
        eval_condition_alternative_chain(runtime, tail, alternatives, enter_consequence);

    runtime.restore_scope(entry.clone());
    runtime.join_branch_scopes(&entry, vec![consequence_outcome, alternative_outcome]);
    runtime.scope_snapshot()
}

pub(super) fn eval_range_node<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    let entry = runtime.scope_snapshot();

    let current_path = runtime.current_document_path();
    let header = range_header_from_source(node, runtime.source());
    let plan = runtime.plan_range_action(node, header.as_ref(), &current_path);
    let range_output_path = plan
        .mapping_entry_indent
        .map(|indent| runtime.current_document_path_at_mapping_entry_indent(indent))
        .unwrap_or_else(|| current_path.clone());
    let iteration_count = runtime.range_iteration_count();

    runtime.enter_local_scope();
    apply_range_action_plan(runtime, &plan, &range_output_path);

    for index in 0..iteration_count {
        runtime.enter_range_iteration(index);
        for child in children_with_field(node, "body") {
            eval_node(runtime, child);
        }
        runtime.exit_range_iteration(index);
    }
    runtime.exit_local_scope();
    let body_outcome = runtime.scope_snapshot();

    runtime.restore_scope(entry.clone());

    let alternatives = children_with_field(node, "alternative");
    runtime.enter_local_scope();
    for child in &alternatives {
        eval_node(runtime, *child);
    }
    runtime.exit_local_scope();
    let alternative_outcome = runtime.scope_snapshot();

    runtime.restore_scope(entry.clone());
    if alternatives.is_empty() {
        runtime.join_range_scopes(&entry, vec![body_outcome, entry.clone()]);
    } else {
        runtime.join_range_scopes(&entry, vec![body_outcome, alternative_outcome]);
    }
}
