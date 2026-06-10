use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use helm_schema_ast::{DefineIndex, HelmAst, Literal, TemplateExpr, parse_action_expressions};

use crate::binding::{BoundHelperCallsCacheKey, FragmentBinding, HelperBinding};
use crate::bound_value_analysis::{
    GetBinding, extract_bound_values, parse_get_binding, parse_literal_list_range,
};
use crate::define_body_cache::{DefineBodyCache, parse_go_template};
use crate::expression_analysis::{
    resolved_default_fallback_paths_for_text, resolved_string_transform_paths_for_text,
    resolved_type_is_paths_for_text, set_default_chart_paths_for_text,
};
use crate::fragment_binding_eval::{
    fragment_binding_from_helper_analysis, fragment_binding_from_outer_expr,
};
use crate::fragment_expr_eval::{
    FragmentEvalContext, bindings_for_helper_arg_with_fragment_locals, fragment_binding_from_text,
    helper_binding_from_expr_with_fragment_locals,
};
use crate::fragment_scope_eval::{
    apply_local_set_mutations, merge_fragment_locals, parse_helper_assignment,
    range_body_emits_sequence_item_from_source, range_has_destructured_variable_definition,
    range_header_text_from_source, range_iterable_binding, range_variable_item_binding,
    range_variable_name,
};
use crate::helper_analysis::{
    BoundHelperAnalysis, HelperFragmentOutputUse, HelperOutputMeta, bound_helper_condition_paths,
    bound_helper_dependency_paths, convert_fragment_outputs_to_dependency_outputs,
    extend_type_hints, helper_dependency_meta_from_analysis, helper_output_meta_from_analysis,
    merge_helper_output_meta_maps, merge_local_default_paths,
};
use crate::helper_binding_eval::{binding_from_expr, bindings_for_helper_arg};
use crate::helper_output_projection::{
    HelperOutputExprContext, collect_fragment_binding_output_uses,
    collect_helper_binding_output_uses, collect_helper_binding_output_uses_from_expr,
    expression_output_use_is_keyed_map_projection, helper_output_meta_with_guards,
    push_helper_fragment_output, static_yaml_fragment_output_path,
};
use crate::local_projection::{
    direct_bound_paths_from_text_in_context, local_bound_paths_from_text,
    local_default_paths_from_text, local_output_meta_from_text, local_rendered_paths_from_text,
};
use crate::output_path;
use crate::rendered_yaml_context::RenderedYamlContext;
use crate::resource_detector::AstResourceDetector;
use crate::static_file_template::{
    StaticFileTemplate, collect_template_requests, literal_helper_calls,
};
use crate::template_expr_analysis::{
    expr_contains_helper_call, text_pipeline_merges_into_var, text_starts_with_helper_call,
    walk_expr_excluding_helper_call_args,
};
use crate::template_expr_cache::{
    clear_template_expr_cache, parse_expr_text as parse_cached_expr_text,
};
use crate::value_path_context::ValuePathContext;
use crate::value_use_postprocess::postprocess_value_uses;
use crate::walker::is_fragment_expr;
use crate::yaml_shape::{first_mapping_colon_offset, parse_yaml_key};
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

struct FragmentOutputWalkState<'context, 'state> {
    local_bindings: &'state mut HashMap<String, FragmentBinding>,
    local_default_paths: &'state mut HashMap<String, BTreeSet<String>>,
    context: FragmentEvalContext<'context>,
    seen: &'state mut HashSet<String>,
    outputs: &'state mut Vec<HelperFragmentOutputUse>,
}

struct HelperValuesWalkState<'context, 'state> {
    local_bindings: &'state mut HashMap<String, FragmentBinding>,
    local_default_paths: &'state mut HashMap<String, BTreeSet<String>>,
    local_output_meta: &'state mut HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    context: FragmentEvalContext<'context>,
    seen: &'state mut HashSet<String>,
    analysis: &'state mut BoundHelperAnalysis,
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
            Self::analyze_bound_helper_call_with_fragment_locals,
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

