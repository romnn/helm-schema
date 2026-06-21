use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateHeader;

use crate::abstract_value::AbstractValue;
use crate::assignment_action_plan::AssignmentActionPlan;
use crate::bound_value_analysis::GetBindingPlan;
use crate::condition_action_plan::ConditionActionPlan;
use crate::contract_sink::ContractUseSink;
use crate::document_projection::{DocumentTracker, collect_document_site_context};
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::fragment_range_scope::{
    range_body_emits_sequence_item_from_source, range_body_renders_mapping_entries_from_ast,
    range_has_destructured_variable_definition,
};
use crate::helper_range_frame::RangeFrame;
use crate::helper_range_plan::{HelperRangeIteration, NonExactRangeVariableBinding};
use crate::helper_runtime_plan::{
    HelperRangeDotSource, HelperRuntimeSemantics, helper_if_condition_plan,
    helper_range_runtime_plan, helper_with_condition_plan,
};
use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta};
use crate::helper_summary_mutation::merge_local_default_paths;
use crate::helper_walk_state::FragmentOutputWalkState;
use crate::node_eval::{NodeActionEffectSink, NodeEvalRuntime, eval_template_body};
use crate::predicate::Predicate;
use crate::range_action_plan::RangeActionPlan;
use crate::{ValueKind, YamlPath};

mod expression_output;

pub(crate) use expression_output::collect_bound_fragment_output_uses_from_exprs;

const FRAGMENT_SEMANTICS: HelperRuntimeSemantics = HelperRuntimeSemantics {
    apply_alternative_predicate: false,
    non_exact_range_variable_binding: NonExactRangeVariableBinding::Skip,
    range_dot_source: HelperRangeDotSource::FragmentValue,
};

#[tracing::instrument(skip_all)]
pub(crate) fn collect_bound_fragment_output_uses_from_tree(
    tree: &tree_sitter::Tree,
    source: &str,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    current_dot_fragment: Option<&AbstractValue>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let mut document_tracker = DocumentTracker::new(source, state.context.defines);
    document_tracker.reset_for_tree(tree);
    let mut runtime = FragmentOutputUseRuntime {
        source,
        bindings,
        dot_stack: vec![current_dot.cloned()],
        dot_fragment_stack: vec![current_dot_fragment.cloned()],
        active_output_predicates: BTreeSet::new(),
        local_bindings: state.local_bindings,
        local_default_paths: state.local_default_paths,
        context: state.context,
        seen: state.seen,
        outputs: state.outputs,
        document_tracker,
        range_frames: Vec::new(),
        no_output_depth: 0,
    };
    eval_template_body(&mut runtime, tree.root_node());
}

struct FragmentOutputUseRuntime<'context, 'state> {
    source: &'state str,
    bindings: &'state HashMap<String, AbstractValue>,
    dot_stack: Vec<Option<AbstractValue>>,
    dot_fragment_stack: Vec<Option<AbstractValue>>,
    active_output_predicates: BTreeSet<Predicate>,
    local_bindings: &'state mut HashMap<String, AbstractValue>,
    local_default_paths: &'state mut HashMap<String, BTreeSet<String>>,
    context: FragmentEvalContext<'context>,
    seen: &'state mut HashSet<String>,
    outputs: &'state mut Vec<HelperFragmentOutputUse>,
    document_tracker: DocumentTracker<'state>,
    range_frames: Vec<RangeFrame<HelperRangeIteration>>,
    no_output_depth: usize,
}

#[derive(Clone)]
struct FragmentOutputUseSnapshot {
    local_bindings: HashMap<String, AbstractValue>,
    local_default_paths: HashMap<String, BTreeSet<String>>,
    dot_stack_len: usize,
    dot_fragment_stack_len: usize,
    active_output_predicates: BTreeSet<Predicate>,
}

impl FragmentOutputUseRuntime<'_, '_> {
    fn current_dot(&self) -> Option<&AbstractValue> {
        self.dot_stack.last().and_then(Option::as_ref)
    }

    fn current_dot_fragment(&self) -> Option<&AbstractValue> {
        self.dot_fragment_stack.last().and_then(Option::as_ref)
    }

