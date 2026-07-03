use std::collections::HashSet;

use helm_schema_ast::{TemplateExpr, TemplateHeader};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::parse_get_binding_from_exprs;
use crate::contract_sink::{ContractUseContext, EmissionWitness};
use crate::fragment_assignment::parse_helper_assignment_from_exprs;
use crate::helper_summary::merge_output_use_meta;
use crate::node_eval::{NodeActionEffectSink, NodeEvalRuntime, push_predicate_contract_guards};
use crate::range_action_plan::{RangeActionPlan, plan_range_action};
use crate::symbolic_scope_state::SymbolicScopeSnapshot;
use crate::value_path_context::ValuePathContext;
use crate::{Guard, ValueKind, YamlPath};
use helm_schema_ast::ControlSite;
use helm_schema_core::Predicate;

use super::SymbolicWalker;

#[derive(Clone)]
pub(crate) struct ConditionActionPlan {
    pub(crate) predicate: Predicate,
    pub(crate) bound_values: Vec<String>,
    pub(crate) dot_binding: Option<AbstractValue>,
}

fn plan_if_condition(
    header: &TemplateHeader,
    value_path_context: &ValuePathContext<'_>,
) -> ConditionActionPlan {
    ConditionActionPlan {
        predicate: value_path_context.condition_predicate_expr(header.expr()),
        bound_values: value_path_context.bound_output_paths_expr(header.expr()),
        dot_binding: None,
    }
}

fn plan_with_condition(
    header: &TemplateHeader,
    value_path_context: &ValuePathContext<'_>,
) -> ConditionActionPlan {
    ConditionActionPlan {
        predicate: value_path_context.with_condition_predicate_expr(header.expr()),
        bound_values: value_path_context.bound_output_paths_expr(header.expr()),
        dot_binding: value_path_context.with_body_fragment_value_expr(header.expr()),
    }
}

impl SymbolicWalker<'_> {
    fn current_source_byte(&self) -> Option<usize> {
        self.current_source_span
            .and_then(|span| span.start.checked_sub(self.source_offset))
    }

    fn current_resource(&self, current_byte: Option<usize>) -> Option<crate::ResourceRef> {
        if self.control_resource_context_depth > 0 {
            let span = self.current_source_span?;
            let start = span.start.checked_sub(self.source_offset)?;
            let end = span.end.checked_sub(self.source_offset)?;
            return self
                .attribution
                .single_resource_in_span(start, end)
                .cloned();
        }
        current_byte
            .and_then(|byte| self.attribution.resource_at(byte))
            .cloned()
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
            Some(byte) => self.attribution.rebase_path_at(byte, path),
            None => path,
        };
        let resource = self.current_resource(current_byte);
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
        let witness =
            EmissionWitness::new(source_expr, Some(path), kind, vec![extra_guards.to_vec()]);
        context.emit(witness, &mut self.contract);
    }

    fn activate_if_condition_plan(&mut self, plan: &ConditionActionPlan) {
        let guards = plan.predicate.contract_guards();
        for value in &plan.bound_values {
            self.push_contract_use(value.clone(), YamlPath(Vec::new()), ValueKind::Scalar, &[]);
        }

        for guard in &guards {
            for path in guard.value_paths() {
                self.push_contract_use(
                    path.to_string(),
                    YamlPath(Vec::new()),
                    ValueKind::Scalar,
                    std::slice::from_ref(guard),
                );
            }
            self.scope
                .push_predicate_if_absent(Predicate::from(guard.clone()));
        }
        if guards.is_empty() {
            self.scope.push_predicate_if_absent(plan.predicate.clone());
        }
    }

    fn activate_with_condition_plan(&mut self, plan: &ConditionActionPlan) {
        // Push the With predicate before emitting header scalar uses so the
        // emitted contract guards on those uses include `Guard::With`.
        // The schema generator uses that marker to identify with-header reads.
        let guards = push_predicate_contract_guards(self, &plan.predicate);

        for value in &plan.bound_values {
            self.push_contract_use(value.clone(), YamlPath(Vec::new()), ValueKind::Scalar, &[]);
        }

        for guard in &guards {
            for path in guard.value_paths() {
                self.push_contract_use(
                    path.to_string(),
                    YamlPath(Vec::new()),
                    ValueKind::Scalar,
                    &[],
                );
            }
        }
        self.scope.push_dot_binding(plan.dot_binding.clone());
    }

    fn activate_range_action_plan(&mut self, plan: &RangeActionPlan, current_path: &YamlPath) {
        if let Some((variable, literals)) = &plan.literal_range {
            self.scope
                .locals_mut()
                .insert_range_domain(variable.clone(), literals.clone());
        }
        for source_path in &plan.source_paths {
            let guard = Guard::Range {
                path: source_path.clone(),
            };
            if plan.emit_header_use {
                self.push_contract_use(
                    source_path.clone(),
                    plan.guard_path.clone(),
                    ValueKind::Scalar,
                    std::slice::from_ref(&guard),
                );
            }
            self.scope.push_predicate_if_absent(Predicate::from(guard));
        }

        if plan.renders_mapping_entries {
            for source_path in &plan.source_paths {
                self.push_contract_use(
                    source_path.clone(),
                    current_path.clone(),
                    ValueKind::Fragment,
                    &[],
                );
            }
        }

        self.scope.push_dot_binding(plan.dot_binding.clone());
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
            let fragment_value = self.fragment_eval_context().fragment_value_from_expr(
                &assignment.rhs_expr,
                &locals,
                current_dot.as_ref(),
                &mut seen,
            );
            self.scope.locals_mut().bind_fragment_value(
                assignment.kind,
                assignment.variable.clone(),
                fragment_value,
            );
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
        let mut output_meta = output_effects.local_output_meta;
        merge_output_use_meta(&mut output_meta, &helper.output_uses);
        self.scope
            .locals_mut()
            .set_default_paths(&variable, output_effects.defaults);
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
        self.attribution
            .control_site_for_node(node)
            .unwrap_or_default()
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

    fn enter_if_condition(&mut self, header: &TemplateHeader) -> ConditionActionPlan {
        let plan = plan_if_condition(header, &self.value_path_context());
        self.control_resource_context_depth += 1;
        self.activate_if_condition_plan(&plan);
        self.control_resource_context_depth = self.control_resource_context_depth.saturating_sub(1);
        plan
    }

    fn enter_with_condition(&mut self, header: &TemplateHeader) -> ConditionActionPlan {
        let plan = plan_with_condition(header, &self.value_path_context());
        self.control_resource_context_depth += 1;
        self.activate_with_condition_plan(&plan);
        self.control_resource_context_depth = self.control_resource_context_depth.saturating_sub(1);
        plan
    }

    fn activate_condition_alternative(&mut self, plan: &ConditionActionPlan) {
        self.push_predicate_if_absent(plan.predicate.negated());
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
        self.control_resource_context_depth += 1;
        self.activate_range_action_plan(plan, current_path);
        self.control_resource_context_depth = self.control_resource_context_depth.saturating_sub(1);
    }
}

impl NodeActionEffectSink for SymbolicWalker<'_> {
    fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        self.scope.push_predicate_if_absent(predicate);
    }
}
