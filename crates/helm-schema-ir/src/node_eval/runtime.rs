use helm_schema_ast::{TemplateExpr, TemplateHeader};

pub(crate) trait NodeEvalRuntime {
    type ScopeSnapshot: Clone;
    type ConditionPlan: Clone;

    fn source(&self) -> &str;

    fn enter_node(&mut self, _node: tree_sitter::Node<'_>) {}

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

    fn enter_no_output(&mut self);

    fn exit_no_output(&mut self);

    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>, exprs: &[TemplateExpr]);

    fn observe_assignment_exprs(&mut self, _exprs: &[TemplateExpr]) {}

    fn enter_if_condition(&mut self, header: &TemplateHeader) -> Self::ConditionPlan;

    fn enter_with_condition(&mut self, header: &TemplateHeader) -> Self::ConditionPlan;

    fn activate_condition_alternative(&mut self, plan: &Self::ConditionPlan);
}

pub(crate) struct BranchOutcome<Plan, Snapshot> {
    pub(crate) plan: Option<Plan>,
    pub(crate) outcome: Snapshot,
}
