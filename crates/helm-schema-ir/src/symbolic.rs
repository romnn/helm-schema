use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use helm_schema_ast::{DefineIndex, HelmAst};

use crate::assignment_action_plan::{AssignmentActionPlan, plan_assignment_action};
use crate::binding::{FragmentBinding, HelperBinding};
use crate::bound_value_analysis::GetBinding;
use crate::condition_action_plan::{ConditionActionPlan, plan_if_condition, plan_with_condition};
use crate::define_body_cache::{DefineBodyCache, parse_go_template};
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_analysis::{BoundHelperAnalysis, HelperOutputMeta};
use crate::helper_binding_eval::bindings_for_helper_arg;
use crate::helper_inline::plan_exact_helper_inline;
use crate::helper_summary::HelperSummaryCache;
use crate::node_action_effect::NodeActionEffectSink;
use crate::node_eval::{NodeEvalRuntime, eval_node};
use crate::output_node_context::output_node_context;
use crate::output_value_analysis::collect_output_value_analysis;
use crate::output_value_emitter::{ValueUseSink, emit_output_value_analysis};
use crate::range_action_plan::{RangeActionPlan, plan_range_action};
use crate::rendered_yaml_context::RenderedYamlContext;
use crate::static_file_template::{
    StaticFileTemplate, collect_template_requests_from_helper, literal_helper_calls,
};
use crate::symbolic_local_state::{SymbolicLocalState, SymbolicLocalStateSnapshot};
use crate::template_expr_cache::clear_template_expr_cache;
use crate::value_path_context::ValuePathContext;
use crate::value_use_postprocess::postprocess_value_uses;
use crate::{Guard, IrGenerator, ValueKind, ValueUse, YamlPath};

pub struct SymbolicIrGenerator;

impl IrGenerator for SymbolicIrGenerator {
    #[tracing::instrument(skip_all, fields(bytes = src.len()))]
    fn generate(&self, src: &str, _ast: &HelmAst, defines: &DefineIndex) -> Vec<ValueUse> {
        SymbolicIrContext::new(defines).generate(src, _ast, defines)
    }
}

/// Reusable state for generating symbolic IR across many templates that
/// share one [`DefineIndex`].
///
/// The context owns exact parse/helper-analysis caches. Reusing it across
/// templates avoids recomputing helper bodies without changing analysis
/// semantics; a cache miss and cache hit return the same structural facts.
#[derive(Clone)]
pub struct SymbolicIrContext {
    inner: Rc<SymbolicIrContextInner>,
}

struct SymbolicIrContextInner {
    define_bodies: DefineBodyCache,
    helper_summaries: HelperSummaryCache,
}

impl SymbolicIrContext {
    #[tracing::instrument(skip_all)]
    pub fn new(defines: &DefineIndex) -> Self {
        clear_template_expr_cache();
        Self {
            inner: Rc::new(SymbolicIrContextInner {
                define_bodies: DefineBodyCache::new(defines),
                helper_summaries: HelperSummaryCache::new(),
            }),
        }
    }

    #[tracing::instrument(skip_all, fields(bytes = src.len()))]
    pub fn generate(&self, src: &str, _ast: &HelmAst, defines: &DefineIndex) -> Vec<ValueUse> {
        let Some(tree) = parse_go_template(src) else {
            return Vec::new();
        };

        let mut w = SymbolicWalker::new_with_context(src, defines, self.clone());
        w.run(&tree)
    }
}

struct SymbolicWalker<'a> {
    source: &'a str,
    defines: &'a DefineIndex,
    ir_context: SymbolicIrContext,
    uses: Vec<ValueUse>,
    guards: Vec<Guard>,
    seed_guards: Vec<Guard>,
    seed_dot: Option<FragmentBinding>,
    no_output_depth: usize,
    dot_stack: Vec<Option<FragmentBinding>>,
    rendered_yaml: RenderedYamlContext<'a>,

    inline_stack: Vec<String>,

    local_state: SymbolicLocalState,

    inline_helpers_in_fragments: bool,
    root_bindings: HashMap<String, HelperBinding>,
}

