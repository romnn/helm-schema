use std::collections::HashSet;

use helm_schema_ast::{TemplateExpr, TemplateHeader};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::parse_get_binding_from_exprs;
use crate::condition_action_plan::{ConditionActionPlan, plan_if_condition, plan_with_condition};
use crate::contract_sink::ContractUseContext;
use crate::document_projection::ControlSite;
use crate::fragment_assignment::{AssignmentKind, parse_helper_assignment_from_exprs};
use crate::fragment_expr_eval::fragment_value_from_expr;
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

    fn observe_symbolic_assignment(&mut self, exprs: &[TemplateExpr]) {
        if let Some(assignment) = parse_helper_assignment_from_exprs(exprs) {
            let mut locals = self.scope.locals().fragment_values.clone();
            for (key, value) in &self.root_bindings {
                locals.insert(key.clone(), value.to_context_value());
            }
            let current_dot = self
                .current_dot_binding()
                .map(|value| value.to_context_value());
            let mut seen = HashSet::new();
            let fragment_value = fragment_value_from_expr(
                &assignment.rhs_expr,
                &locals,
                current_dot.as_ref(),
                self.fragment_eval_context(),
                &mut seen,
            );
            match assignment.kind {
                AssignmentKind::Declaration => self
                    .scope
                    .locals_mut()
                    .declare_fragment_value(assignment.variable.clone(), fragment_value),
                AssignmentKind::Assignment => self
                    .scope
                    .locals_mut()
                    .assign_fragment_value(assignment.variable.clone(), fragment_value),
            }
            self.refresh_assignment_facts(assignment.variable, &assignment.rhs_expr);
        }

        if let Some(get_binding) = parse_get_binding_from_exprs(exprs) {
            self.scope.locals_mut().apply_get_binding(get_binding);
        }
    }

    fn refresh_assignment_facts(&mut self, variable: String, rhs_expr: &TemplateExpr) {
        let exprs = std::slice::from_ref(rhs_expr);
        let output_effects = self.value_path_context().expression_output_effects(exprs);
        let helper = self.summarize_bound_helper_calls_in_exprs(exprs);
        let mut output_meta = output_effects.local_output_meta.clone();
        for output in &helper.output_uses {
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

    fn observe_assignment_exprs(&mut self, exprs: &[helm_schema_ast::TemplateExpr]) {
        self.observe_symbolic_assignment(exprs);
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
