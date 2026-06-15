use std::collections::{BTreeSet, HashMap, HashSet};

use crate::assignment_action_plan::AssignmentActionPlan;
use crate::binding::{FragmentBinding, HelperBinding};
use crate::bound_value_analysis::GetBindingPlan;
use crate::condition_action_plan::ConditionActionPlan;
use crate::contract_sink::ContractUseSink;
use crate::document_hole_context::collect_document_hole_context;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::fragment_scope_eval::{
    apply_local_set_mutations, merge_fragment_locals, range_body_emits_sequence_item_from_source,
    range_body_renders_mapping_entries_from_ast, range_has_destructured_variable_definition,
    range_header_text_from_source, range_iterable_binding, range_variable_name,
};
use crate::helper_analysis::{HelperFragmentOutputUse, HelperOutputMeta};
use crate::helper_analysis_mutation::merge_local_default_paths;
use crate::helper_analysis_projection::bound_helper_condition_paths;
use crate::helper_output_projection::push_helper_fragment_output;
use crate::helper_walk_state::FragmentOutputWalkState;
use crate::local_projection::{
    direct_bound_paths_from_text_in_context, local_bound_paths_from_text,
};
use crate::node_action_effect::NodeActionEffectSink;
use crate::node_eval::{NodeEvalRuntime, eval_template_body};
use crate::predicate::Predicate;
use crate::range_action_plan::RangeActionPlan;
use crate::rendered_yaml_context::RenderedYamlContext;
use crate::value_path_context::computed_with_body_fragment_binding;
use crate::{ValueKind, YamlPath};

mod expression_output;

use expression_output::collect_bound_fragment_output_uses_from_expr;

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
        let nested = self.context.helper_summaries().analyze_bound_helper_calls(
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

    fn collect_destructured_range_fragment_outputs(
        &mut self,
        node: tree_sitter::Node<'_>,
        range_binding: Option<&FragmentBinding>,
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
        for source_expr in FragmentBinding::paths(range_binding) {
            push_helper_fragment_output(
                self.outputs,
                source_expr,
                current_path,
                ValueKind::Fragment,
                meta.clone(),
            );
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
        let current_dot_fragment = self.current_dot_fragment().cloned();
        let body_dot = computed_with_body_fragment_binding(
            header,
            self.bindings,
            self.local_bindings,
            self.context,
            current_dot_fragment.as_ref(),
            current_dot.as_ref(),
        );
        ConditionActionPlan {
            predicate: Self::truthy_predicate_for_paths(&branch_guard_paths),
            bound_values: Vec::new(),
            dot_binding: body_dot,
            apply_alternative_predicate: false,
        }
    }

    fn plan_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        current_path: &YamlPath,
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
        self.collect_destructured_range_fragment_outputs(
            node,
            range_binding.as_ref(),
            current_path,
        );

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
