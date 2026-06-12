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

    fn scope_snapshot(&self) -> Self::ScopeSnapshot;

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot);

    fn enter_local_scope(&mut self);

    fn exit_local_scope(&mut self);

    fn join_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    );

    fn join_range_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        self.join_branch_scopes(entry, outcomes);
    }

    fn range_iteration_count(&self) -> usize {
        1
    }

    fn enter_range_iteration(&mut self, _index: usize) {}

    fn exit_range_iteration(&mut self, _index: usize) {}

    fn enter_no_output(&mut self);

    fn exit_no_output(&mut self);

    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>);

    fn apply_assignment_side_effects(&mut self, _text: &str) -> bool {
        false
    }

    fn plan_assignment_action(&self, text: &str) -> AssignmentActionPlan;

    fn plan_if_condition(&mut self, header: &str) -> ConditionActionPlan;

    fn plan_with_condition(&mut self, header: &str) -> ConditionActionPlan;

    fn plan_range_action(
        &mut self,
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
            eval_condition_node(runtime, node, |runtime, header| {
                let plan = runtime.plan_if_condition(header);
                apply_if_condition_plan(runtime, plan.clone());
                plan
            });
        }
        NodeActionKind::With => {
            eval_condition_node(runtime, node, |runtime, header| {
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

pub(crate) fn eval_template_body<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    if !eval_nested_template_bodies(runtime, node) {
        eval_node(runtime, node);
    }
}

fn eval_nested_template_bodies<R>(runtime: &mut R, node: tree_sitter::Node<'_>) -> bool
where
    R: NodeEvalRuntime,
{
    if matches!(node.kind(), "define_action" | "block_action") {
        for child in children_with_field(node, "body") {
            eval_node(runtime, child);
        }
        return true;
    }

    let mut cursor = node.walk();
    let mut found_body = false;
    for child in node.named_children(&mut cursor) {
        found_body |= eval_nested_template_bodies(runtime, child);
    }
    found_body
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
        let text = text.to_string();
        if !runtime.apply_assignment_side_effects(&text) {
            let plan = runtime.plan_assignment_action(&text);
            apply_assignment_action_plan(runtime, plan);
        }
    }

    runtime.enter_no_output();
    eval_children(runtime, node);
    runtime.exit_no_output();
}

fn eval_condition_node<R, F>(runtime: &mut R, node: tree_sitter::Node<'_>, mut enter_consequence: F)
where
    R: NodeEvalRuntime,
    F: FnMut(&mut R, &str) -> ConditionActionPlan,
{
    let entry = runtime.scope_snapshot();
    let else_if_pairs = else_if_pairs(node);
    let alternatives = children_with_field(node, "alternative");

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
    F: FnMut(&mut R, &str) -> ConditionActionPlan,
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
    let condition_text = condition
        .utf8_text(runtime.source().as_bytes())
        .ok()
        .map(str::to_string);
    let condition_plan = condition_text
        .as_deref()
        .map(|text| enter_consequence(runtime, text));

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

fn eval_range_node<R>(runtime: &mut R, node: tree_sitter::Node<'_>)
where
    R: NodeEvalRuntime,
{
    let entry = runtime.scope_snapshot();

    let current_path = runtime.current_rendered_path();
    let plan = runtime.plan_range_action(node, &current_path);
    let iteration_count = runtime.range_iteration_count();

    runtime.enter_local_scope();
    apply_range_action_plan(runtime, &plan, &current_path);

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
