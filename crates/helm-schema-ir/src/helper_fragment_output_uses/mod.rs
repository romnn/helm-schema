use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateHeader;

use crate::abstract_value::AbstractValue;
use crate::document_projection::{DocumentTracker, collect_document_site_context};
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::fragment_range_scope::{
    range_body_emits_sequence_item_from_source, range_body_renders_mapping_entries_from_ast,
    range_has_destructured_variable_definition,
};
use crate::helper_range_plan::NonExactRangeVariableBinding;
use crate::helper_runtime_plan::{
    HelperConditionPlan, HelperRangeDotSource, HelperRangeRuntimePlan, HelperRuntimeSemantics,
    helper_if_condition_plan, helper_range_runtime_plan, helper_with_condition_plan,
};
use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta};
use crate::helper_walk_state::{
    FragmentOutputWalkState, HelperRuntimeControlSnapshot, HelperRuntimeControlState,
    HelperRuntimeLocals, HelperRuntimeScopeJoin,
};
use crate::node_eval::{
    AssignmentObservation, NodeActionEffectSink, NodeEvalRuntime,
    activate_condition_alternative_guards, activate_if_condition_plan, activate_range_action_plan,
    activate_with_condition_plan,
};
use crate::predicate::Predicate;
use crate::{ValueKind, YamlPath};

mod expression_output;

pub(crate) use expression_output::collect_bound_fragment_output_uses_from_exprs;

const FRAGMENT_SEMANTICS: HelperRuntimeSemantics = HelperRuntimeSemantics {
    apply_alternative_predicate: false,
    non_exact_range_variable_binding: NonExactRangeVariableBinding::Skip,
    range_dot_source: HelperRangeDotSource::FragmentValue,
};

pub(crate) struct FragmentOutputUseRuntime<'context, 'state> {
    source: &'state str,
    bindings: &'state HashMap<String, AbstractValue>,
    control: HelperRuntimeControlState,
    locals: &'state mut HelperRuntimeLocals,
    context: FragmentEvalContext<'context>,
    seen: &'state mut HashSet<String>,
    outputs: &'state mut Vec<HelperFragmentOutputUse>,
    document_tracker: DocumentTracker<'state>,
}

#[derive(Clone)]
pub(crate) struct FragmentOutputUseSnapshot {
    locals: HelperRuntimeLocals,
    control: HelperRuntimeControlSnapshot,
}

impl FragmentOutputUseRuntime<'_, '_> {
    pub(crate) fn new<'context, 'state>(
        tree: &tree_sitter::Tree,
        source: &'state str,
        bindings: &'state HashMap<String, AbstractValue>,
        current_dot: Option<&AbstractValue>,
        current_dot_fragment: Option<&AbstractValue>,
        state: &'state mut FragmentOutputWalkState<'context, 'state>,
    ) -> FragmentOutputUseRuntime<'context, 'state> {
        let mut document_tracker = DocumentTracker::new(source, state.context.defines);
        document_tracker.reset_for_tree(tree);
        FragmentOutputUseRuntime {
            source,
            bindings,
            control: HelperRuntimeControlState::for_fragment(current_dot, current_dot_fragment),
            locals: state.locals,
            context: state.context,
            seen: state.seen,
            outputs: state.outputs,
            document_tracker,
        }
    }

    fn current_dot(&self) -> Option<&AbstractValue> {
        self.control.current_helper_dot()
    }

    fn current_dot_fragment(&self) -> Option<&AbstractValue> {
        self.control.current_fragment_dot()
    }

    fn collect_expression(
        &mut self,
        exprs: &[helm_schema_ast::TemplateExpr],
        relative_path: &YamlPath,
        kind: ValueKind,
    ) {
        let current_dot = self.current_dot().cloned();
        let current_dot_fragment = self.current_dot_fragment().cloned();
        let active_output_predicates = self.control.active_output_predicates().clone();
        let mut state = FragmentOutputWalkState {
            locals: &mut *self.locals,
            context: self.context,
            seen: self.seen,
            outputs: self.outputs,
        };
        collect_bound_fragment_output_uses_from_exprs(
            exprs,
            self.bindings,
            current_dot.as_ref(),
            current_dot_fragment.as_ref(),
            relative_path,
            kind,
            &active_output_predicates,
            &mut state,
        );
    }

    fn merge_outcomes(&mut self, outcomes: Vec<FragmentOutputUseSnapshot>) {
        let mut iter = outcomes.into_iter();
        let Some(first) = iter.next() else {
            return;
        };
        let mut locals = first.locals;
        for outcome in iter {
            locals = locals.merge(outcome.locals);
        }
        *self.locals = locals;
    }

    fn promote_outcome(&mut self, outcome: FragmentOutputUseSnapshot) {
        *self.locals = outcome.locals;
    }

    fn collect_destructured_range_fragment_outputs(
        &mut self,
        node: tree_sitter::Node<'_>,
        range_binding: Option<&AbstractValue>,
        current_path: &YamlPath,
    ) {
        if !range_has_destructured_variable_definition(node)
            || range_body_emits_sequence_item_from_source(node, self.source)
            || !range_body_renders_mapping_entries_from_ast(node, self.source)
        {
            return;
        }
        let Some(range_binding) = range_binding else {
            return;
        };

        let meta =
            HelperOutputMeta::with_predicates(self.control.active_output_predicates(), false);
        for source_expr in range_binding.fragment_source_paths() {
            self.outputs.push(HelperFragmentOutputUse::new(
                source_expr,
                current_path.clone(),
                ValueKind::Fragment,
                meta.clone(),
            ));
        }
    }
}

