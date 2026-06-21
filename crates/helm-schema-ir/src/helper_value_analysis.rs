use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateHeader;

use crate::abstract_value::AbstractValue;
use crate::assignment_action_plan::AssignmentActionPlan;
use crate::bound_value_analysis::GetBindingPlan;
use crate::condition_action_plan::ConditionActionPlan;
use crate::contract_sink::ContractUseSink;
use crate::fragment_assignment::merge_fragment_locals;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_range_frame::RangeFrame;
use crate::helper_range_plan::{HelperRangeIteration, NonExactRangeVariableBinding};
use crate::helper_runtime_plan::{
    HelperRangeDotSource, HelperRuntimeSemantics, helper_if_condition_plan,
    helper_range_runtime_plan, helper_with_condition_plan,
};
use crate::helper_summary::{HelperOutputMeta, HelperSummary};
use crate::helper_summary_mutation::{merge_helper_output_meta_maps, merge_local_default_paths};
use crate::helper_value_expression::collect_helper_value_expression_from_exprs;
use crate::helper_walk_state::HelperValuesWalkState;
use crate::node_eval::{NodeActionEffectSink, NodeEvalRuntime, eval_template_body};
use crate::predicate::Predicate;
use crate::range_action_plan::RangeActionPlan;
use crate::{ValueKind, YamlPath};

/// Walks a helper body collecting the values and effects it contributes to
/// callers that include/template it.
#[tracing::instrument(skip_all)]
pub(crate) fn collect_bound_helper_values_from_tree(
    node: tree_sitter::Node<'_>,
    source: &str,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    state: &mut HelperValuesWalkState<'_, '_>,
) {
    let mut runtime = HelperValueRuntime {
        source,
        bindings,
        dot_stack: vec![current_dot.cloned()],
        active_output_predicates: BTreeSet::new(),
        local_bindings: state.local_bindings,
        local_default_paths: state.local_default_paths,
        local_output_meta: state.local_output_meta,
        context: state.context,
        seen: state.seen,
        analysis: state.analysis,
        range_frames: Vec::new(),
        no_output_depth: 0,
    };
    eval_template_body(&mut runtime, node);
}

struct HelperValueRuntime<'context, 'state> {
    source: &'state str,
    bindings: &'state HashMap<String, AbstractValue>,
    dot_stack: Vec<Option<AbstractValue>>,
    active_output_predicates: BTreeSet<Predicate>,
    local_bindings: &'state mut HashMap<String, AbstractValue>,
    local_default_paths: &'state mut HashMap<String, BTreeSet<String>>,
    local_output_meta: &'state mut HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    context: FragmentEvalContext<'context>,
    seen: &'state mut HashSet<String>,
    analysis: &'state mut HelperSummary,
    range_frames: Vec<RangeFrame<HelperRangeIteration>>,
    no_output_depth: usize,
}

#[derive(Clone)]
struct HelperValueSnapshot {
    local_bindings: HashMap<String, AbstractValue>,
    local_default_paths: HashMap<String, BTreeSet<String>>,
    local_output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    dot_stack_len: usize,
    active_output_predicates: BTreeSet<Predicate>,
}

impl HelperValueRuntime<'_, '_> {
    const SEMANTICS: HelperRuntimeSemantics = HelperRuntimeSemantics {
        record_guard_paths: true,
        apply_alternative_predicate: true,
        non_exact_range_variable_binding: NonExactRangeVariableBinding::Bind,
        range_dot_source: HelperRangeDotSource::HelperValue,
    };

    fn current_dot(&self) -> Option<&AbstractValue> {
        self.dot_stack.last().and_then(Option::as_ref)
    }

    fn current_dot_fragment(&self) -> Option<AbstractValue> {
        self.current_dot().map(AbstractValue::to_context_value)
    }

