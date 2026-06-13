use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::assignment_action_plan::AssignmentActionPlan;
use crate::binding::{FragmentBinding, HelperBinding};
use crate::bound_value_analysis::GetBindingPlan;
use crate::condition_action_plan::ConditionActionPlan;
use crate::contract::ContractUseSink;
use crate::expression_analysis::{
    resolved_default_fallback_paths_for_text, resolved_string_transform_paths_for_text,
    resolved_type_is_paths_for_text, set_default_chart_paths_for_text,
};
use crate::fragment_expr_eval::{
    FragmentEvalContext, fragment_binding_from_expr,
    fragment_binding_from_text_with_helper_context, helper_binding_from_expr_with_fragment_locals,
};
use crate::fragment_scope_eval::{
    apply_local_set_mutations, merge_fragment_locals, range_header_text_from_source,
    range_iterable_binding, range_variable_item_binding, range_variable_name,
};
use crate::helper_analysis::{
    BoundHelperAnalysis, HelperOutputMeta, bound_helper_condition_paths,
    bound_helper_dependency_paths, extend_type_hints, helper_dependency_meta_from_analysis,
    merge_helper_output_meta_maps, merge_local_default_paths,
};
use crate::helper_output_projection::helper_binding_output_meta;
use crate::local_projection::{
    direct_bound_paths_from_text_in_context, local_bound_paths_from_text,
    local_default_paths_from_text, local_output_meta_from_text, local_rendered_paths_from_text,
};
use crate::node_action_effect::NodeActionEffectSink;
use crate::node_eval::{NodeEvalRuntime, eval_template_body};
use crate::predicate::Predicate;
use crate::range_action_plan::RangeActionPlan;
use crate::template_expr_cache::parse_expr_text;
use crate::value_path_context::computed_with_body_dot;
use crate::walker::is_fragment_expr;
use crate::{ValueKind, YamlPath};

pub(crate) struct HelperValuesWalkState<'context, 'state> {
    pub(crate) local_bindings: &'state mut HashMap<String, FragmentBinding>,
    pub(crate) local_default_paths: &'state mut HashMap<String, BTreeSet<String>>,
    pub(crate) local_output_meta: &'state mut HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'state mut HashSet<String>,
    pub(crate) analysis: &'state mut BoundHelperAnalysis,
}

/// Walks a helper body collecting the values and effects it contributes to
/// callers that include/template it.
#[tracing::instrument(skip_all)]
pub(crate) fn collect_bound_helper_values_from_tree(
    node: tree_sitter::Node<'_>,
    source: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
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
    bindings: &'state HashMap<String, HelperBinding>,
    dot_stack: Vec<Option<HelperBinding>>,
    active_output_predicates: BTreeSet<Predicate>,
    local_bindings: &'state mut HashMap<String, FragmentBinding>,
    local_default_paths: &'state mut HashMap<String, BTreeSet<String>>,
    local_output_meta: &'state mut HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    context: FragmentEvalContext<'context>,
    seen: &'state mut HashSet<String>,
    analysis: &'state mut BoundHelperAnalysis,
    range_frames: Vec<RangeFrame>,
    no_output_depth: usize,
}

#[derive(Clone)]
struct HelperValueSnapshot {
    local_bindings: HashMap<String, FragmentBinding>,
    local_default_paths: HashMap<String, BTreeSet<String>>,
    local_output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    dot_stack_len: usize,
    active_output_predicates: BTreeSet<Predicate>,
}

#[derive(Clone)]
struct RangeFrame {
    definitely_nonempty: bool,
    iterations: Option<Vec<RangeIteration>>,
}

#[derive(Clone)]
struct RangeIteration {
    dot_binding: Option<HelperBinding>,
    variable_binding: Option<(String, FragmentBinding)>,
}

impl HelperValueRuntime<'_, '_> {
    fn current_dot(&self) -> Option<&HelperBinding> {
        self.dot_stack.last().and_then(Option::as_ref)
    }

    fn current_dot_fragment(&self) -> Option<FragmentBinding> {
        self.current_dot().map(HelperBinding::to_fragment_binding)
    }