struct WalkerScopeSnapshot {
    guards_len: usize,
    dot_stack_len: Option<usize>,
    local_state: SymbolicLocalStateSnapshot,
}

impl<'a> SymbolicWalker<'a> {
    fn new_with_context(
        source: &'a str,
        defines: &'a DefineIndex,
        ir_context: SymbolicIrContext,
    ) -> Self {
        Self {
            source,
            defines,
            ir_context,
            uses: Vec::new(),
            guards: Vec::new(),
            seed_guards: Vec::new(),
            seed_dot: None,
            no_output_depth: 0,
            dot_stack: Vec::new(),
            rendered_yaml: RenderedYamlContext::new(source, defines),

            inline_stack: Vec::new(),

            local_state: SymbolicLocalState::default(),

            inline_helpers_in_fragments: false,
            root_bindings: HashMap::new(),
        }
    }

    fn with_initial_guards(mut self, guards: Vec<Guard>) -> Self {
        self.seed_guards = guards;
        self.guards = self.seed_guards.clone();
        self
    }

    fn with_initial_dot_binding(mut self, dot: Option<FragmentBinding>) -> Self {
        self.seed_dot = dot;
        self
    }

    fn with_inline_stack(mut self, stack: Vec<String>) -> Self {
        self.inline_stack = stack;
        self
    }

    fn with_inline_helpers_in_fragments(mut self, enabled: bool) -> Self {
        self.inline_helpers_in_fragments = enabled;
        self
    }

    fn with_helper_bindings(mut self, bindings: HashMap<String, HelperBinding>) -> Self {
        self.root_bindings = bindings;
        self
    }