    fn collect_expression(&mut self, exprs: &[helm_schema_ast::TemplateExpr]) {
        let current_dot = self.current_dot().cloned();
        let active_output_predicates = self.active_output_predicates.clone();
        let mut state = HelperValuesWalkState {
            local_bindings: &mut *self.local_bindings,
            local_default_paths: &mut *self.local_default_paths,
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

    fn record_guard_paths(&mut self, guard_paths: &BTreeSet<String>) {
        if !Self::SEMANTICS.record_guard_paths {
            return;
        }
        for path in guard_paths {
            self.analysis.add_guard_path(path.clone());
        }
    }

    fn merge_outcome_maps(&mut self, outcomes: Vec<HelperValueSnapshot>) {
        let mut iter = outcomes.into_iter();
        let Some(first) = iter.next() else {
            return;
        };
        let mut local_bindings = first.local_bindings;
        let mut local_default_paths = first.local_default_paths;
        let mut local_output_meta = first.local_output_meta;
        for outcome in iter {
            local_bindings = merge_fragment_locals(local_bindings, outcome.local_bindings);
            local_default_paths =
                merge_local_default_paths(local_default_paths, outcome.local_default_paths);
            local_output_meta =
                merge_helper_output_meta_maps(local_output_meta, outcome.local_output_meta);
        }
        *self.local_bindings = local_bindings;
        *self.local_default_paths = local_default_paths;
        *self.local_output_meta = local_output_meta;
    }

    fn promote_outcome_maps(&mut self, outcome: HelperValueSnapshot) {
        *self.local_bindings = outcome.local_bindings;
        *self.local_default_paths = outcome.local_default_paths;
        *self.local_output_meta = outcome.local_output_meta;
    }
}

impl ContractUseSink for HelperValueRuntime<'_, '_> {
    fn emit_contract_use(&mut self, _source_expr: String, _path: YamlPath, _kind: ValueKind) {}

    fn emit_contract_use_with_extra_guards(
        &mut self,
        _source_expr: String,
        _path: YamlPath,
        _kind: ValueKind,
        _extra_guards: &[crate::Guard],
    ) {
    }
}

impl NodeActionEffectSink for HelperValueRuntime<'_, '_> {
    fn apply_get_binding(&mut self, _plan: GetBindingPlan) {}

    fn declare_fragment_value(&mut self, _variable: String, _binding: Option<AbstractValue>) {}

    fn assign_fragment_value(&mut self, _variable: String, _binding: Option<AbstractValue>) {}

    fn refresh_default_paths(
        &mut self,
        _variable: &str,
        _rhs_expr: &helm_schema_ast::TemplateExpr,
    ) {
    }

    fn refresh_helper_output_meta(
        &mut self,
        _variable: String,
        _rhs_expr: &helm_schema_ast::TemplateExpr,
    ) {
    }

    fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        if !predicate.is_trivial() {
            self.active_output_predicates.insert(predicate);
        }
    }

    fn push_dot_binding(&mut self, binding: Option<AbstractValue>) {
        self.dot_stack
            .push(binding.map(|binding| binding.to_context_value()));
    }

    fn insert_range_domain(&mut self, _variable: String, _literals: Vec<String>) {}
}

impl NodeEvalRuntime for HelperValueRuntime<'_, '_> {
    type ScopeSnapshot = HelperValueSnapshot;

    fn source(&self) -> &str {
        self.source
    }

    fn enter_node(&mut self, _node: tree_sitter::Node<'_>) {}

    fn document_path_for_node(&self, _node: tree_sitter::Node<'_>) -> YamlPath {
        YamlPath(Vec::new())
    }

    fn scope_snapshot(&self) -> Self::ScopeSnapshot {
        HelperValueSnapshot {
            local_bindings: self.local_bindings.clone(),
            local_default_paths: self.local_default_paths.clone(),
            local_output_meta: self.local_output_meta.clone(),
            dot_stack_len: self.dot_stack.len(),
            active_output_predicates: self.active_output_predicates.clone(),
        }
    }

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot) {
        *self.local_bindings = snapshot.local_bindings;
        *self.local_default_paths = snapshot.local_default_paths;
        *self.local_output_meta = snapshot.local_output_meta;
        self.dot_stack.truncate(snapshot.dot_stack_len);
        self.active_output_predicates = snapshot.active_output_predicates;
    }

    fn enter_local_scope(&mut self) {}

    fn exit_local_scope(&mut self) {}