    fn collect_expression(&mut self, text: &str) {
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
        collect_bound_helper_values_from_expr(
            text,
            self.bindings,
            current_dot.as_ref(),
            &active_output_predicates,
            &mut state,
        );
    }

    fn branch_guard_paths(&mut self, text: &str) -> BTreeSet<String> {
        let current_dot = self.current_dot().cloned();
        let mut branch_guard_paths =
            direct_bound_paths_from_text_in_context(text, self.bindings, current_dot.as_ref());
        branch_guard_paths.extend(local_bound_paths_from_text(text, self.local_bindings));
        let nested = self.context.helper_summaries().analyze_bound_helper_calls(
            text,
            Some(self.bindings),
            current_dot.as_ref(),
            self.local_bindings,
            self.context,
            self.seen,
        );
        branch_guard_paths.extend(bound_helper_condition_paths(&nested));
        self.analysis
            .guard_paths
            .extend(branch_guard_paths.iter().cloned());
        branch_guard_paths
    }

    fn truthy_predicate_for_paths(paths: &BTreeSet<String>) -> Predicate {
        Predicate::all(paths.iter().cloned().map(Predicate::truthy_path).collect())
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

    fn empty_range_action_plan() -> RangeActionPlan {
        RangeActionPlan {
            header_text: None,
            source_paths: Vec::new(),
            literal_range: None,
            guard_path: YamlPath(Vec::new()),
            emit_header_use: false,
            renders_mapping_entries: false,
            dot_binding: None,
            apply_dot_binding: true,
        }
    }

    fn range_action_plan(
        dot_binding: Option<FragmentBinding>,
        apply_dot_binding: bool,
    ) -> RangeActionPlan {
        RangeActionPlan {
            dot_binding,
            apply_dot_binding,
            ..Self::empty_range_action_plan()
        }
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

    fn declare_fragment_binding(&mut self, _variable: String, _binding: Option<FragmentBinding>) {}

    fn assign_fragment_binding(&mut self, _variable: String, _binding: Option<FragmentBinding>) {}

    fn refresh_default_paths(&mut self, _variable: &str, _rhs: &str) {}

    fn refresh_helper_output_meta(&mut self, _variable: String, _rhs: &str) {}

    fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        if !predicate.is_trivial() {
            self.active_output_predicates.insert(predicate);
        }
    }

    fn push_dot_binding(&mut self, binding: Option<FragmentBinding>) {
        self.dot_stack
            .push(binding.and_then(|binding| binding.to_helper_binding()));
    }

    fn insert_range_domain(&mut self, _variable: String, _literals: Vec<String>) {}
}

impl NodeEvalRuntime for HelperValueRuntime<'_, '_> {
    type ScopeSnapshot = HelperValueSnapshot;

    fn source(&self) -> &str {
        self.source
    }