    fn collect_expression(
        &mut self,
        exprs: &[helm_schema_ast::TemplateExpr],
        relative_path: &YamlPath,
        kind: ValueKind,
    ) {
        let current_dot = self.current_dot().cloned();
        let current_dot_fragment = self.current_dot_fragment().cloned();
        let active_output_predicates = self.active_output_predicates.clone();
        let mut state = FragmentOutputWalkState {
            local_bindings: &mut *self.local_bindings,
            local_default_paths: &mut *self.local_default_paths,
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

    fn merge_outcome_maps(&mut self, outcomes: Vec<FragmentOutputUseSnapshot>) {
        let mut iter = outcomes.into_iter();
        let Some(first) = iter.next() else {
            return;
        };
        let mut local_bindings = first.local_bindings;
        let mut local_default_paths = first.local_default_paths;
        for outcome in iter {
            local_bindings = crate::fragment_assignment::merge_fragment_locals(
                local_bindings,
                outcome.local_bindings,
            );
            local_default_paths =
                merge_local_default_paths(local_default_paths, outcome.local_default_paths);
        }
        *self.local_bindings = local_bindings;
        *self.local_default_paths = local_default_paths;
    }

    fn promote_outcome_maps(&mut self, outcome: FragmentOutputUseSnapshot) {
        *self.local_bindings = outcome.local_bindings;
        *self.local_default_paths = outcome.local_default_paths;
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

        let meta = HelperOutputMeta::with_predicates(&self.active_output_predicates, false);
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

impl ContractUseSink for FragmentOutputUseRuntime<'_, '_> {
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

impl NodeActionEffectSink for FragmentOutputUseRuntime<'_, '_> {
    fn apply_get_binding(&mut self, _plan: GetBindingPlan) {}

    fn declare_fragment_value(&mut self, variable: String, binding: Option<AbstractValue>) {
        if let Some(binding) = binding {
            self.local_bindings.insert(variable, binding);
        } else {
            self.local_bindings.remove(&variable);
        }
    }

    fn assign_fragment_value(&mut self, variable: String, binding: Option<AbstractValue>) {
        self.declare_fragment_value(variable, binding);
    }

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
        self.dot_fragment_stack.push(binding.clone());
        self.dot_stack
            .push(binding.map(|binding| binding.to_context_value()));
    }

    fn insert_range_domain(&mut self, _variable: String, _literals: Vec<String>) {}
}

impl NodeEvalRuntime for FragmentOutputUseRuntime<'_, '_> {
    type ScopeSnapshot = FragmentOutputUseSnapshot;

    fn source(&self) -> &str {
        self.source
    }

    fn enter_node(&mut self, _node: tree_sitter::Node<'_>) {}

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
            local_bindings: self.local_bindings.clone(),
            local_default_paths: self.local_default_paths.clone(),
            dot_stack_len: self.dot_stack.len(),
            dot_fragment_stack_len: self.dot_fragment_stack.len(),
            active_output_predicates: self.active_output_predicates.clone(),
        }
    }

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot) {
        *self.local_bindings = snapshot.local_bindings;
        *self.local_default_paths = snapshot.local_default_paths;
        self.dot_stack.truncate(snapshot.dot_stack_len);
        self.dot_fragment_stack
            .truncate(snapshot.dot_fragment_stack_len);
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
        self.dot_fragment_stack
            .truncate(entry.dot_fragment_stack_len);
        self.active_output_predicates = entry.active_output_predicates.clone();
        self.merge_outcome_maps(outcomes);
    }

    fn join_range_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        self.dot_stack.truncate(entry.dot_stack_len);
        self.dot_fragment_stack
            .truncate(entry.dot_fragment_stack_len);
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
        self.dot_fragment_stack.push(iteration.fragment_dot_binding);
    }

    fn exit_range_iteration(&mut self, _index: usize) {
        if self
            .range_frames
            .last()
            .is_some_and(RangeFrame::has_exact_iterations)
        {
            self.dot_stack.pop();
            self.dot_fragment_stack.pop();
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
        node: tree_sitter::Node<'_>,
        exprs: &[helm_schema_ast::TemplateExpr],
    ) {
        if self.no_output_depth > 0 {
            return;
        }
        let site_context =
            collect_document_site_context(self.source, &self.document_tracker, node, exprs);
        if site_context.in_mapping_key {
            return;
        }
        let kind = if site_context.kind == ValueKind::Scalar
            && !site_context.entire_scalar_value
            && !site_context.path.0.is_empty()
        {
            ValueKind::PartialScalar
        } else {
            site_context.kind
        };
        self.collect_expression(exprs, &site_context.path, kind);
    }

    fn apply_assignment_side_effects(&mut self, exprs: &[helm_schema_ast::TemplateExpr]) -> bool {
        let mut seen_set = HashSet::new();
        if crate::fragment_assignment::apply_local_set_mutations_from_exprs(
            exprs,
            self.local_bindings,
            self.current_dot_fragment().cloned().as_ref(),
            self.context,
            &mut seen_set,
        ) {
            return true;
        }

        self.collect_expression(exprs, &YamlPath(Vec::new()), ValueKind::Scalar);
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
        helper_if_condition_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            self.local_bindings,
            self.context,
            self.seen,
            FRAGMENT_SEMANTICS,
        )
        .action
    }

    fn plan_with_condition(&mut self, header: &TemplateHeader) -> ConditionActionPlan {
        let current_dot = self.current_dot().cloned();
        let current_dot_fragment = self.current_dot_fragment().cloned();
        helper_with_condition_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            current_dot_fragment.as_ref(),
            self.local_bindings,
            self.context,
            self.seen,
            FRAGMENT_SEMANTICS,
        )
        .action
    }

    fn plan_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        header: Option<&TemplateHeader>,
        current_path: &YamlPath,
    ) -> RangeActionPlan {
        let current_dot = self.current_dot().cloned();
        let current_dot_fragment = self.current_dot_fragment().cloned();
        let plan = helper_range_runtime_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            current_dot_fragment.as_ref(),
            self.local_bindings,
            self.context,
            self.seen,
            FRAGMENT_SEMANTICS,
        );
        self.active_output_predicates
            .extend(plan.guard_paths.iter().cloned().map(Predicate::truthy_path));
        self.collect_destructured_range_fragment_outputs(
            node,
            plan.range_fragment_value.as_ref(),
            current_path,
        );

        self.range_frames.push(plan.frame);
        plan.action
    }
}