    fn fragment_eval_context(&self) -> FragmentEvalContext<'_> {
        FragmentEvalContext::new(
            self.defines,
            &self.ir_context.inner.define_bodies,
            &self.ir_context.inner.helper_summaries,
        )
    }

    fn value_path_context(&self) -> ValuePathContext<'_> {
        ValuePathContext {
            root_bindings: &self.root_bindings,
            template_bindings: &self.local_state.fragment_bindings,
            template_default_paths: &self.local_state.default_paths,
            template_output_meta: &self.local_state.output_meta,
            fragment_context: self.fragment_eval_context(),
            current_dot_fragment: self.current_dot_fragment(),
            current_dot_binding: self.current_dot_binding(),
        }
    }

    /// Seed this walker's chart-level defaults from a parent walker so a
    /// nested static-file template walk inherits the same render-time
    /// mutation context. The parent's `include "X.defaultValues" .`
    /// already ran above the nested fragment in source order, so the
    /// fragment's reads see the same defaulted state.
    fn with_chart_value_defaults(mut self, defaults: BTreeSet<String>) -> Self {
        self.local_state.set_chart_value_defaults(defaults);
        self
    }

    fn scope_snapshot(&self, include_dot_stack: bool) -> WalkerScopeSnapshot {
        WalkerScopeSnapshot {
            guards_len: self.guards.len(),
            dot_stack_len: include_dot_stack.then_some(self.dot_stack.len()),
            local_state: self.local_state.snapshot(),
        }
    }

    fn restore_scope(&mut self, snapshot: WalkerScopeSnapshot) {
        self.guards.truncate(snapshot.guards_len);
        if let Some(dot_stack_len) = snapshot.dot_stack_len {
            self.dot_stack.truncate(dot_stack_len);
        }
        self.local_state.restore(snapshot.local_state);
    }

    fn inline_static_file_templates_from_helper_calls(&mut self, text: &str) {
        for helper_call in literal_helper_calls(text) {
            let requests = {
                let context = self.fragment_eval_context();
                let current_dot = self.current_dot_fragment();
                let mut seen = HashSet::new();
                let helper_dot = helper_call.arg.as_ref().and_then(|arg| {
                    context.fragment_binding_from_expr(
                        arg,
                        &self.local_state.fragment_bindings,
                        current_dot.as_ref(),
                        &mut seen,
                    )
                });
                collect_template_requests_from_helper(
                    &helper_call.name,
                    helper_dot.as_ref(),
                    context,
                )
            };
            for request in requests {
                self.inline_static_file_template(request);
            }
        }
    }

    fn inline_static_file_template(&mut self, request: StaticFileTemplate) {
        let token = format!("file:{}", request.path);
        if self.inline_stack.iter().any(|entry| entry == &token) {
            return;
        }
        let Some(src) = self.defines.get_file(&request.path) else {
            return;
        };
        let Some(tree) = parse_go_template(src) else {
            return;
        };

        let mut stack = self.inline_stack.clone();
        stack.push(token);
        let mut nested =
            SymbolicWalker::new_with_context(src, self.defines, self.ir_context.clone())
                .with_initial_guards(self.guards.clone())
                .with_initial_dot_binding(request.dot)
                .with_inline_stack(stack)
                .with_inline_helpers_in_fragments(true)
                .with_chart_value_defaults(self.local_state.chart_value_defaults.clone());
        let uses = nested.run(&tree);
        self.uses.extend(uses);
    }

    #[tracing::instrument(skip_all)]
    fn run(&mut self, tree: &tree_sitter::Tree) -> Vec<ValueUse> {
        self.rendered_yaml.reset_for_tree(tree);
        self.guards = self.seed_guards.clone();
        self.dot_stack.clear();
        if let Some(dot) = self.seed_dot.clone() {
            self.dot_stack.push(Some(dot));
        }
        self.no_output_depth = 0;
        eval_node(self, tree.root_node());
        postprocess_value_uses(&mut self.uses);
        std::mem::take(&mut self.uses)
    }

    fn emit_use(&mut self, source_expr: String, path: YamlPath, kind: ValueKind) {
        self.emit_use_with_extra_guards(source_expr, path, kind, &[]);
    }

    fn emit_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    ) {
        let path = if self.no_output_depth > 0 {
            YamlPath(Vec::new())
        } else {
            self.rendered_yaml.rebase_path(path)
        };

        let mut guards = self.guards.clone();
        for guard in extra_guards {
            if !guards.contains(guard) {
                guards.push(guard.clone());
            }
        }
        // If a helper already invoked above this walk in source order
        // structurally set a default for this exact path (the chart
        // writer's `set OPERAND "K" (OPERAND.K | default V)` mutation —
        // see `set_default_chart_paths_for_text`), surface that as a
        // `Guard::Default` so the nullability pass sees the same null
        // tolerance Helm's render-time `set` produces. The chart-default
        // applies only to reads with a non-empty `path` (i.e. ones
        // contributing to a rendered field): without a render use the
        // guard would be meaningless.
        if !path.0.is_empty() && self.local_state.chart_value_defaults.contains(&source_expr) {
            let default_guard = Guard::Default {
                path: source_expr.clone(),
            };
            if !guards.contains(&default_guard) {
                guards.push(default_guard);
            }
        }

        self.uses.push(ValueUse {
            source_expr,
            path,
            kind,
            guards,
            resource: self.rendered_yaml.current_resource().cloned(),
        });
    }

    fn emit_helper_use(&mut self, source_expr: String) {
        self.emit_helper_use_kind_with_extra_guards(source_expr, ValueKind::Scalar, &[]);
    }

    fn emit_helper_use_with_extra_guards(&mut self, source_expr: String, extra_guards: &[Guard]) {
        self.emit_helper_use_kind_with_extra_guards(source_expr, ValueKind::Scalar, extra_guards);
    }

    fn emit_helper_use_kind_with_extra_guards(
        &mut self,
        source_expr: String,
        kind: ValueKind,
        extra_guards: &[Guard],
    ) {
        if source_expr.trim().is_empty() {
            return;
        }
        let mut guards = self.guards.clone();
        for guard in extra_guards {
            if !guards.contains(guard) {
                guards.push(guard.clone());
            }
        }
        self.uses.push(ValueUse {
            source_expr,
            path: YamlPath(Vec::new()),
            kind,
            guards,
            resource: None,
        });
    }

    fn current_dot_binding(&self) -> Option<HelperBinding> {
        self.dot_stack
            .last()
            .and_then(|binding| binding.as_ref())
            .and_then(FragmentBinding::to_current_dot_helper_binding)
    }

    fn current_dot_fragment(&self) -> Option<FragmentBinding> {
        self.dot_stack.last().and_then(|binding| binding.clone())
    }

    fn helper_output_meta_for_text(&self, text: &str) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = self
            .value_path_context()
            .local_alias_output_meta_for_text(text);
        let analysis = self.analyze_bound_helper_calls(text);
        for (path, meta) in analysis.output {
            let entry = out.entry(path).or_default();
            entry.guards.extend(meta.guards);
            entry.defaulted |= meta.defaulted;
        }
        for output in analysis.fragment_output_uses {
            let entry = out.entry(output.source_expr).or_default();
            entry.guards.extend(output.meta.guards);
            entry.defaulted |= output.meta.defaulted;
        }
        out
    }

    fn inline_exact_helper_call(&mut self, text: &str) -> bool {
        let Some(plan) = plan_exact_helper_inline(
            text,
            self.defines,
            &self.ir_context.inner.define_bodies,
            &self.inline_stack,
        ) else {
            return false;
        };

        let current_dot = self.current_dot_binding();
        let bindings = bindings_for_helper_arg(
            plan.arg.as_ref(),
            Some(&self.root_bindings),
            current_dot.as_ref(),
        );
        let mut stack = self.inline_stack.clone();
        stack.push(plan.token);
        let mut nested =
            SymbolicWalker::new_with_context(plan.source, self.defines, self.ir_context.clone())
                .with_initial_guards(self.guards.clone())
                .with_inline_stack(stack)
                .with_inline_helpers_in_fragments(true)
                .with_helper_bindings(bindings)
                .with_chart_value_defaults(self.local_state.chart_value_defaults.clone());
        let uses = nested.run(&plan.tree);
        self.uses.extend(uses);
        true
    }

    #[tracing::instrument(skip_all)]
    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>) {
        let Ok(text) = node.utf8_text(self.source.as_bytes()) else {
            return;
        };

        self.inline_static_file_templates_from_helper_calls(text);

        let output_context = output_node_context(self.source, &self.rendered_yaml, node, text);
        let kind = output_context.kind;

        let helper_inlined = self.inline_exact_helper_call(text);

        let helper_analysis = if helper_inlined {
            None
        } else {
            Some(self.analyze_bound_helper_calls(text))
        };
        let value_path_context = self.value_path_context();
        let mut output_values = collect_output_value_analysis(
            text,
            kind,
            &value_path_context,
            &self.local_state.range_domains,
            &self.local_state.get_bindings,
            helper_analysis,
        );
        // Stash chart-level `set X "K" (X.K | default V)` mutations discovered
        // in any helper called from this text. Subsequent `emit_use` calls in
        // this walker attach `Guard::Default { path }` for matching reads,
        // modeling that the helper's `set` has already run by the time those
        // reads are evaluated.
        self.local_state
            .append_chart_value_defaults(&mut output_values.chart_value_defaults);
        if output_values.is_empty() {
            return;
        }

        emit_output_value_analysis(self, &output_context, helper_inlined, output_values);
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn analyze_bound_helper_calls(&self, text: &str) -> BoundHelperAnalysis {
        self.ir_context.inner.helper_summaries.analyze_bound_calls(
            text,
            &self.root_bindings,
            self.current_dot_binding(),
            &self.local_state.fragment_bindings,
            self.fragment_eval_context(),
        )
    }
}

