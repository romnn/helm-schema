use helm_schema_ast::TemplateHeader;

use crate::abstract_value::AbstractValue;
use crate::assignment_action_plan::{AssignmentActionPlan, plan_assignment_action};
use crate::bound_value_analysis::GetBindingPlan;
use crate::condition_action_plan::{ConditionActionPlan, plan_if_condition, plan_with_condition};
use crate::contract_sink::ContractUseContext;
use crate::node_eval::{NodeActionEffectSink, NodeEvalRuntime};
use crate::predicate::Predicate;
use crate::range_action_plan::{RangeActionPlan, plan_range_action};
use crate::symbolic_scope_state::SymbolicScopeSnapshot;
use crate::{Guard, ResourceRef, ValueKind, YamlPath};

use super::SymbolicWalker;

impl SymbolicWalker<'_> {
    fn current_source_byte(&self) -> Option<usize> {
        self.current_source_span
            .and_then(|span| span.start.checked_sub(self.source_offset))
    }

    fn current_resource(&self) -> Option<ResourceRef> {
        self.current_source_byte()
            .and_then(|byte| self.document_tracker.resource_at(byte))
            .cloned()
    }

    fn push_contract_use_with_resource(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
        resource: Option<ResourceRef>,
    ) {
        let path = match self.current_source_byte() {
            Some(byte) => self.document_tracker.rebase_path_at(byte, path),
            None => path,
        };
        let guards = self.contract_guards();
        let context = ContractUseContext::new(
            &guards,
            &self.scope.locals().chart_value_defaults,
            self.no_output_depth > 0,
            self.source_path,
            self.current_source_span,
            self.provenance_helper_chain(),
        );
        self.contract
            .push(context.contract_use(source_expr, path, kind, extra_guards, resource));
    }
}

impl NodeEvalRuntime for SymbolicWalker<'_> {
    type ScopeSnapshot = SymbolicScopeSnapshot;

    fn source(&self) -> &str {
        self.source
    }

    fn enter_node(&mut self, node: tree_sitter::Node<'_>) {
        self.current_source_span = Some(crate::SourceSpan::new(
            self.source_offset + node.start_byte(),
            self.source_offset + node.end_byte(),
        ));
    }

    fn document_path_for_node(&self, node: tree_sitter::Node<'_>) -> YamlPath {
        self.document_tracker.path_for_node(node)
    }

    fn document_path_for_mapping_entry_indent(
        &self,
        node: tree_sitter::Node<'_>,
        indent: usize,
    ) -> YamlPath {
        self.document_tracker
            .path_at_mapping_entry_indent(node, indent)
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
        plan_if_condition(
            header,
            &value_path_context,
            &self.scope.locals().range_domains,
            &self.scope.locals().get_bindings,
        )
    }

    fn plan_with_condition(&mut self, header: &TemplateHeader) -> ConditionActionPlan {
        let value_path_context = self.value_path_context();
        plan_with_condition(
            header,
            &value_path_context,
            &self.scope.locals().range_domains,
            &self.scope.locals().get_bindings,
        )
    }

    fn plan_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        header: Option<&TemplateHeader>,
        current_path: &YamlPath,
    ) -> RangeActionPlan {
        let value_path_context = self.value_path_context();
        plan_range_action(node, header, self.source, &value_path_context, current_path)
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

    fn refresh_default_paths(&mut self, variable: &str, rhs_expr: &helm_schema_ast::TemplateExpr) {
        let default_paths = self
            .value_path_context()
            .resolved_default_fallback_paths_in_exprs(std::slice::from_ref(rhs_expr));
        self.scope
            .locals_mut()
            .set_default_paths(variable, default_paths);
    }

    fn refresh_helper_output_meta(
        &mut self,
        variable: String,
        rhs_expr: &helm_schema_ast::TemplateExpr,
    ) {
        let helper_meta = self.helper_output_meta_for_exprs(std::slice::from_ref(rhs_expr));
        self.scope
            .locals_mut()
            .set_output_meta(variable, helper_meta);
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
        self.push_contract_use_with_resource(
            source_expr,
            path,
            kind,
            extra_guards,
            self.current_resource(),
        );
    }
}
