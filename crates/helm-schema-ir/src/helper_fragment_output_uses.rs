use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::assignment_action_plan::AssignmentActionPlan;
use crate::binding::{FragmentBinding, HelperBinding};
use crate::bound_value_analysis::GetBindingPlan;
use crate::condition_action_plan::ConditionActionPlan;
use crate::document_hole_context::collect_document_hole_context;
use crate::expression_analysis::resolved_default_fallback_paths_for_text;
use crate::fragment_expr_eval::{
    FragmentEvalContext, fragment_binding_from_text_with_helper_context,
    helper_binding_from_expr_with_fragment_locals,
};
use crate::fragment_scope_eval::{
    apply_local_set_mutations, merge_fragment_locals, parse_helper_assignment,
    range_header_text_from_source, range_iterable_binding, range_variable_name,
};
use crate::helper_analysis::{
    HelperFragmentOutputUse, bound_helper_condition_paths, bound_helper_dependency_paths,
    merge_local_default_paths,
};
use crate::helper_output_projection::{
    HelperOutputExprContext, collect_fragment_binding_output_uses,
    collect_helper_binding_output_uses, collect_helper_binding_output_uses_from_expr,
    expression_output_use_is_keyed_map_projection, helper_output_meta_with_predicates,
    push_helper_fragment_output, static_yaml_fragment_output_path,
};
use crate::local_projection::{
    direct_bound_paths_from_text_in_context, local_bound_paths_from_text,
    local_default_paths_from_text, local_rendered_paths_from_text,
};
use crate::node_action_effect::NodeActionEffectSink;
use crate::node_eval::{NodeEvalRuntime, eval_template_body};
use crate::output_path;
use crate::predicate::Predicate;
use crate::range_action_plan::RangeActionPlan;
use crate::rendered_yaml_context::RenderedYamlContext;
use crate::template_expr_analysis::{
    expr_contains_helper_call, text_pipeline_merges_into_var, text_starts_with_helper_call,
    walk_expr_excluding_helper_call_args,
};
use crate::template_expr_cache::parse_expr_text;
use crate::value_path_context::computed_with_body_dot;
use crate::value_use_sink::ValueUseSink;
use crate::walker::is_fragment_expr;
use crate::{ValueKind, YamlPath};

pub(crate) struct FragmentOutputWalkState<'context, 'state> {
    pub(crate) local_bindings: &'state mut HashMap<String, FragmentBinding>,
    pub(crate) local_default_paths: &'state mut HashMap<String, BTreeSet<String>>,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'state mut HashSet<String>,
    pub(crate) outputs: &'state mut Vec<HelperFragmentOutputUse>,
}

struct FragmentExpressionOutputScope<'a> {
    bindings: &'a HashMap<String, HelperBinding>,
    current_dot: Option<&'a HelperBinding>,
    output_path: &'a YamlPath,
    kind: ValueKind,
    active_output_predicates: &'a BTreeSet<Predicate>,
    fallback_paths: &'a BTreeSet<String>,
}

#[tracing::instrument(skip_all)]
pub(crate) fn collect_bound_fragment_output_uses_from_tree(
    tree: &tree_sitter::Tree,
    source: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    current_dot_fragment: Option<&FragmentBinding>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let mut rendered_yaml = RenderedYamlContext::new(source, state.context.defines);
    rendered_yaml.reset_for_tree(tree);
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
        rendered_yaml,
        range_frames: Vec::new(),
        no_output_depth: 0,
    };
    eval_template_body(&mut runtime, tree.root_node());
}

struct FragmentOutputUseRuntime<'context, 'state> {
    source: &'state str,
    bindings: &'state HashMap<String, HelperBinding>,
    dot_stack: Vec<Option<HelperBinding>>,
    dot_fragment_stack: Vec<Option<FragmentBinding>>,
    active_output_predicates: BTreeSet<Predicate>,
    local_bindings: &'state mut HashMap<String, FragmentBinding>,
    local_default_paths: &'state mut HashMap<String, BTreeSet<String>>,
    context: FragmentEvalContext<'context>,
    seen: &'state mut HashSet<String>,
    outputs: &'state mut Vec<HelperFragmentOutputUse>,
    rendered_yaml: RenderedYamlContext<'state>,
    range_frames: Vec<RangeFrame>,
    no_output_depth: usize,
}

