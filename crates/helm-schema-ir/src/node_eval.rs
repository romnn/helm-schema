use crate::YamlPath;
use crate::assignment_action_plan::AssignmentActionPlan;
use crate::condition_action_plan::ConditionActionPlan;
use crate::node_action_effect::{
    NodeActionEffectSink, apply_assignment_action_plan, apply_condition_alternative_guards,
    apply_if_condition_plan, apply_range_action_plan, apply_with_condition_plan,
};
use crate::node_action_kind::{NodeActionKind, classify_node_action};
use crate::range_action_plan::RangeActionPlan;
use crate::tree_sitter_utils::children_with_field;

pub(crate) trait NodeEvalRuntime: NodeActionEffectSink {
    type ScopeSnapshot: Clone;

    fn source(&self) -> &str;

    fn enter_node(&mut self, node: tree_sitter::Node<'_>);

    fn ingest_text_up_to(&mut self, end_byte: usize);

    fn current_rendered_path(&self) -> YamlPath;

    fn scope_snapshot(&self, include_dot_stack: bool) -> Self::ScopeSnapshot;

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot);

    fn enter_local_scope(&mut self);

    fn exit_local_scope(&mut self);

    fn join_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    );

    fn enter_no_output(&mut self);

    fn exit_no_output(&mut self);

    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>);

    fn plan_assignment_action(&self, text: &str) -> AssignmentActionPlan;

    fn plan_if_condition(&self, header: &str) -> ConditionActionPlan;

    fn plan_with_condition(&self, header: &str) -> ConditionActionPlan;

    fn plan_range_action(
        &self,
        node: tree_sitter::Node<'_>,
        current_path: &YamlPath,
    ) -> RangeActionPlan;
}

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
            eval_assignment_node(runtime, node);
        }
        NodeActionKind::If => {
            eval_condition_node(runtime, node, false, |runtime, header| {
                let plan = runtime.plan_if_condition(header);
                apply_if_condition_plan(runtime, plan.clone());
                plan
            });
        }
        NodeActionKind::With => {
            eval_condition_node(runtime, node, true, |runtime, header| {
                let plan = runtime.plan_with_condition(header);
                apply_with_condition_plan(runtime, plan.clone());
                plan
            });
        }
        NodeActionKind::Range => {
            eval_range_node(runtime, node);
        }
        NodeActionKind::Output => {
            runtime.handle_output_node(node);
        }
        NodeActionKind::Descend => {
            eval_children(runtime, node);
        }
    }
}

fn eval_children<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        eval_node(runtime, child);
    }
}

fn eval_assignment_node<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    if let Ok(text) = node.utf8_text(runtime.source().as_bytes()) {
        let plan = runtime.plan_assignment_action(text);
        apply_assignment_action_plan(runtime, plan);
    }

    runtime.enter_no_output();
    eval_children(runtime, node);
    runtime.exit_no_output();
}

fn eval_condition_node<R, F>(
    runtime: &mut R,
    node: tree_sitter::Node<'_>,
    include_dot_stack: bool,
    mut enter_consequence: F,
) where
    R: NodeEvalRuntime,
    F: FnMut(&mut R, &str) -> ConditionActionPlan,
{
    let entry = runtime.scope_snapshot(include_dot_stack);

    let condition_plan = if let Some(condition) = node.child_by_field_name("condition")
        && let Ok(text) = condition.utf8_text(runtime.source().as_bytes())
    {
        let text = text.to_string();
        Some(enter_consequence(runtime, &text))
    } else {
        None
    };

    runtime.enter_local_scope();
    for child in children_with_field(node, "consequence") {
        eval_node(runtime, child);
    }
    runtime.exit_local_scope();
    let consequence_outcome = runtime.scope_snapshot(include_dot_stack);

    runtime.restore_scope(entry.clone());
    if let Some(plan) = &condition_plan {
        apply_condition_alternative_guards(runtime, plan);
    }

    // Else-if chains are represented as repeated condition/option fields; the
    // outer alternative first inherits the negated current predicate, then any
    // nested else-if contributes its own condition when it is evaluated.
    runtime.enter_local_scope();
    for child in children_with_field(node, "alternative") {
        eval_node(runtime, child);
    }
    runtime.exit_local_scope();
    let alternative_outcome = runtime.scope_snapshot(include_dot_stack);

    runtime.restore_scope(entry.clone());
    runtime.join_branch_scopes(&entry, vec![consequence_outcome, alternative_outcome]);
}

fn eval_range_node<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    let entry = runtime.scope_snapshot(true);

    let current_path = runtime.current_rendered_path();
    let plan = runtime.plan_range_action(node, &current_path);

    runtime.enter_local_scope();
    apply_range_action_plan(runtime, &plan, &current_path);

    for child in children_with_field(node, "body") {
        eval_node(runtime, child);
    }
    runtime.exit_local_scope();
    let body_outcome = runtime.scope_snapshot(true);

    runtime.restore_scope(entry.clone());

    let alternatives = children_with_field(node, "alternative");
    runtime.enter_local_scope();
    for child in &alternatives {
        eval_node(runtime, *child);
    }
    runtime.exit_local_scope();
    let alternative_outcome = runtime.scope_snapshot(true);

    runtime.restore_scope(entry.clone());
    if alternatives.is_empty() {
        runtime.join_branch_scopes(&entry, vec![body_outcome, entry.clone()]);
    } else {
        runtime.join_branch_scopes(&entry, vec![body_outcome, alternative_outcome]);
    }
}
