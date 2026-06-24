use helm_schema_ast::TemplateHeader;

use crate::tree_sitter_utils::children_with_field;

use super::action;
use super::effects::apply_assignment_action_plan;
use super::{AssignmentObservation, NodeEvalRuntime, eval_children, eval_node};

pub(super) fn eval_assignment_node<R>(
    runtime: &mut R,
    node: tree_sitter::Node<'_>,
    exprs: Option<&[helm_schema_ast::TemplateExpr]>,
) where
    R: NodeEvalRuntime,
{
    if let Some(exprs) = exprs {
        match runtime.observe_assignment_exprs(exprs) {
            AssignmentObservation::Unhandled => {
                let plan = runtime.plan_assignment_action(exprs);
                apply_assignment_action_plan(runtime, plan);
            }
            AssignmentObservation::ExpressionObserved
            | AssignmentObservation::LocalMutationApplied => {}
        }
    }

    runtime.enter_no_output();
    eval_children(runtime, node);
    runtime.exit_no_output();
}

pub(super) fn eval_if_node<R>(
    runtime: &mut R,
    node: tree_sitter::Node<'_>,
    header: Option<&TemplateHeader>,
) where
    R: NodeEvalRuntime,
{
    eval_condition_node(runtime, node, header, |runtime, header| {
        let plan = runtime.plan_if_condition(header);
        runtime.activate_if_condition(&plan);
        plan
    });
}

pub(super) fn eval_with_node<R>(
    runtime: &mut R,
    node: tree_sitter::Node<'_>,
    header: Option<&TemplateHeader>,
) where
    R: NodeEvalRuntime,
{
    eval_condition_node(runtime, node, header, |runtime, header| {
        let plan = runtime.plan_with_condition(header);
        runtime.activate_with_condition(&plan);
        plan
    });
}

fn eval_condition_node<R, F>(
    runtime: &mut R,
    node: tree_sitter::Node<'_>,
    header: Option<&TemplateHeader>,
    mut enter_consequence: F,
) where
    R: NodeEvalRuntime,
    F: FnMut(&mut R, &TemplateHeader) -> R::ConditionPlan,
{
    let entry = runtime.scope_snapshot();
    let else_if_pairs = else_if_pairs(node, runtime.source());
    let alternatives = children_with_field(node, "alternative");

    let condition_plan = header.map(|header| enter_consequence(runtime, header));

    runtime.enter_local_scope();
    for child in children_with_field(node, "consequence") {
        eval_node(runtime, child);
    }
    runtime.exit_local_scope();
    let consequence_outcome = runtime.scope_snapshot();

    runtime.restore_scope(entry.clone());
    if let Some(plan) = &condition_plan {
        runtime.activate_condition_alternative(plan);
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
                    pairs.push((action::control_header(source, child), Vec::new()));
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
    else_if_pairs: &[(Option<TemplateHeader>, Vec<tree_sitter::Node<'_>>)],
    alternatives: &[tree_sitter::Node<'_>],
    enter_consequence: &mut F,
) -> R::ScopeSnapshot
where
    R: NodeEvalRuntime,
    F: FnMut(&mut R, &TemplateHeader) -> R::ConditionPlan,
{
    let Some(((condition_header, option_children), tail)) = else_if_pairs.split_first() else {
        runtime.enter_local_scope();
        for child in alternatives {
            eval_node(runtime, *child);
        }
        runtime.exit_local_scope();
        return runtime.scope_snapshot();
    };

    let entry = runtime.scope_snapshot();
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
        runtime.activate_condition_alternative(plan);
    }
    let alternative_outcome =
        eval_condition_alternative_chain(runtime, tail, alternatives, enter_consequence);

    runtime.restore_scope(entry.clone());
    runtime.join_branch_scopes(&entry, vec![consequence_outcome, alternative_outcome]);
    runtime.scope_snapshot()
}

pub(super) fn eval_range_node<R>(
    runtime: &mut R,
    node: tree_sitter::Node<'_>,
    header: Option<&TemplateHeader>,
) where
    R: NodeEvalRuntime,
{
    let entry = runtime.scope_snapshot();

    let control_site = runtime.document_control_site_for_node(node);
    let plan = runtime.plan_range_action(
        node,
        header,
        &control_site.path,
        control_site.range_mapping_entry_path.as_ref(),
    );
    let range_output_path = control_site
        .range_mapping_entry_path
        .unwrap_or_else(|| control_site.path.clone());

    runtime.enter_local_scope();
    runtime.activate_range_action(node, &plan, &range_output_path);
    let iteration_count = runtime.range_iteration_count();

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
