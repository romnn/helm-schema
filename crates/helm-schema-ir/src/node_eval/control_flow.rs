use helm_schema_ast::TemplateHeader;

use helm_schema_ast::children_with_field;

use super::action;
use super::{BranchOutcome, NodeEvalRuntime, eval_children, eval_node};

pub(super) fn eval_assignment_node<R>(
    runtime: &mut R,
    node: tree_sitter::Node<'_>,
    exprs: Option<&[helm_schema_ast::TemplateExpr]>,
) where
    R: NodeEvalRuntime,
{
    if let Some(exprs) = exprs {
        runtime.observe_assignment_exprs(exprs);
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
    eval_condition_node(runtime, node, header, R::enter_if_condition);
}

pub(super) fn eval_with_node<R>(
    runtime: &mut R,
    node: tree_sitter::Node<'_>,
    header: Option<&TemplateHeader>,
) where
    R: NodeEvalRuntime,
{
    eval_condition_node(runtime, node, header, R::enter_with_condition);
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

    // Every arm evaluates the same way: restore the entry scope, apply the
    // negations of all prior arm conditions, activate this arm's own
    // condition (the trailing `else` arm has none), then evaluate the body.
    let mut arms = vec![(header.cloned(), children_with_field(node, "consequence"))];
    arms.extend(else_if_pairs(node, runtime.source()));
    arms.push((None, children_with_field(node, "alternative")));

    let mut branch_outcomes = Vec::new();
    let mut prior_plans = Vec::new();
    for (condition_header, children) in arms {
        runtime.restore_scope(entry.clone());
        for plan in &prior_plans {
            runtime.activate_condition_alternative(plan);
        }
        let condition_plan = condition_header
            .as_ref()
            .map(|header| enter_consequence(runtime, header));

        runtime.enter_local_scope();
        for child in children {
            eval_node(runtime, child);
        }
        runtime.exit_local_scope();
        branch_outcomes.push(BranchOutcome {
            plan: condition_plan.clone(),
            outcome: runtime.scope_snapshot(),
        });
        if let Some(plan) = condition_plan {
            prior_plans.push(plan);
        }
    }

    runtime.restore_scope(entry.clone());
    runtime.join_condition_branch_scopes(&entry, branch_outcomes);
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

pub(super) fn eval_range_node<R>(
    runtime: &mut R,
    node: tree_sitter::Node<'_>,
    _header: Option<&TemplateHeader>,
) where
    R: NodeEvalRuntime,
{
    let entry = runtime.scope_snapshot();

    runtime.enter_local_scope();
    for child in children_with_field(node, "body") {
        eval_node(runtime, child);
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
