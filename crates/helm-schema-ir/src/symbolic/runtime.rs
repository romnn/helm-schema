use helm_schema_ast::TemplateHeader;

use crate::abstract_value::AbstractValue;
use crate::assignment_action_plan::{AssignmentActionPlan, plan_assignment_action};
use crate::bound_value_analysis::GetBindingPlan;
use crate::condition_action_plan::{ConditionActionPlan, plan_if_condition, plan_with_condition};
use crate::contract_sink::ContractUseContext;
use crate::document_projection::ControlSite;
use crate::node_eval::{
    NodeActionEffectSink, NodeEvalRuntime, activate_condition_alternative_guards,
    activate_if_condition_plan, activate_range_action_plan, activate_with_condition_plan,
};
use crate::predicate::Predicate;
use crate::range_action_plan::{RangeActionPlan, plan_range_action};
use crate::symbolic_scope_state::SymbolicScopeSnapshot;
use crate::{Guard, ValueKind, YamlPath};

use super::SymbolicWalker;

impl SymbolicWalker<'_> {
    fn current_source_byte(&self) -> Option<usize> {
        self.current_source_span
            .and_then(|span| span.start.checked_sub(self.source_offset))
    }

    fn push_contract_use(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    ) {
        let current_byte = self.current_source_byte();
        let path = match current_byte {
            Some(byte) => self.document_tracker.rebase_path_at(byte, path),
            None => path,
        };
        let resource = current_byte
            .and_then(|byte| self.document_tracker.resource_at(byte))
            .cloned();
        let guards = self.contract_guards();
        let context = ContractUseContext::new(
            &guards,
            &self.scope.locals().chart_value_defaults,
            self.no_output_depth > 0,
            resource,
            self.source_path,
            self.current_source_span,
            self.provenance_helper_chain(),
        );
        self.contract
            .push(context.contract_use(source_expr, path, kind, extra_guards));
    }
}

impl NodeEvalRuntime for SymbolicWalker<'_> {
    type ScopeSnapshot = SymbolicScopeSnapshot;
    type ConditionPlan = ConditionActionPlan;
    type RangePlan = RangeActionPlan;

    fn source(&self) -> &str {
        self.source
    }

    fn enter_node(&mut self, node: tree_sitter::Node<'_>) {
        self.current_source_span = Some(crate::SourceSpan::new(
            self.source_offset + node.start_byte(),
            self.source_offset + node.end_byte(),
        ));
    }

    fn document_control_site_for_node(&self, node: tree_sitter::Node<'_>) -> ControlSite {
        self.document_tracker.control_site_for_node(node)
    }

    fn scope_snapshot(&self) -> Self::ScopeSnapshot {
        self.scope.snapshot()
    }

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot) {
        self.scope.restore(snapshot);
    }

    fn enter_local_scope(&mut self) {
        self.scope.locals_mut().enter_local_scope();
    }

    fn exit_local_scope(&mut self) {
        self.scope.locals_mut().exit_local_scope();
    }

    fn join_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        self.scope.join_branch_outcomes(entry, outcomes);
    }

    fn enter_no_output(&mut self) {
        self.no_output_depth += 1;
    }

    fn exit_no_output(&mut self) {
        self.no_output_depth = self.no_output_depth.saturating_sub(1);
    }

    fn handle_output_node(
        &mut self,
        node: tree_sitter::Node<'_>,
        exprs: &[helm_schema_ast::TemplateExpr],
    ) {
        SymbolicWalker::handle_output_node(self, node, exprs);
    }

    fn plan_assignment_action(
        &self,
        exprs: &[helm_schema_ast::TemplateExpr],
    ) -> AssignmentActionPlan {
        let fragment_context = self.fragment_eval_context();
        let current_dot = self.current_dot_binding();
        plan_assignment_action(
            exprs,
            fragment_context,
            &self.scope.locals().fragment_values,
            &self.root_bindings,
            current_dot.as_ref(),
        )
    }

    fn plan_if_condition(&mut self, header: &TemplateHeader) -> ConditionActionPlan {
        let value_path_context = self.value_path_context();
        plan_if_condition(header, &value_path_context)
    }

    fn activate_if_condition(&mut self, plan: &ConditionActionPlan) {
        activate_if_condition_plan(self, plan);
    }

    fn plan_with_condition(&mut self, header: &TemplateHeader) -> ConditionActionPlan {
        let value_path_context = self.value_path_context();
        plan_with_condition(header, &value_path_context)
    }

    fn activate_with_condition(&mut self, plan: &ConditionActionPlan) {
        activate_with_condition_plan(self, plan);
    }

    fn activate_condition_alternative(&mut self, plan: &ConditionActionPlan) {
        activate_condition_alternative_guards(self, plan);
    }

    fn plan_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        header: Option<&TemplateHeader>,
        current_path: &YamlPath,
        mapping_entry_path: Option<&YamlPath>,
    ) -> RangeActionPlan {
        let value_path_context = self.value_path_context();
        plan_range_action(
            node,
            header,
            self.source,
            &value_path_context,
            current_path,
            mapping_entry_path,
        )
    }

    fn activate_range_action(
        &mut self,
        _node: tree_sitter::Node<'_>,
        plan: &RangeActionPlan,
        current_path: &YamlPath,
    ) {
        activate_range_action_plan(self, plan, current_path);
    }
}

impl NodeActionEffectSink for SymbolicWalker<'_> {
    fn apply_get_binding(&mut self, plan: GetBindingPlan) {
        self.scope.locals_mut().apply_get_binding(plan);
    }

    fn declare_fragment_value(&mut self, variable: String, binding: Option<AbstractValue>) {
        self.scope
            .locals_mut()
            .declare_fragment_value(variable, binding);
    }

    fn assign_fragment_value(&mut self, variable: String, binding: Option<AbstractValue>) {
        self.scope
            .locals_mut()
            .assign_fragment_value(variable, binding);
    }

    fn refresh_assignment_facts(
        &mut self,
        variable: String,
        rhs_expr: &helm_schema_ast::TemplateExpr,
    ) {
        let exprs = std::slice::from_ref(rhs_expr);
        let output_effects = self.value_path_context().expression_output_effects(exprs);
        let helper = self.summarize_bound_helper_calls_in_exprs(exprs);
        let mut output_meta = output_effects.local_output_meta.clone();
        for (path, meta) in &helper.scalar_output_meta {
            output_meta.entry(path.clone()).or_default().merge_ref(meta);
        }
        for output in &helper.fragment_output_uses {
            output_meta
                .entry(output.source_expr.clone())
                .or_default()
                .merge_ref(&output.meta);
        }
        self.scope
            .locals_mut()
            .set_default_paths(&variable, output_effects.defaults.clone());
        self.scope
            .locals_mut()
            .set_output_meta(variable, output_meta);
    }

    fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        self.scope.push_predicate_if_absent(predicate);
    }

    fn push_dot_binding(&mut self, binding: Option<AbstractValue>) {
        self.scope.push_dot_binding(binding);
    }

    fn insert_range_domain(&mut self, variable: String, literals: Vec<String>) {
        self.scope
            .locals_mut()
            .insert_range_domain(variable, literals);
    }

    fn observe_value_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    ) {
        self.push_contract_use(source_expr, path, kind, extra_guards);
    }
}
