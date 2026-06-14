use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use helm_schema_ast::DefineIndex;

use crate::abstract_document::AbstractDocumentOutput;
use crate::assignment_action_plan::{AssignmentActionPlan, plan_assignment_action};
use crate::binding::{FragmentBinding, HelperBinding};
use crate::bound_value_analysis::GetBindingPlan;
use crate::condition_action_plan::{ConditionActionPlan, plan_if_condition, plan_with_condition};
use crate::contract::ContractIr;
use crate::contract_sink::{ContractUseContext, ContractUseSink};
use crate::define_body_cache::{DefineBodyCache, parse_go_template};
use crate::document_hole_context::collect_document_hole_context;
use crate::document_value_analysis::collect_document_value_analysis;
use crate::expression_analysis::helper_bindings_for_arg;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_analysis::{
    BoundHelperAnalysis, HelperOutputMeta, helper_output_meta_from_analysis,
};
use crate::helper_inline::plan_exact_helper_inline;
use crate::helper_summary::HelperSummaryCache;
use crate::node_action_effect::NodeActionEffectSink;
use crate::node_eval::{NodeEvalRuntime, eval_node};
use crate::predicate::Predicate;
use crate::range_action_plan::{RangeActionPlan, plan_range_action};
use crate::rendered_yaml_context::RenderedYamlContext;
use crate::static_file_template::{
    StaticFileTemplate, collect_template_requests_from_helper, literal_helper_calls,
};
use crate::symbolic_scope_state::{SymbolicScopeSnapshot, SymbolicScopeState};
use crate::template_expr_cache::clear_template_expr_cache;
use crate::value_path_context::ValuePathContext;
use crate::{Guard, ValueKind, YamlPath};

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

    /// Generate the opaque contract graph without projecting to fixture DTOs.
    ///
    /// Callers that need to combine, scope, or otherwise transform chart-local
    /// contracts should use this method and derive schema facts with
    /// [`ContractIr::into_schema_signals`]. [`ContractIr::into_value_uses`] is
    /// reserved for fixture and external inspection output.
    pub fn generate_contract_ir(&self, src: &str, defines: &DefineIndex) -> ContractIr {
        let Some(tree) = parse_go_template(src) else {
            return ContractIr::default();
        };

        let mut w = SymbolicWalker::new_with_context(src, defines, self.clone());
        w.run_contract(&tree)
    }
}