    fn join_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        self.dot_stack.truncate(entry.dot_stack_len);
        self.active_output_predicates = entry.active_output_predicates.clone();
        self.merge_outcome_maps(outcomes);
    }

    fn join_range_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        self.dot_stack.truncate(entry.dot_stack_len);
        self.active_output_predicates = entry.active_output_predicates.clone();
        if self
            .range_frames
            .pop()
            .is_some_and(|frame| frame.is_definitely_nonempty())
        {
            if let Some(body_outcome) = outcomes.into_iter().next() {
                self.promote_outcome_maps(body_outcome);
            }
            return;
        }

        self.merge_outcome_maps(outcomes);
    }

    fn range_iteration_count(&self) -> usize {
        self.range_frames
            .last()
            .map(RangeFrame::iteration_count)
            .unwrap_or(1)
    }

    fn enter_range_iteration(&mut self, index: usize) {
        let Some(iteration) = self
            .range_frames
            .last()
            .and_then(|frame| frame.iteration(index))
        else {
            return;
        };
        if let Some((variable, binding)) = iteration.variable_binding {
            self.local_bindings.insert(variable, binding);
        }
        self.dot_stack.push(iteration.helper_dot_binding);
    }

    fn exit_range_iteration(&mut self, _index: usize) {
        if self
            .range_frames
            .last()
            .is_some_and(RangeFrame::has_exact_iterations)
        {
            self.dot_stack.pop();
        }
    }

    fn enter_no_output(&mut self) {
        self.no_output_depth += 1;
    }

    fn exit_no_output(&mut self) {
        self.no_output_depth = self.no_output_depth.saturating_sub(1);
    }

    fn handle_output_node(
        &mut self,
        _node: tree_sitter::Node<'_>,
        exprs: &[helm_schema_ast::TemplateExpr],
    ) {
        if self.no_output_depth > 0 {
            return;
        }
        self.collect_expression(exprs);
    }

    fn apply_assignment_side_effects(&mut self, exprs: &[helm_schema_ast::TemplateExpr]) -> bool {
        self.collect_expression(exprs);
        true
    }

    fn plan_assignment_action(
        &self,
        _exprs: &[helm_schema_ast::TemplateExpr],
    ) -> AssignmentActionPlan {
        AssignmentActionPlan {
            get_binding: None,
            local_assignment: None,
        }
    }

    fn plan_if_condition(&mut self, header: &TemplateHeader) -> ConditionActionPlan {
        let current_dot = self.current_dot().cloned();
        let plan = helper_if_condition_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            self.local_bindings,
            self.context,
            self.seen,
            Self::SEMANTICS,
        );
        self.record_guard_paths(&plan.guard_paths);
        plan.action
    }

    fn plan_with_condition(&mut self, header: &TemplateHeader) -> ConditionActionPlan {
        let current_dot = self.current_dot().cloned();
        let current_dot_fragment = current_dot.as_ref().map(AbstractValue::to_context_value);
        let plan = helper_with_condition_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            current_dot_fragment.as_ref(),
            self.local_bindings,
            self.context,
            self.seen,
            Self::SEMANTICS,
        );
        self.record_guard_paths(&plan.guard_paths);
        plan.action
    }

    fn plan_range_action(
        &mut self,
        _node: tree_sitter::Node<'_>,
        header: Option<&TemplateHeader>,
        _current_path: &YamlPath,
    ) -> RangeActionPlan {
        let current_dot_fragment = self.current_dot_fragment();
        let mut seen_range = self.seen.clone();
        let plan = helper_range_runtime_plan(
            header,
            self.bindings,
            self.current_dot(),
            current_dot_fragment.as_ref(),
            self.local_bindings,
            self.context,
            &mut seen_range,
            Self::SEMANTICS,
        );
        self.record_guard_paths(&plan.guard_paths);
        self.active_output_predicates
            .extend(plan.guard_paths.iter().cloned().map(Predicate::truthy_path));
        if let Some((variable, binding)) = plan.non_exact_variable_binding {
            self.local_bindings.insert(variable, binding);
        }
        self.range_frames.push(plan.frame);
        plan.action
    }
}