impl NodeActionEffectSink for FragmentOutputUseRuntime<'_, '_> {
    fn declare_fragment_value(&mut self, variable: String, binding: Option<AbstractValue>) {
        if let Some(binding) = binding {
            self.locals.bindings.insert(variable, binding);
        } else {
            self.locals.bindings.remove(&variable);
        }
    }

    fn assign_fragment_value(&mut self, variable: String, binding: Option<AbstractValue>) {
        self.declare_fragment_value(variable, binding);
    }

    fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        self.control.push_predicate_if_absent(predicate);
    }

    fn push_dot_binding(&mut self, binding: Option<AbstractValue>) {
        self.control.push_effect_dot_binding(binding);
    }
}

impl NodeEvalRuntime for FragmentOutputUseRuntime<'_, '_> {
    type ScopeSnapshot = FragmentOutputUseSnapshot;
    type ConditionPlan = HelperConditionPlan;
    type RangePlan = HelperRangeRuntimePlan;

    fn source(&self) -> &str {
        self.source
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
        FragmentOutputUseSnapshot {
            locals: self.locals.clone(),
            control: self.control.snapshot(),
        }
    }

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot) {
        *self.locals = snapshot.locals;
        self.control.restore(&snapshot.control);
    }

    fn join_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        let outcomes = self.control.branch_join_outcomes(&entry.control, outcomes);
        self.merge_outcomes(outcomes);
    }

    fn join_range_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        match self.control.range_join_outcomes(&entry.control, outcomes) {
            HelperRuntimeScopeJoin::Promote(body_outcome) => self.promote_outcome(body_outcome),
            HelperRuntimeScopeJoin::Merge(outcomes) => self.merge_outcomes(outcomes),
            HelperRuntimeScopeJoin::Noop => {}
        }
    }

    fn range_iteration_count(&self) -> usize {
        self.control.range_iteration_count()
    }

    fn enter_range_iteration(&mut self, index: usize) {
        self.control.enter_range_iteration(index, self.locals);
    }

    fn exit_range_iteration(&mut self, _index: usize) {
        self.control.exit_range_iteration();
    }

    fn enter_no_output(&mut self) {
        self.control.enter_no_output();
    }

    fn exit_no_output(&mut self) {
        self.control.exit_no_output();
    }

    fn handle_output_node(
        &mut self,
        node: tree_sitter::Node<'_>,
        exprs: &[helm_schema_ast::TemplateExpr],
    ) {
        if self.control.suppresses_output() {
            return;
        }
        let site_context =
            collect_document_site_context(self.source, &self.document_tracker, node, exprs);
        let Some(site) = site_context.fragment_output_site() else {
            return;
        };
        self.collect_expression(exprs, &site.path, site.kind);
    }

    fn observe_assignment_exprs(
        &mut self,
        exprs: &[helm_schema_ast::TemplateExpr],
    ) -> AssignmentObservation {
        let mut seen_set = HashSet::new();
        let current_dot_fragment = self.current_dot_fragment().cloned();
        if crate::fragment_assignment::apply_local_set_mutations_from_exprs(
            exprs,
            &mut self.locals.bindings,
            current_dot_fragment.as_ref(),
            self.context,
            &mut seen_set,
        ) {
            return AssignmentObservation::LocalMutationApplied;
        }

        self.collect_expression(exprs, &YamlPath(Vec::new()), ValueKind::Scalar);
        AssignmentObservation::ExpressionObserved
    }

    fn plan_if_condition(&mut self, header: &TemplateHeader) -> HelperConditionPlan {
        let current_dot = self.current_dot().cloned();
        helper_if_condition_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            &self.locals.bindings,
            self.context,
            self.seen,
            FRAGMENT_SEMANTICS,
        )
    }

    fn activate_if_condition(&mut self, plan: &HelperConditionPlan) {
        activate_if_condition_plan(self, &plan.action);
    }

    fn plan_with_condition(&mut self, header: &TemplateHeader) -> HelperConditionPlan {
        let current_dot = self.current_dot().cloned();
        let current_dot_fragment = self.current_dot_fragment().cloned();
        helper_with_condition_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            current_dot_fragment.as_ref(),
            &self.locals.bindings,
            self.context,
            self.seen,
            FRAGMENT_SEMANTICS,
        )
    }

    fn activate_with_condition(&mut self, plan: &HelperConditionPlan) {
        activate_with_condition_plan(self, &plan.action);
    }

    fn activate_condition_alternative(&mut self, plan: &HelperConditionPlan) {
        activate_condition_alternative_guards(self, &plan.action);
    }

    fn plan_range_action(
        &mut self,
        _node: tree_sitter::Node<'_>,
        header: Option<&TemplateHeader>,
        _current_path: &YamlPath,
    ) -> HelperRangeRuntimePlan {
        let current_dot = self.current_dot().cloned();
        let current_dot_fragment = self.current_dot_fragment().cloned();
        helper_range_runtime_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            current_dot_fragment.as_ref(),
            &self.locals.bindings,
            self.context,
            self.seen,
            FRAGMENT_SEMANTICS,
        )
    }

    fn range_output_path(
        &self,
        node: tree_sitter::Node<'_>,
        current_path: &YamlPath,
        plan: &HelperRangeRuntimePlan,
    ) -> YamlPath {
        plan.action
            .mapping_entry_indent
            .map(|indent| self.document_path_for_mapping_entry_indent(node, indent))
            .unwrap_or_else(|| current_path.clone())
    }

    fn activate_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        plan: &HelperRangeRuntimePlan,
        current_path: &YamlPath,
    ) {
        let activated = plan.clone().activate(&mut self.control, self.locals);
        self.collect_destructured_range_fragment_outputs(
            node,
            activated.range_fragment_value.as_ref(),
            current_path,
        );
        activate_range_action_plan(self, &activated.action, current_path);
    }
}
