use helm_schema_ast::{TemplateExpr, TemplateHeader};

use super::effects::NodeActionEffectSink;
use crate::YamlPath;
use crate::assignment_action_plan::AssignmentActionPlan;
use crate::condition_action_plan::ConditionActionPlan;
use crate::range_action_plan::RangeActionPlan;

pub(crate) trait NodeEvalRuntime: NodeActionEffectSink {
    type ScopeSnapshot: Clone;

    fn source(&self) -> &str;

    fn enter_node(&mut self, node: tree_sitter::Node<'_>);

    fn ingest_text_up_to(&mut self, end_byte: usize);

    fn current_document_path(&self) -> YamlPath;

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

    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>, exprs: &[TemplateExpr]);

    fn apply_assignment_side_effects(&mut self, _exprs: &[TemplateExpr]) -> bool {
        false
    }

    fn plan_assignment_action(&self, exprs: &[TemplateExpr]) -> AssignmentActionPlan;

    fn plan_if_condition(&mut self, header: &TemplateHeader) -> ConditionActionPlan;

    fn plan_with_condition(&mut self, header: &TemplateHeader) -> ConditionActionPlan;

    fn plan_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        header: Option<&TemplateHeader>,
        current_path: &YamlPath,
    ) -> RangeActionPlan;
}
