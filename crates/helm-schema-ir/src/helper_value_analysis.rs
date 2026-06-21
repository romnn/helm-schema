use std::collections::{BTreeMap, HashMap, HashSet};

use helm_schema_ast::TemplateHeader;

use crate::YamlPath;
use crate::abstract_value::AbstractValue;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_range_plan::NonExactRangeVariableBinding;
use crate::helper_runtime_plan::{
    HelperConditionPlan, HelperRangeDotSource, HelperRangeRuntimePlan, HelperRuntimeSemantics,
    helper_if_condition_plan, helper_range_runtime_plan, helper_with_condition_plan,
};
use crate::helper_summary::{HelperOutputMeta, HelperSummary};
use crate::helper_summary_mutation::merge_helper_output_meta_maps;
use crate::helper_value_expression::collect_helper_value_expression_from_exprs;
use crate::helper_walk_state::{
    HelperRangeJoinBehavior, HelperRuntimeControlSnapshot, HelperRuntimeControlState,
    HelperRuntimeLocals, HelperValuesWalkState,
};
use crate::node_eval::{
    AssignmentObservation, NodeActionEffectSink, NodeEvalRuntime,
    activate_condition_alternative_guards, activate_if_condition_plan, activate_range_action_plan,
    activate_with_condition_plan,
};
use crate::predicate::Predicate;

const VALUE_SEMANTICS: HelperRuntimeSemantics = HelperRuntimeSemantics {
    apply_alternative_predicate: true,
    non_exact_range_variable_binding: NonExactRangeVariableBinding::Bind,
    range_dot_source: HelperRangeDotSource::HelperValue,
};

pub(crate) struct HelperValueRuntime<'context, 'state> {
    source: &'state str,
    bindings: &'state HashMap<String, AbstractValue>,
    control: HelperRuntimeControlState,
    locals: &'state mut HelperRuntimeLocals,
    local_output_meta: &'state mut HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    context: FragmentEvalContext<'context>,
    seen: &'state mut HashSet<String>,
    analysis: &'state mut HelperSummary,
}

#[derive(Clone)]
pub(crate) struct HelperValueSnapshot {
    locals: HelperRuntimeLocals,
    local_output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    control: HelperRuntimeControlSnapshot,
}

impl HelperValueRuntime<'_, '_> {
    pub(crate) fn new<'context, 'state>(
        source: &'state str,
        bindings: &'state HashMap<String, AbstractValue>,
        current_dot: Option<&AbstractValue>,
        state: &'state mut HelperValuesWalkState<'context, 'state>,
    ) -> HelperValueRuntime<'context, 'state> {
        HelperValueRuntime {
            source,
            bindings,
            control: HelperRuntimeControlState::for_value(current_dot),
            locals: state.locals,
            local_output_meta: state.local_output_meta,
            context: state.context,
            seen: state.seen,
            analysis: state.analysis,
        }
    }

    fn current_dot(&self) -> Option<&AbstractValue> {
        self.control.current_helper_dot()
    }

    fn current_dot_fragment(&self) -> Option<AbstractValue> {
        self.current_dot().map(AbstractValue::to_context_value)
    }

    fn collect_expression(&mut self, exprs: &[helm_schema_ast::TemplateExpr]) {
        let current_dot = self.current_dot().cloned();
        let active_output_predicates = self.control.active_output_predicates().clone();
        let mut state = HelperValuesWalkState {
            locals: &mut *self.locals,
            local_output_meta: &mut *self.local_output_meta,
            context: self.context,
            seen: self.seen,
            analysis: self.analysis,
        };
        collect_helper_value_expression_from_exprs(
            exprs,
            self.bindings,
            current_dot.as_ref(),
            &active_output_predicates,
            &mut state,
        );
    }