#[derive(Clone)]
struct FragmentOutputUseSnapshot {
    local_bindings: HashMap<String, FragmentBinding>,
    local_default_paths: HashMap<String, BTreeSet<String>>,
    dot_stack_len: usize,
    dot_fragment_stack_len: usize,
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
    dot_fragment_binding: Option<FragmentBinding>,
    variable_binding: Option<(String, FragmentBinding)>,
}

impl FragmentOutputUseRuntime<'_, '_> {
    fn current_dot(&self) -> Option<&HelperBinding> {
        self.dot_stack.last().and_then(Option::as_ref)
    }

    fn current_dot_fragment(&self) -> Option<&FragmentBinding> {
        self.dot_fragment_stack.last().and_then(Option::as_ref)
    }

    fn collect_expression(&mut self, text: &str, relative_path: &YamlPath, kind: ValueKind) {
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
        collect_bound_fragment_output_uses_from_expr(
            text,
            self.bindings,
            current_dot.as_ref(),
            current_dot_fragment.as_ref(),
            relative_path,
            kind,
            &active_output_predicates,
            &mut state,
        );
    }

    fn branch_guard_paths(&mut self, text: &str) -> BTreeSet<String> {
        let current_dot = self.current_dot().cloned();
        let mut branch_guard_paths =
            direct_bound_paths_from_text_in_context(text, self.bindings, current_dot.as_ref());
        branch_guard_paths.extend(local_bound_paths_from_text(text, self.local_bindings));
        let nested = self
            .context
            .helper_call_analyzer()
            .analyze_bound_helper_calls(
                text,
                Some(self.bindings),
                current_dot.as_ref(),
                self.local_bindings,
                self.context,
                self.seen,
            );
        branch_guard_paths.extend(bound_helper_condition_paths(&nested));
        branch_guard_paths
    }

    fn truthy_predicate_for_paths(paths: &BTreeSet<String>) -> Predicate {
        Predicate::all(paths.iter().cloned().map(Predicate::truthy_path).collect())
    }

