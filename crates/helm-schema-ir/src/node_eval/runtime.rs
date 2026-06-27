use helm_schema_ast::{TemplateExpr, TemplateHeader};

use super::effects::NodeActionEffectSink;
use crate::YamlPath;
use helm_schema_ast::ControlSite;

pub(crate) trait NodeEvalRuntime: NodeActionEffectSink {
    type ScopeSnapshot: Clone;
    type ConditionPlan: Clone;
    type RangePlan;

    fn source(&self) -> &str;

    fn enter_node(&mut self, _node: tree_sitter::Node<'_>) {}

    fn document_control_site_for_node(&self, _node: tree_sitter::Node<'_>) -> ControlSite {
        ControlSite::default()
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

    fn join_condition_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        branches: Vec<BranchOutcome<Self::ConditionPlan, Self::ScopeSnapshot>>,
    ) {
        self.join_branch_scopes(
            entry,
            branches.into_iter().map(|branch| branch.outcome).collect(),
        );
    }

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

    fn observe_assignment_exprs(&mut self, _exprs: &[TemplateExpr]) {}

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

pub(crate) struct BranchOutcome<Plan, Snapshot> {
    pub(crate) plan: Option<Plan>,
    pub(crate) outcome: Snapshot,
}
