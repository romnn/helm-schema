use helm_schema_ast::{TemplateExpr, TemplateHeader};

use super::effects::NodeActionEffectSink;
use crate::YamlPath;
use crate::assignment_action_plan::AssignmentActionPlan;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssignmentObservation {
    Unhandled,
    ExpressionObserved,
    LocalMutationApplied,
}

pub(crate) trait NodeEvalRuntime: NodeActionEffectSink {
    type ScopeSnapshot: Clone;
    type ConditionPlan;
    type RangePlan;

    fn source(&self) -> &str;

    fn enter_node(&mut self, _node: tree_sitter::Node<'_>) {}

    fn document_path_for_node(&self, node: tree_sitter::Node<'_>) -> YamlPath;

    fn document_mapping_entry_path_for_range_node(
        &self,
        _node: tree_sitter::Node<'_>,
    ) -> Option<YamlPath> {
        None
    }

    fn scope_snapshot(&self) -> Self::ScopeSnapshot;

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot);

    fn enter_local_scope(&mut self) {}

    fn exit_local_scope(&mut self) {}

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

    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>, exprs: &[TemplateExpr]);

    fn observe_assignment_exprs(&mut self, _exprs: &[TemplateExpr]) -> AssignmentObservation {
        AssignmentObservation::Unhandled
    }

    fn plan_assignment_action(&self, _exprs: &[TemplateExpr]) -> AssignmentActionPlan {
        AssignmentActionPlan {
            get_binding: None,
            local_assignment: None,
        }
    }

    fn plan_if_condition(&mut self, header: &TemplateHeader) -> Self::ConditionPlan;

    fn activate_if_condition(&mut self, plan: &Self::ConditionPlan);

    fn plan_with_condition(&mut self, header: &TemplateHeader) -> Self::ConditionPlan;

    fn activate_with_condition(&mut self, plan: &Self::ConditionPlan);

    fn activate_condition_alternative(&mut self, plan: &Self::ConditionPlan);

    fn plan_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        header: Option<&TemplateHeader>,
        current_path: &YamlPath,
        mapping_entry_path: Option<&YamlPath>,
    ) -> Self::RangePlan;

    fn activate_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        plan: &Self::RangePlan,
        current_path: &YamlPath,
    );
}
