use std::collections::{BTreeMap, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::condition_action_plan::ConditionActionPlan;
use crate::document_projection::{DocumentTracker, collect_document_site_context};
use crate::fragment_expr_eval::{
    FragmentEvalContext, context_value_from_outer_expr,
    helper_result_from_expr_with_fragment_locals, values_for_helper_arg_with_fragment_locals,
};
use crate::fragment_range_scope::{
    range_body_emits_sequence_item_from_source, range_body_renders_mapping_entries_from_ast,
    range_has_destructured_variable_definition,
};
use crate::helper_fragment_output_uses::collect_bound_fragment_output_uses_from_exprs;
use crate::helper_range_plan::NonExactRangeVariableBinding;
use crate::helper_runtime_plan::{
    HelperConditionPlan, HelperRangeDotSource, HelperRangeRuntimePlan, HelperRuntimeSemantics,
    helper_if_condition_plan, helper_range_runtime_plan, helper_with_condition_plan,
};
use crate::helper_summary::HelperSummary;
use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta};
use crate::helper_value_expression::collect_helper_value_expression_from_exprs;
use crate::helper_walk_state::{
    HelperRangeJoinBehavior, HelperRuntimeControlSnapshot, HelperRuntimeControlState,
    HelperRuntimeLocals, HelperValuesWalkState,
};
use crate::node_eval::{
    AssignmentObservation, NodeActionEffectSink, NodeEvalRuntime, eval_template_body,
};
use crate::predicate::Predicate;
use crate::range_action_plan::RangeActionPlan;
use crate::{ContractProvenance, Guard, SourceSpan, ValueKind, YamlPath};

const VALUE_SEMANTICS: HelperRuntimeSemantics = HelperRuntimeSemantics {
    apply_alternative_predicate: true,
    non_exact_range_variable_binding: NonExactRangeVariableBinding::Bind,
    range_dot_source: HelperRangeDotSource::HelperValue,
};

