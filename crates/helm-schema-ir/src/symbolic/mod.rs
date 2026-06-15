mod inline;
mod output;
mod runtime;

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use helm_schema_ast::DefineIndex;

use crate::Guard;
use crate::contract::ContractIr;
use crate::define_body_cache::{DefineBodyCache, parse_go_template};
use crate::fragment_binding::FragmentBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_analysis::BoundHelperAnalysis;
use crate::helper_binding::HelperBinding;
use crate::helper_summary::HelperSummaryCache;
use crate::node_eval::eval_node;
use crate::predicate::Predicate;
use crate::rendered_yaml_context::RenderedYamlContext;
use crate::symbolic_scope_state::SymbolicScopeState;
use crate::template_expr_cache::clear_template_expr_cache;
use crate::value_path_context::ValuePathContext;

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
    /// [`ContractIr::into_schema_signals`]. Fixture and external inspection
    /// output should first call [`ContractIr::project`] and then explicitly
    /// convert the projection to compatibility DTO rows.
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

    fn current_dot_binding(&self) -> Option<HelperBinding> {
        self.scope.current_dot_binding()
    }

    fn current_dot_fragment(&self) -> Option<FragmentBinding> {
        self.scope.current_dot_fragment()
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