    fn merge_outcome_maps(&mut self, outcomes: Vec<FragmentOutputUseSnapshot>) {
        let mut iter = outcomes.into_iter();
        let Some(first) = iter.next() else {
            return;
        };
        let mut local_bindings = first.local_bindings;
        let mut local_default_paths = first.local_default_paths;
        for outcome in iter {
            local_bindings = merge_fragment_locals(local_bindings, outcome.local_bindings);
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
}

impl ValueUseSink for FragmentOutputUseRuntime<'_, '_> {
    fn emit_use(&mut self, _source_expr: String, _path: YamlPath, _kind: ValueKind) {}

    fn emit_use_with_extra_guards(
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

    fn declare_fragment_binding(&mut self, variable: String, binding: Option<FragmentBinding>) {
        if let Some(binding) = binding {
            self.local_bindings.insert(variable, binding);
        } else {
            self.local_bindings.remove(&variable);
        }
    }

    fn assign_fragment_binding(&mut self, variable: String, binding: Option<FragmentBinding>) {
        self.declare_fragment_binding(variable, binding);
    }

    fn refresh_default_paths(&mut self, _variable: &str, _rhs: &str) {}

    fn refresh_helper_output_meta(&mut self, _variable: String, _rhs: &str) {}

    fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        if !predicate.is_trivial() {
            self.active_output_predicates.insert(predicate);
        }
    }

    fn push_dot_binding(&mut self, binding: Option<FragmentBinding>) {
        self.dot_fragment_stack.push(binding.clone());
        self.dot_stack
            .push(binding.and_then(|binding| binding.to_helper_binding()));
    }

    fn insert_range_domain(&mut self, _variable: String, _literals: Vec<String>) {}
}

impl NodeEvalRuntime for FragmentOutputUseRuntime<'_, '_> {
    type ScopeSnapshot = FragmentOutputUseSnapshot;

    fn source(&self) -> &str {
        self.source
    }

    fn enter_node(&mut self, node: tree_sitter::Node<'_>) {
        self.rendered_yaml.enter_node(node);
    }

    fn ingest_text_up_to(&mut self, end_byte: usize) {
        self.rendered_yaml.ingest_text_up_to(end_byte);
    }

    fn current_rendered_path(&self) -> YamlPath {
        self.rendered_yaml.current_path()
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
        self.dot_fragment_stack.push(iteration.dot_fragment_binding);
    }

    fn exit_range_iteration(&mut self, _index: usize) {
        if self
            .range_frames
            .last()
            .and_then(|frame| frame.iterations.as_ref())
            .is_some()
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

    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>) {
        if self.no_output_depth > 0 {
            return;
        }
        let Ok(text) = node.utf8_text(self.source.as_bytes()) else {
            return;
        };
        let hole_context =
            collect_document_hole_context(self.source, &self.rendered_yaml, node, text);
        if hole_context.in_mapping_key {
            return;
        }
        let kind = if hole_context.kind == ValueKind::Scalar
            && !hole_context.entire_scalar_value
            && !hole_context.path.0.is_empty()
        {
            ValueKind::PartialScalar
        } else {
            hole_context.kind
        };
        self.collect_expression(text, &hole_context.path, kind);
    }

    fn apply_assignment_side_effects(&mut self, text: &str) -> bool {
        let mut seen_set = HashSet::new();
        if apply_local_set_mutations(
            text,
            self.local_bindings,
            self.current_dot_fragment().cloned().as_ref(),
            self.context,
            &mut seen_set,
        ) {
            return true;
        }

        self.collect_expression(text, &YamlPath(Vec::new()), ValueKind::Scalar);
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

        let mut seen_range_binding = HashSet::new();
        let current_dot_fragment = self.current_dot_fragment().cloned();
        let range_binding = range_iterable_binding(
            &header,
            self.local_bindings,
            current_dot_fragment.as_ref(),
            self.context,
            &mut seen_range_binding,
        );
        let body_dot_fragment = range_binding
            .as_ref()
            .and_then(FragmentBinding::item_binding);

        let exact_iterations = if let Some(FragmentBinding::List(items)) = &range_binding {
            let range_variable = range_variable_name(&header);
            Some(
                items
                    .iter()
                    .map(|item| RangeIteration {
                        dot_binding: item.to_helper_binding(),
                        dot_fragment_binding: Some(item.clone()),
                        variable_binding: range_variable
                            .as_ref()
                            .map(|variable| (variable.clone(), item.clone())),
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };
        let apply_dot_binding = exact_iterations.is_none();
        self.range_frames.push(RangeFrame {
            definitely_nonempty: range_binding
                .as_ref()
                .is_some_and(FragmentBinding::definitely_nonempty_iterable),
            iterations: exact_iterations,
        });

        RangeActionPlan {
            dot_binding: body_dot_fragment,
            apply_dot_binding,
            ..Self::empty_range_action_plan()
        }
    }
}

fn collect_bound_fragment_output_uses_from_expr(
    text: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    current_dot_fragment: Option<&FragmentBinding>,
    relative_path: &YamlPath,
    output_kind: ValueKind,
    active_output_predicates: &BTreeSet<Predicate>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let mut seen_set = HashSet::new();
    if apply_local_set_mutations(
        text,
        state.local_bindings,
        current_dot_fragment,
        state.context,
        &mut seen_set,
    ) {
        return;
    }

    if let Some(assignment) = parse_helper_assignment(text) {
        collect_bound_fragment_output_assignment_uses(
            &assignment.variable,
            &assignment.rhs,
            bindings,
            current_dot,
            current_dot_fragment,
            state,
        );
        return;
    }

    let kind = if matches!(output_kind, ValueKind::Fragment) || is_fragment_expr(text) {
        ValueKind::Fragment
    } else {
        ValueKind::Scalar
    };
    let output_path = static_yaml_fragment_output_path(text)
        .map(|output_path| output_path::append_relative_path(relative_path, &output_path))
        .unwrap_or_else(|| relative_path.clone());
    let direct_outputs = direct_bound_paths_from_text_in_context(text, bindings, current_dot);
    let fallback_paths =
        resolved_default_fallback_paths_for_text(text, Some(bindings), current_dot);
    let local_outputs = local_rendered_paths_from_text(text, state.local_bindings);
    let handled_outputs: BTreeSet<String> = direct_outputs
        .iter()
        .chain(local_outputs.iter())
        .cloned()
        .collect();

    let mut direct_output_uses = Vec::new();
    for expr in parse_expr_text(text) {
        collect_helper_binding_output_uses_from_expr(
            &expr,
            HelperOutputExprContext {
                bindings,
                current_dot,
                relative_path: &output_path,
                kind,
                active_output_predicates,
                defaulted_paths: &fallback_paths,
            },
            &mut direct_output_uses,
        );
    }
    state.outputs.extend(direct_output_uses);

    let local_fallback_paths = local_default_paths_from_text(text, state.local_default_paths);
    let local_output_uses = local_output_uses_from_text(
        text,
        &output_path,
        kind,
        active_output_predicates,
        &local_fallback_paths,
        state.local_bindings,
    );

    let mut nested = state
        .context
        .helper_call_analyzer()
        .analyze_bound_helper_calls(
            text,
            Some(bindings),
            current_dot,
            state.local_bindings,
            state.context,
            state.seen,
        );
    let nested_structured_sources: BTreeSet<String> = nested
        .fragment_output_uses
        .iter()
        .map(|output| output.source_expr.clone())
        .collect();
    let empty_output_path = YamlPath(Vec::new());
    let nested_descendant_structured_sources: BTreeSet<String> = nested
        .fragment_output_uses
        .iter()
        .filter(|output| expression_output_use_is_keyed_map_projection(output, &empty_output_path))
        .map(|output| output.source_expr.clone())
        .collect();
    let nested_scalar_sources: BTreeSet<String> = nested.output.keys().cloned().collect();
    let nested_has_fragment_outputs =
        !nested.fragment_output.is_empty() || !nested.fragment_output_uses.is_empty();

    let expression_output_scope = FragmentExpressionOutputScope {
        bindings,
        current_dot,
        output_path: &output_path,
        kind,
        active_output_predicates,
        fallback_paths: &fallback_paths,
    };
    let expression_output_uses =
        helper_expression_output_uses_from_text(text, expression_output_scope, state);
    let expression_descendant_sources: BTreeSet<String> = expression_output_uses
        .iter()
        .filter(|output| !output.relative_path.0.is_empty())
        .map(|output| output.source_expr.clone())
        .collect();

    state.outputs.extend(local_output_uses);
    for output in expression_output_uses {
        if output.relative_path.0.is_empty()
            && (handled_outputs.contains(&output.source_expr)
                || nested_structured_sources.contains(&output.source_expr)
                || nested_scalar_sources.contains(&output.source_expr))
        {
            continue;
        }
        state.outputs.push(output);
    }
    for (source_expr, meta) in nested.output {
        if kind == ValueKind::Fragment && nested_has_fragment_outputs {
            continue;
        }
        if nested_structured_sources.contains(&source_expr)
            || expression_descendant_sources.contains(&source_expr)
        {
            continue;
        }
        let meta = helper_output_meta_with_predicates(meta, active_output_predicates);
        push_helper_fragment_output(state.outputs, source_expr, relative_path, kind, meta);
    }
    for nested_output in nested.fragment_output_uses.drain(..) {
        if kind == ValueKind::Fragment
            && nested_output.relative_path.0.is_empty()
            && (nested_scalar_sources.contains(&nested_output.source_expr)
                || nested_descendant_structured_sources.contains(&nested_output.source_expr)
                || expression_descendant_sources.contains(&nested_output.source_expr))
        {
            continue;
        }
        let meta = helper_output_meta_with_predicates(nested_output.meta, active_output_predicates);
        push_helper_fragment_output(
            state.outputs,
            nested_output.source_expr,
            &output_path::append_relative_path(relative_path, &nested_output.relative_path),
            nested_output.kind,
            meta,
        );
    }
}

fn collect_bound_fragment_output_assignment_uses(
    var: &str,
    rhs: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    current_dot_fragment: Option<&FragmentBinding>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let mut seen_rhs = HashSet::new();
    let mut binding = fragment_binding_from_text_with_helper_context(
        rhs,
        state.local_bindings,
        Some(bindings),
        current_dot,
        state.context,
        &mut seen_rhs,
    );
    let mut top_level_helper_dependency_paths = BTreeSet::new();
    if text_starts_with_helper_call(rhs) {
        let mut rhs_seen = state.seen.clone();
        let nested = state
            .context
            .helper_call_analyzer()
            .analyze_bound_helper_calls(
                rhs,
                Some(bindings),
                current_dot,
                state.local_bindings,
                state.context,
                &mut rhs_seen,
            );
        top_level_helper_dependency_paths = bound_helper_dependency_paths(&nested);
        if let Some(nested_binding) = nested.into_fragment_binding() {
            binding = match binding {
                Some(binding) => FragmentBinding::merge_all(vec![binding, nested_binding]),
                None => Some(nested_binding),
            };
        }
    }
    if text_pipeline_merges_into_var(rhs, var)
        && let Some(current_dot_fragment) = current_dot_fragment
        && matches!(
            current_dot_fragment,
            FragmentBinding::Dict(_) | FragmentBinding::Overlay { .. }
        )
    {
        let current_item_paths = FragmentBinding::paths(current_dot_fragment);
        let mut internal_item_paths = current_item_paths;
        internal_item_paths.extend(top_level_helper_dependency_paths);
        if !internal_item_paths.is_empty() {
            binding = binding.and_then(|binding| binding.remove_paths(&internal_item_paths));
        }
        binding = match binding {
            Some(binding) => {
                FragmentBinding::merge_all(vec![binding, current_dot_fragment.clone()])
            }
            None => Some(current_dot_fragment.clone()),
        };
    }
    if let Some(binding) = binding {
        state.local_bindings.insert(var.to_string(), binding);
    }
    let mut defaulted_paths =
        resolved_default_fallback_paths_for_text(rhs, Some(bindings), current_dot);
    defaulted_paths.extend(local_default_paths_from_text(
        rhs,
        state.local_default_paths,
    ));
    if defaulted_paths.is_empty() {
        state.local_default_paths.remove(var);
    } else {
        state
            .local_default_paths
            .insert(var.to_string(), defaulted_paths);
    }
}

fn local_output_uses_from_text(
    text: &str,
    output_path: &YamlPath,
    kind: ValueKind,
    active_output_predicates: &BTreeSet<Predicate>,
    local_fallback_paths: &BTreeSet<String>,
    local_bindings: &HashMap<String, FragmentBinding>,
) -> Vec<HelperFragmentOutputUse> {
    let mut local_output_uses = Vec::new();
    for expr in parse_expr_text(text) {
        walk_expr_excluding_helper_call_args(&expr, &mut |node| {
            let binding = match node {
                TemplateExpr::Variable(var) if !var.is_empty() => local_bindings.get(var).cloned(),
                TemplateExpr::Selector { operand, path } => {
                    let TemplateExpr::Variable(var) = operand.as_ref() else {
                        return;
                    };
                    if var.is_empty() {
                        return;
                    }
                    local_bindings
                        .get(var)
                        .and_then(|binding| binding.apply_to_binding(path))
                }
                _ => None,
            };
            if let Some(binding) = binding {
                collect_fragment_binding_output_uses(
                    &mut local_output_uses,
                    &binding,
                    output_path,
                    kind,
                    active_output_predicates,
                    local_fallback_paths,
                );
            }
        });
    }
    local_output_uses
}

fn helper_expression_output_uses_from_text(
    text: &str,
    scope: FragmentExpressionOutputScope<'_>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) -> Vec<HelperFragmentOutputUse> {
    let mut expression_output_uses = Vec::new();
    let mut expression_seen = state.seen.clone();
    for expr in parse_expr_text(text) {
        if !expr_contains_helper_call(&expr) {
            continue;
        }
        if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
            &expr,
            state.local_bindings,
            Some(scope.bindings),
            scope.current_dot,
            state.context,
            &mut expression_seen,
        ) {
            collect_helper_binding_output_uses(
                &mut expression_output_uses,
                &binding,
                scope.output_path,
                scope.kind,
                scope.active_output_predicates,
                scope.fallback_paths,
            );
        }
    }
    expression_output_uses
        .retain(|output| expression_output_use_is_keyed_map_projection(output, scope.output_path));
    expression_output_uses
}