    fn children_with_field<'n>(
        node: tree_sitter::Node<'n>,
        field: &str,
    ) -> Vec<tree_sitter::Node<'n>> {
        let mut cursor = node.walk();
        node.children_by_field_name(field, &mut cursor)
            .filter(tree_sitter::Node::is_named)
            .collect()
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

    fn helper_output_extra_guards(source_expr: &str, meta: &HelperOutputMeta) -> Vec<Guard> {
        let mut guards: Vec<Guard> = meta
            .guards
            .iter()
            .filter(|path| !path.trim().is_empty())
            .cloned()
            .map(|path| Guard::Truthy { path })
            .collect();
        if meta.defaulted {
            guards.push(Guard::Default {
                path: source_expr.to_string(),
            });
        }
        guards
    }

    fn helper_dependency_extra_guards(source_expr: &str, meta: &HelperOutputMeta) -> Vec<Guard> {
        let mut guards: Vec<Guard> = meta
            .guards
            .iter()
            .filter(|path| !path.trim().is_empty())
            .cloned()
            .map(|path| Guard::Truthy { path })
            .collect();
        if meta.defaulted {
            guards.push(Guard::Default {
                path: source_expr.to_string(),
            });
        }
        guards
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

        let enclosing_action_text = self.enclosing_action_text(node);
        let kind = if enclosing_action_text
            .as_deref()
            .is_some_and(is_fragment_expr)
            || is_fragment_expr(text)
        {
            ValueKind::Fragment
        } else {
            ValueKind::Scalar
        };

        let in_mapping_key = self.output_node_is_mapping_key_part(node);
        let mut path = if in_mapping_key {
            YamlPath(Vec::new())
        } else {
            self.rendered_yaml.current_path()
        };
        if kind == ValueKind::Fragment
            && let Ok(node_text) = node.utf8_text(self.source.as_bytes())
        {
            let (physical_indent, _physical_col) =
                self.rendered_yaml.line_indent_and_col(node.start_byte());
            if self
                .rendered_yaml
                .starts_template_action_line(node.start_byte())
            {
                let mut logical_indent = physical_indent;
                if let Some(virtual_indent) = RenderedYamlContext::fragment_indent_width(node_text)
                {
                    logical_indent = virtual_indent;
                }

                let trailing_pending_segments = self
                    .rendered_yaml
                    .trailing_pending_mapping_segments_at_or_above(logical_indent);
                for _ in 0..trailing_pending_segments {
                    path.0.pop();
                }
            }
        }
        if kind == ValueKind::Fragment {
            if let Some(last) = path.0.last_mut()
                && let Some(stripped) = last.strip_suffix("[*]")
            {
                *last = stripped.to_string();
            }
            if matches!(path.0.last().map(std::string::String::as_str), Some("")) {
                path.0.pop();
            }
            if let Some(inline_path) = self.rendered_yaml.inline_mapping_value_path(node) {
                path = inline_path;
            }
        }
        if self
            .rendered_yaml
            .output_inside_block_scalar_at(node.start_byte())
        {
            path = YamlPath(Vec::new());
        }

        let helper_inlined = self.inline_exact_helper_call(text);

        let (default_fallback_values, values, local_output_meta) = {
            let context = self.value_path_context();
            let default_fallback_values = context.resolved_default_fallback_paths(text);
            let mut values: BTreeSet<String> =
                context.resolved_values_paths(text).into_iter().collect();
            let local_output_meta = context.local_alias_output_meta_for_text(text);
            values.extend(default_fallback_values.iter().cloned());
            (default_fallback_values, values, local_output_meta)
        };

        let bound_values = extract_bound_values(text, &self.range_domains, &self.get_bindings);

        let mut helper_output_values: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        let mut helper_fragment_output_values: Vec<String> = Vec::new();
        let mut helper_fragment_output_uses: Vec<HelperFragmentOutputUse> = Vec::new();
        let mut helper_dependency_values: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        let mut helper_guard_values: BTreeSet<String> = BTreeSet::new();
        let mut helper_type_hints: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut suppress_direct_values: BTreeSet<String> = BTreeSet::new();
        if !helper_inlined {
            let bound = self.analyze_bound_helper_calls(text);
            helper_output_values.extend(bound.output);
            helper_fragment_output_uses.extend(bound.fragment_output_uses);
            for path in bound.dependency_paths {
                helper_dependency_values.entry(path).or_default();
            }
            for (path, meta) in bound.dependency_meta {
                let entry = helper_dependency_values.entry(path).or_default();
                entry.guards.extend(meta.guards);
                entry.defaulted |= meta.defaulted;
            }
            if kind == ValueKind::Fragment {
                helper_fragment_output_values.extend(bound.fragment_output);
            }
            helper_guard_values.extend(bound.guard_paths);
            extend_type_hints(&mut helper_type_hints, bound.type_hints);
            suppress_direct_values.extend(bound.suppress_roots);
            // Stash chart-level `set X "K" (X.K | default V)` mutations
            // discovered in any helper called from this text. Subsequent
            // `emit_use` calls in this walker (same template scope) will
            // attach `Guard::Default { path }` for matching reads,
            // modeling that the helper's `set` has already run by the
            // time those reads are evaluated.
            self.chart_value_defaults.extend(bound.chart_defaults);
            helper_fragment_output_values.sort();
            helper_fragment_output_values.dedup();
        }

        if values.is_empty()
            && bound_values.is_empty()
            && helper_output_values.is_empty()
            && helper_fragment_output_values.is_empty()
            && helper_fragment_output_uses.is_empty()
            && helper_dependency_values.is_empty()
            && helper_guard_values.is_empty()
            && helper_type_hints.is_empty()
        {
            return;
        }
        for v in values {
            if suppress_direct_values.contains(&v) {
                self.emit_use(v, YamlPath(Vec::new()), ValueKind::Scalar);
                continue;
            }
            let in_sequence_item = path
                .0
                .last()
                .map(std::string::String::as_str)
                .is_some_and(|s| s.ends_with("[*]"));

            let emit_path = if v.ends_with(".*") && !in_sequence_item {
                YamlPath(Vec::new())
            } else {
                path.clone()
            };
            let default_guard = Guard::Default { path: v.clone() };
            let mut extra_guards: Vec<Guard> = Vec::new();
            if let Some(meta) = local_output_meta.get(&v) {
                extra_guards.extend(
                    meta.guards
                        .iter()
                        .cloned()
                        .map(|path| Guard::Truthy { path }),
                );
                if meta.defaulted {
                    extra_guards.push(default_guard.clone());
                }
            }
            if default_fallback_values.contains(&v) {
                extra_guards.push(default_guard);
            }
            if extra_guards.is_empty() {
                self.emit_use(v, emit_path, kind);
            } else {
                self.emit_use_with_extra_guards(v, emit_path, kind, &extra_guards);
            }
        }

        for v in bound_values {
            self.emit_use(v, YamlPath(Vec::new()), ValueKind::Scalar);
        }

        let helper_call_caller_scalar_path = !helper_inlined
            && !in_mapping_key
            && !path.0.is_empty()
            && !helper_output_values.is_empty()
            && helper_fragment_output_values.is_empty()
            && helper_fragment_output_uses.is_empty()
            && kind == ValueKind::Scalar
            && self.output_node_is_entire_scalar_value(node);
        let helper_call_caller_fragment_path = !helper_inlined
            && !in_mapping_key
            && !path.0.is_empty()
            && (!helper_fragment_output_values.is_empty()
                || !helper_fragment_output_uses.is_empty())
            && kind == ValueKind::Fragment;
        let helper_call_caller_structured_path = !helper_inlined
            && !in_mapping_key
            && !path.0.is_empty()
            && !helper_fragment_output_uses.is_empty()
            && (kind == ValueKind::Fragment
                || (kind == ValueKind::Scalar && self.output_node_is_entire_scalar_value(node)));
        let structured_fragment_sources: BTreeSet<String> = helper_fragment_output_uses
            .iter()
            .map(|output| output.source_expr.clone())
            .collect();
        let mut helper_rendered_sources = structured_fragment_sources.clone();
        helper_rendered_sources.extend(helper_output_values.keys().cloned());
        helper_rendered_sources.extend(helper_fragment_output_values.iter().cloned());

        for (v, meta) in &helper_output_values {
            if structured_fragment_sources.contains(v) {
                continue;
            }
            let has_rendered_descendant =
                output_path::values_path_has_descendant(v, &helper_rendered_sources);
            if helper_call_caller_scalar_path && !has_rendered_descendant {
                let extra_guards = Self::helper_output_extra_guards(v, meta);
                self.emit_use_with_extra_guards(v.clone(), path.clone(), kind, &extra_guards);
            } else {
                let extra_guards = Self::helper_dependency_extra_guards(v, meta);
                self.emit_helper_use_with_extra_guards(v.clone(), &extra_guards);
            }
        }

        for output in helper_fragment_output_uses {
            let extra_guards = Self::helper_output_extra_guards(&output.source_expr, &output.meta);
            let has_rendered_descendant = output_path::values_path_has_descendant(
                &output.source_expr,
                &helper_rendered_sources,
            );
            if helper_call_caller_structured_path && !has_rendered_descendant {
                let emit_path = output_path::append_relative_path(&path, &output.relative_path);
                self.emit_use_with_extra_guards(
                    output.source_expr,
                    emit_path,
                    output.kind,
                    &extra_guards,
                );
            } else {
                let dependency_guards =
                    Self::helper_dependency_extra_guards(&output.source_expr, &output.meta);
                self.emit_helper_use_kind_with_extra_guards(
                    output.source_expr,
                    output.kind,
                    &dependency_guards,
                );
            }
        }

        for v in helper_fragment_output_values {
            if structured_fragment_sources.contains(&v) {
                continue;
            }
            let has_rendered_descendant =
                output_path::values_path_has_descendant(&v, &helper_rendered_sources);
            if helper_call_caller_fragment_path && !has_rendered_descendant {
                self.emit_use(v, path.clone(), kind);
            } else {
                self.emit_helper_use_kind_with_extra_guards(v, kind, &[]);
            }
        }

        for (v, meta) in helper_dependency_values {
            let extra_guards = Self::helper_dependency_extra_guards(&v, &meta);
            self.emit_helper_use_with_extra_guards(v, &extra_guards);
        }

        for v in helper_guard_values {
            self.emit_helper_use(v);
        }

        for (path, schema_types) in helper_type_hints {
            for schema_type in schema_types {
                self.emit_use_with_extra_guards(
                    path.clone(),
                    YamlPath(Vec::new()),
                    ValueKind::Scalar,
                    &[Guard::TypeIs {
                        path: path.clone(),
                        schema_type,
                    }],
                );
            }
        }
    }

    fn output_node_is_mapping_key_part(&self, node: tree_sitter::Node<'_>) -> bool {
        let start = node.start_byte();
        let end = node.end_byte();
        let line_start = self.source[..start].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = self.source[end..]
            .find('\n')
            .map_or(self.source.len(), |idx| end + idx);
        let line = &self.source[line_start..line_end];
        let rel_start = start - line_start;
        let rel_end = end - line_start;
        let Some(colon_offset) = first_mapping_colon_offset(line) else {
            return false;
        };
        // A template action used before the first mapping separator contributes
        // to key construction (for example `{{ .name }}.json: ...`), not to the
        // parent value slot.
        rel_start < colon_offset && rel_end <= colon_offset
    }

    fn enclosing_action_text(&self, node: tree_sitter::Node<'_>) -> Option<String> {
        let mut current = node;
        loop {
            match current.kind() {
                "template_action" => {
                    return current
                        .utf8_text(self.source.as_bytes())
                        .ok()
                        .map(std::string::ToString::to_string);
                }
                "if_action" | "with_action" | "range_action" => return None,
                _ => {
                    current = current.parent()?;
                }
            }
        }
    }

    fn output_node_is_entire_scalar_value(&self, node: tree_sitter::Node<'_>) -> bool {
        fn normalize_value_text(value_text: &str) -> &str {
            let trimmed = value_text.trim();
            let unquoted = if trimmed.len() >= 2
                && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
                    || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
            {
                &trimmed[1..trimmed.len() - 1]
            } else {
                trimmed
            };

            let Some(rest) = unquoted.strip_prefix("{{") else {
                return unquoted.trim();
            };
            let rest = rest.strip_prefix('-').unwrap_or(rest);
            let Some(rest) = rest.strip_suffix("}}") else {
                return unquoted.trim();
            };
            let rest = rest.strip_suffix('-').unwrap_or(rest);
            rest.trim()
        }

        let start = node.start_byte();
        let end = node.end_byte();
        let line_start = self.source[..start].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = self.source[end..]
            .find('\n')
            .map_or(self.source.len(), |idx| end + idx);
        let line = &self.source[line_start..line_end];
        let rel_start = start - line_start;
        let rel_end = end - line_start;
        let node_text = &line[rel_start..rel_end];

        if let Some(colon_offset) = first_mapping_colon_offset(line) {
            if rel_start <= colon_offset {
                return false;
            }
            let value_text = line[colon_offset + 1..].trim();
            return normalize_value_text(value_text) == node_text.trim();
        }

        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('-') {
            return normalize_value_text(rest.trim_start()) == node_text.trim();
        }

        false
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn parse_expr_text(text: &str) -> Vec<TemplateExpr> {
        parse_cached_expr_text(text)
    }

    fn collect_bound_fragment_outputs_from_tree(
        node: tree_sitter::Node<'_>,
        source: &str,
        locals: &mut HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
        outputs: &mut BTreeSet<String>,
    ) {
        match node.kind() {
            "variable_definition" | "assignment" => {
                if let Ok(text) = node.utf8_text(source.as_bytes()) {
                    if apply_local_set_mutations(text, locals, current_dot, context, seen) {
                        return;
                    }
                    if let Some((var, _declares, rhs)) = parse_helper_assignment(text) {
                        let binding =
                            fragment_binding_from_text(&rhs, locals, current_dot, context, seen);
                        if let Some(binding) = binding {
                            locals.insert(var, binding);
                        }
                    }
                }
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
                if let Ok(text) = node.utf8_text(source.as_bytes())
                    && let Some(binding) =
                        fragment_binding_from_text(text, locals, current_dot, context, seen)
                {
                    outputs.extend(FragmentBinding::paths(&binding));
                }
            }
            "if_action" => {
                let mut then_locals = locals.clone();
                for child in Self::children_with_field(node, "consequence") {
                    Self::collect_bound_fragment_outputs_from_tree(
                        child,
                        source,
                        &mut then_locals,
                        current_dot,
                        context,
                        seen,
                        outputs,
                    );
                }

                let mut else_locals = locals.clone();
                for child in Self::children_with_field(node, "alternative") {
                    Self::collect_bound_fragment_outputs_from_tree(
                        child,
                        source,
                        &mut else_locals,
                        current_dot,
                        context,
                        seen,
                        outputs,
                    );
                }

                *locals = merge_fragment_locals(then_locals, else_locals);
            }
            "with_action" => {
                let binding = node
                    .child_by_field_name("condition")
                    .and_then(|condition| condition.utf8_text(source.as_bytes()).ok())
                    .and_then(|text| {
                        fragment_binding_from_text(text, locals, current_dot, context, seen)
                    });

                let mut body_locals = locals.clone();
                for child in Self::children_with_field(node, "consequence") {
                    Self::collect_bound_fragment_outputs_from_tree(
                        child,
                        source,
                        &mut body_locals,
                        binding.as_ref(),
                        context,
                        seen,
                        outputs,
                    );
                }

                let mut else_locals = locals.clone();
                for child in Self::children_with_field(node, "alternative") {
                    Self::collect_bound_fragment_outputs_from_tree(
                        child,
                        source,
                        &mut else_locals,
                        current_dot,
                        context,
                        seen,
                        outputs,
                    );
                }
            }
            "range_action" => {
                let has_destructured_variable_definition =
                    range_has_destructured_variable_definition(node);
                let header = range_header_text_from_source(node, source);
                let binding = header.as_deref().and_then(|text| {
                    range_iterable_binding(text, locals, current_dot, context, seen)
                });
                if has_destructured_variable_definition
                    && !range_body_emits_sequence_item_from_source(node, source)
                    && let Some(binding) = &binding
                {
                    outputs.extend(FragmentBinding::paths(binding));
                }

                let body_dot = binding.as_ref().and_then(FragmentBinding::item_binding);
                let mut body_locals = locals.clone();
                for child in Self::children_with_field(node, "body") {
                    Self::collect_bound_fragment_outputs_from_tree(
                        child,
                        source,
                        &mut body_locals,
                        body_dot.as_ref(),
                        context,
                        seen,
                        outputs,
                    );
                }
                if binding
                    .as_ref()
                    .is_some_and(FragmentBinding::definitely_nonempty_iterable)
                {
                    *locals = body_locals;
                } else {
                    *locals = merge_fragment_locals(locals.clone(), body_locals);
                }
            }
            _ => {
                let mut walker = node.walk();
                for child in node.named_children(&mut walker) {
                    Self::collect_bound_fragment_outputs_from_tree(
                        child,
                        source,
                        locals,
                        current_dot,
                        context,
                        seen,
                        outputs,
                    );
                }
            }
        }
    }

    fn collect_bound_fragment_output_uses_from_items(
        items: &[HelmAst],
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
        current_dot_fragment: Option<&FragmentBinding>,
        relative_path: &YamlPath,
        active_output_guards: &BTreeSet<String>,
        state: &mut FragmentOutputWalkState<'_, '_>,
    ) {
        let mut pending_path: Option<YamlPath> = None;
        for item in items {
            if let Some(path) = output_path::pending_mapping_key_path(item, relative_path) {
                pending_path = Some(path);
                continue;
            }
            let item_path = pending_path.as_ref().unwrap_or(relative_path);
            Self::collect_bound_fragment_output_uses_from_ast(
                item,
                bindings,
                current_dot,
                current_dot_fragment,
                item_path,
                active_output_guards,
                state,
            );
            pending_path = output_path::trailing_pending_mapping_key_path(item, item_path);
        }
    }

    fn collect_bound_fragment_output_uses_from_ast(
        node: &HelmAst,
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
        current_dot_fragment: Option<&FragmentBinding>,
        relative_path: &YamlPath,
        active_output_guards: &BTreeSet<String>,
        state: &mut FragmentOutputWalkState<'_, '_>,
    ) {
        match node {
            HelmAst::Document { items }
            | HelmAst::Mapping { items }
            | HelmAst::Define { body: items, .. }
            | HelmAst::Block { body: items, .. } => {
                Self::collect_bound_fragment_output_uses_from_items(
                    items,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    active_output_guards,
                    state,
                );
            }
            HelmAst::Sequence { items } => {
                let item_path = output_path::sequence_item_path(relative_path);
                for item in items {
                    Self::collect_bound_fragment_output_uses_from_ast(
                        item,
                        bindings,
                        current_dot,
                        current_dot_fragment,
                        &item_path,
                        active_output_guards,
                        state,
                    );
                }
            }
            HelmAst::Pair { key, value } => {
                if let Some(segment) = output_path::key_segment(key) {
                    let mut value_path = relative_path.clone();
                    value_path.0.push(segment);
                    if let Some(value) = value {
                        Self::collect_bound_fragment_output_uses_from_ast(
                            value,
                            bindings,
                            current_dot,
                            current_dot_fragment,
                            &value_path,
                            active_output_guards,
                            state,
                        );
                    }
                    return;
                }

                Self::collect_bound_fragment_output_uses_from_ast(
                    key,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    active_output_guards,
                    state,
                );
                if let Some(value) = value {
                    Self::collect_bound_fragment_output_uses_from_ast(
                        value,
                        bindings,
                        current_dot,
                        current_dot_fragment,
                        relative_path,
                        active_output_guards,
                        state,
                    );
                }
            }
            HelmAst::HelmExpr { text } => {
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

                if let Some((var, _declares, rhs)) = parse_helper_assignment(text) {
                    let mut seen_rhs = HashSet::new();
                    let mut binding = fragment_binding_from_text(
                        &rhs,
                        state.local_bindings,
                        current_dot_fragment,
                        state.context,
                        &mut seen_rhs,
                    );
                    let mut top_level_helper_dependency_paths = BTreeSet::new();
                    if text_starts_with_helper_call(&rhs) {
                        let mut rhs_seen = state.seen.clone();
                        let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                            &rhs,
                            Some(bindings),
                            current_dot,
                            state.local_bindings,
                            state.context,
                            &mut rhs_seen,
                        );
                        top_level_helper_dependency_paths = bound_helper_dependency_paths(&nested);
                        if let Some(nested_binding) = fragment_binding_from_helper_analysis(nested)
                        {
                            binding = match binding {
                                Some(binding) => {
                                    FragmentBinding::merge_all(vec![binding, nested_binding])
                                }
                                None => Some(nested_binding),
                            };
                        }
                    }
                    if text_pipeline_merges_into_var(&rhs, &var)
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
                            binding = binding
                                .and_then(|binding| binding.remove_paths(&internal_item_paths));
                        }
                        binding = match binding {
                            Some(binding) => FragmentBinding::merge_all(vec![
                                binding,
                                current_dot_fragment.clone(),
                            ]),
                            None => Some(current_dot_fragment.clone()),
                        };
                    }
                    if let Some(binding) = binding {
                        state.local_bindings.insert(var.clone(), binding);
                    }
                    let mut defaulted_paths =
                        resolved_default_fallback_paths_for_text(&rhs, Some(bindings), current_dot);
                    defaulted_paths.extend(local_default_paths_from_text(
                        &rhs,
                        state.local_default_paths,
                    ));
                    if defaulted_paths.is_empty() {
                        state.local_default_paths.remove(&var);
                    } else {
                        state
                            .local_default_paths
                            .insert(var.clone(), defaulted_paths);
                    }
                    return;
                }

                let kind = if is_fragment_expr(text) {
                    ValueKind::Fragment
                } else {
                    ValueKind::Scalar
                };
                let output_path = static_yaml_fragment_output_path(text)
                    .map(|output_path| {
                        output_path::append_relative_path(relative_path, &output_path)
                    })
                    .unwrap_or_else(|| relative_path.clone());
                let direct_outputs =
                    direct_bound_paths_from_text_in_context(text, bindings, current_dot);
                let fallback_paths =
                    resolved_default_fallback_paths_for_text(text, Some(bindings), current_dot);
                let local_outputs = local_rendered_paths_from_text(text, state.local_bindings);
                let handled_outputs: BTreeSet<String> = direct_outputs
                    .iter()
                    .chain(local_outputs.iter())
                    .cloned()
                    .collect();
                let mut direct_output_uses = Vec::new();
                for expr in Self::parse_expr_text(text) {
                    collect_helper_binding_output_uses_from_expr(
                        &expr,
                        HelperOutputExprContext {
                            bindings,
                            current_dot,
                            relative_path: &output_path,
                            kind,
                            active_output_guards,
                            defaulted_paths: &fallback_paths,
                        },
                        &mut direct_output_uses,
                    );
                }
                state.outputs.extend(direct_output_uses);

                let local_fallback_paths =
                    local_default_paths_from_text(text, state.local_default_paths);
                let mut local_output_uses = Vec::new();
                for expr in Self::parse_expr_text(text) {
                    walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                        let binding = match node {
                            TemplateExpr::Variable(var) if !var.is_empty() => {
                                state.local_bindings.get(var).cloned()
                            }
                            TemplateExpr::Selector { operand, path } => {
                                let TemplateExpr::Variable(var) = operand.as_ref() else {
                                    return;
                                };
                                if var.is_empty() {
                                    return;
                                }
                                state
                                    .local_bindings
                                    .get(var)
                                    .and_then(|binding| binding.apply_to_binding(path))
                            }
                            _ => None,
                        };
                        if let Some(binding) = binding {
                            collect_fragment_binding_output_uses(
                                &mut local_output_uses,
                                &binding,
                                &output_path,
                                kind,
                                active_output_guards,
                                &local_fallback_paths,
                            );
                        }
                    });
                }
                let mut nested = Self::analyze_bound_helper_calls_with_fragment_locals(
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
                    .filter(|output| {
                        expression_output_use_is_keyed_map_projection(output, &empty_output_path)
                    })
                    .map(|output| output.source_expr.clone())
                    .collect();
                let nested_scalar_sources: BTreeSet<String> =
                    nested.output.keys().cloned().collect();
                let nested_has_fragment_outputs =
                    !nested.fragment_output.is_empty() || !nested.fragment_output_uses.is_empty();

                let mut expression_output_uses = Vec::new();
                let mut expression_seen = state.seen.clone();
                for expr in Self::parse_expr_text(text) {
                    if !expr_contains_helper_call(&expr) {
                        continue;
                    }
                    if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
                        &expr,
                        state.local_bindings,
                        Some(bindings),
                        current_dot,
                        state.context,
                        &mut expression_seen,
                    ) {
                        collect_helper_binding_output_uses(
                            &mut expression_output_uses,
                            &binding,
                            &output_path,
                            kind,
                            active_output_guards,
                            &fallback_paths,
                        );
                    }
                }
                expression_output_uses.retain(|output| {
                    expression_output_use_is_keyed_map_projection(output, &output_path)
                });
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
                    let meta = helper_output_meta_with_guards(meta, active_output_guards);
                    push_helper_fragment_output(
                        state.outputs,
                        source_expr,
                        relative_path,
                        kind,
                        meta,
                    );
                }
                for nested_output in nested.fragment_output_uses.drain(..) {
                    if kind == ValueKind::Fragment
                        && nested_output.relative_path.0.is_empty()
                        && (nested_scalar_sources.contains(&nested_output.source_expr)
                            || nested_descendant_structured_sources
                                .contains(&nested_output.source_expr)
                            || expression_descendant_sources.contains(&nested_output.source_expr))
                    {
                        continue;
                    }
                    let meta =
                        helper_output_meta_with_guards(nested_output.meta, active_output_guards);
                    push_helper_fragment_output(
                        state.outputs,
                        nested_output.source_expr,
                        &output_path::append_relative_path(
                            relative_path,
                            &nested_output.relative_path,
                        ),
                        nested_output.kind,
                        meta,
                    );
                }
            }
            HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let mut branch_guard_paths =
                    direct_bound_paths_from_text_in_context(cond, bindings, current_dot);
                branch_guard_paths.extend(local_bound_paths_from_text(cond, state.local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    cond,
                    Some(bindings),
                    current_dot,
                    state.local_bindings,
                    state.context,
                    state.seen,
                );
                branch_guard_paths.extend(bound_helper_condition_paths(&nested));

                let mut then_guards = active_output_guards.clone();
                then_guards.extend(branch_guard_paths);
                let mut then_bindings = state.local_bindings.clone();
                let mut then_defaults = state.local_default_paths.clone();
                let mut then_state = FragmentOutputWalkState {
                    local_bindings: &mut then_bindings,
                    local_default_paths: &mut then_defaults,
                    context: state.context,
                    seen: state.seen,
                    outputs: state.outputs,
                };
                Self::collect_bound_fragment_output_uses_from_items(
                    then_branch,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    &then_guards,
                    &mut then_state,
                );

                let mut else_bindings = state.local_bindings.clone();
                let mut else_defaults = state.local_default_paths.clone();
                let mut else_state = FragmentOutputWalkState {
                    local_bindings: &mut else_bindings,
                    local_default_paths: &mut else_defaults,
                    context: state.context,
                    seen: state.seen,
                    outputs: state.outputs,
                };
                Self::collect_bound_fragment_output_uses_from_items(
                    else_branch,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    active_output_guards,
                    &mut else_state,
                );
                *state.local_bindings = merge_fragment_locals(then_bindings, else_bindings);
                *state.local_default_paths =
                    merge_local_default_paths(then_defaults, else_defaults);
            }
            HelmAst::With {
                header,
                body,
                else_branch,
            } => {
                let mut branch_guard_paths =
                    direct_bound_paths_from_text_in_context(header, bindings, current_dot);
                branch_guard_paths
                    .extend(local_bound_paths_from_text(header, state.local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    header,
                    Some(bindings),
                    current_dot,
                    state.local_bindings,
                    state.context,
                    state.seen,
                );
                branch_guard_paths.extend(bound_helper_condition_paths(&nested));
                let body_dot = Self::computed_with_body_dot(header, bindings, current_dot);

                let mut body_guards = active_output_guards.clone();
                body_guards.extend(branch_guard_paths);
                let mut body_bindings = state.local_bindings.clone();
                let mut body_defaults = state.local_default_paths.clone();
                let body_dot_fragment = body_dot.as_ref().map(HelperBinding::to_fragment_binding);
                let mut body_state = FragmentOutputWalkState {
                    local_bindings: &mut body_bindings,
                    local_default_paths: &mut body_defaults,
                    context: state.context,
                    seen: state.seen,
                    outputs: state.outputs,
                };
                Self::collect_bound_fragment_output_uses_from_items(
                    body,
                    bindings,
                    body_dot.as_ref(),
                    body_dot_fragment.as_ref(),
                    relative_path,
                    &body_guards,
                    &mut body_state,
                );

                let mut else_bindings = state.local_bindings.clone();
                let mut else_defaults = state.local_default_paths.clone();
                let mut else_state = FragmentOutputWalkState {
                    local_bindings: &mut else_bindings,
                    local_default_paths: &mut else_defaults,
                    context: state.context,
                    seen: state.seen,
                    outputs: state.outputs,
                };
                Self::collect_bound_fragment_output_uses_from_items(
                    else_branch,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    active_output_guards,
                    &mut else_state,
                );
                *state.local_bindings = merge_fragment_locals(body_bindings, else_bindings);
                *state.local_default_paths =
                    merge_local_default_paths(body_defaults, else_defaults);
            }
            HelmAst::Range {
                header,
                body,
                else_branch,
            } => {
                let mut branch_guard_paths =
                    direct_bound_paths_from_text_in_context(header, bindings, current_dot);
                branch_guard_paths
                    .extend(local_bound_paths_from_text(header, state.local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    header,
                    Some(bindings),
                    current_dot,
                    state.local_bindings,
                    state.context,
                    state.seen,
                );
                branch_guard_paths.extend(bound_helper_condition_paths(&nested));
                let mut seen_range_binding = HashSet::new();
                let range_binding = range_iterable_binding(
                    header,
                    state.local_bindings,
                    current_dot_fragment,
                    state.context,
                    &mut seen_range_binding,
                );
                let body_dot_fragment = range_binding
                    .as_ref()
                    .and_then(FragmentBinding::item_binding);
                let body_dot = body_dot_fragment
                    .as_ref()
                    .and_then(FragmentBinding::to_helper_binding);

                let mut body_guards = active_output_guards.clone();
                body_guards.extend(branch_guard_paths);
                let mut body_bindings = state.local_bindings.clone();
                let mut body_defaults = state.local_default_paths.clone();
                if let Some(FragmentBinding::List(items)) = &range_binding {
                    let range_var = range_variable_name(header);
                    for item_binding in items {
                        if let Some(range_var) = &range_var {
                            body_bindings.insert(range_var.clone(), item_binding.clone());
                        }
                        let item_dot = item_binding.to_helper_binding();
                        let mut item_seen = state.seen.clone();
                        let mut item_state = FragmentOutputWalkState {
                            local_bindings: &mut body_bindings,
                            local_default_paths: &mut body_defaults,
                            context: state.context,
                            seen: &mut item_seen,
                            outputs: state.outputs,
                        };
                        Self::collect_bound_fragment_output_uses_from_items(
                            body,
                            bindings,
                            item_dot.as_ref(),
                            Some(item_binding),
                            relative_path,
                            &body_guards,
                            &mut item_state,
                        );
                    }
                } else {
                    let mut body_state = FragmentOutputWalkState {
                        local_bindings: &mut body_bindings,
                        local_default_paths: &mut body_defaults,
                        context: state.context,
                        seen: state.seen,
                        outputs: state.outputs,
                    };
                    Self::collect_bound_fragment_output_uses_from_items(
                        body,
                        bindings,
                        body_dot.as_ref(),
                        body_dot_fragment.as_ref(),
                        relative_path,
                        &body_guards,
                        &mut body_state,
                    );
                }

                if range_binding
                    .as_ref()
                    .is_some_and(FragmentBinding::definitely_nonempty_iterable)
                {
                    *state.local_bindings = body_bindings;
                    *state.local_default_paths = body_defaults;
                } else {
                    let mut else_bindings = state.local_bindings.clone();
                    let mut else_defaults = state.local_default_paths.clone();
                    let mut else_state = FragmentOutputWalkState {
                        local_bindings: &mut else_bindings,
                        local_default_paths: &mut else_defaults,
                        context: state.context,
                        seen: state.seen,
                        outputs: state.outputs,
                    };
                    Self::collect_bound_fragment_output_uses_from_items(
                        else_branch,
                        bindings,
                        current_dot,
                        current_dot_fragment,
                        relative_path,
                        active_output_guards,
                        &mut else_state,
                    );
                    *state.local_bindings = merge_fragment_locals(body_bindings, else_bindings);
                    *state.local_default_paths =
                        merge_local_default_paths(body_defaults, else_defaults);
                }
            }
            HelmAst::Scalar { .. } | HelmAst::HelmComment { .. } => {}
        }
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
        let analysis = Self::analyze_bound_helper_calls_with_fragment_locals(
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

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn analyze_bound_helper_calls_with_fragment_locals(
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> BoundHelperAnalysis {
        let mut analysis = BoundHelperAnalysis::default();
        for expr in Self::parse_expr_text(text) {
            walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                let TemplateExpr::Call { function, args } = node else {
                    return;
                };
                if !matches!(function.as_str(), "include" | "template") {
                    return;
                }
                let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
                    return;
                };
                let nested = Self::analyze_bound_helper_call_with_fragment_locals(
                    name,
                    args.get(1),
                    bindings,
                    current_dot,
                    fragment_locals,
                    context,
                    seen,
                );
                analysis.extend(nested);
            });
        }
        analysis
    }

    #[tracing::instrument(skip_all)]
    fn analyze_bound_helper_call_with_fragment_locals(
        name: &str,
        arg: Option<&TemplateExpr>,
        outer_bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> BoundHelperAnalysis {
        if !seen.insert(name.to_string()) {
            return BoundHelperAnalysis::default();
        }

        let mut binding_seen = seen.clone();
        let bindings = bindings_for_helper_arg_with_fragment_locals(
            arg,
            outer_bindings,
            current_dot,
            fragment_locals,
            context,
            &mut binding_seen,
        );
        // Inside the helper body, `.` is what the caller passed as the
        // helper argument. `binding_from_expr` against the outer
        // `current_dot` resolves that for us (and falls back to
        // `RootContext` when the caller passes the bare `.` from a
        // template-root context). `None` is acceptable when the
        // caller's dot can't be statically pinned.
        let helper_body_dot = {
            let mut dot_seen = seen.clone();
            arg.and_then(|expr| {
                helper_binding_from_expr_with_fragment_locals(
                    expr,
                    fragment_locals,
                    outer_bindings,
                    current_dot,
                    context,
                    &mut dot_seen,
                )
            })
            .or_else(|| current_dot.cloned())
        };
        let mut analysis = BoundHelperAnalysis::default();
        if let Some(body) = context.defines.get(name) {
            let active_output_guards = BTreeSet::new();
            let mut local_bindings = HashMap::new();
            let mut local_default_paths = HashMap::new();
            let mut local_output_meta = HashMap::new();
            let mut helper_values_state = HelperValuesWalkState {
                local_bindings: &mut local_bindings,
                local_default_paths: &mut local_default_paths,
                local_output_meta: &mut local_output_meta,
                context,
                seen,
                analysis: &mut analysis,
            };
            for node in body {
                Self::collect_bound_helper_values_from_ast(
                    node,
                    &bindings,
                    helper_body_dot.as_ref(),
                    &active_output_guards,
                    &mut helper_values_state,
                );
            }
        }
        let mut helper_fragment_locals = HashMap::new();
        let helper_dot = arg.and_then(|expr| {
            fragment_binding_from_outer_expr(
                expr,
                Some(fragment_locals),
                outer_bindings,
                current_dot,
            )
        });
        if let Some(src) = context.define_bodies.source(name)
            && let Some(tree) = context.define_bodies.tree(name)
        {
            Self::collect_bound_fragment_outputs_from_tree(
                tree.root_node(),
                src,
                &mut helper_fragment_locals,
                helper_dot.as_ref(),
                context,
                seen,
                &mut analysis.fragment_output,
            );
        }
        if let Some(body) = context.defines.get(name) {
            let mut fragment_output_uses = Vec::new();
            let mut local_bindings = helper_fragment_locals;
            let mut local_default_paths = HashMap::new();
            let active_output_guards = BTreeSet::new();
            let mut fragment_output_state = FragmentOutputWalkState {
                local_bindings: &mut local_bindings,
                local_default_paths: &mut local_default_paths,
                context,
                seen,
                outputs: &mut fragment_output_uses,
            };
            Self::collect_bound_fragment_output_uses_from_items(
                body,
                &bindings,
                helper_body_dot.as_ref(),
                helper_dot.as_ref(),
                &YamlPath(Vec::new()),
                &active_output_guards,
                &mut fragment_output_state,
            );
            for source in analysis.output.keys() {
                analysis.fragment_output.remove(source);
            }
            let structured_sources: BTreeSet<String> = fragment_output_uses
                .iter()
                .map(|output| output.source_expr.clone())
                .collect();
            for source in &structured_sources {
                analysis.output.remove(source);
                analysis.fragment_output.remove(source);
            }
            analysis.fragment_output_uses.extend(fragment_output_uses);
        }

        for binding in bindings.values() {
            let HelperBinding::ValuesPath(root) = binding else {
                continue;
            };
            let prefix = format!("{root}.");
            if analysis
                .output
                .keys()
                .chain(analysis.guard_paths.iter())
                .any(|path| path.starts_with(&prefix))
            {
                analysis.suppress_roots.insert(root.clone());
            }
        }

        seen.remove(name);
        analysis
    }

    /// Walk a helper's AST node collecting bound helper values.
    ///
    /// `current_dot` tracks the value the current `.` resolves to as
    /// the walk descends into `with`/`range` blocks. Without it, a
    /// helper body like
    ///
    /// ```text
    /// {{- define "X.defaults" }}
    ///   {{- with .Values }}
    ///     {{- $_ := set .a "name" (.a.name | default "fallback") }}
    ///   {{- end }}
    /// {{- end }}
    /// ```
    ///
    /// would not register `a.name` as a chart-declared default — the
    /// default-fallback detector inside the inner `set` action only
    /// sees `.a.name` (no `.Values` prefix), so it can't resolve to a
    /// values path. Threading the with-shifted dot lets the detector
    /// re-root `.a.name` against `.Values` and record the default.
    #[tracing::instrument(skip_all)]
    fn collect_bound_helper_values_from_ast(
        node: &HelmAst,
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
        active_output_guards: &BTreeSet<String>,
        state: &mut HelperValuesWalkState<'_, '_>,
    ) {
        match node {
            HelmAst::Document { items }
            | HelmAst::Mapping { items }
            | HelmAst::Sequence { items }
            | HelmAst::Define { body: items, .. }
            | HelmAst::Block { body: items, .. } => {
                for item in items {
                    Self::collect_bound_helper_values_from_ast(
                        item,
                        bindings,
                        current_dot,
                        active_output_guards,
                        state,
                    );
                }
            }
            HelmAst::Pair { key, value } => {
                Self::collect_bound_helper_values_from_ast(
                    key,
                    bindings,
                    current_dot,
                    active_output_guards,
                    state,
                );
                if let Some(value) = value {
                    Self::collect_bound_helper_values_from_ast(
                        value,
                        bindings,
                        current_dot,
                        active_output_guards,
                        state,
                    );
                }
            }
            HelmAst::HelmExpr { text } => {
                if let Some((var, _declares, rhs)) = parse_helper_assignment(text) {
                    let set_default_paths =
                        set_default_chart_paths_for_text(text, Some(bindings), current_dot);
                    state.analysis.chart_defaults.extend(set_default_paths);
                    extend_type_hints(
                        &mut state.analysis.type_hints,
                        resolved_type_is_paths_for_text(&rhs, Some(bindings), current_dot),
                    );
                    extend_type_hints(
                        &mut state.analysis.type_hints,
                        resolved_string_transform_paths_for_text(&rhs, Some(bindings), current_dot),
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

                    let fallback_paths =
                        resolved_default_fallback_paths_for_text(&rhs, Some(bindings), current_dot);
                    let direct_outputs =
                        direct_bound_paths_from_text_in_context(&rhs, bindings, current_dot);
                    let local_fallback_paths =
                        local_default_paths_from_text(&rhs, state.local_default_paths);
                    let local_outputs = local_bound_paths_from_text(&rhs, state.local_bindings);
                    let local_meta_by_path = local_output_meta_from_text(
                        &rhs,
                        state.local_bindings,
                        state.local_output_meta,
                    );
                    state
                        .analysis
                        .dependency_paths
                        .extend(direct_outputs.iter().cloned());
                    state
                        .analysis
                        .dependency_paths
                        .extend(local_outputs.iter().cloned());
                    let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                        &rhs,
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

                    let mut rhs_output_meta: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
                    for output in &direct_outputs {
                        let entry = rhs_output_meta.entry(output.clone()).or_default();
                        entry.guards.extend(active_output_guards.iter().cloned());
                        entry.defaulted |= fallback_paths.contains(output);
                    }
                    for output in &local_outputs {
                        let mut meta = local_meta_by_path.get(output).cloned().unwrap_or_default();
                        meta.guards.extend(active_output_guards.iter().cloned());
                        meta.defaulted |= local_fallback_paths.contains(output);
                        let entry = rhs_output_meta.entry(output.clone()).or_default();
                        entry.guards.extend(meta.guards);
                        entry.defaulted |= meta.defaulted;
                    }
                    for (output, meta) in helper_output_meta_from_analysis(&nested) {
                        let meta = helper_output_meta_with_guards(meta, active_output_guards);
                        let entry = rhs_output_meta.entry(output).or_default();
                        entry.guards.extend(meta.guards);
                        entry.defaulted |= meta.defaulted;
                    }

                    let mut seen_rhs = HashSet::new();
                    if let Some(binding) = fragment_binding_from_text(
                        &rhs,
                        state.local_bindings,
                        current_dot_fragment.as_ref(),
                        state.context,
                        &mut seen_rhs,
                    ) {
                        state.local_bindings.insert(var.clone(), binding);
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
                        state.local_default_paths.remove(&var);
                    } else {
                        state
                            .local_default_paths
                            .insert(var.clone(), defaulted_paths);
                    }
                    if rhs_output_meta.is_empty() {
                        state.local_output_meta.remove(&var);
                    } else {
                        state.local_output_meta.insert(var, rhs_output_meta);
                    }
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
                    let set_default_paths =
                        set_default_chart_paths_for_text(text, Some(bindings), current_dot);
                    state.analysis.chart_defaults.extend(set_default_paths);
                    return;
                }

                let direct_outputs =
                    direct_bound_paths_from_text_in_context(text, bindings, current_dot);
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
                let local_fallback_paths =
                    local_default_paths_from_text(text, state.local_default_paths);
                let local_meta_by_path = local_output_meta_from_text(
                    text,
                    state.local_bindings,
                    state.local_output_meta,
                );
                let expression_kind = if is_fragment_expr(text) {
                    ValueKind::Fragment
                } else {
                    ValueKind::Scalar
                };
                let empty_path = YamlPath(Vec::new());
                if expression_kind == ValueKind::Scalar {
                    for output in direct_outputs {
                        let meta = HelperOutputMeta {
                            guards: active_output_guards.clone(),
                            defaulted: fallback_paths.contains(&output),
                        };
                        state.analysis.add_output_meta(output, meta);
                    }
                    for output in local_outputs {
                        let mut meta = local_meta_by_path.get(&output).cloned().unwrap_or_default();
                        meta.guards.extend(active_output_guards.iter().cloned());
                        meta.defaulted |= local_fallback_paths.contains(&output);
                        state.analysis.add_output_meta(output, meta);
                    }
                }
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    text,
                    Some(bindings),
                    current_dot,
                    state.local_bindings,
                    state.context,
                    state.seen,
                );
                let mut nested = nested;
                if expression_kind == ValueKind::Fragment {
                    for (output, mut meta) in nested.output {
                        meta.guards.extend(active_output_guards.iter().cloned());
                        state.analysis.add_output_meta(output, meta);
                    }
                    for output in nested.fragment_output {
                        push_helper_fragment_output(
                            &mut state.analysis.fragment_output_uses,
                            output,
                            &empty_path,
                            expression_kind,
                            HelperOutputMeta {
                                guards: active_output_guards.clone(),
                                defaulted: false,
                            },
                        );
                    }
                    for mut output in nested.fragment_output_uses {
                        output
                            .meta
                            .guards
                            .extend(active_output_guards.iter().cloned());
                        state.analysis.fragment_output_uses.push(output);
                    }
                    state
                        .analysis
                        .dependency_paths
                        .extend(nested.dependency_paths);
                    state
                        .analysis
                        .add_dependency_meta_map(nested.dependency_meta);
                    state.analysis.guard_paths.extend(nested.guard_paths);
                    extend_type_hints(&mut state.analysis.type_hints, nested.type_hints);
                    state.analysis.suppress_roots.extend(nested.suppress_roots);
                    state.analysis.chart_defaults.extend(nested.chart_defaults);
                } else {
                    convert_fragment_outputs_to_dependency_outputs(&mut nested);
                    for meta in nested.output.values_mut() {
                        meta.guards.extend(active_output_guards.iter().cloned());
                    }
                    state.analysis.extend(nested);
                }
                let set_default_paths =
                    set_default_chart_paths_for_text(text, Some(bindings), current_dot);
                state.analysis.chart_defaults.extend(set_default_paths);
            }
            HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let mut branch_guard_paths =
                    direct_bound_paths_from_text_in_context(cond, bindings, current_dot);
                branch_guard_paths.extend(local_bound_paths_from_text(cond, state.local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    cond,
                    Some(bindings),
                    current_dot,
                    state.local_bindings,
                    state.context,
                    state.seen,
                );
                branch_guard_paths.extend(bound_helper_condition_paths(&nested));
                state
                    .analysis
                    .guard_paths
                    .extend(branch_guard_paths.iter().cloned());
                let mut then_output_guards = active_output_guards.clone();
                then_output_guards.extend(branch_guard_paths);
                let mut then_bindings = state.local_bindings.clone();
                let mut then_default_paths = state.local_default_paths.clone();
                let mut then_output_meta = state.local_output_meta.clone();
                let mut then_state = HelperValuesWalkState {
                    local_bindings: &mut then_bindings,
                    local_default_paths: &mut then_default_paths,
                    local_output_meta: &mut then_output_meta,
                    context: state.context,
                    seen: state.seen,
                    analysis: state.analysis,
                };
                for item in then_branch {
                    Self::collect_bound_helper_values_from_ast(
                        item,
                        bindings,
                        current_dot,
                        &then_output_guards,
                        &mut then_state,
                    );
                }
                let mut else_bindings = state.local_bindings.clone();
                let mut else_default_paths = state.local_default_paths.clone();
                let mut else_output_meta = state.local_output_meta.clone();
                let mut else_state = HelperValuesWalkState {
                    local_bindings: &mut else_bindings,
                    local_default_paths: &mut else_default_paths,
                    local_output_meta: &mut else_output_meta,
                    context: state.context,
                    seen: state.seen,
                    analysis: state.analysis,
                };
                for item in else_branch {
                    Self::collect_bound_helper_values_from_ast(
                        item,
                        bindings,
                        current_dot,
                        active_output_guards,
                        &mut else_state,
                    );
                }
                *state.local_bindings = merge_fragment_locals(then_bindings, else_bindings);
                *state.local_default_paths =
                    merge_local_default_paths(then_default_paths, else_default_paths);
                *state.local_output_meta =
                    merge_helper_output_meta_maps(then_output_meta, else_output_meta);
            }
            HelmAst::Range {
                header,
                body,
                else_branch,
            }
            | HelmAst::With {
                header,
                body,
                else_branch,
            } => {
                let is_with = matches!(node, HelmAst::With { .. });
                let mut branch_guard_paths =
                    direct_bound_paths_from_text_in_context(header, bindings, current_dot);
                branch_guard_paths
                    .extend(local_bound_paths_from_text(header, state.local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    header,
                    Some(bindings),
                    current_dot,
                    state.local_bindings,
                    state.context,
                    state.seen,
                );
                branch_guard_paths.extend(bound_helper_condition_paths(&nested));
                state
                    .analysis
                    .guard_paths
                    .extend(branch_guard_paths.iter().cloned());

                let mut range_fragment_binding = None;
                let mut range_binding = None;
                if !is_with {
                    let current_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
                    let mut seen_range = HashSet::new();
                    range_fragment_binding = range_iterable_binding(
                        header,
                        state.local_bindings,
                        current_dot_fragment.as_ref(),
                        state.context,
                        &mut seen_range,
                    );
                    range_binding = range_fragment_binding
                        .as_ref()
                        .and_then(FragmentBinding::to_helper_binding);
                }
                // Inside a `with` body the dot re-roots to the value of
                // the with-header. Inside a `range` body the dot re-roots
                // to the per-iteration item, even when the template does
                // not bind the item to a variable.
                let body_dot = if is_with {
                    Self::computed_with_body_dot(header, bindings, current_dot)
                } else {
                    range_binding.as_ref().and_then(HelperBinding::item_binding)
                };
                let mut body_output_guards = active_output_guards.clone();
                body_output_guards.extend(branch_guard_paths);
                let mut body_bindings = state.local_bindings.clone();
                let mut body_default_paths = state.local_default_paths.clone();
                let mut body_output_meta = state.local_output_meta.clone();
                if !is_with {
                    let header_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
                    let mut seen_range = HashSet::new();
                    if let Some((var, binding)) = range_variable_item_binding(
                        header,
                        &body_bindings,
                        header_dot_fragment.as_ref(),
                        state.context,
                        &mut seen_range,
                    ) {
                        body_bindings.insert(var, binding);
                    }
                }
                if !is_with && let Some(FragmentBinding::List(items)) = &range_fragment_binding {
                    let range_var = range_variable_name(header);
                    for item_binding in items {
                        if let Some(range_var) = &range_var {
                            body_bindings.insert(range_var.clone(), item_binding.clone());
                        }
                        let item_dot = item_binding.to_helper_binding();
                        let mut item_seen = state.seen.clone();
                        let mut item_state = HelperValuesWalkState {
                            local_bindings: &mut body_bindings,
                            local_default_paths: &mut body_default_paths,
                            local_output_meta: &mut body_output_meta,
                            context: state.context,
                            seen: &mut item_seen,
                            analysis: state.analysis,
                        };
                        for item in body {
                            Self::collect_bound_helper_values_from_ast(
                                item,
                                bindings,
                                item_dot.as_ref(),
                                &body_output_guards,
                                &mut item_state,
                            );
                        }
                    }
                } else {
                    let mut body_state = HelperValuesWalkState {
                        local_bindings: &mut body_bindings,
                        local_default_paths: &mut body_default_paths,
                        local_output_meta: &mut body_output_meta,
                        context: state.context,
                        seen: state.seen,
                        analysis: state.analysis,
                    };
                    for item in body {
                        Self::collect_bound_helper_values_from_ast(
                            item,
                            bindings,
                            body_dot.as_ref(),
                            &body_output_guards,
                            &mut body_state,
                        );
                    }
                }
                if !is_with
                    && range_binding
                        .as_ref()
                        .is_some_and(HelperBinding::definitely_nonempty_iterable)
                {
                    *state.local_bindings = body_bindings;
                    *state.local_default_paths = body_default_paths;
                    *state.local_output_meta = body_output_meta;
                } else {
                    let mut else_bindings = state.local_bindings.clone();
                    let mut else_default_paths = state.local_default_paths.clone();
                    let mut else_output_meta = state.local_output_meta.clone();
                    let mut else_state = HelperValuesWalkState {
                        local_bindings: &mut else_bindings,
                        local_default_paths: &mut else_default_paths,
                        local_output_meta: &mut else_output_meta,
                        context: state.context,
                        seen: state.seen,
                        analysis: state.analysis,
                    };
                    for item in else_branch {
                        // `with ... else ...` else-branch executes with
                        // the outer `.`, not the with-shifted one.
                        Self::collect_bound_helper_values_from_ast(
                            item,
                            bindings,
                            current_dot,
                            active_output_guards,
                            &mut else_state,
                        );
                    }
                    *state.local_bindings = merge_fragment_locals(body_bindings, else_bindings);
                    *state.local_default_paths =
                        merge_local_default_paths(body_default_paths, else_default_paths);
                    *state.local_output_meta =
                        merge_helper_output_meta_maps(body_output_meta, else_output_meta);
                }
            }
            HelmAst::Scalar { .. } | HelmAst::HelmComment { .. } => {}
        }
    }

    /// Resolve a `with` header's value to the binding callers should
    /// use as `current_dot` while walking the body. Returns `None` when
    /// the header can't be statically pinned to a single Values-rooted
    /// path (e.g. `with $variable`, `with .Chart.Name`, multi-expression
    /// headers): the body's dot is then unknown and the caller should
    /// leave `current_dot` untouched rather than guess.
    fn computed_with_body_dot(
        header: &str,
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
    ) -> Option<HelperBinding> {
        let exprs = Self::parse_expr_text(header);
        let [expr] = exprs.as_slice() else {
            return None;
        };
        // Bare `.Values` resolves to the values root rather than a concrete descendant path.
        // for this because the strict path extractor requires at least
        // one segment after `Values`. Treat it as a Values-root binding
        // so the body's `.X` resolves to `.Values.X`.
        if matches!(expr, TemplateExpr::Field(path) if matches!(path.as_slice(), [head] if head == "Values"))
            || matches!(
                expr,
                TemplateExpr::Selector { operand, path }
                    if matches!(operand.as_ref(), TemplateExpr::Variable(v) if v.is_empty())
                        && matches!(path.as_slice(), [head] if head == "Values"),
            )
        {
            return Some(HelperBinding::ValuesPath(String::new()));
        }
        binding_from_expr(expr, Some(bindings), current_dot)
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
        let cond_guards = self.value_path_context().condition_guards(cond_text);

        // In a `with` block, every path that contributes to the binding is
        // null-tolerant (`with nil` skips the body). Tag each such path with
        // `Guard::With { path }` so downstream consumers can identify
        // with-bound paths uniformly:
        //
        //   `with .Values.X`           → Truthy{X}      → With{X}
        //   `with or .A .B`            → Or{[A,B]}      → With{A}, With{B}, Or{[A,B]}
        //   `with and (.A) (.B)`       → Truthy{A,B}    → With{A}, With{B}
        //
        // For non-trivial control flow (`Or`, `Not`, `Eq`) we KEEP the
        // original guard alongside the per-path `With` so downstream
        // consumers retain exact semantics. `Truthy { path }` is fully
        // subsumed by `With { path }` and is dropped.
        let cond_guards: Vec<Guard> = cond_guards
            .into_iter()
            .flat_map(|g| match g {
                Guard::Truthy { path } => vec![Guard::With { path }],
                Guard::Or { ref paths } => {
                    let mut out: Vec<Guard> = paths
                        .iter()
                        .map(|p| Guard::With { path: p.clone() })
                        .collect();
                    out.push(g);
                    out
                }
                Guard::Not { ref path } | Guard::Eq { ref path, .. } => {
                    vec![Guard::With { path: path.clone() }, g]
                }
                Guard::Range { .. } => vec![g],
                Guard::With { .. } => vec![g],
                Guard::Default { .. } => vec![g],
                Guard::TypeIs { .. } => vec![g],
            })
            .collect();

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

    fn range_header_text(&self, node: tree_sitter::Node<'_>) -> Option<String> {
        if let Some(p) = node.child_by_field_name("range") {
            return p
                .utf8_text(self.source.as_bytes())
                .ok()
                .map(|s| s.trim().to_string());
        }
        let mut w = node.walk();
        for ch in node.named_children(&mut w) {
            if ch.kind() == "range_variable_definition"
                && let Some(p) = ch.child_by_field_name("range")
            {
                return p
                    .utf8_text(self.source.as_bytes())
                    .ok()
                    .map(|s| s.trim().to_string());
            }
        }
        None
    }

    fn range_body_renders_scalar_sequence_items(&self, node: tree_sitter::Node<'_>) -> bool {
        let mut saw_sequence_item = false;
        let mut body_text = String::new();

        for body_node in Self::children_with_field(node, "body") {
            let Ok(text) = body_node.utf8_text(self.source.as_bytes()) else {
                continue;
            };
            body_text.push_str(text);
        }

        for line in body_text.lines() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix('-') else {
                continue;
            };
            let rest = rest.trim_start();
            saw_sequence_item = true;

            if rest.is_empty() || parse_yaml_key(rest).is_some() || is_fragment_expr(rest) {
                return false;
            }
        }

        saw_sequence_item
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
        let saved = self.guards.len();
        let saved_domains = self.range_domains.clone();
        let saved_bindings = self.get_bindings.clone();
        let saved_template_bindings = self.template_bindings.clone();
        let saved_template_default_paths = self.template_default_paths.clone();
        let saved_template_output_meta = self.template_output_meta.clone();

        if let Some(cond) = node.child_by_field_name("condition")
            && let Ok(txt) = cond.utf8_text(self.source.as_bytes())
        {
            self.collect_if_with_guards(txt);
        }

        let consequence = Self::children_with_field(node, "consequence");
        for ch in consequence {
            self.walk(ch);
        }

        self.guards.truncate(saved);
        self.range_domains = saved_domains;
        self.get_bindings = saved_bindings;
        self.template_bindings = saved_template_bindings;
        self.template_default_paths = saved_template_default_paths;
        self.template_output_meta = saved_template_output_meta;

        // Note: else-if chains are represented as repeated condition/option fields.
        // For now, we only handle the plain else branch.
        let alternative = Self::children_with_field(node, "alternative");
        for ch in alternative {
            self.walk(ch);
        }
    }

    fn handle_with_action(&mut self, node: tree_sitter::Node<'_>) {
        let saved = self.guards.len();
        let saved_dot = self.dot_stack.len();
        let saved_domains = self.range_domains.clone();
        let saved_bindings = self.get_bindings.clone();
        let saved_template_bindings = self.template_bindings.clone();
        let saved_template_default_paths = self.template_default_paths.clone();
        let saved_template_output_meta = self.template_output_meta.clone();

        if let Some(cond) = node.child_by_field_name("condition")
            && let Ok(txt) = cond.utf8_text(self.source.as_bytes())
        {
            self.collect_with_guards(txt);
            self.push_with_dot_binding(txt);
        }

        let consequence = Self::children_with_field(node, "consequence");
        for ch in consequence {
            self.walk(ch);
        }

        self.guards.truncate(saved);
        self.dot_stack.truncate(saved_dot);
        self.range_domains = saved_domains;
        self.get_bindings = saved_bindings;
        self.template_bindings = saved_template_bindings;
        self.template_default_paths = saved_template_default_paths;
        self.template_output_meta = saved_template_output_meta;

        let alternative = Self::children_with_field(node, "alternative");
        for ch in alternative {
            self.walk(ch);
        }
    }

    fn handle_range_action(&mut self, node: tree_sitter::Node<'_>) {
        let saved = self.guards.len();
        let saved_dot = self.dot_stack.len();
        let saved_domains = self.range_domains.clone();
        let saved_bindings = self.get_bindings.clone();
        let saved_template_bindings = self.template_bindings.clone();
        let saved_template_default_paths = self.template_default_paths.clone();
        let saved_template_output_meta = self.template_output_meta.clone();

        let mut header_text: Option<String> = None;
        let mut has_variable_definition = false;
        {
            let mut w = node.walk();
            for ch in node.named_children(&mut w) {
                if ch.kind() == "range_variable_definition" {
                    has_variable_definition = true;
                    break;
                }
            }
        }

        let mut body_emits_sequence_item = false;
        for ch in Self::children_with_field(node, "body") {
            if let Ok(txt) = ch.utf8_text(self.source.as_bytes()) {
                for line in txt.lines() {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("- ") || trimmed == "-" {
                        body_emits_sequence_item = true;
                        break;
                    }
                }
            }
            if body_emits_sequence_item {
                break;
            }
        }
        let body_renders_scalar_sequence_items =
            !has_variable_definition && self.range_body_renders_scalar_sequence_items(node);
        if let Some(txt) = self.range_header_text(node) {
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

        let body = Self::children_with_field(node, "body");
        for ch in body {
            self.walk(ch);
        }

        self.guards.truncate(saved);
        self.dot_stack.truncate(saved_dot);
        self.range_domains = saved_domains;
        self.get_bindings = saved_bindings;
        self.template_bindings = saved_template_bindings;
        self.template_default_paths = saved_template_default_paths;
        self.template_output_meta = saved_template_output_meta;

        let alternative = Self::children_with_field(node, "alternative");
        for ch in alternative {
            self.walk(ch);
        }
    }
}