struct SymbolicWalker<'a> {
    source: &'a str,
    defines: &'a DefineIndex,
    ir_context: SymbolicIrContext,
    contract: ContractIr,
    seed_predicates: Vec<Predicate>,
    seed_dot: Option<FragmentBinding>,
    no_output_depth: usize,
    rendered_yaml: RenderedYamlContext<'a>,

    inline_stack: Vec<String>,

    scope: SymbolicScopeState,

    inline_helpers_in_fragments: bool,
    root_bindings: HashMap<String, HelperBinding>,
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
            contract: ContractIr::default(),
            seed_predicates: Vec::new(),
            seed_dot: None,
            no_output_depth: 0,
            rendered_yaml: RenderedYamlContext::new(source, defines),

            inline_stack: Vec::new(),

            scope: SymbolicScopeState::default(),

            inline_helpers_in_fragments: false,
            root_bindings: HashMap::new(),
        }
    }

    fn with_initial_predicates(mut self, predicates: Vec<Predicate>) -> Self {
        self.seed_predicates = predicates;
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
            template_bindings: &self.scope.locals().fragment_bindings,
            template_default_paths: &self.scope.locals().default_paths,
            template_output_meta: &self.scope.locals().output_meta,
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
        self.scope.locals_mut().set_chart_value_defaults(defaults);
        self
    }

    fn scope_snapshot(&self) -> SymbolicScopeSnapshot {
        self.scope.snapshot()
    }

    fn restore_scope(&mut self, snapshot: SymbolicScopeSnapshot) {
        self.scope.restore(snapshot);
    }

    fn join_branch_scopes(
        &mut self,
        entry: &SymbolicScopeSnapshot,
        outcomes: Vec<SymbolicScopeSnapshot>,
    ) {
        self.scope.join_branch_outcomes(entry, outcomes);
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
                        &self.scope.locals().fragment_bindings,
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
                .with_initial_predicates(self.scope.predicates().to_vec())
                .with_initial_dot_binding(request.dot)
                .with_inline_stack(stack)
                .with_inline_helpers_in_fragments(true)
                .with_chart_value_defaults(self.scope.locals().chart_value_defaults.clone());
        let contract = nested.run_contract(&tree);
        self.contract.append(contract);
    }

    fn run_contract(&mut self, tree: &tree_sitter::Tree) -> ContractIr {
        self.rendered_yaml.reset_for_tree(tree);
        self.scope
            .reset_control(&self.seed_predicates, self.seed_dot.clone());
        self.no_output_depth = 0;
        eval_node(self, tree.root_node());
        std::mem::take(&mut self.contract)
    }

    fn compatibility_guards(&self) -> Vec<Guard> {
        self.scope.compatibility_guards()
    }

    fn emit_contract_use(&mut self, source_expr: String, path: YamlPath, kind: ValueKind) {
        self.emit_contract_use_with_extra_guards(source_expr, path, kind, &[]);
    }

    fn emit_contract_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    ) {
        self.emit_contract_use_with_resource(
            source_expr,
            path,
            kind,
            extra_guards,
            self.rendered_yaml.current_resource().cloned(),
        );
    }

    fn emit_contract_use_with_resource(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
        resource: Option<crate::ResourceRef>,
    ) {
        let path = self.rendered_yaml.rebase_path(path);
        let guards = self.compatibility_guards();
        let context = ContractUseContext::new(
            &guards,
            &self.scope.locals().chart_value_defaults,
            self.no_output_depth > 0,
        );
        self.contract
            .push(context.contract_use(source_expr, path, kind, extra_guards, resource));
    }

    fn current_dot_binding(&self) -> Option<HelperBinding> {
        self.scope.current_dot_binding()
    }

    fn current_dot_fragment(&self) -> Option<FragmentBinding> {
        self.scope.current_dot_fragment()
    }

    fn helper_output_meta_for_text(&self, text: &str) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = self
            .value_path_context()
            .local_alias_output_meta_for_text(text);
        let analysis = self.analyze_bound_helper_calls(text);
        for (path, meta) in helper_output_meta_from_analysis(&analysis) {
            out.entry(path).or_default().merge(meta);
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
        let bindings = helper_bindings_for_arg(
            plan.arg.as_ref(),
            Some(&self.root_bindings),
            current_dot.as_ref(),
        );
        let mut stack = self.inline_stack.clone();
        stack.push(plan.token);
        let mut nested =
            SymbolicWalker::new_with_context(plan.source, self.defines, self.ir_context.clone())
                .with_initial_predicates(self.scope.predicates().to_vec())
                .with_inline_stack(stack)
                .with_inline_helpers_in_fragments(true)
                .with_helper_bindings(bindings)
                .with_chart_value_defaults(self.scope.locals().chart_value_defaults.clone());
        let contract = nested.run_contract(&plan.tree);
        self.contract.append(contract);
        true
    }

    #[tracing::instrument(skip_all)]
    fn handle_output_node(&mut self, node: tree_sitter::Node<'_>) {
        let Ok(text) = node.utf8_text(self.source.as_bytes()) else {
            return;
        };

        self.inline_static_file_templates_from_helper_calls(text);

        let hole_context =
            collect_document_hole_context(self.source, &self.rendered_yaml, node, text);
        let kind = hole_context.kind;

        let helper_inlined = self.inline_exact_helper_call(text);

        let helper_analysis = if helper_inlined {
            None
        } else {
            Some(self.analyze_bound_helper_calls(text))
        };
        let value_path_context = self.value_path_context();
        let mut output_values = collect_document_value_analysis(
            text,
            kind,
            &value_path_context,
            &self.scope.locals().range_domains,
            &self.scope.locals().get_bindings,
            helper_analysis,
        );
        // Stash chart-level `set X "K" (X.K | default V)` mutations discovered
        // in any helper called from this text. Subsequent contract emissions
        // in this walker attach `Guard::Default { path }` for matching reads,
        // modeling that the helper's `set` has already run by the time those
        // reads are evaluated.
        self.scope
            .locals_mut()
            .append_chart_value_defaults(&mut output_values.chart_value_defaults);
        if output_values.is_empty() {
            return;
        }

        let document_contract = {
            let guards = self.compatibility_guards();
            let projection_context = ContractUseContext::new(
                &guards,
                &self.scope.locals().chart_value_defaults,
                self.no_output_depth > 0,
            );
            AbstractDocumentOutput::new(hole_context, helper_inlined, output_values)
                .into_contract_ir(&projection_context)
        };
        self.contract.append(document_contract);
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn analyze_bound_helper_calls(&self, text: &str) -> BoundHelperAnalysis {
        self.ir_context.inner.helper_summaries.analyze_bound_calls(
            text,
            &self.root_bindings,
            self.current_dot_binding(),
            &self.scope.locals().fragment_bindings,
            self.fragment_eval_context(),
        )
    }
}