impl ValueUseSink for SymbolicWalker<'_> {
    fn emit_use(&mut self, source_expr: String, path: YamlPath, kind: ValueKind) {
        SymbolicWalker::emit_use(self, source_expr, path, kind);
    }

    fn emit_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    ) {
        SymbolicWalker::emit_use_with_extra_guards(self, source_expr, path, kind, extra_guards);
    }

    fn emit_helper_use(&mut self, source_expr: String) {
        SymbolicWalker::emit_helper_use(self, source_expr);
    }

    fn emit_helper_use_with_extra_guards(&mut self, source_expr: String, extra_guards: &[Guard]) {
        SymbolicWalker::emit_helper_use_with_extra_guards(self, source_expr, extra_guards);
    }

    fn emit_helper_use_kind_with_extra_guards(
        &mut self,
        source_expr: String,
        kind: ValueKind,
        extra_guards: &[Guard],
    ) {
        SymbolicWalker::emit_helper_use_kind_with_extra_guards(
            self,
            source_expr,
            kind,
            extra_guards,
        );
    }
}

impl NodeEvalRuntime for SymbolicWalker<'_> {
    type ScopeSnapshot = WalkerScopeSnapshot;

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

    fn scope_snapshot(&self, include_dot_stack: bool) -> Self::ScopeSnapshot {
        SymbolicWalker::scope_snapshot(self, include_dot_stack)
    }

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot) {
        SymbolicWalker::restore_scope(self, snapshot);
    }

    fn enter_no_output(&mut self) {
        self.no_output_depth += 1;
    }

    fn exit_no_output(&mut self) {
        self.no_output_depth = self.no_output_depth.saturating_sub(1);
    }

    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>) {
        SymbolicWalker::handle_output_node(self, node);
    }

    fn plan_assignment_action(&self, text: &str) -> AssignmentActionPlan {
        let fragment_context = self.fragment_eval_context();
        let current_dot = self.current_dot_binding();
        plan_assignment_action(
            text,
            fragment_context,
            &self.local_state.fragment_bindings,
            &self.root_bindings,
            current_dot.as_ref(),
        )
    }

    fn plan_if_condition(&self, header: &str) -> ConditionActionPlan {
        let value_path_context = self.value_path_context();
        plan_if_condition(
            header,
            &value_path_context,
            &self.local_state.range_domains,
            &self.local_state.get_bindings,
        )
    }

    fn plan_with_condition(&self, header: &str) -> ConditionActionPlan {
        let value_path_context = self.value_path_context();
        plan_with_condition(
            header,
            &value_path_context,
            &self.local_state.range_domains,
            &self.local_state.get_bindings,
        )
    }

    fn plan_range_action(
        &self,
        node: tree_sitter::Node<'_>,
        current_path: &YamlPath,
    ) -> RangeActionPlan {
        let value_path_context = self.value_path_context();
        plan_range_action(node, self.source, &value_path_context, current_path)
    }
}

