use std::collections::{HashMap, HashSet};

use helm_schema_ast::{
    AttributionIndex, ControlSite, TemplateExpr, range_body_emits_sequence_item_from_source,
    range_body_renders_mapping_entries_from_ast, range_has_destructured_variable_definition,
};

use crate::abstract_value::AbstractValue;
use crate::fragment_expr_eval::{
    FragmentEvalContext, FragmentLocalFacts, context_value_from_outer_expr,
    helper_result_from_expr_with_fragment_locals, values_for_helper_arg_with_fragment_locals,
};
use crate::helper_fragment_output_uses::collect_bound_fragment_output_uses_from_exprs;
use crate::helper_runtime_plan::{
    HelperConditionPlan, HelperRangeRuntimePlan, helper_if_condition_plan,
    helper_range_runtime_plan, helper_with_condition_plan,
};
use crate::helper_summary::HelperSummary;
use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta};
use crate::helper_walk_state::{
    HelperRangeJoinBehavior, HelperRuntimeControlSnapshot, HelperRuntimeControlState,
};
use crate::node_eval::{
    NodeActionEffectSink, NodeEvalRuntime, eval_template_body, push_predicate_contract_guards,
};
use crate::symbolic_local_state::SymbolicLocalState;
use crate::{ValueKind, YamlPath};
use helm_schema_core::Predicate;

pub(crate) struct BoundHelperCallResolution {
    pub(crate) bindings: HashMap<String, AbstractValue>,
    pub(crate) helper_body_dot: Option<AbstractValue>,
    pub(crate) helper_fragment_dot: Option<AbstractValue>,
}

pub(crate) struct ResolveBoundHelperCallParams<'a, 'context> {
    pub(crate) helper_name: &'a str,
    pub(crate) arg: Option<&'a TemplateExpr>,
    pub(crate) outer_bindings: Option<&'a HashMap<String, AbstractValue>>,
    pub(crate) current_dot: Option<&'a AbstractValue>,
    pub(crate) fragment_locals: &'a HashMap<String, AbstractValue>,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'a HashSet<String>,
}

pub(crate) fn resolve_bound_helper_call(
    params: ResolveBoundHelperCallParams<'_, '_>,
) -> BoundHelperCallResolution {
    let mut binding_seen = params.seen.clone();
    let arg_resolution = values_for_helper_arg_with_fragment_locals(
        params.arg,
        params.outer_bindings,
        params.current_dot,
        params.fragment_locals,
        params.context,
        &mut binding_seen,
    );
    let mut bindings = arg_resolution.bindings;

    // The binding resolution already evaluated the whole arg unless the arg
    // was a dot/root or merge call; only those shapes still need their own
    // helper-dot evaluation here.
    let mut helper_body_dot = arg_resolution
        .value
        .or_else(|| {
            let mut dot_seen = params.seen.clone();
            params.arg.and_then(|expr| {
                helper_result_from_expr_with_fragment_locals(
                    expr,
                    FragmentLocalFacts::bindings_only(params.fragment_locals),
                    params.outer_bindings,
                    params.current_dot,
                    params.context,
                    &mut dot_seen,
                )
                .value
            })
        })
        .or_else(|| params.current_dot.cloned());

    let mut helper_fragment_dot = params.arg.and_then(|expr| {
        context_value_from_outer_expr(
            expr,
            Some(params.fragment_locals),
            params.outer_bindings,
            params.current_dot,
        )
    });

    if helper_uses_large_config_arg(params.helper_name) {
        if let Some(binding) = bindings.remove("config") {
            bindings.insert("config".to_string(), abstract_config_binding(binding));
        }
        helper_body_dot = helper_body_dot.map(abstract_config_entry_in_binding);
        helper_fragment_dot = helper_fragment_dot.map(abstract_config_entry_in_binding);
    }

    BoundHelperCallResolution {
        bindings,
        helper_body_dot,
        helper_fragment_dot,
    }
}

fn helper_uses_large_config_arg(name: &str) -> bool {
    name.starts_with("opentelemetry-collector.apply")
}

fn abstract_config_binding(binding: AbstractValue) -> AbstractValue {
    let paths = binding.paths();
    if paths.is_empty() {
        AbstractValue::Top
    } else {
        AbstractValue::path_choices(paths).unwrap_or(AbstractValue::Top)
    }
}

fn abstract_config_entry_in_binding(binding: AbstractValue) -> AbstractValue {
    match binding {
        AbstractValue::Dict(mut entries) => {
            if let Some(config) = entries.remove("config") {
                entries.insert("config".to_string(), abstract_config_binding(config));
            }
            AbstractValue::Dict(entries)
        }
        other => other,
    }
}