impl ContractUseSink for SymbolicWalker<'_> {
    fn emit_contract_use(&mut self, source_expr: String, path: YamlPath, kind: ValueKind) {
        SymbolicWalker::emit_contract_use(self, source_expr, path, kind);
    }

    fn emit_contract_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    ) {
        SymbolicWalker::emit_contract_use_with_extra_guards(
            self,
            source_expr,
            path,
            kind,
            extra_guards,
        );
    }
}

impl NodeEvalRuntime for SymbolicWalker<'_> {
    type ScopeSnapshot = SymbolicScopeSnapshot;

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
        SymbolicWalker::scope_snapshot(self)
    }

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot) {
        SymbolicWalker::restore_scope(self, snapshot);
    }

    fn enter_local_scope(&mut self) {
        self.scope.locals_mut().enter_local_scope();
    }

    fn exit_local_scope(&mut self) {
        self.scope.locals_mut().exit_local_scope();
    }

    fn join_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        SymbolicWalker::join_branch_scopes(self, entry, outcomes);
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
            &self.scope.locals().fragment_bindings,
            &self.root_bindings,
            current_dot.as_ref(),
        )
    }

    fn plan_if_condition(&mut self, header: &str) -> ConditionActionPlan {
        let value_path_context = self.value_path_context();
        plan_if_condition(
            header,
            &value_path_context,
            &self.scope.locals().range_domains,
            &self.scope.locals().get_bindings,
        )
    }

    fn plan_with_condition(&mut self, header: &str) -> ConditionActionPlan {
        let value_path_context = self.value_path_context();
        plan_with_condition(
            header,
            &value_path_context,
            &self.scope.locals().range_domains,
            &self.scope.locals().get_bindings,
        )
    }

    fn plan_range_action(
        &mut self,
        node: tree_sitter::Node<'_>,
        current_path: &YamlPath,
    ) -> RangeActionPlan {
        let value_path_context = self.value_path_context();
        plan_range_action(node, self.source, &value_path_context, current_path)
    }
}

impl NodeActionEffectSink for SymbolicWalker<'_> {
    fn apply_get_binding(&mut self, plan: GetBindingPlan) {
        self.scope.locals_mut().apply_get_binding(plan);
    }

    fn declare_fragment_binding(&mut self, variable: String, binding: Option<FragmentBinding>) {
        self.scope
            .locals_mut()
            .declare_fragment_binding(variable, binding);
    }

    fn assign_fragment_binding(&mut self, variable: String, binding: Option<FragmentBinding>) {
        self.scope
            .locals_mut()
            .assign_fragment_binding(variable, binding);
    }

    fn refresh_default_paths(&mut self, variable: &str, rhs: &str) {
        let default_paths = self
            .value_path_context()
            .resolved_default_fallback_paths(rhs);
        self.scope
            .locals_mut()
            .set_default_paths(variable, default_paths);
    }

    fn refresh_helper_output_meta(&mut self, variable: String, rhs: &str) {
        let helper_meta = self.helper_output_meta_for_text(rhs);
        self.scope
            .locals_mut()
            .set_output_meta(variable, helper_meta);
    }

    fn push_predicate_if_absent(&mut self, predicate: Predicate) {
        self.scope.push_predicate_if_absent(predicate);
    }

    fn push_dot_binding(&mut self, binding: Option<FragmentBinding>) {
        self.scope.push_dot_binding(binding);
    }

    fn insert_range_domain(&mut self, variable: String, literals: Vec<String>) {
        self.scope
            .locals_mut()
            .insert_range_domain(variable, literals);
    }
}