    fn merge_outcomes(&mut self, outcomes: Vec<HelperValueSnapshot>) {
        let mut iter = outcomes.into_iter();
        let Some(first) = iter.next() else {
            return;
        };
        let mut locals = first.locals;
        let mut local_output_meta = first.local_output_meta;
        for outcome in iter {
            locals = locals.merge(outcome.locals);
            local_output_meta =
                merge_helper_output_meta_maps(local_output_meta, outcome.local_output_meta);
        }
        *self.locals = locals;
        *self.local_output_meta = local_output_meta;
    }

    fn promote_outcome(&mut self, outcome: HelperValueSnapshot) {
        *self.locals = outcome.locals;
        *self.local_output_meta = outcome.local_output_meta;
    }
}

impl NodeActionEffectSink for HelperValueRuntime<'_, '_> {
    fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        self.control.push_predicate_if_absent(predicate);
    }

    fn push_dot_binding(&mut self, binding: Option<AbstractValue>) {
        self.control.push_effect_dot_binding(binding);
    }
}

impl NodeEvalRuntime for HelperValueRuntime<'_, '_> {
    type ScopeSnapshot = HelperValueSnapshot;
    type ConditionPlan = HelperConditionPlan;
    type RangePlan = HelperRangeRuntimePlan;

    fn source(&self) -> &str {
        self.source
    }

    fn document_path_for_node(&self, _node: tree_sitter::Node<'_>) -> YamlPath {
        YamlPath(Vec::new())
    }

    fn scope_snapshot(&self) -> Self::ScopeSnapshot {
        HelperValueSnapshot {
            locals: self.locals.clone(),
            local_output_meta: self.local_output_meta.clone(),
            control: self.control.snapshot(),
        }
    }

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot) {
        *self.locals = snapshot.locals;
        *self.local_output_meta = snapshot.local_output_meta;
        self.control.restore(&snapshot.control);
    }

    fn join_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        self.control.prepare_branch_join(&entry.control);
        self.merge_outcomes(outcomes);
    }

    fn join_range_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        match self.control.prepare_range_join(&entry.control) {
            HelperRangeJoinBehavior::PromoteBodyOutcome => {
                if let Some(body_outcome) = outcomes.into_iter().next() {
                    self.promote_outcome(body_outcome);
                }
            }
            HelperRangeJoinBehavior::MergeAllOutcomes => {
                self.merge_outcomes(outcomes);
            }
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
        _node: tree_sitter::Node<'_>,
        exprs: &[helm_schema_ast::TemplateExpr],
    ) {
        if self.control.suppresses_output() {
            return;
        }
        self.collect_expression(exprs);
    }

    fn observe_assignment_exprs(
        &mut self,
        exprs: &[helm_schema_ast::TemplateExpr],
    ) -> AssignmentObservation {
        self.collect_expression(exprs);
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
            VALUE_SEMANTICS,
        )
    }

    fn activate_if_condition(&mut self, plan: &HelperConditionPlan) {
        plan.record_guard_paths_into(self.analysis);
        activate_if_condition_plan(self, &plan.action);
    }

    fn plan_with_condition(&mut self, header: &TemplateHeader) -> HelperConditionPlan {
        let current_dot = self.current_dot().cloned();
        let current_dot_fragment = current_dot.as_ref().map(AbstractValue::to_context_value);
        helper_with_condition_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            current_dot_fragment.as_ref(),
            &self.locals.bindings,
            self.context,
            self.seen,
            VALUE_SEMANTICS,
        )
    }

    fn activate_with_condition(&mut self, plan: &HelperConditionPlan) {
        plan.record_guard_paths_into(self.analysis);
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
        let current_dot_fragment = self.current_dot_fragment();
        helper_range_runtime_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            current_dot_fragment.as_ref(),
            &self.locals.bindings,
            self.context,
            self.seen,
            VALUE_SEMANTICS,
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
        _node: tree_sitter::Node<'_>,
        plan: &HelperRangeRuntimePlan,
        current_path: &YamlPath,
    ) {
        let activated = plan.clone().activate(&mut self.control, self.locals);
        activated.record_guard_paths_into(self.analysis);
        activate_range_action_plan(self, &activated.action, current_path);
    }
}