#[tracing::instrument(skip_all, fields(helper = name))]
pub(crate) fn interpret_bound_helper_body(
    name: &str,
    resolution: &BoundHelperCallResolution,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HelperSummary {
    let Some(body) = context.analysis_db.parsed_helper_body(name) else {
        return HelperSummary::default();
    };
    let Some(attribution) = context.analysis_db.helper_attribution(name) else {
        return HelperSummary::default();
    };
    let mut analysis = HelperSummary::default();
    let mut locals = SymbolicLocalState::default();
    let mut fragment_output_uses = Vec::new();
    let mut output_seen = seen.clone();
    let mut runtime = HelperAnalysisRuntime {
        source: body.source,
        bindings: &resolution.bindings,
        control: HelperRuntimeControlState::for_fragment(
            resolution.helper_body_dot.as_ref(),
            resolution.helper_fragment_dot.as_ref(),
        ),
        locals: &mut locals,
        context,
        seen: &mut output_seen,
        analysis: &mut analysis,
        outputs: &mut fragment_output_uses,
        attribution,
    };
    eval_template_body(&mut runtime, body.tree.root_node());
    // Rendered rows land in `fragment_output_uses` during the walk (the
    // summary only accumulates dependency rows there), so scalar-vs-structured
    // reconciliation happens on the local vec before the rows join the summary.
    let structured_sources: std::collections::BTreeSet<String> = fragment_output_uses
        .iter()
        .filter(|output| output.is_structured_output())
        .map(|output| output.source_expr.clone())
        .collect();
    fragment_output_uses.retain(|output| {
        output.is_structured_output() || !structured_sources.contains(&output.source_expr)
    });
    analysis.add_output_uses(fragment_output_uses);
    analysis.add_provenance(body.provenance(name));
    analysis
}

struct HelperAnalysisRuntime<'context, 'state> {
    source: &'state str,
    bindings: &'state HashMap<String, AbstractValue>,
    control: HelperRuntimeControlState,
    locals: &'state mut SymbolicLocalState,
    context: FragmentEvalContext<'context>,
    seen: &'state mut HashSet<String>,
    analysis: &'state mut HelperSummary,
    outputs: &'state mut Vec<HelperFragmentOutputUse>,
    attribution: AttributionIndex,
}

#[derive(Clone)]
struct HelperAnalysisSnapshot {
    locals: SymbolicLocalState,
    control: HelperRuntimeControlSnapshot,
}

impl<'context: 'state, 'state> HelperAnalysisRuntime<'context, 'state> {
    fn current_helper_dot(&self) -> Option<&AbstractValue> {
        self.control.current_helper_dot()
    }

    fn current_fragment_dot(&self) -> Option<&AbstractValue> {
        self.control.current_fragment_dot()
    }

    fn collect_fragment_expression(
        &mut self,
        exprs: &[helm_schema_ast::TemplateExpr],
        relative_path: &YamlPath,
        kind: ValueKind,
    ) {
        let current_dot = self.current_helper_dot().cloned();
        let current_dot_fragment = self.current_fragment_dot().cloned();
        let active_output_predicates = if kind == ValueKind::Fragment || !relative_path.0.is_empty()
        {
            self.control.active_fragment_predicates().clone()
        } else {
            self.control.active_output_predicates().clone()
        };
        let active_source_relations = self.control.active_source_relations().clone();
        let mut state = crate::helper_walk_state::FragmentOutputWalkState {
            locals: &mut *self.locals,
            context: self.context,
            seen: self.seen,
            analysis: self.analysis,
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
            &active_source_relations,
            &mut state,
        );
    }

    fn collect_dependency_expression(&mut self, exprs: &[helm_schema_ast::TemplateExpr]) {
        let output_len = self.outputs.len();
        self.collect_fragment_expression(exprs, &YamlPath(Vec::new()), ValueKind::Scalar);
        let dependency_outputs = self.outputs.split_off(output_len);
        for output in dependency_outputs {
            self.analysis
                .merge_dependency_meta(output.source_expr, output.meta);
        }
    }

    fn merge_outcomes(&mut self, outcomes: Vec<HelperAnalysisSnapshot>) {
        let mut iter = outcomes.into_iter();
        let Some(first) = iter.next() else {
            return;
        };
        let mut locals = first.locals;
        for outcome in iter {
            locals = locals.merge_helper_outcome(outcome.locals);
        }
        *self.locals = locals;
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
            HelperOutputMeta::with_predicates(self.control.active_fragment_predicates(), false);
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

impl NodeActionEffectSink for HelperAnalysisRuntime<'_, '_> {
    fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        self.control.push_predicate_if_absent(predicate);
    }

    fn push_dot_binding(&mut self, binding: Option<AbstractValue>) {
        self.control.push_effect_dot_binding(binding);
    }
}

impl<'context: 'state, 'state> NodeEvalRuntime for HelperAnalysisRuntime<'context, 'state> {
    type ScopeSnapshot = HelperAnalysisSnapshot;
    type ConditionPlan = HelperConditionPlan;
    type RangePlan = HelperRangeRuntimePlan;

    fn source(&self) -> &str {
        self.source
    }

    fn document_control_site_for_node(&self, node: tree_sitter::Node<'_>) -> ControlSite {
        self.attribution
            .control_site_for_node(node)
            .unwrap_or_default()
    }

    fn scope_snapshot(&self) -> Self::ScopeSnapshot {
        HelperAnalysisSnapshot {
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
        self.control.restore(&entry.control);
        self.merge_outcomes(outcomes);
    }

    fn join_range_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        let join = self.control.prepare_range_join(&entry.control);

        let first_body_outcome = outcomes.first().cloned();
        if join == HelperRangeJoinBehavior::PromoteBodyOutcome {
            if let Some(outcome) = first_body_outcome {
                *self.locals = outcome.locals;
            }
        } else {
            self.merge_outcomes(outcomes);
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
        let output_slot = self
            .attribution
            .output_slot_for_node(node)
            .unwrap_or_default();
        if output_slot.suppresses_fragment_output() {
            self.collect_dependency_expression(exprs);
            return;
        }
        self.collect_fragment_expression(exprs, &output_slot.path, output_slot.direct_value_kind());
    }

    fn observe_assignment_exprs(&mut self, exprs: &[helm_schema_ast::TemplateExpr]) {
        let mut seen_set = HashSet::new();
        let current_dot_fragment = self.current_fragment_dot().cloned();
        if crate::fragment_assignment::apply_local_set_mutations_from_exprs(
            exprs,
            &mut self.locals.fragment_values,
            current_dot_fragment.as_ref(),
            self.context,
            &mut seen_set,
        ) {
            return;
        }
        self.collect_fragment_expression(exprs, &YamlPath(Vec::new()), ValueKind::Scalar);
    }

    fn plan_if_condition(
        &mut self,
        header: &helm_schema_ast::TemplateHeader,
    ) -> Self::ConditionPlan {
        let current_dot = self.current_helper_dot().cloned();
        helper_if_condition_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            &self.locals.fragment_values,
            &self.locals.default_paths,
            &self.locals.output_meta,
            self.context,
            self.seen,
        )
    }

    fn activate_if_condition(&mut self, plan: &Self::ConditionPlan) {
        for path in &plan.guard_paths {
            self.analysis.add_guard_path(path.clone());
        }
        push_predicate_contract_guards(self, &plan.predicate);
        self.control
            .extend_source_relations(plan.source_relations.iter().cloned());
    }

    fn plan_with_condition(
        &mut self,
        header: &helm_schema_ast::TemplateHeader,
    ) -> Self::ConditionPlan {
        let current_dot = self.current_helper_dot().cloned();
        let fragment_current_dot = self.current_fragment_dot().cloned();
        helper_with_condition_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            fragment_current_dot.as_ref(),
            &self.locals.fragment_values,
            &self.locals.default_paths,
            &self.locals.output_meta,
            self.context,
            self.seen,
        )
    }

    fn activate_with_condition(&mut self, plan: &Self::ConditionPlan) {
        for path in &plan.guard_paths {
            self.analysis.add_guard_path(path.clone());
        }
        push_predicate_contract_guards(self, &plan.predicate);
        self.control
            .extend_source_relations(plan.source_relations.iter().cloned());
        self.control
            .push_effect_dot_binding(plan.dot_binding.clone());
    }

    fn activate_condition_alternative(&mut self, plan: &Self::ConditionPlan) {
        self.control
            .push_value_predicate_if_absent(plan.predicate.negated());
    }

    fn plan_range_action(
        &mut self,
        _node: tree_sitter::Node<'_>,
        header: Option<&helm_schema_ast::TemplateHeader>,
        _current_path: &YamlPath,
        _mapping_entry_path: Option<&YamlPath>,
    ) -> Self::RangePlan {
        let current_dot = self.current_helper_dot().cloned();
        let fragment_current_dot = self.current_fragment_dot().cloned();
        helper_range_runtime_plan(
            header,
            self.bindings,
            current_dot.as_ref(),
            fragment_current_dot.as_ref(),
            &self.locals.fragment_values,
            self.context,
            self.seen,
        )
    }

    fn activate_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        plan: &Self::RangePlan,
        current_path: &YamlPath,
    ) {
        plan.activate(&mut self.control, self.locals);
        for path in &plan.guard_paths {
            self.analysis.add_guard_path(path.clone());
        }
        self.collect_destructured_range_fragment_outputs(
            node,
            plan.range_fragment_value.as_ref(),
            current_path,
        );
    }
}

#[cfg(test)]
#[path = "tests/helper_body_analysis.rs"]
mod tests;
