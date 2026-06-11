use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use helm_schema_ast::{DefineIndex, HelmAst, Literal, TemplateExpr, parse_action_expressions};

use crate::binding::{BoundHelperCallsCacheKey, FragmentBinding, HelperBinding};
use crate::bound_helper_call_analysis::{
    analyze_bound_helper_call_with_fragment_locals, analyze_bound_helper_calls_with_fragment_locals,
};
use crate::bound_value_analysis::{
    GetBinding, extract_bound_values, parse_get_binding, parse_literal_list_range,
};
use crate::define_body_cache::{DefineBodyCache, parse_go_template};
use crate::fragment_binding_eval::fragment_binding_from_outer_expr;
use crate::fragment_expr_eval::{FragmentEvalContext, fragment_binding_from_text};
use crate::fragment_scope_eval::{
    parse_helper_assignment, range_body_emits_sequence_item_from_source,
    range_body_renders_scalar_sequence_items_from_source,
    range_has_destructured_variable_definition, range_header_text_from_source,
};
use crate::helper_analysis::{BoundHelperAnalysis, HelperOutputMeta};
use crate::helper_binding_eval::bindings_for_helper_arg;
use crate::output_node_context::output_node_context;
use crate::output_value_analysis::collect_output_value_analysis;
use crate::output_value_emitter::{ValueUseSink, emit_output_value_analysis};
use crate::rendered_yaml_context::RenderedYamlContext;
use crate::resource_detector::AstResourceDetector;
use crate::static_file_template::{
    StaticFileTemplate, collect_template_requests, literal_helper_calls,
};
use crate::template_expr_cache::{
    clear_template_expr_cache, parse_expr_text as parse_cached_expr_text,
};
use crate::tree_sitter_utils::children_with_field;
use crate::value_path_context::ValuePathContext;
use crate::value_use_postprocess::postprocess_value_uses;
use crate::{Guard, IrGenerator, ResourceRef, ValueKind, ValueUse, YamlPath};

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
    bound_helper_calls_cache: RefCell<BTreeMap<BoundHelperCallsCacheKey, BoundHelperAnalysis>>,
}