impl NodeActionEffectSink for SymbolicWalker<'_> {
    fn insert_get_binding(&mut self, variable: String, binding: GetBinding) {
        self.local_state.insert_get_binding(variable, binding);
    }

    fn declare_fragment_binding(&mut self, variable: String, binding: FragmentBinding) {
        self.local_state.declare_fragment_binding(variable, binding);
    }

    fn assign_fragment_binding(&mut self, variable: String, binding: FragmentBinding) {
        self.local_state.assign_fragment_binding(variable, binding);
    }

    fn refresh_default_paths(&mut self, variable: &str, rhs: &str) {
        let default_paths = self
            .value_path_context()
            .resolved_default_fallback_paths(rhs);
        self.local_state.set_default_paths(variable, default_paths);
    }

    fn refresh_helper_output_meta(&mut self, variable: String, rhs: &str) {
        let helper_meta = self.helper_output_meta_for_text(rhs);
        self.local_state.set_output_meta(variable, helper_meta);
    }

    fn push_guard_if_absent(&mut self, guard: Guard) {
        if !self.guards.contains(&guard) {
            self.guards.push(guard);
        }
    }

    fn push_dot_binding(&mut self, binding: Option<FragmentBinding>) {
        self.dot_stack.push(binding);
    }

    fn insert_range_domain(&mut self, variable: String, literals: Vec<String>) {
        self.local_state.insert_range_domain(variable, literals);
    }
}