const FRAGMENT_SEMANTICS: HelperRuntimeSemantics = HelperRuntimeSemantics {
    apply_alternative_predicate: false,
    non_exact_range_variable_binding: NonExactRangeVariableBinding::Skip,
    range_dot_source: HelperRangeDotSource::FragmentValue,
};

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
    let mut bindings = values_for_helper_arg_with_fragment_locals(
        params.arg,
        params.outer_bindings,
        params.current_dot,
        params.fragment_locals,
        params.context,
        &mut binding_seen,
    );

    let mut dot_seen = params.seen.clone();
    let mut helper_body_dot = params
        .arg
        .and_then(|expr| {
            helper_result_from_expr_with_fragment_locals(
                expr,
                params.fragment_locals,
                params.outer_bindings,
                params.current_dot,
                params.context,
                &mut dot_seen,
            )
            .value
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
        AbstractValue::PathSet(paths)
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

struct ResolvedHelperBody<'a> {
    source: &'a str,
    tree: tree_sitter::Tree,
    provenance: Option<ContractProvenance>,
}

impl<'a> ResolvedHelperBody<'a> {
    fn resolve(name: &str, context: FragmentEvalContext<'a>) -> Option<Self> {
        let source = context.define_bodies.source(name)?;
        let tree = context.define_bodies.tree(name)?;
        let provenance = context
            .define_bodies
            .source_path(name)
            .zip(context.define_bodies.body_offset(name))
            .map(|(source_path, body_offset)| {
                ContractProvenance::new(
                    source_path,
                    SourceSpan::new(body_offset, body_offset + source.len()),
                    vec![name.to_string()],
                )
            });
        Some(Self {
            source,
            tree,
            provenance,
        })
    }

    fn attach_provenance(&self, analysis: &mut HelperSummary) {
        let Some(provenance) = self.provenance.clone() else {
            return;
        };
        analysis.add_provenance_to_outputs(provenance.clone());
        analysis.add_provenance_to_fragment_outputs(provenance.clone());
        analysis.add_provenance_to_dependencies(provenance);
    }
}

#[tracing::instrument(skip_all, fields(helper = name))]
pub(crate) fn interpret_bound_helper_body(
    name: &str,
    resolution: &BoundHelperCallResolution,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HelperSummary {
    let Some(body) = ResolvedHelperBody::resolve(name, context) else {
        return HelperSummary::default();
    };
    let mut analysis = HelperSummary::default();
    collect_helper_summary(&body, resolution, context, seen, &mut analysis);
    body.attach_provenance(&mut analysis);

    analysis
}

fn collect_helper_summary(
    body: &ResolvedHelperBody<'_>,
    resolution: &BoundHelperCallResolution,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    analysis: &mut HelperSummary,
) {
    let mut value_locals = HelperRuntimeLocals::default();
    let mut fragment_locals = HelperRuntimeLocals::default();
    let mut local_output_meta = HashMap::new();
    let mut fragment_output_uses = Vec::new();
    let mut value_seen = seen.clone();
    let mut fragment_seen = seen.clone();
    let mut document_tracker = DocumentTracker::new(body.source, context.defines);
    document_tracker.reset_for_tree(&body.tree);
    let mut runtime = HelperAnalysisRuntime {
        source: body.source,
        bindings: &resolution.bindings,
        value_control: HelperRuntimeControlState::for_value(resolution.helper_body_dot.as_ref()),
        fragment_control: HelperRuntimeControlState::for_fragment(
            resolution.helper_body_dot.as_ref(),
            resolution.helper_fragment_dot.as_ref(),
        ),
        value_locals: &mut value_locals,
        fragment_locals: &mut fragment_locals,
        local_output_meta: &mut local_output_meta,
        context,
        value_seen: &mut value_seen,
        fragment_seen: &mut fragment_seen,
        analysis,
        outputs: &mut fragment_output_uses,
        document_tracker,
    };
    eval_template_body(&mut runtime, body.tree.root_node());
    analysis.add_fragment_output_uses(fragment_output_uses);
}

struct HelperAnalysisRuntime<'context, 'state> {
    source: &'state str,
    bindings: &'state HashMap<String, AbstractValue>,
    value_control: HelperRuntimeControlState,
    fragment_control: HelperRuntimeControlState,
    value_locals: &'state mut HelperRuntimeLocals,
    fragment_locals: &'state mut HelperRuntimeLocals,
    local_output_meta: &'state mut HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    context: FragmentEvalContext<'context>,
    value_seen: &'state mut HashSet<String>,
    fragment_seen: &'state mut HashSet<String>,
    analysis: &'state mut HelperSummary,
    outputs: &'state mut Vec<HelperFragmentOutputUse>,
    document_tracker: DocumentTracker<'state>,
}

#[derive(Clone)]
struct HelperAnalysisSnapshot {
    value_locals: HelperRuntimeLocals,
    fragment_locals: HelperRuntimeLocals,
    local_output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    value_control: HelperRuntimeControlSnapshot,
    fragment_control: HelperRuntimeControlSnapshot,
}

struct HelperAnalysisConditionPlan {
    value: HelperConditionPlan,
    fragment: HelperConditionPlan,
}

struct HelperAnalysisRangePlan {
    value: HelperRangeRuntimePlan,
    fragment: HelperRangeRuntimePlan,
}

impl<'context: 'state, 'state> HelperAnalysisRuntime<'context, 'state> {
    fn current_value_dot(&self) -> Option<&AbstractValue> {
        self.value_control.current_helper_dot()
    }

    fn current_fragment_helper_dot(&self) -> Option<&AbstractValue> {
        self.fragment_control.current_helper_dot()
    }

    fn current_fragment_dot(&self) -> Option<&AbstractValue> {
        self.fragment_control.current_fragment_dot()
    }

    fn collect_value_expression(&mut self, exprs: &[helm_schema_ast::TemplateExpr]) {
        let current_dot = self.current_value_dot().cloned();
        let active_output_predicates = self.value_control.active_output_predicates().clone();
        let mut state = HelperValuesWalkState {
            locals: &mut *self.value_locals,
            local_output_meta: &mut *self.local_output_meta,
            context: self.context,
            seen: self.value_seen,
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

    fn collect_fragment_expression(
        &mut self,
        exprs: &[helm_schema_ast::TemplateExpr],
        relative_path: &YamlPath,
        kind: ValueKind,
    ) {
        let current_dot = self.current_fragment_helper_dot().cloned();
        let current_dot_fragment = self.current_fragment_dot().cloned();
        let active_output_predicates = self.fragment_control.active_output_predicates().clone();
        let mut state = crate::helper_walk_state::FragmentOutputWalkState {
            locals: &mut *self.fragment_locals,
            context: self.context,
            seen: self.fragment_seen,
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

    fn merge_value_outcomes(&mut self, outcomes: Vec<HelperAnalysisSnapshot>) {
        let mut iter = outcomes.into_iter();
        let Some(first) = iter.next() else {
            return;
        };
        let mut locals = first.value_locals;
        let mut local_output_meta = first.local_output_meta;
        for outcome in iter {
            locals = locals.merge(outcome.value_locals);
            local_output_meta =
                merge_helper_output_meta_maps(local_output_meta, outcome.local_output_meta);
        }
        *self.value_locals = locals;
        *self.local_output_meta = local_output_meta;
    }

    fn merge_fragment_outcomes(&mut self, outcomes: Vec<HelperAnalysisSnapshot>) {
        let mut iter = outcomes.into_iter();
        let Some(first) = iter.next() else {
            return;
        };
        let mut locals = first.fragment_locals;
        for outcome in iter {
            locals = locals.merge(outcome.fragment_locals);
        }
        *self.fragment_locals = locals;
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

        let meta = HelperOutputMeta::with_predicates(
            self.fragment_control.active_output_predicates(),
            false,
        );
        for source_expr in range_binding.fragment_source_paths() {
            self.outputs.push(HelperFragmentOutputUse::new(
                source_expr,
                current_path.clone(),
                ValueKind::Fragment,
                meta.clone(),
            ));
        }
    }

    fn activate_if_control(control: &mut HelperRuntimeControlState, plan: &ConditionActionPlan) {
        push_condition_predicates(control, plan);
    }

    fn activate_with_control(control: &mut HelperRuntimeControlState, plan: &ConditionActionPlan) {
        push_condition_predicates(control, plan);
        control.push_effect_dot_binding(plan.dot_binding.clone());
    }

    fn activate_alternative_control(
        control: &mut HelperRuntimeControlState,
        plan: &ConditionActionPlan,
    ) {
        if plan.apply_alternative_predicate {
            control.push_predicate_if_absent(plan.predicate.negated());
        }
    }

    fn activate_range_control(control: &mut HelperRuntimeControlState, plan: &RangeActionPlan) {
        if plan.has_header {
            for source_path in &plan.source_paths {
                control.push_predicate_if_absent(Predicate::from(Guard::Range {
                    path: source_path.clone(),
                }));
            }
        }
        if plan.apply_dot_binding {
            control.push_effect_dot_binding(plan.dot_binding.clone());
        }
    }
}

fn push_condition_predicates(control: &mut HelperRuntimeControlState, plan: &ConditionActionPlan) {
    let guards = plan.contract_guards();
    for guard in &guards {
        control.push_predicate_if_absent(Predicate::from(guard.clone()));
    }
    if guards.is_empty() {
        control.push_predicate_if_absent(plan.predicate.clone());
    }
}

fn merge_helper_output_meta_maps(
    mut base: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    other: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
) -> HashMap<String, BTreeMap<String, HelperOutputMeta>> {
    for (var, meta_by_path) in other {
        let entry = base.entry(var).or_default();
        for (path, meta) in meta_by_path {
            entry.entry(path).or_default().merge(meta);
        }
    }
    base
}

impl<'context: 'state, 'state> NodeActionEffectSink for HelperAnalysisRuntime<'context, 'state> {
    fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        self.value_control
            .push_predicate_if_absent(predicate.clone());
        self.fragment_control.push_predicate_if_absent(predicate);
    }

    fn push_dot_binding(&mut self, binding: Option<AbstractValue>) {
        self.value_control.push_effect_dot_binding(binding.clone());
        self.fragment_control.push_effect_dot_binding(binding);
    }
}

impl<'context: 'state, 'state> NodeEvalRuntime for HelperAnalysisRuntime<'context, 'state> {
    type ScopeSnapshot = HelperAnalysisSnapshot;
    type ConditionPlan = HelperAnalysisConditionPlan;
    type RangePlan = HelperAnalysisRangePlan;

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
        HelperAnalysisSnapshot {
            value_locals: self.value_locals.clone(),
            fragment_locals: self.fragment_locals.clone(),
            local_output_meta: self.local_output_meta.clone(),
            value_control: self.value_control.snapshot(),
            fragment_control: self.fragment_control.snapshot(),
        }
    }

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot) {
        *self.value_locals = snapshot.value_locals;
        *self.fragment_locals = snapshot.fragment_locals;
        *self.local_output_meta = snapshot.local_output_meta;
        self.value_control.restore(&snapshot.value_control);
        self.fragment_control.restore(&snapshot.fragment_control);
    }

    fn join_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        self.value_control.prepare_branch_join(&entry.value_control);
        self.fragment_control
            .prepare_branch_join(&entry.fragment_control);
        self.merge_value_outcomes(outcomes.clone());
        self.merge_fragment_outcomes(outcomes);
    }

    fn join_range_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        let value_join = self.value_control.prepare_range_join(&entry.value_control);
        let fragment_join = self
            .fragment_control
            .prepare_range_join(&entry.fragment_control);

        let first_body_outcome = outcomes.first().cloned();
        if value_join == HelperRangeJoinBehavior::PromoteBodyOutcome {
            if let Some(outcome) = first_body_outcome.clone() {
                *self.value_locals = outcome.value_locals;
                *self.local_output_meta = outcome.local_output_meta;
            }
        } else {
            self.merge_value_outcomes(outcomes.clone());
        }

        if fragment_join == HelperRangeJoinBehavior::PromoteBodyOutcome {
            if let Some(outcome) = first_body_outcome {
                *self.fragment_locals = outcome.fragment_locals;
            }
        } else {
            self.merge_fragment_outcomes(outcomes);
        }
    }

    fn range_iteration_count(&self) -> usize {
        let value_count = self.value_control.range_iteration_count();
        let fragment_count = self.fragment_control.range_iteration_count();
        debug_assert!(value_count == fragment_count);
        value_count
    }

    fn enter_range_iteration(&mut self, index: usize) {
        self.value_control
            .enter_range_iteration(index, self.value_locals);
        self.fragment_control
            .enter_range_iteration(index, self.fragment_locals);
    }

    fn exit_range_iteration(&mut self, _index: usize) {
        self.value_control.exit_range_iteration();
        self.fragment_control.exit_range_iteration();
    }

    fn enter_no_output(&mut self) {
        self.value_control.enter_no_output();
        self.fragment_control.enter_no_output();
    }

    fn exit_no_output(&mut self) {
        self.value_control.exit_no_output();
        self.fragment_control.exit_no_output();
    }

    fn handle_output_node(
        &mut self,
        node: tree_sitter::Node<'_>,
        exprs: &[helm_schema_ast::TemplateExpr],
    ) {
        if !self.value_control.suppresses_output() {
            self.collect_value_expression(exprs);
        }
        if self.fragment_control.suppresses_output() {
            return;
        }
        let site_context =
            collect_document_site_context(self.source, &self.document_tracker, node, exprs);
        let Some(site) = site_context.fragment_output_site() else {
            return;
        };
        self.collect_fragment_expression(exprs, &site.path, site.kind);
    }

    fn observe_assignment_exprs(
        &mut self,
        exprs: &[helm_schema_ast::TemplateExpr],
    ) -> AssignmentObservation {
        self.collect_value_expression(exprs);

        let mut seen_set = HashSet::new();
        let current_dot_fragment = self.current_fragment_dot().cloned();
        if crate::fragment_assignment::apply_local_set_mutations_from_exprs(
            exprs,
            &mut self.fragment_locals.bindings,
            current_dot_fragment.as_ref(),
            self.context,
            &mut seen_set,
        ) {
            return AssignmentObservation::LocalMutationApplied;
        }
        self.collect_fragment_expression(exprs, &YamlPath(Vec::new()), ValueKind::Scalar);
        AssignmentObservation::ExpressionObserved
    }

    fn plan_if_condition(
        &mut self,
        header: &helm_schema_ast::TemplateHeader,
    ) -> Self::ConditionPlan {
        let value_dot = self.current_value_dot().cloned();
        let fragment_dot = self.current_fragment_helper_dot().cloned();
        HelperAnalysisConditionPlan {
            value: helper_if_condition_plan(
                header,
                self.bindings,
                value_dot.as_ref(),
                &self.value_locals.bindings,
                &self.value_locals.default_paths,
                self.local_output_meta,
                self.context,
                self.value_seen,
                VALUE_SEMANTICS,
            ),
            fragment: helper_if_condition_plan(
                header,
                self.bindings,
                fragment_dot.as_ref(),
                &self.fragment_locals.bindings,
                &self.fragment_locals.default_paths,
                self.local_output_meta,
                self.context,
                self.fragment_seen,
                FRAGMENT_SEMANTICS,
            ),
        }
    }

    fn activate_if_condition(&mut self, plan: &Self::ConditionPlan) {
        plan.value.record_guard_paths_into(self.analysis);
        Self::activate_if_control(&mut self.value_control, &plan.value.action);
        Self::activate_if_control(&mut self.fragment_control, &plan.fragment.action);
    }

    fn plan_with_condition(
        &mut self,
        header: &helm_schema_ast::TemplateHeader,
    ) -> Self::ConditionPlan {
        let value_dot = self.current_value_dot().cloned();
        let value_fragment_dot = value_dot.as_ref().map(AbstractValue::to_context_value);
        let fragment_dot = self.current_fragment_helper_dot().cloned();
        let fragment_current_dot = self.current_fragment_dot().cloned();
        HelperAnalysisConditionPlan {
            value: helper_with_condition_plan(
                header,
                self.bindings,
                value_dot.as_ref(),
                value_fragment_dot.as_ref(),
                &self.value_locals.bindings,
                &self.value_locals.default_paths,
                self.local_output_meta,
                self.context,
                self.value_seen,
                VALUE_SEMANTICS,
            ),
            fragment: helper_with_condition_plan(
                header,
                self.bindings,
                fragment_dot.as_ref(),
                fragment_current_dot.as_ref(),
                &self.fragment_locals.bindings,
                &self.fragment_locals.default_paths,
                self.local_output_meta,
                self.context,
                self.fragment_seen,
                FRAGMENT_SEMANTICS,
            ),
        }
    }

    fn activate_with_condition(&mut self, plan: &Self::ConditionPlan) {
        plan.value.record_guard_paths_into(self.analysis);
        Self::activate_with_control(&mut self.value_control, &plan.value.action);
        Self::activate_with_control(&mut self.fragment_control, &plan.fragment.action);
    }

    fn activate_condition_alternative(&mut self, plan: &Self::ConditionPlan) {
        Self::activate_alternative_control(&mut self.value_control, &plan.value.action);
        Self::activate_alternative_control(&mut self.fragment_control, &plan.fragment.action);
    }

    fn plan_range_action(
        &mut self,
        _node: tree_sitter::Node<'_>,
        header: Option<&helm_schema_ast::TemplateHeader>,
        _current_path: &YamlPath,
    ) -> Self::RangePlan {
        let value_dot = self.current_value_dot().cloned();
        let value_fragment_dot = value_dot.as_ref().map(AbstractValue::to_context_value);
        let fragment_dot = self.current_fragment_helper_dot().cloned();
        let fragment_current_dot = self.current_fragment_dot().cloned();
        HelperAnalysisRangePlan {
            value: helper_range_runtime_plan(
                header,
                self.bindings,
                value_dot.as_ref(),
                value_fragment_dot.as_ref(),
                &self.value_locals.bindings,
                self.context,
                self.value_seen,
                VALUE_SEMANTICS,
            ),
            fragment: helper_range_runtime_plan(
                header,
                self.bindings,
                fragment_dot.as_ref(),
                fragment_current_dot.as_ref(),
                &self.fragment_locals.bindings,
                self.context,
                self.fragment_seen,
                FRAGMENT_SEMANTICS,
            ),
        }
    }

    fn range_output_path(
        &self,
        node: tree_sitter::Node<'_>,
        current_path: &YamlPath,
        plan: &Self::RangePlan,
    ) -> YamlPath {
        let value_path = plan
            .value
            .action
            .mapping_entry_indent
            .map(|indent| self.document_path_for_mapping_entry_indent(node, indent))
            .unwrap_or_else(|| current_path.clone());
        let fragment_path = plan
            .fragment
            .action
            .mapping_entry_indent
            .map(|indent| self.document_path_for_mapping_entry_indent(node, indent))
            .unwrap_or_else(|| current_path.clone());
        debug_assert!(value_path == fragment_path);
        value_path
    }

    fn activate_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        plan: &Self::RangePlan,
        current_path: &YamlPath,
    ) {
        let value_activated = plan
            .value
            .clone()
            .activate(&mut self.value_control, self.value_locals);
        value_activated.record_guard_paths_into(self.analysis);
        Self::activate_range_control(&mut self.value_control, &value_activated.action);

        let fragment_activated = plan
            .fragment
            .clone()
            .activate(&mut self.fragment_control, self.fragment_locals);
        self.collect_destructured_range_fragment_outputs(
            node,
            fragment_activated.range_fragment_value.as_ref(),
            current_path,
        );
        Self::activate_range_control(&mut self.fragment_control, &fragment_activated.action);
    }
}

#[cfg(test)]
#[path = "tests/helper_body_analysis.rs"]
mod tests;