impl SymbolicIrContext {
    #[tracing::instrument(skip_all)]
    pub fn new(defines: &DefineIndex) -> Self {
        clear_template_expr_cache();
        Self {
            inner: Rc::new(SymbolicIrContextInner {
                define_bodies: DefineBodyCache::new(defines),
                bound_helper_calls_cache: RefCell::new(BTreeMap::new()),
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

    range_domains: HashMap<String, Vec<String>>,
    get_bindings: HashMap<String, GetBinding>,
    template_bindings: HashMap<String, FragmentBinding>,
    template_default_paths: HashMap<String, BTreeSet<String>>,
    template_output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,

    inline_helpers_in_fragments: bool,
    root_bindings: HashMap<String, HelperBinding>,

    /// Paths the chart has structurally declared as null-tolerant via a
    /// `set OPERAND "KEY" (OPERAND.KEY | default V)` mutation inside a
    /// helper. Populated as the walker traverses templates that include
    /// such helpers; consumed by `emit_use_with_extra_guards` to attach
    /// `Guard::Default { path }` to any subsequent ValueUse whose
    /// `source_expr` matches.
    ///
    /// This models Helm's render-time semantics: a `set` action in a
    /// chart helper run before downstream reads (typical pattern: a
    /// `<chart>.defaultValues` helper `include`d at the top of every
    /// consumer template) means the merged values dict has the default
    /// applied before any read. Reads of that path therefore tolerate a
    /// null from values.yaml — `helm-lint --strict` sees the post-`set`
    /// value, not the raw user input.
    ///
    /// Walker scope is per-template, so a path is only widened in
    /// templates that actually traverse through an include of the
    /// defaulting helper. Templates that read `.Values.X` without
    /// running the helper produce ungrouped uses that the nullability
    /// pass treats as null-intolerant, which is the conservative read.
    chart_value_defaults: BTreeSet<String>,
}

struct WalkerScopeSnapshot {
    guards_len: usize,
    dot_stack_len: Option<usize>,
    range_domains: HashMap<String, Vec<String>>,
    get_bindings: HashMap<String, GetBinding>,
    template_bindings: HashMap<String, FragmentBinding>,
    template_default_paths: HashMap<String, BTreeSet<String>>,
    template_output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
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

            range_domains: HashMap::new(),
            get_bindings: HashMap::new(),
            template_bindings: HashMap::new(),
            template_default_paths: HashMap::new(),
            template_output_meta: HashMap::new(),

            inline_helpers_in_fragments: false,
            root_bindings: HashMap::new(),

            chart_value_defaults: BTreeSet::new(),
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
            analyze_bound_helper_call_with_fragment_locals,
        )
    }

    fn value_path_context(&self) -> ValuePathContext<'_> {
        ValuePathContext {
            root_bindings: &self.root_bindings,
            template_bindings: &self.template_bindings,
            template_default_paths: &self.template_default_paths,
            template_output_meta: &self.template_output_meta,
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
        self.chart_value_defaults = defaults;
        self
    }

    fn scope_snapshot(&self, include_dot_stack: bool) -> WalkerScopeSnapshot {
        WalkerScopeSnapshot {
            guards_len: self.guards.len(),
            dot_stack_len: include_dot_stack.then_some(self.dot_stack.len()),
            range_domains: self.range_domains.clone(),
            get_bindings: self.get_bindings.clone(),
            template_bindings: self.template_bindings.clone(),
            template_default_paths: self.template_default_paths.clone(),
            template_output_meta: self.template_output_meta.clone(),
        }
    }

    fn restore_scope(&mut self, snapshot: WalkerScopeSnapshot) {
        self.guards.truncate(snapshot.guards_len);
        if let Some(dot_stack_len) = snapshot.dot_stack_len {
            self.dot_stack.truncate(dot_stack_len);
        }
        self.range_domains = snapshot.range_domains;
        self.get_bindings = snapshot.get_bindings;
        self.template_bindings = snapshot.template_bindings;
        self.template_default_paths = snapshot.template_default_paths;
        self.template_output_meta = snapshot.template_output_meta;
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
                        &self.template_bindings,
                        current_dot.as_ref(),
                        &mut seen,
                    )
                });
                self.static_file_templates_from_helper(&helper_call.name, helper_dot.as_ref())
            };
            for request in requests {
                self.inline_static_file_template(request);
            }
        }
    }

    fn static_file_templates_from_helper(
        &self,
        name: &str,
        helper_dot: Option<&FragmentBinding>,
    ) -> BTreeSet<StaticFileTemplate> {
        let Some(src) = self.define_body_source(name) else {
            return BTreeSet::new();
        };
        let locals = HashMap::new();
        let context = self.fragment_eval_context();
        let mut requests = BTreeSet::new();
        for expr in parse_action_expressions(src) {
            let mut seen = HashSet::new();
            collect_template_requests(
                &expr,
                &mut |expr| {
                    context.fragment_binding_from_expr(expr, &locals, helper_dot, &mut seen)
                },
                &mut requests,
            );
        }
        requests
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
                .with_chart_value_defaults(self.chart_value_defaults.clone());
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
        self.walk(tree.root_node());
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
        if !path.0.is_empty() && self.chart_value_defaults.contains(&source_expr) {
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

    fn fragment_binding_in_context(
        &self,
        expr: &TemplateExpr,
        current_dot: Option<&FragmentBinding>,
    ) -> Option<FragmentBinding> {
        let context = self.fragment_eval_context();
        let mut seen = HashSet::new();
        context.fragment_binding_from_expr(expr, &self.template_bindings, current_dot, &mut seen)
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

    fn define_body_source(&self, name: &str) -> Option<&str> {
        self.ir_context.inner.define_bodies.source(name)
    }

    fn define_body_resource(&self, name: &str) -> Option<ResourceRef> {
        let body = self.defines.get(name)?;
        let ast = HelmAst::Document {
            items: body.to_vec(),
        };
        AstResourceDetector::new(self.defines).detect(&ast)
    }

    fn inline_exact_helper_call(&mut self, text: &str) -> bool {
        let exprs = Self::parse_expr_text(text);
        if exprs.len() != 1 {
            return false;
        }

        let TemplateExpr::Call { function, args } = &exprs[0] else {
            return false;
        };
        if !matches!(function.as_str(), "include" | "template") {
            return false;
        }
        let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
            return false;
        };
        if self.define_body_resource(name).is_none() {
            return false;
        }

        let Some(src) = self.define_body_source(name) else {
            return false;
        };
        let token = format!("define:{name}");
        if self.inline_stack.iter().any(|entry| entry == &token) {
            return false;
        }
        let Some(tree) = self.ir_context.inner.define_bodies.tree(name) else {
            return false;
        };

        let current_dot = self.current_dot_binding();
        let bindings =
            bindings_for_helper_arg(args.get(1), Some(&self.root_bindings), current_dot.as_ref());
        let mut stack = self.inline_stack.clone();
        stack.push(token);
        let mut nested =
            SymbolicWalker::new_with_context(src, self.defines, self.ir_context.clone())
                .with_initial_guards(self.guards.clone())
                .with_inline_stack(stack)
                .with_inline_helpers_in_fragments(true)
                .with_helper_bindings(bindings)
                .with_chart_value_defaults(self.chart_value_defaults.clone());
        let uses = nested.run(&tree);
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
            &self.range_domains,
            &self.get_bindings,
            helper_analysis,
        );
        // Stash chart-level `set X "K" (X.K | default V)` mutations discovered
        // in any helper called from this text. Subsequent `emit_use` calls in
        // this walker attach `Guard::Default { path }` for matching reads,
        // modeling that the helper's `set` has already run by the time those
        // reads are evaluated.
        self.chart_value_defaults
            .append(&mut output_values.chart_value_defaults);
        if output_values.is_empty() {
            return;
        }

        emit_output_value_analysis(self, &output_context, helper_inlined, output_values);
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn parse_expr_text(text: &str) -> Vec<TemplateExpr> {
        parse_cached_expr_text(text)
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn analyze_bound_helper_calls(&self, text: &str) -> BoundHelperAnalysis {
        let current_dot = self.current_dot_binding();
        let root_bindings: BTreeMap<String, HelperBinding> = self
            .root_bindings
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let fragment_locals: BTreeMap<String, FragmentBinding> = self
            .template_bindings
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let key = BoundHelperCallsCacheKey {
            text: text.to_string(),
            current_dot: current_dot.clone(),
            root_bindings: root_bindings.clone(),
            fragment_locals: fragment_locals.clone(),
        };
        if let Some(cached) = self
            .ir_context
            .inner
            .bound_helper_calls_cache
            .borrow()
            .get(&key)
        {
            return cached.clone();
        }

        let mut seen = HashSet::new();
        let context = self.fragment_eval_context();
        let analysis = analyze_bound_helper_calls_with_fragment_locals(
            text,
            if self.root_bindings.is_empty() {
                None
            } else {
                Some(&self.root_bindings)
            },
            current_dot.as_ref(),
            &self.template_bindings,
            context,
            &mut seen,
        );
        self.ir_context
            .inner
            .bound_helper_calls_cache
            .borrow_mut()
            .insert(key, analysis.clone());
        analysis
    }

    fn collect_if_with_guards(&mut self, cond_text: &str) {
        let cond_guards = self.value_path_context().condition_guards(cond_text);

        for v in extract_bound_values(cond_text, &self.range_domains, &self.get_bindings) {
            self.emit_use(v, YamlPath(Vec::new()), ValueKind::Scalar);
        }

        for g in &cond_guards {
            for path in g.value_paths() {
                self.emit_use_with_extra_guards(
                    path.to_string(),
                    YamlPath(Vec::new()),
                    ValueKind::Scalar,
                    std::slice::from_ref(g),
                );
            }
            if !self.guards.contains(g) {
                self.guards.push(g.clone());
            }
        }
    }

    fn collect_with_guards(&mut self, cond_text: &str) {
        let cond_guards = self.value_path_context().with_condition_guards(cond_text);

        // Push the With guards before emitting header scalar uses so the
        // emitted uses themselves carry the With guard. This lets the schema
        // generator identify with-header uses by the presence of a matching
        // `Guard::With { path: source_expr }` in the use's guard list.
        for g in &cond_guards {
            if !self.guards.contains(g) {
                self.guards.push(g.clone());
            }
        }

        for v in extract_bound_values(cond_text, &self.range_domains, &self.get_bindings) {
            self.emit_use(v, YamlPath(Vec::new()), ValueKind::Scalar);
        }

        for g in &cond_guards {
            for path in g.value_paths() {
                self.emit_use(path.to_string(), YamlPath(Vec::new()), ValueKind::Scalar);
            }
        }
    }

    fn push_with_dot_binding(&mut self, header_text: &str) {
        let mut locals = self.template_bindings.clone();
        for (key, value) in &self.root_bindings {
            locals.insert(key.clone(), value.to_fragment_binding());
        }
        let current_dot = self.current_dot_fragment();
        let current_dot_binding = self.current_dot_binding();
        let exprs = Self::parse_expr_text(header_text);
        let binding = match exprs.as_slice() {
            [expr] => fragment_binding_from_outer_expr(
                expr,
                Some(&locals),
                Some(&self.root_bindings),
                current_dot_binding.as_ref(),
            )
            .or_else(|| self.fragment_binding_in_context(expr, current_dot.as_ref())),
            _ => None,
        };
        self.dot_stack.push(binding);
    }

    fn collect_range_guards(&mut self, header_text: &str, path: &YamlPath, emit_use: bool) {
        let values = self.range_source_paths(header_text);
        for v in &values {
            let guard = Guard::Range { path: v.clone() };
            if emit_use {
                self.emit_use_with_extra_guards(
                    v.clone(),
                    path.clone(),
                    ValueKind::Scalar,
                    std::slice::from_ref(&guard),
                );
            }
            if !self.guards.contains(&guard) {
                self.guards.push(guard);
            }
        }
    }

    fn range_source_paths(&self, header_text: &str) -> Vec<String> {
        self.value_path_context().resolved_values_paths(header_text)
    }

    fn direct_iterable_header_path(&self, header_text: &str) -> Option<String> {
        let mut txt = header_text.trim();
        loop {
            let trimmed = txt.trim();
            if trimmed.len() >= 2 && trimmed.starts_with('(') && trimmed.ends_with(')') {
                txt = &trimmed[1..trimmed.len() - 1];
                continue;
            }
            break;
        }

        self.value_path_context()
            .single_direct_iterable_range_path(txt)
    }

    fn walk(&mut self, node: tree_sitter::Node<'_>) {
        self.rendered_yaml.enter_node(node);

        if self.walk_control_node(node) {
            return;
        }
        if self.walk_action_node(node) {
            return;
        }

        let mut c = node.walk();
        for ch in node.children(&mut c) {
            self.walk(ch);
        }
    }

    fn walk_control_node(&mut self, node: tree_sitter::Node<'_>) -> bool {
        match node.kind() {
            "text" | "yaml_no_injection_text" => {
                self.rendered_yaml.ingest_text_up_to(node.end_byte());
                true
            }
            "define_action" | "block_action" => true,
            _ => false,
        }
    }

    fn walk_action_node(&mut self, node: tree_sitter::Node<'_>) -> bool {
        match node.kind() {
            "variable_definition" | "assignment" => {
                self.handle_variable_definition_or_assignment(node);
                true
            }
            "if_action" => {
                self.handle_if_action(node);
                true
            }
            "with_action" => {
                self.handle_with_action(node);
                true
            }
            "range_action" => {
                self.handle_range_action(node);
                true
            }
            "template_action"
            | "dot"
            | "variable"
            | "field"
            | "chained_pipeline"
            | "parenthesized_pipeline"
            | "selector_expression"
            | "function_call"
            | "method_call" => {
                self.handle_output_node(node);
                true
            }
            _ => false,
        }
    }

    fn handle_variable_definition_or_assignment(&mut self, node: tree_sitter::Node<'_>) {
        if let Ok(txt) = node.utf8_text(self.source.as_bytes()) {
            if let Some((var, binding)) = parse_get_binding(txt) {
                self.get_bindings.insert(var, binding);
            }

            if let Some((var, _declares, rhs)) = parse_helper_assignment(txt) {
                let mut locals = self.template_bindings.clone();
                for (key, value) in &self.root_bindings {
                    locals.insert(key.clone(), value.to_fragment_binding());
                }
                let current_dot = self
                    .current_dot_binding()
                    .map(|binding| binding.to_fragment_binding());
                let context = self.fragment_eval_context();
                let mut seen = HashSet::new();
                if let Some(binding) = fragment_binding_from_text(
                    &rhs,
                    &locals,
                    current_dot.as_ref(),
                    context,
                    &mut seen,
                ) {
                    self.template_bindings.insert(var.clone(), binding);
                }
                let default_paths = self
                    .value_path_context()
                    .resolved_default_fallback_paths(&rhs);
                if default_paths.is_empty() {
                    self.template_default_paths.remove(&var);
                } else {
                    self.template_default_paths
                        .insert(var.clone(), default_paths);
                }

                let helper_meta = self.helper_output_meta_for_text(&rhs);
                if helper_meta.is_empty() {
                    self.template_output_meta.remove(&var);
                } else {
                    self.template_output_meta.insert(var, helper_meta);
                }
            }
        }

        self.no_output_depth += 1;
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            self.walk(ch);
        }
        self.no_output_depth = self.no_output_depth.saturating_sub(1);
    }

    fn handle_if_action(&mut self, node: tree_sitter::Node<'_>) {
        let saved = self.scope_snapshot(false);

        if let Some(cond) = node.child_by_field_name("condition")
            && let Ok(txt) = cond.utf8_text(self.source.as_bytes())
        {
            self.collect_if_with_guards(txt);
        }

        let consequence = children_with_field(node, "consequence");
        for ch in consequence {
            self.walk(ch);
        }

        self.restore_scope(saved);

        // Note: else-if chains are represented as repeated condition/option fields.
        // For now, we only handle the plain else branch.
        let alternative = children_with_field(node, "alternative");
        for ch in alternative {
            self.walk(ch);
        }
    }

    fn handle_with_action(&mut self, node: tree_sitter::Node<'_>) {
        let saved = self.scope_snapshot(true);

        if let Some(cond) = node.child_by_field_name("condition")
            && let Ok(txt) = cond.utf8_text(self.source.as_bytes())
        {
            self.collect_with_guards(txt);
            self.push_with_dot_binding(txt);
        }

        let consequence = children_with_field(node, "consequence");
        for ch in consequence {
            self.walk(ch);
        }

        self.restore_scope(saved);

        let alternative = children_with_field(node, "alternative");
        for ch in alternative {
            self.walk(ch);
        }
    }

    fn handle_range_action(&mut self, node: tree_sitter::Node<'_>) {
        let saved = self.scope_snapshot(true);

        let mut header_text: Option<String> = None;
        let has_variable_definition = range_has_destructured_variable_definition(node);
        let body_emits_sequence_item =
            range_body_emits_sequence_item_from_source(node, self.source);
        let body_renders_scalar_sequence_items = !has_variable_definition
            && range_body_renders_scalar_sequence_items_from_source(node, self.source);
        if let Some(txt) = range_header_text_from_source(node, self.source) {
            header_text = Some(txt.clone());
            if let Some((var, literals)) = parse_literal_list_range(&txt) {
                self.range_domains.insert(var, literals);
            }
            let current_path = self.rendered_yaml.current_path();
            let direct_iterable_header_path = self.direct_iterable_header_path(&txt);
            let guard_path = if has_variable_definition {
                // Destructured range headers (`range $k, $v := .Values.map`) describe
                // the INPUT collection, not the rendered YAML shape of each output item.
                // Attaching the current rendered path here lets downstream provider
                // schemas project output arrays (for example `env:`) back onto map-like
                // chart inputs such as `.Values.environment`, producing bogus
                // `object | array` unions. Keep the header use pathless; values.yaml and
                // body uses still carry the input contract.
                YamlPath(Vec::new())
            } else if body_emits_sequence_item
                && body_renders_scalar_sequence_items
                && direct_iterable_header_path.is_some()
            {
                // A direct iterable source contributes the whole collection to the
                // current YAML sequence field only when each input item becomes the
                // rendered sequence item directly (`- {{ . }}`).
                self.rendered_yaml.current_path()
            } else {
                YamlPath(Vec::new())
            };
            let emit_header_use = has_variable_definition
                || !body_emits_sequence_item
                || (body_renders_scalar_sequence_items && direct_iterable_header_path.is_some());
            self.collect_range_guards(&txt, &guard_path, emit_header_use);

            let renders_mapping_entries = has_variable_definition
                && !body_emits_sequence_item
                && !current_path.0.is_empty()
                && current_path
                    .0
                    .last()
                    .is_some_and(|segment| !segment.ends_with("[*]"));
            if renders_mapping_entries {
                // A destructured map range under a concrete object field
                // (`annotations:`, `matchLabels:`, ...) is effectively
                // rendering a YAML fragment for the whole source map.
                // Keep the header's scalar use pathless to avoid projecting
                // array output shapes like `env:` back onto map inputs, and
                // emit this separate fragment use so provider object schemas
                // can still type the destination field precisely.
                for source_path in self.range_source_paths(&txt) {
                    self.emit_use(source_path, current_path.clone(), ValueKind::Fragment);
                }
            }
        }

        // If the range header is a single `.Values.*` path, treat `.` inside
        // the range body as one item of that collection:
        //   {{- range .Values.someList }}
        //     {{ .name }}
        //   {{- end }}
        let dot_prefix = header_text
            .as_deref()
            .and_then(|raw| self.direct_iterable_header_path(raw))
            .map(|path| FragmentBinding::ValuesPath(format!("{path}.*")));

        self.dot_stack.push(dot_prefix);

        let body = children_with_field(node, "body");
        for ch in body {
            self.walk(ch);
        }

        self.restore_scope(saved);

        let alternative = children_with_field(node, "alternative");
        for ch in alternative {
            self.walk(ch);
        }
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