    fn enter_node(&mut self, _node: tree_sitter::Node<'_>) {}

    fn ingest_text_up_to(&mut self, _end_byte: usize) {}

    fn current_rendered_path(&self) -> YamlPath {
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
            .is_some_and(|frame| frame.definitely_nonempty)
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
            .and_then(|frame| frame.iterations.as_ref().map(Vec::len))
            .unwrap_or(1)
    }

    fn enter_range_iteration(&mut self, index: usize) {
        let Some(iteration) = self
            .range_frames
            .last()
            .and_then(|frame| frame.iterations.as_ref())
            .and_then(|iterations| iterations.get(index))
            .cloned()
        else {
            return;
        };
        if let Some((variable, binding)) = iteration.variable_binding {
            self.local_bindings.insert(variable, binding);
        }
        self.dot_stack.push(iteration.dot_binding);
    }

    fn exit_range_iteration(&mut self, _index: usize) {
        if self
            .range_frames
            .last()
            .and_then(|frame| frame.iterations.as_ref())
            .is_some()
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

    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>) {
        if self.no_output_depth > 0 {
            return;
        }
        if let Ok(text) = node.utf8_text(self.source.as_bytes()) {
            let text = text.to_string();
            self.collect_expression(&text);
        }
    }

    fn apply_assignment_side_effects(&mut self, text: &str) -> bool {
        self.collect_expression(text);
        true
    }

    fn plan_assignment_action(&self, _text: &str) -> AssignmentActionPlan {
        AssignmentActionPlan {
            get_binding: None,
            local_assignment: None,
        }
    }

    fn plan_if_condition(&mut self, header: &str) -> ConditionActionPlan {
        let branch_guard_paths = self.branch_guard_paths(header);
        ConditionActionPlan {
            predicate: Self::truthy_predicate_for_paths(&branch_guard_paths),
            bound_values: Vec::new(),
            dot_binding: None,
            apply_alternative_predicate: false,
        }
    }

    fn plan_with_condition(&mut self, header: &str) -> ConditionActionPlan {
        let branch_guard_paths = self.branch_guard_paths(header);
        let current_dot = self.current_dot().cloned();
        let body_dot = computed_with_body_dot(header, self.bindings, current_dot.as_ref());
        ConditionActionPlan {
            predicate: Self::truthy_predicate_for_paths(&branch_guard_paths),
            bound_values: Vec::new(),
            dot_binding: body_dot.as_ref().map(HelperBinding::to_fragment_binding),
            apply_alternative_predicate: false,
        }
    }

    fn plan_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        _current_path: &YamlPath,
    ) -> RangeActionPlan {
        let Some(header) = range_header_text_from_source(node, self.source) else {
            self.range_frames.push(RangeFrame {
                definitely_nonempty: false,
                iterations: None,
            });
            return Self::empty_range_action_plan();
        };
        let branch_guard_paths = self.branch_guard_paths(&header);
        self.active_output_predicates
            .extend(branch_guard_paths.into_iter().map(Predicate::truthy_path));

        let current_dot_fragment = self.current_dot_fragment();
        let mut seen_range = HashSet::new();
        let range_fragment_binding = range_iterable_binding(
            &header,
            self.local_bindings,
            current_dot_fragment.as_ref(),
            self.context,
            &mut seen_range,
        );
        let range_binding = range_fragment_binding
            .as_ref()
            .and_then(FragmentBinding::to_helper_binding);
        let body_dot = range_binding.as_ref().and_then(HelperBinding::item_binding);

        let exact_iterations = if let Some(FragmentBinding::List(items)) = &range_fragment_binding {
            let range_variable = range_variable_name(&header);
            Some(
                items
                    .iter()
                    .map(|item| RangeIteration {
                        dot_binding: item.to_helper_binding(),
                        variable_binding: range_variable
                            .as_ref()
                            .map(|variable| (variable.clone(), item.clone())),
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            let mut seen_range_variable = HashSet::new();
            if let Some((variable, binding)) = range_variable_item_binding(
                &header,
                self.local_bindings,
                current_dot_fragment.as_ref(),
                self.context,
                &mut seen_range_variable,
            ) {
                self.local_bindings.insert(variable, binding);
            }
            None
        };

        let apply_dot_binding = exact_iterations.is_none();
        self.range_frames.push(RangeFrame {
            definitely_nonempty: range_binding
                .as_ref()
                .is_some_and(HelperBinding::definitely_nonempty_iterable),
            iterations: exact_iterations,
        });

        Self::range_action_plan(
            body_dot.as_ref().map(HelperBinding::to_fragment_binding),
            apply_dot_binding,
        )
    }
}

fn collect_bound_helper_values_from_expr(
    text: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    active_output_predicates: &BTreeSet<Predicate>,
    state: &mut HelperValuesWalkState<'_, '_>,
) {
    if let Some(assignment) = crate::fragment_scope_eval::parse_helper_assignment(text) {
        collect_assignment_bound_helper_values(
            &assignment.variable,
            &assignment.rhs,
            text,
            bindings,
            current_dot,
            active_output_predicates,
            state,
        );
        return;
    }

    let current_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
    let mut seen_set = HashSet::new();
    if apply_local_set_mutations(
        text,
        state.local_bindings,
        current_dot_fragment.as_ref(),
        state.context,
        &mut seen_set,
    ) {
        let set_default_paths = set_default_chart_paths_for_text(text, Some(bindings), current_dot);
        state.analysis.chart_defaults.extend(set_default_paths);
        return;
    }

    let direct_outputs = direct_bound_paths_from_text_in_context(text, bindings, current_dot);
    let fallback_paths =
        resolved_default_fallback_paths_for_text(text, Some(bindings), current_dot);
    extend_type_hints(
        &mut state.analysis.type_hints,
        resolved_type_is_paths_for_text(text, Some(bindings), current_dot),
    );
    extend_type_hints(
        &mut state.analysis.type_hints,
        resolved_string_transform_paths_for_text(text, Some(bindings), current_dot),
    );
    let local_outputs = local_rendered_paths_from_text(text, state.local_bindings);
    let local_fallback_paths = local_default_paths_from_text(text, state.local_default_paths);
    let local_meta_by_path =
        local_output_meta_from_text(text, state.local_bindings, state.local_output_meta);
    let expression_kind = if is_fragment_expr(text) {
        ValueKind::Fragment
    } else {
        ValueKind::Scalar
    };
    if expression_kind == ValueKind::Scalar {
        for output in direct_outputs {
            let meta = HelperOutputMeta::with_predicates(
                active_output_predicates,
                fallback_paths.contains(&output),
            );
            state.analysis.add_output_meta(output, meta);
        }
        let mut local_output_sources = local_outputs;
        local_output_sources.extend(local_meta_by_path.keys().cloned());
        for output in local_output_sources {
            let mut meta = local_meta_by_path.get(&output).cloned().unwrap_or_default();
            meta.add_predicates(active_output_predicates.iter().cloned());
            meta.defaulted |= local_fallback_paths.contains(&output);
            state.analysis.add_output_meta(output, meta);
        }
        let mut string_seen = state.seen.clone();
        state
            .analysis
            .string_output
            .extend(string_outputs_from_text(
                text,
                bindings,
                current_dot,
                state.local_bindings,
                state.context,
                &mut string_seen,
            ));
    }
    let nested = state.context.helper_summaries().analyze_bound_helper_calls(
        text,
        Some(bindings),
        current_dot,
        state.local_bindings,
        state.context,
        state.seen,
    );
    if expression_kind == ValueKind::Fragment {
        state.analysis.extend_nested_fragment_render(
            nested,
            active_output_predicates,
            expression_kind,
        );
    } else {
        state
            .analysis
            .extend_nested_scalar_render(nested, active_output_predicates);
    }
    let set_default_paths = set_default_chart_paths_for_text(text, Some(bindings), current_dot);
    state.analysis.chart_defaults.extend(set_default_paths);
}

fn string_outputs_from_text(
    text: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    local_bindings: &HashMap<String, FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> BTreeSet<String> {
    let mut strings = BTreeSet::new();
    let current_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
    for expr in parse_expr_text(text) {
        if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
            &expr,
            local_bindings,
            Some(bindings),
            current_dot,
            context,
            seen,
        ) {
            strings.extend(binding.strings());
            continue;
        }
        if let Some(binding) = fragment_binding_from_expr(
            &expr,
            local_bindings,
            current_dot_fragment.as_ref(),
            context,
            seen,
        ) {
            strings.extend(binding.strings());
        }
    }
    strings
}

fn collect_assignment_bound_helper_values(
    var: &str,
    rhs: &str,
    text: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    active_output_predicates: &BTreeSet<Predicate>,
    state: &mut HelperValuesWalkState<'_, '_>,
) {
    let set_default_paths = set_default_chart_paths_for_text(text, Some(bindings), current_dot);
    state.analysis.chart_defaults.extend(set_default_paths);
    extend_type_hints(
        &mut state.analysis.type_hints,
        resolved_type_is_paths_for_text(rhs, Some(bindings), current_dot),
    );
    extend_type_hints(
        &mut state.analysis.type_hints,
        resolved_string_transform_paths_for_text(rhs, Some(bindings), current_dot),
    );

    let current_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
    let mut seen_set = HashSet::new();
    if apply_local_set_mutations(
        text,
        state.local_bindings,
        current_dot_fragment.as_ref(),
        state.context,
        &mut seen_set,
    ) {
        return;
    }

    let fallback_paths = resolved_default_fallback_paths_for_text(rhs, Some(bindings), current_dot);
    let direct_outputs = BTreeSet::new();
    let local_fallback_paths = local_default_paths_from_text(rhs, state.local_default_paths);
    let local_outputs = local_bound_paths_from_text(rhs, state.local_bindings);
    let local_meta_by_path =
        local_output_meta_from_text(rhs, state.local_bindings, state.local_output_meta);
    let mut result_seen = state.seen.clone();
    let result_meta_by_path = helper_binding_output_meta_from_text(
        rhs,
        state.local_bindings,
        bindings,
        current_dot,
        state.context,
        &mut result_seen,
    );
    let nested = state.context.helper_summaries().analyze_bound_helper_calls(
        rhs,
        Some(bindings),
        current_dot,
        state.local_bindings,
        state.context,
        state.seen,
    );
    state
        .analysis
        .chart_defaults
        .extend(nested.chart_defaults.clone());
    extend_type_hints(&mut state.analysis.type_hints, nested.type_hints.clone());
    state
        .analysis
        .dependency_paths
        .extend(bound_helper_dependency_paths(&nested));
    state
        .analysis
        .add_dependency_meta_map(helper_dependency_meta_from_analysis(&nested));

    let rhs_output_meta = rhs_output_meta(
        &direct_outputs,
        &local_outputs,
        &fallback_paths,
        &local_fallback_paths,
        &local_meta_by_path,
        &result_meta_by_path,
        active_output_predicates,
    );

    let mut seen_rhs = HashSet::new();
    if let Some(binding) = fragment_binding_from_text_with_helper_context(
        rhs,
        state.local_bindings,
        Some(bindings),
        current_dot,
        state.context,
        &mut seen_rhs,
    ) {
        state.local_bindings.insert(var.to_string(), binding);
    }
    let mut defaulted_paths = fallback_paths;
    defaulted_paths.extend(local_fallback_paths);
    defaulted_paths.extend(
        nested
            .output
            .iter()
            .filter(|(_path, meta)| meta.defaulted)
            .map(|(path, _meta)| path.clone()),
    );
    defaulted_paths.extend(
        nested
            .fragment_output_uses
            .iter()
            .filter(|output| output.meta.defaulted)
            .map(|output| output.source_expr.clone()),
    );
    if defaulted_paths.is_empty() {
        state.local_default_paths.remove(var);
    } else {
        state
            .local_default_paths
            .insert(var.to_string(), defaulted_paths);
    }
    if rhs_output_meta.is_empty() {
        state.local_output_meta.remove(var);
    } else {
        state
            .local_output_meta
            .insert(var.to_string(), rhs_output_meta);
    }
}

fn rhs_output_meta(
    direct_outputs: &BTreeSet<String>,
    local_outputs: &BTreeSet<String>,
    fallback_paths: &BTreeSet<String>,
    local_fallback_paths: &BTreeSet<String>,
    local_meta_by_path: &BTreeMap<String, HelperOutputMeta>,
    result_meta_by_path: &BTreeMap<String, HelperOutputMeta>,
    active_output_predicates: &BTreeSet<Predicate>,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut rhs_output_meta: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
    for (output, meta) in result_meta_by_path {
        let mut meta = meta.clone();
        meta.add_predicates(active_output_predicates.iter().cloned());
        meta.defaulted |= fallback_paths.contains(output);
        rhs_output_meta
            .entry(output.clone())
            .or_default()
            .merge(meta);
    }
    for output in direct_outputs {
        let entry = rhs_output_meta.entry(output.clone()).or_default();
        entry.add_predicates(active_output_predicates.iter().cloned());
        entry.defaulted |= fallback_paths.contains(output);
    }
    for output in local_outputs {
        let mut meta = local_meta_by_path.get(output).cloned().unwrap_or_default();
        meta.add_predicates(active_output_predicates.iter().cloned());
        meta.defaulted |= local_fallback_paths.contains(output);
        rhs_output_meta
            .entry(output.clone())
            .or_default()
            .merge(meta);
    }
    rhs_output_meta
}

fn helper_binding_output_meta_from_text(
    text: &str,
    local_bindings: &HashMap<String, FragmentBinding>,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut out: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
    for expr in parse_expr_text(text) {
        if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
            &expr,
            local_bindings,
            Some(bindings),
            current_dot,
            context,
            seen,
        ) {
            for (path, meta) in helper_binding_output_meta(&binding) {
                out.entry(path).or_default().merge(meta);
            }
        }
    }
    out
}
