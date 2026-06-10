use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use helm_schema_ast::{DefineIndex, HelmAst, Literal, TemplateExpr, parse_action_expressions};

use crate::binding::{BoundHelperCallsCacheKey, FragmentBinding, HelperBinding};
use crate::define_body_cache::{DefineBodyCache, parse_go_template};
use crate::helper_analysis::{
    BoundHelperAnalysis, HelperFragmentOutputUse, HelperOutputMeta, bound_helper_condition_paths,
    bound_helper_dependency_paths, convert_fragment_outputs_to_dependency_outputs,
    extend_type_hints, insert_type_hint, merge_helper_output_meta_maps, merge_local_default_paths,
};
use crate::output_path;
use crate::rendered_yaml_context::RenderedYamlContext;
use crate::resource_detector::AstResourceDetector;
use crate::static_file_template::{
    StaticFileTemplate, collect_template_requests, literal_helper_calls,
};
use crate::template_expr_analysis::{
    expr_contains_helper_call, is_merge_function, text_pipeline_merges_into_var,
    text_starts_with_helper_call, walk_expr_excluding_helper_call_args,
};
use crate::template_expr_cache::{
    clear_template_expr_cache, parse_expr_text as parse_cached_expr_text,
};
use crate::value_use_postprocess::postprocess_value_uses;
use crate::walker::{is_fragment_expr, parse_condition, values_path_from_expr};
use crate::yaml_shape::{first_mapping_colon_offset, parse_yaml_key};
use crate::{Guard, IrGenerator, ResourceRef, ValueKind, ValueUse, YamlPath};

fn strip_template_action_wrapping(line: &str) -> Option<String> {
    let after_open = line.trim_start().strip_prefix("{{")?;
    let close_at = after_open.find("}}")?;
    let body = &after_open[..close_at];
    let body = body.strip_prefix('-').unwrap_or(body);
    let body = body.strip_suffix('-').unwrap_or(body);
    Some(body.trim().to_string())
}

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

#[derive(Clone, Debug)]
struct GetBinding {
    base: String,
    key_var: String,
}

#[derive(Clone, Copy)]
struct FragmentEvalContext<'a> {
    defines: &'a DefineIndex,
    define_bodies: &'a DefineBodyCache,
}

impl<'a> FragmentEvalContext<'a> {
    fn new(defines: &'a DefineIndex, define_bodies: &'a DefineBodyCache) -> Self {
        Self {
            defines,
            define_bodies,
        }
    }

    fn fragment_binding_from_expr(
        &self,
        expr: &TemplateExpr,
        locals: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        seen: &mut HashSet<String>,
    ) -> Option<FragmentBinding> {
        SymbolicWalker::fragment_binding_from_expr(expr, locals, current_dot, *self, seen)
    }
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
        FragmentEvalContext::new(self.defines, &self.ir_context.inner.define_bodies)
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

    fn parse_literal_list_range(header: &str) -> Option<(String, Vec<String>)> {
        if !header.contains("list") {
            return None;
        }

        let toks: Vec<&str> = header.split_whitespace().collect();
        let list_pos = toks.iter().position(|t| *t == "list")?;

        // `range $k := list ...` or `$k := list ...` or `list ...` (in some tree-sitter nodes).
        // We only care about the bound variable name and the literal domain.
        let var = toks
            .iter()
            .take(list_pos)
            .find_map(|t| t.strip_prefix('$'))
            .filter(|v| !v.is_empty() && !v.contains('.') && !v.contains('/') && !v.contains('('))
            .map(std::string::ToString::to_string)?;

        let mut out = Vec::new();
        for t in toks.iter().skip(list_pos + 1) {
            if let Some(s) = t.strip_prefix('"').and_then(|x| x.strip_suffix('"'))
                && !s.is_empty()
            {
                out.push(s.to_string());
            }
        }
        if out.is_empty() {
            None
        } else {
            Some((var, out))
        }
    }

    fn parse_get_binding(text: &str) -> Option<(String, GetBinding)> {
        // Patterns like:
        //   $x := get $.Values.foo.bar $k
        //   $x = get $.Values.foo $k
        let toks: Vec<&str> = text.split_whitespace().collect();
        let get_pos = toks.iter().position(|t| *t == "get")?;
        if get_pos < 2 {
            return None;
        }
        if get_pos + 2 >= toks.len() {
            return None;
        }

        let op = toks[get_pos - 1];
        if op != ":=" && op != "=" {
            return None;
        }

        let var_tok = toks[get_pos - 2];
        let var = var_tok.strip_prefix('$')?.to_string();

        let base_tok = toks[get_pos + 1];
        let base = base_tok
            .strip_prefix("$.Values.")
            .or_else(|| base_tok.strip_prefix(".Values."))?
            .to_string();

        let key_tok = toks[get_pos + 2];
        let key_var = key_tok.strip_prefix('$')?.to_string();
        Some((var, GetBinding { base, key_var }))
    }

    fn eq_literals_for_var(text: &str, var: &str) -> Vec<String> {
        let needle = format!("eq ${var} \"");
        let mut literals = Vec::new();
        let mut rest = text;
        while let Some(i) = rest.find(&needle) {
            let after = &rest[(i + needle.len())..];
            if let Some(end) = after.find('"') {
                let lit = &after[..end];
                if !lit.is_empty() {
                    literals.push(lit.to_string());
                }
                rest = &after[end..];
            } else {
                break;
            }
        }
        literals
    }

    fn extract_bound_values(&self, text: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();

        // Track `$var.field[.subfield]...` reads for templates that bind
        // `$var := get $.Values.someMap $key` inside a known key-domain range.
        for tok in text.split_whitespace() {
            let Some(tok) = tok.strip_prefix('$') else {
                continue;
            };
            let Some((var, rest)) = tok.split_once('.') else {
                continue;
            };

            let rest = rest
                .trim_end_matches(',')
                .trim_end_matches(')')
                .trim_end_matches('}')
                .trim_end_matches('|');

            let Some(binding) = self.get_bindings.get(var) else {
                continue;
            };
            let Some(domain) = self.range_domains.get(&binding.key_var) else {
                continue;
            };

            let mut skip_literals: HashSet<String> = HashSet::new();
            if rest == "enabled" && binding.base == "config" {
                for lit in Self::eq_literals_for_var(text, &binding.key_var) {
                    skip_literals.insert(lit);
                }
            }
            for v in domain {
                if skip_literals.contains(v) {
                    continue;
                }
                out.push(format!("{}.{}.{}", binding.base, v, rest));
            }
        }

        out.sort();
        out.dedup();
        out
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

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn resolved_values_paths_in_context(&self, text: &str) -> Vec<String> {
        let exprs = Self::parse_expr_text(text);
        let mut paths = Self::direct_values_paths_from_exprs(&exprs);

        if !self.root_bindings.is_empty() {
            for expr in &exprs {
                walk_expr_excluding_helper_call_args(expr, &mut |node| {
                    if let Some(path) = Self::resolve_bound_path_expr(node, &self.root_bindings) {
                        paths.insert(path);
                    }
                });
            }
        }

        if !self.template_bindings.is_empty() {
            for expr in &exprs {
                walk_expr_excluding_helper_call_args(expr, &mut |node| {
                    paths.extend(self.local_alias_paths_for_expr(node));
                });
            }
        }

        if paths.is_empty() {
            paths.extend(self.resolved_values_paths_in_expr_tree_context(text));
        }

        paths.into_iter().collect()
    }

    fn direct_values_paths_from_exprs(exprs: &[TemplateExpr]) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        for expr in exprs {
            walk_expr_excluding_helper_call_args(expr, &mut |node| {
                if let Some(path) = values_path_from_expr(node) {
                    paths.insert(path);
                }
            });
        }
        paths
    }

    fn resolve_expr_to_values_path_in_context(
        expr: &TemplateExpr,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> Option<String> {
        if let Some(path) = values_path_from_expr(expr) {
            return Some(path);
        }

        match Self::binding_from_expr(expr, bindings, current_dot) {
            Some(HelperBinding::ValuesPath(path)) => Some(path),
            _ => None,
        }
    }

    fn resolve_expr_to_values_paths_in_context(
        expr: &TemplateExpr,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeSet<String> {
        if let Some(path) = values_path_from_expr(expr) {
            return [path].into_iter().collect();
        }

        Self::binding_from_expr(expr, bindings, current_dot)
            .map(|binding| binding.paths())
            .unwrap_or_default()
    }

    fn resolved_default_fallback_paths_in_context(&self, text: &str) -> BTreeSet<String> {
        let current_dot = self.current_dot_binding();
        let mut paths = Self::resolved_default_fallback_paths_for_text(
            text,
            Some(&self.root_bindings),
            current_dot.as_ref(),
        );
        for expr in Self::parse_expr_text(text) {
            paths.extend(self.resolved_default_fallback_paths_for_expr_in_current_context(&expr));
        }
        if !self.template_default_paths.is_empty() {
            for expr in Self::parse_expr_text(text) {
                expr.walk(|node| {
                    paths.extend(self.local_alias_default_paths_for_expr(node));
                });
            }
        }
        paths
    }

    fn resolved_default_fallback_paths_for_expr_in_current_context(
        &self,
        expr: &TemplateExpr,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        expr.walk(|node| match node {
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                out.extend(self.resolve_expr_to_values_paths_in_current_context(&args[1]));
            }
            TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
                for window in stages.windows(2) {
                    let TemplateExpr::Call { function, .. } = &window[1] else {
                        continue;
                    };
                    if function != "default" {
                        continue;
                    }
                    out.extend(self.resolve_expr_to_values_paths_in_current_context(&window[0]));
                }
            }
            _ => {}
        });
        out
    }

    fn resolve_expr_to_values_paths_in_current_context(
        &self,
        expr: &TemplateExpr,
    ) -> BTreeSet<String> {
        if let Some(path) = values_path_from_expr(expr) {
            return [path].into_iter().collect();
        }

        let mut locals = self.template_bindings.clone();
        for (key, value) in &self.root_bindings {
            locals.insert(key.clone(), value.to_fragment_binding());
        }

        let current_dot_fragment = self.current_dot_fragment();
        let current_dot_binding = self.current_dot_binding();
        let outer_binding = Self::fragment_binding_from_outer_expr(
            expr,
            Some(&locals),
            Some(&self.root_bindings),
            current_dot_binding.as_ref(),
        );
        let binding = match outer_binding {
            Some(binding) if !FragmentBinding::paths(&binding).is_empty() => Some(binding),
            _ => self.fragment_binding_in_context(expr, current_dot_fragment.as_ref()),
        };

        binding
            .map(|binding| {
                FragmentBinding::paths(&binding)
                    .into_iter()
                    .filter(|path| !path.trim().is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn local_alias_paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => self
                .template_bindings
                .get(var)
                .map(FragmentBinding::paths)
                .unwrap_or_default(),
            TemplateExpr::Selector { operand, path } => match operand.as_ref() {
                TemplateExpr::Variable(var) if !var.is_empty() => self
                    .template_bindings
                    .get(var)
                    .and_then(|binding| binding.apply_to_binding(path))
                    .map(|binding| FragmentBinding::paths(&binding))
                    .unwrap_or_default(),
                _ => BTreeSet::new(),
            },
            _ => BTreeSet::new(),
        }
    }

    fn local_alias_default_paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => self
                .template_default_paths
                .get(var)
                .cloned()
                .unwrap_or_default(),
            _ => BTreeSet::new(),
        }
    }

    fn local_alias_output_meta_for_text(&self, text: &str) -> BTreeMap<String, HelperOutputMeta> {
        let mut out: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        for expr in Self::parse_expr_text(text) {
            walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                for (path, meta) in self.local_alias_output_meta_for_expr(node) {
                    let entry = out.entry(path).or_default();
                    entry.guards.extend(meta.guards);
                    entry.defaulted |= meta.defaulted;
                }
            });
        }
        out
    }

    fn helper_output_meta_for_text(&self, text: &str) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = self.local_alias_output_meta_for_text(text);
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

    fn local_alias_output_meta_for_expr(
        &self,
        expr: &TemplateExpr,
    ) -> BTreeMap<String, HelperOutputMeta> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => self
                .template_output_meta
                .get(var)
                .cloned()
                .unwrap_or_default(),
            TemplateExpr::Selector { operand, path } => {
                let TemplateExpr::Variable(var) = operand.as_ref() else {
                    return BTreeMap::new();
                };
                if var.is_empty() {
                    return BTreeMap::new();
                }
                let Some(binding) = self.template_bindings.get(var) else {
                    return BTreeMap::new();
                };
                let Some(bound) = binding.apply_to_binding(path) else {
                    return BTreeMap::new();
                };
                let selected_paths = FragmentBinding::paths(&bound);
                self.template_output_meta
                    .get(var)
                    .into_iter()
                    .flat_map(|meta_by_path| meta_by_path.iter())
                    .filter(|(path, _meta)| selected_paths.contains(*path))
                    .map(|(path, meta)| (path.clone(), meta.clone()))
                    .collect()
            }
            _ => BTreeMap::new(),
        }
    }

    fn condition_guards_in_context(&self, text: &str) -> Vec<Guard> {
        let mut cond_guards = parse_condition(text);
        let alias_guards = self.condition_guards_from_aliases_in_context(text);
        cond_guards
            .retain(|guard| !Self::guard_is_subsumed_by_alias_or_guard(guard, &alias_guards));
        for guard in alias_guards {
            if !cond_guards.contains(&guard) {
                cond_guards.push(guard);
            }
        }
        if !cond_guards.is_empty() {
            return cond_guards;
        }
        if self.condition_has_unrepresentable_values_comparison(text) {
            return Vec::new();
        }
        self.resolved_values_paths_in_expr_tree_context(text)
            .into_iter()
            .map(|path| Guard::Truthy { path })
            .collect()
    }

    fn guard_is_subsumed_by_alias_or_guard(guard: &Guard, alias_guards: &[Guard]) -> bool {
        if !matches!(guard, Guard::Truthy { .. } | Guard::Or { .. }) {
            return false;
        }

        alias_guards.iter().any(|alias_guard| {
            let Guard::Or { paths } = alias_guard else {
                return false;
            };
            guard.value_paths().iter().all(|path| {
                paths
                    .iter()
                    .any(|alias_guard_path| alias_guard_path == path)
            })
        })
    }

    fn expr_needs_context_value_resolution(&self, expr: &TemplateExpr) -> bool {
        !self.local_alias_paths_for_expr(expr).is_empty()
            || (values_path_from_expr(expr).is_none()
                && !self
                    .resolve_expr_to_values_paths_in_current_context(expr)
                    .is_empty())
    }

    fn condition_guards_from_aliases_in_context(&self, text: &str) -> Vec<Guard> {
        fn string_literal(arg: &TemplateExpr) -> Option<String> {
            match arg.deparen() {
                TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
                    Some(value.clone())
                }
                _ => None,
            }
        }

        fn paths_for_expr(walker: &SymbolicWalker<'_>, expr: &TemplateExpr) -> BTreeSet<String> {
            let mut paths = walker.resolve_expr_to_values_paths_in_current_context(expr);
            paths.extend(walker.local_alias_paths_for_expr(expr));
            paths
                .into_iter()
                .filter(|path| !path.trim().is_empty())
                .collect()
        }

        let mut out = Vec::new();
        for expr in Self::parse_expr_text(text) {
            let TemplateExpr::Call { function, args } = expr.deparen() else {
                continue;
            };
            match function.as_str() {
                "not" => {
                    let [arg] = args.as_slice() else {
                        continue;
                    };
                    if !self.expr_needs_context_value_resolution(arg) {
                        continue;
                    }
                    let paths = paths_for_expr(self, arg);
                    out.extend(paths.into_iter().map(|path| Guard::Not { path }));
                }
                "or" => {
                    if !args
                        .iter()
                        .any(|arg| self.expr_needs_context_value_resolution(arg))
                    {
                        continue;
                    }
                    let paths: BTreeSet<String> = args
                        .iter()
                        .flat_map(|arg| paths_for_expr(self, arg))
                        .collect();
                    if !paths.is_empty() {
                        out.push(Guard::Or {
                            paths: paths.into_iter().collect(),
                        });
                    }
                }
                "eq" => {
                    let [left, right] = args.as_slice() else {
                        continue;
                    };
                    if !self.expr_needs_context_value_resolution(left)
                        && !self.expr_needs_context_value_resolution(right)
                    {
                        continue;
                    }
                    let (value, paths) = match (string_literal(left), string_literal(right)) {
                        (Some(value), None) => (value, paths_for_expr(self, right)),
                        (None, Some(value)) => (value, paths_for_expr(self, left)),
                        _ => continue,
                    };
                    out.extend(paths.into_iter().map(|path| Guard::Eq {
                        path,
                        value: value.clone(),
                    }));
                }
                "typeIs" => {
                    let Some(schema_type) = Self::type_is_schema_type(args.first()) else {
                        continue;
                    };
                    if !args
                        .iter()
                        .skip(1)
                        .any(|arg| self.expr_needs_context_value_resolution(arg))
                    {
                        continue;
                    }
                    let paths: BTreeSet<String> = args
                        .iter()
                        .skip(1)
                        .flat_map(|arg| paths_for_expr(self, arg))
                        .collect();
                    out.extend(paths.into_iter().map(|path| Guard::TypeIs {
                        path,
                        schema_type: schema_type.clone(),
                    }));
                }
                _ => {}
            }
        }
        out
    }

    fn condition_has_unrepresentable_values_comparison(&self, text: &str) -> bool {
        fn string_literal(arg: &TemplateExpr) -> Option<&str> {
            match arg.deparen() {
                TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
                    Some(value)
                }
                _ => None,
            }
        }

        Self::parse_expr_text(text).into_iter().any(|expr| {
            let TemplateExpr::Call { function, args } = expr.deparen() else {
                return false;
            };
            match function.as_str() {
                "eq" => {
                    let has_values_path = args
                        .iter()
                        .any(|arg| self.expr_needs_context_value_resolution(arg));
                    if !has_values_path {
                        return false;
                    }
                    let [left, right] = args.as_slice() else {
                        return true;
                    };
                    !matches!(
                        (string_literal(left), string_literal(right)),
                        (Some(_), None) | (None, Some(_))
                    )
                }
                "ne" | "typeIs" => args
                    .iter()
                    .any(|arg| self.expr_needs_context_value_resolution(arg)),
                _ => false,
            }
        })
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn resolved_values_paths_in_expr_tree_context(&self, text: &str) -> BTreeSet<String> {
        let mut locals = self.template_bindings.clone();
        for (key, value) in &self.root_bindings {
            locals.insert(key.clone(), value.to_fragment_binding());
        }

        let current_dot_fragment = self.current_dot_fragment();
        let current_dot_binding = self.current_dot_binding();
        let mut paths = BTreeSet::new();
        for expr in Self::parse_expr_text(text) {
            walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                if expr_contains_helper_call(node) {
                    return;
                }
                let outer_binding = Self::fragment_binding_from_outer_expr(
                    node,
                    Some(&locals),
                    Some(&self.root_bindings),
                    current_dot_binding.as_ref(),
                );
                let binding = match outer_binding {
                    Some(binding) if !FragmentBinding::paths(&binding).is_empty() => Some(binding),
                    _ => self.fragment_binding_in_context(node, current_dot_fragment.as_ref()),
                };
                if let Some(binding) = binding {
                    paths.extend(
                        FragmentBinding::paths(&binding)
                            .into_iter()
                            .filter(|path| !path.trim().is_empty()),
                    );
                }
            });
        }
        paths
    }

    fn single_resolved_values_path(&self, text: &str) -> Option<String> {
        let mut paths = self.resolved_values_paths_in_context(text);
        if paths.len() == 1 { paths.pop() } else { None }
    }

    fn is_direct_path_expr(expr: &TemplateExpr, bindings: &HashMap<String, HelperBinding>) -> bool {
        match expr {
            TemplateExpr::Parenthesized(inner) => Self::is_direct_path_expr(inner, bindings),
            TemplateExpr::Field(_) => true,
            TemplateExpr::Selector { .. } => {
                Self::resolve_bound_path_expr(expr, bindings).is_some()
            }
            _ => false,
        }
    }

    fn single_direct_iterable_range_path(&self, text: &str) -> Option<String> {
        let exprs = Self::parse_expr_text(text);
        if exprs.len() != 1 || !Self::is_direct_path_expr(&exprs[0], &self.root_bindings) {
            return None;
        }
        self.single_resolved_values_path(text)
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
        let bindings = Self::bindings_for_helper_arg(
            args.get(1),
            Some(&self.root_bindings),
            current_dot.as_ref(),
        );
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

        let default_fallback_values = self.resolved_default_fallback_paths_in_context(text);
        let mut values: BTreeSet<String> = self
            .resolved_values_paths_in_context(text)
            .into_iter()
            .collect();
        values.extend(default_fallback_values.iter().cloned());
        let local_output_meta = self.local_alias_output_meta_for_text(text);

        let bound_values = self.extract_bound_values(text);

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

    fn resolve_bound_path_expr(
        expr: &TemplateExpr,
        bindings: &HashMap<String, HelperBinding>,
    ) -> Option<String> {
        if let Some(path) = values_path_from_expr(expr) {
            return Some(path);
        }

        match expr {
            TemplateExpr::Parenthesized(inner) => Self::resolve_bound_path_expr(inner, bindings),
            TemplateExpr::Field(path) => Self::resolve_bound_segments(path, bindings),
            TemplateExpr::Selector { operand, path } => {
                if let TemplateExpr::Variable(var) = operand.as_ref()
                    && var.is_empty()
                    && let Some((head, tail)) = path.split_first()
                    && let Some(binding) = bindings.get(head)
                {
                    return binding.apply_unique_path(tail);
                }
                if let Some(binding) = Self::binding_from_expr(operand, Some(bindings), None) {
                    return binding.apply_unique_path(path);
                }
                None
            }
            _ => None,
        }
    }

    fn resolve_bound_segments(
        segments: &[String],
        bindings: &HashMap<String, HelperBinding>,
    ) -> Option<String> {
        let binding = Self::binding_from_bound_segments(segments, bindings)?;
        let paths = binding.paths();
        let mut paths = paths.into_iter();
        let first = paths.next()?;
        if paths.next().is_none() {
            Some(first)
        } else {
            None
        }
    }

    fn binding_from_bound_segments(
        segments: &[String],
        bindings: &HashMap<String, HelperBinding>,
    ) -> Option<HelperBinding> {
        let (first, rest) = segments.split_first()?;
        let binding = bindings.get(first)?;
        binding.apply_to_binding(rest)
    }

    fn binding_from_expr(
        expr: &TemplateExpr,
        outer: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> Option<HelperBinding> {
        if let Some(path) = values_path_from_expr(expr) {
            return Some(HelperBinding::ValuesPath(path));
        }

        match expr {
            TemplateExpr::Parenthesized(inner) => {
                Self::binding_from_expr(inner, outer, current_dot)
            }
            TemplateExpr::Field(path) if path.is_empty() => {
                current_dot.cloned().or(Some(HelperBinding::RootContext))
            }
            TemplateExpr::Variable(var) if var.is_empty() => Some(HelperBinding::RootContext),
            TemplateExpr::Variable(_) => None,
            TemplateExpr::Selector { operand, path } => {
                let binding = Self::binding_from_expr(operand, outer, current_dot)?;
                binding.apply_to_binding(path)
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "list" | "tuple") =>
            {
                let mut items = Vec::new();
                for arg in args {
                    items.push(
                        Self::binding_from_expr(arg, outer, current_dot)
                            .unwrap_or(HelperBinding::Unknown),
                    );
                }
                Some(HelperBinding::List(items))
            }
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut map = BTreeMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                        &args[index]
                    else {
                        index += 1;
                        continue;
                    };
                    let binding = Self::binding_from_expr(&args[index + 1], outer, current_dot)
                        .unwrap_or(HelperBinding::Unknown);
                    map.insert(key.clone(), binding);
                    index += 2;
                }
                Some(HelperBinding::Dict(map))
            }
            TemplateExpr::Call { function, args } if is_merge_function(function) => {
                let mut bindings = Vec::new();
                for arg in args {
                    if let Some(binding) = Self::binding_from_expr(arg, outer, current_dot) {
                        bindings.push(binding);
                    }
                }
                HelperBinding::merge_all(bindings)
            }
            TemplateExpr::Call { function, args } if function == "coalesce" => {
                let mut choices = Vec::new();
                for arg in args {
                    if let Some(binding) = Self::binding_from_expr(arg, outer, current_dot) {
                        choices.push(binding);
                    }
                }
                HelperBinding::choice(choices)
            }
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                let mut choices = Vec::new();
                if let Some(primary) = Self::binding_from_expr(&args[1], outer, current_dot) {
                    choices.push(primary);
                }
                if let Some(fallback) = Self::binding_from_expr(&args[0], outer, current_dot) {
                    choices.push(fallback);
                }
                HelperBinding::choice(choices)
            }
            TemplateExpr::Call { function, args } if function == "ternary" => {
                let mut choices = Vec::new();
                for arg in args.iter().take(2) {
                    if let Some(binding) = Self::binding_from_expr(arg, outer, current_dot) {
                        choices.push(binding);
                    }
                }
                HelperBinding::choice(choices)
            }
            TemplateExpr::Pipeline(stages) => {
                let mut current = Self::binding_from_expr(&stages[0], outer, current_dot);
                for stage in &stages[1..] {
                    let TemplateExpr::Call { function, args } = stage else {
                        continue;
                    };
                    current = match function.as_str() {
                        "default" => {
                            let mut choices = Vec::new();
                            if let Some(current) = current {
                                choices.push(current);
                            }
                            for arg in args {
                                if let Some(binding) =
                                    Self::binding_from_expr(arg, outer, current_dot)
                                {
                                    choices.push(binding);
                                }
                            }
                            HelperBinding::choice(choices)
                        }
                        function if is_merge_function(function) => {
                            let mut bindings = Vec::new();
                            if let Some(current) = current {
                                bindings.push(current);
                            }
                            for arg in args {
                                if let Some(binding) =
                                    Self::binding_from_expr(arg, outer, current_dot)
                                {
                                    bindings.push(binding);
                                }
                            }
                            HelperBinding::merge_all(bindings)
                        }
                        "toYaml" | "fromYaml" | "quote" | "toString" | "deepCopy" | "tpl"
                        | "nindent" | "indent" => current,
                        _ => None,
                    };
                }
                current
            }
            TemplateExpr::Call { function, args } if function == "index" => {
                let mut binding = Self::binding_from_expr(args.first()?, outer, current_dot)?;
                for arg in &args[1..] {
                    let segment = match arg {
                        TemplateExpr::Literal(
                            Literal::String(value) | Literal::RawString(value),
                        ) => value.clone(),
                        TemplateExpr::Literal(Literal::Int(value)) => value.to_string(),
                        _ => return None,
                    };
                    binding = match &binding {
                        HelperBinding::ValuesPath(_)
                        | HelperBinding::RootContext
                        | HelperBinding::Unknown
                        | HelperBinding::OutputSet(_)
                        | HelperBinding::PathSet(_)
                        | HelperBinding::Dict(_)
                        | HelperBinding::List(_)
                        | HelperBinding::Overlay { .. }
                        | HelperBinding::Choice(_) => binding.apply_to_binding(&[segment])?,
                    };
                }
                Some(binding)
            }
            TemplateExpr::Field(path) => {
                // Helper bindings take priority: a `Field` whose head
                // names a helper-bound key resolves through that
                // binding (e.g. a `dict "ctx" .Values.cfg` helper call
                // binds `ctx`, so the callee's `.ctx.X` becomes
                // `cfg.X`).
                if let Some(bound) =
                    outer.and_then(|bindings| Self::binding_from_bound_segments(path, bindings))
                {
                    return Some(bound);
                }
                // Otherwise apply the field selector to the current dot.
                // This covers direct values dots (`with .Values.foo`) and
                // overlay dots produced by helper arguments like
                // `merge (dict "file" "...") .`, where explicit dict keys
                // shadow the fallback map but missing keys still resolve
                // through that fallback.
                if let Some(current_dot) = current_dot
                    && let Some(bound) = current_dot.apply_to_binding(path)
                {
                    return Some(bound);
                }
                None
            }
            _ => outer
                .and_then(|bindings| Self::resolve_bound_path_expr(expr, bindings))
                .map(HelperBinding::ValuesPath),
        }
    }

    fn bindings_for_helper_arg(
        arg: Option<&TemplateExpr>,
        outer: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> HashMap<String, HelperBinding> {
        let Some(arg) = arg else {
            return HashMap::new();
        };

        match arg {
            TemplateExpr::Parenthesized(inner) => {
                Self::bindings_for_helper_arg(Some(inner), outer, current_dot)
            }
            TemplateExpr::Field(path) if path.is_empty() => outer.cloned().unwrap_or_default(),
            TemplateExpr::Variable(var) if var.is_empty() => outer.cloned().unwrap_or_default(),
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut bindings = HashMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key)) = &args[index] else {
                        index += 1;
                        continue;
                    };
                    let binding = Self::binding_from_expr(&args[index + 1], outer, current_dot)
                        .unwrap_or(HelperBinding::Unknown);
                    bindings.insert(key.clone(), binding);
                    index += 2;
                }
                bindings
            }
            TemplateExpr::Call { function, args } if is_merge_function(function) => {
                let mut merged = HashMap::new();
                for arg in args {
                    match Self::binding_from_expr(arg, outer, current_dot) {
                        Some(HelperBinding::Dict(map)) => {
                            for (key, value) in map {
                                merged.insert(key, value);
                            }
                        }
                        Some(HelperBinding::RootContext) => {
                            if let Some(outer) = outer {
                                for (key, value) in outer {
                                    merged.insert(key.clone(), value.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                merged
            }
            _ => HashMap::new(),
        }
    }

    fn helper_binding_from_expr_with_fragment_locals(
        expr: &TemplateExpr,
        fragment_locals: &HashMap<String, FragmentBinding>,
        outer: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> Option<HelperBinding> {
        match expr {
            TemplateExpr::Parenthesized(inner) => {
                Self::helper_binding_from_expr_with_fragment_locals(
                    inner,
                    fragment_locals,
                    outer,
                    current_dot,
                    context,
                    seen,
                )
            }
            TemplateExpr::Variable(var) if !var.is_empty() => fragment_locals
                .get(var)
                .and_then(FragmentBinding::to_helper_binding),
            TemplateExpr::Selector { operand, path } => {
                if let TemplateExpr::Variable(var) = operand.as_ref()
                    && !var.is_empty()
                    && let Some(binding) = fragment_locals
                        .get(var)
                        .and_then(FragmentBinding::to_helper_binding)
                {
                    return binding.apply_to_binding(path);
                }
                Self::binding_from_expr(expr, outer, current_dot)
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "include" | "template") =>
            {
                let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
                    return None;
                };
                let analysis = Self::analyze_bound_helper_call_with_fragment_locals(
                    name,
                    args.get(1),
                    outer,
                    current_dot,
                    fragment_locals,
                    context,
                    seen,
                );
                Self::helper_binding_from_helper_analysis(analysis)
            }
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut map = BTreeMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                        &args[index]
                    else {
                        index += 1;
                        continue;
                    };
                    let binding = Self::helper_binding_from_expr_with_fragment_locals(
                        &args[index + 1],
                        fragment_locals,
                        outer,
                        current_dot,
                        context,
                        seen,
                    )
                    .unwrap_or(HelperBinding::Unknown);
                    map.insert(key.clone(), binding);
                    index += 2;
                }
                Some(HelperBinding::Dict(map))
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "list" | "tuple") =>
            {
                Some(HelperBinding::List(
                    args.iter()
                        .map(|arg| {
                            Self::helper_binding_from_expr_with_fragment_locals(
                                arg,
                                fragment_locals,
                                outer,
                                current_dot,
                                context,
                                seen,
                            )
                            .unwrap_or(HelperBinding::Unknown)
                        })
                        .collect(),
                ))
            }
            TemplateExpr::Call { function, args } if is_merge_function(function) => {
                let bindings = args
                    .iter()
                    .filter_map(|arg| {
                        Self::helper_binding_from_expr_with_fragment_locals(
                            arg,
                            fragment_locals,
                            outer,
                            current_dot,
                            context,
                            seen,
                        )
                    })
                    .collect();
                HelperBinding::merge_all(bindings)
            }
            TemplateExpr::Pipeline(stages) => {
                let mut current = Self::helper_binding_from_expr_with_fragment_locals(
                    &stages[0],
                    fragment_locals,
                    outer,
                    current_dot,
                    context,
                    seen,
                );
                for stage in &stages[1..] {
                    let TemplateExpr::Call { function, args } = stage else {
                        continue;
                    };
                    current = match function.as_str() {
                        "default" => {
                            let mut bindings = Vec::new();
                            if let Some(current) = current {
                                bindings.push(current);
                            }
                            for arg in args {
                                if let Some(binding) =
                                    Self::helper_binding_from_expr_with_fragment_locals(
                                        arg,
                                        fragment_locals,
                                        outer,
                                        current_dot,
                                        context,
                                        seen,
                                    )
                                {
                                    bindings.push(binding);
                                }
                            }
                            HelperBinding::choice(bindings)
                        }
                        function if is_merge_function(function) => {
                            let mut bindings = Vec::new();
                            if let Some(current) = current {
                                bindings.push(current);
                            }
                            for arg in args {
                                if let Some(binding) =
                                    Self::helper_binding_from_expr_with_fragment_locals(
                                        arg,
                                        fragment_locals,
                                        outer,
                                        current_dot,
                                        context,
                                        seen,
                                    )
                                {
                                    bindings.push(binding);
                                }
                            }
                            HelperBinding::merge_all(bindings)
                        }
                        "toYaml" | "fromYaml" | "toJson" | "fromJson" | "quote" | "toString"
                        | "deepCopy" | "tpl" | "nindent" | "indent" => current,
                        _ => None,
                    };
                }
                current
            }
            _ => Self::binding_from_expr(expr, outer, current_dot),
        }
    }

    fn bindings_for_helper_arg_with_fragment_locals(
        arg: Option<&TemplateExpr>,
        outer: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> HashMap<String, HelperBinding> {
        let Some(arg) = arg else {
            return HashMap::new();
        };

        match arg {
            TemplateExpr::Parenthesized(inner) => {
                Self::bindings_for_helper_arg_with_fragment_locals(
                    Some(inner),
                    outer,
                    current_dot,
                    fragment_locals,
                    context,
                    seen,
                )
            }
            TemplateExpr::Field(path) if path.is_empty() => outer.cloned().unwrap_or_default(),
            TemplateExpr::Variable(var) if var.is_empty() => outer.cloned().unwrap_or_default(),
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut bindings = HashMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                        &args[index]
                    else {
                        index += 1;
                        continue;
                    };
                    let binding = Self::helper_binding_from_expr_with_fragment_locals(
                        &args[index + 1],
                        fragment_locals,
                        outer,
                        current_dot,
                        context,
                        seen,
                    )
                    .unwrap_or(HelperBinding::Unknown);
                    bindings.insert(key.clone(), binding);
                    index += 2;
                }
                bindings
            }
            TemplateExpr::Call { function, args } if is_merge_function(function) => {
                let mut merged = HashMap::new();
                for arg in args {
                    match Self::helper_binding_from_expr_with_fragment_locals(
                        arg,
                        fragment_locals,
                        outer,
                        current_dot,
                        context,
                        seen,
                    ) {
                        Some(HelperBinding::Dict(map)) => {
                            for (key, value) in map {
                                merged.insert(key, value);
                            }
                        }
                        Some(HelperBinding::RootContext) => {
                            if let Some(outer) = outer {
                                for (key, value) in outer {
                                    merged.insert(key.clone(), value.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                merged
            }
            _ => HashMap::new(),
        }
    }

    fn fragment_binding_from_helper_analysis(
        mut analysis: BoundHelperAnalysis,
    ) -> Option<FragmentBinding> {
        let structured_sources: BTreeSet<String> = analysis
            .fragment_output_uses
            .iter()
            .map(|output| output.source_expr.clone())
            .collect();
        let has_fragment_outputs =
            !analysis.fragment_output.is_empty() || !analysis.fragment_output_uses.is_empty();
        let mut rendered_sources = structured_sources.clone();
        rendered_sources.extend(analysis.fragment_output.iter().cloned());
        rendered_sources.extend(analysis.output.keys().cloned());
        let mut bindings = Vec::new();
        for output in analysis.fragment_output_uses.drain(..) {
            bindings.push(FragmentBinding::for_output_path(
                output.source_expr,
                &output.relative_path,
            ));
        }
        for source in analysis.fragment_output {
            if !structured_sources.contains(&source)
                && !output_path::values_path_has_descendant(&source, &rendered_sources)
            {
                bindings.push(FragmentBinding::OutputSet([source].into_iter().collect()));
            }
        }
        if !has_fragment_outputs {
            for source in analysis.output.into_keys() {
                if !structured_sources.contains(&source)
                    && !output_path::values_path_has_descendant(&source, &rendered_sources)
                {
                    bindings.push(FragmentBinding::OutputSet([source].into_iter().collect()));
                }
            }
        }
        FragmentBinding::merge_all(bindings)
    }

    fn helper_binding_from_helper_analysis(
        mut analysis: BoundHelperAnalysis,
    ) -> Option<HelperBinding> {
        let structured_sources: BTreeSet<String> = analysis
            .fragment_output_uses
            .iter()
            .map(|output| output.source_expr.clone())
            .collect();
        let has_fragment_outputs =
            !analysis.fragment_output.is_empty() || !analysis.fragment_output_uses.is_empty();
        let mut rendered_sources = structured_sources.clone();
        rendered_sources.extend(analysis.fragment_output.iter().cloned());
        rendered_sources.extend(analysis.output.keys().cloned());

        let mut bindings = Vec::new();
        for output in analysis.fragment_output_uses.drain(..) {
            bindings.push(HelperBinding::for_output_path(
                output.source_expr,
                &output.relative_path,
                output.meta,
            ));
        }
        for source in analysis.fragment_output {
            if !structured_sources.contains(&source)
                && !output_path::values_path_has_descendant(&source, &rendered_sources)
            {
                bindings.push(HelperBinding::PathSet([source].into_iter().collect()));
            }
        }
        if !has_fragment_outputs {
            for (source, meta) in analysis.output {
                if !structured_sources.contains(&source)
                    && !output_path::values_path_has_descendant(&source, &rendered_sources)
                {
                    bindings.push(HelperBinding::OutputSet(
                        [(source, meta)].into_iter().collect(),
                    ));
                }
            }
        }
        HelperBinding::merge_all(bindings)
    }

    fn fragment_binding_from_outer_expr(
        expr: &TemplateExpr,
        outer_locals: Option<&HashMap<String, FragmentBinding>>,
        outer: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> Option<FragmentBinding> {
        if let Some(path) = values_path_from_expr(expr) {
            return Some(FragmentBinding::ValuesPath(path));
        }

        match expr {
            TemplateExpr::Literal(Literal::String(value)) => Some(FragmentBinding::StringSet(
                [value.clone()].into_iter().collect(),
            )),
            TemplateExpr::Parenthesized(inner) => {
                Self::fragment_binding_from_outer_expr(inner, outer_locals, outer, current_dot)
            }
            TemplateExpr::Field(path) if path.is_empty() => {
                if let Some(bindings) = outer {
                    return Some(FragmentBinding::Dict(
                        bindings
                            .iter()
                            .map(|(key, binding)| (key.clone(), binding.to_fragment_binding()))
                            .collect(),
                    ));
                }
                current_dot
                    .map(HelperBinding::to_fragment_binding)
                    .or(Some(FragmentBinding::RootContext))
            }
            TemplateExpr::Field(path)
                if path.first().is_some_and(|segment| segment == "Values") =>
            {
                Some(FragmentBinding::ValuesPath(path[1..].join(".")))
            }
            TemplateExpr::Variable(var) if var.is_empty() => {
                if let Some(bindings) = outer {
                    return Some(FragmentBinding::Dict(
                        bindings
                            .iter()
                            .map(|(key, binding)| (key.clone(), binding.to_fragment_binding()))
                            .collect(),
                    ));
                }
                Some(FragmentBinding::RootContext)
            }
            TemplateExpr::Variable(var) if !var.is_empty() => {
                outer_locals.and_then(|locals| locals.get(var).cloned())
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "list" | "tuple") =>
            {
                let mut items = Vec::new();
                for arg in args {
                    items.push(
                        Self::fragment_binding_from_outer_expr(
                            arg,
                            outer_locals,
                            outer,
                            current_dot,
                        )
                        .unwrap_or(FragmentBinding::Unknown),
                    );
                }
                Some(FragmentBinding::List(items))
            }
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut map = BTreeMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key)) = &args[index] else {
                        index += 1;
                        continue;
                    };
                    if let Some(binding) = Self::fragment_binding_from_outer_expr(
                        &args[index + 1],
                        outer_locals,
                        outer,
                        current_dot,
                    ) {
                        map.insert(key.clone(), binding);
                    }
                    index += 2;
                }
                Some(FragmentBinding::Dict(map))
            }
            TemplateExpr::Call { function, args } if function == "coalesce" => {
                let mut choices = Vec::new();
                for arg in args {
                    if let Some(binding) = Self::fragment_binding_from_outer_expr(
                        arg,
                        outer_locals,
                        outer,
                        current_dot,
                    ) {
                        choices.push(binding);
                    }
                }
                FragmentBinding::choice(choices)
            }
            TemplateExpr::Call { function, args } if function == "ternary" => {
                let mut choices = Vec::new();
                for arg in args.iter().take(2) {
                    if let Some(binding) = Self::fragment_binding_from_outer_expr(
                        arg,
                        outer_locals,
                        outer,
                        current_dot,
                    ) {
                        choices.push(binding);
                    }
                }
                FragmentBinding::choice(choices)
            }
            _ => Self::binding_from_expr(expr, outer, current_dot)
                .map(|binding| binding.to_fragment_binding()),
        }
    }

    fn fragment_binding_from_expr(
        expr: &TemplateExpr,
        locals: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> Option<FragmentBinding> {
        if let Some(path) = values_path_from_expr(expr) {
            return Some(FragmentBinding::ValuesPath(path));
        }

        match expr {
            TemplateExpr::Literal(Literal::String(value)) => Some(FragmentBinding::StringSet(
                [value.clone()].into_iter().collect(),
            )),
            TemplateExpr::Parenthesized(inner) => {
                Self::fragment_binding_from_expr(inner, locals, current_dot, context, seen)
            }
            TemplateExpr::Field(path)
                if path.first().is_some_and(|segment| segment == "Values") =>
            {
                Some(FragmentBinding::ValuesPath(path[1..].join(".")))
            }
            TemplateExpr::Field(path) if path.is_empty() => {
                current_dot.cloned().or(Some(FragmentBinding::RootContext))
            }
            TemplateExpr::Field(path) => {
                let dot = current_dot?;
                dot.apply_to_binding(path)
            }
            TemplateExpr::Variable(var) if var.is_empty() => Some(FragmentBinding::RootContext),
            TemplateExpr::Variable(var) => locals.get(var).cloned(),
            TemplateExpr::Selector { operand, path } => {
                if let TemplateExpr::Variable(var) = operand.as_ref()
                    && var.is_empty()
                    && let Some((head, tail)) = path.split_first()
                    && let Some(binding) = locals.get(head)
                {
                    return binding.apply_to_binding(tail);
                }
                let binding =
                    Self::fragment_binding_from_expr(operand, locals, current_dot, context, seen)?;
                binding.apply_to_binding(path)
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "list" | "tuple") =>
            {
                let mut items = Vec::new();
                for arg in args {
                    items.push(
                        Self::fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                            .unwrap_or(FragmentBinding::Unknown),
                    );
                }
                Some(FragmentBinding::List(items))
            }
            TemplateExpr::Call { function, args } if function == "append" => {
                let mut items = match Self::fragment_binding_from_expr(
                    args.first()?,
                    locals,
                    current_dot,
                    context,
                    seen,
                ) {
                    Some(FragmentBinding::List(items)) => items,
                    Some(binding) => vec![binding],
                    None => Vec::new(),
                };
                for arg in &args[1..] {
                    if let Some(binding) =
                        Self::fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                    {
                        items.push(binding);
                    }
                }
                Some(FragmentBinding::List(items))
            }
            TemplateExpr::Call { function, args } if function == "dict" => {
                let mut map = BTreeMap::new();
                let mut index = 0usize;
                while index + 1 < args.len() {
                    let TemplateExpr::Literal(Literal::String(key)) = &args[index] else {
                        index += 1;
                        continue;
                    };
                    if let Some(binding) = Self::fragment_binding_from_expr(
                        &args[index + 1],
                        locals,
                        current_dot,
                        context,
                        seen,
                    ) {
                        map.insert(key.clone(), binding);
                    }
                    index += 2;
                }
                Some(FragmentBinding::Dict(map))
            }
            TemplateExpr::Call { function, args } if is_merge_function(function) => {
                let mut bindings = Vec::new();
                for arg in args {
                    let Some(binding) =
                        Self::fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                    else {
                        continue;
                    };
                    bindings.push(binding);
                }
                FragmentBinding::merge_all(bindings)
            }
            TemplateExpr::Call { function, args } if function == "coalesce" => {
                let mut choices = Vec::new();
                for arg in args {
                    if let Some(binding) =
                        Self::fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                    {
                        choices.push(binding);
                    }
                }
                FragmentBinding::choice(choices)
            }
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                let mut choices = Vec::new();
                if let Some(binding) =
                    Self::fragment_binding_from_expr(&args[1], locals, current_dot, context, seen)
                {
                    choices.push(binding);
                }
                if let Some(binding) =
                    Self::fragment_binding_from_expr(&args[0], locals, current_dot, context, seen)
                {
                    choices.push(binding);
                }
                FragmentBinding::choice(choices)
            }
            TemplateExpr::Call { function, args }
                if matches!(
                    function.as_str(),
                    "toYaml"
                        | "fromYaml"
                        | "quote"
                        | "toString"
                        | "int"
                        | "tpl"
                        | "b64enc"
                        | "b64dec"
                ) =>
            {
                Self::fragment_binding_from_expr(args.first()?, locals, current_dot, context, seen)
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "indent" | "nindent" | "trimAll") =>
            {
                Self::fragment_binding_from_expr(args.last()?, locals, current_dot, context, seen)
            }
            TemplateExpr::Call { function, args } if function == "printf" => {
                let TemplateExpr::Literal(Literal::String(format)) = args.first()? else {
                    return None;
                };
                let mut rendered: BTreeSet<String> = [format.clone()].into_iter().collect();
                for arg in &args[1..] {
                    let strings = FragmentBinding::strings(&Self::fragment_binding_from_expr(
                        arg,
                        locals,
                        current_dot,
                        context,
                        seen,
                    )?);
                    if strings.is_empty() {
                        return None;
                    }
                    let mut next = BTreeSet::new();
                    for current in &rendered {
                        for value in &strings {
                            next.insert(current.replacen("%s", value, 1));
                        }
                    }
                    rendered = next;
                }
                Some(FragmentBinding::StringSet(rendered))
            }
            TemplateExpr::Call { function, args } if function == "index" => {
                let base = Self::fragment_binding_from_expr(
                    args.first()?,
                    locals,
                    current_dot,
                    context,
                    seen,
                )?;
                match base {
                    FragmentBinding::List(items) if args.len() == 2 => {
                        let index = match &args[1] {
                            TemplateExpr::Literal(Literal::Int(value)) => {
                                usize::try_from(*value).ok()?
                            }
                            _ => {
                                let strings =
                                    FragmentBinding::strings(&Self::fragment_binding_from_expr(
                                        &args[1],
                                        locals,
                                        current_dot,
                                        context,
                                        seen,
                                    )?);
                                strings.iter().next()?.parse::<usize>().ok()?
                            }
                        };
                        items.get(index).cloned()
                    }
                    binding => {
                        let mut segment_options: Vec<Vec<String>> = Vec::new();
                        for arg in &args[1..] {
                            let arg_binding = Self::fragment_binding_from_expr(
                                arg,
                                locals,
                                current_dot,
                                context,
                                seen,
                            );
                            let strings = FragmentBinding::strings(&arg_binding?);
                            if strings.is_empty() {
                                return None;
                            }
                            segment_options.push(strings.into_iter().collect());
                        }

                        let mut bindings = vec![binding.clone()];
                        for options in segment_options {
                            let mut next = Vec::new();
                            for binding in &bindings {
                                for option in &options {
                                    if let Some(bound) =
                                        binding.apply_to_binding(std::slice::from_ref(option))
                                    {
                                        next.push(bound);
                                    }
                                }
                            }
                            bindings = next;
                        }
                        FragmentBinding::choice(bindings)
                    }
                }
            }
            TemplateExpr::Call { function, args }
                if matches!(function.as_str(), "include" | "template") =>
            {
                let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
                    return None;
                };
                let current_dot_helper = current_dot.and_then(FragmentBinding::to_helper_binding);
                let analysis = Self::analyze_bound_helper_call_with_fragment_locals(
                    name,
                    args.get(1),
                    None,
                    current_dot_helper.as_ref(),
                    locals,
                    context,
                    seen,
                );
                Self::fragment_binding_from_helper_analysis(analysis)
            }
            TemplateExpr::Call { function, args } if function == "tpl" => {
                Self::fragment_binding_from_expr(args.first()?, locals, current_dot, context, seen)
            }
            TemplateExpr::Call { function, args } if function == "ternary" => {
                let mut choices = Vec::new();
                for arg in args.iter().take(2) {
                    if let Some(binding) =
                        Self::fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                    {
                        choices.push(binding);
                    }
                }
                FragmentBinding::choice(choices)
            }
            TemplateExpr::Pipeline(stages) => {
                let mut current = Self::fragment_binding_from_expr(
                    &stages[0],
                    locals,
                    current_dot,
                    context,
                    seen,
                );
                for stage in &stages[1..] {
                    let TemplateExpr::Call { function, args } = stage else {
                        continue;
                    };
                    current = match function.as_str() {
                        "quote" | "toString" | "toYaml" | "fromYaml" | "indent" | "nindent"
                        | "trimAll" | "trimPrefix" | "trimSuffix" | "trunc" | "replace" | "int"
                        | "uniq" | "b64enc" | "b64dec" => current,
                        function if is_merge_function(function) => {
                            let mut bindings = Vec::new();
                            if let Some(current) = current {
                                bindings.push(current);
                            }
                            for arg in args {
                                if let Some(binding) = Self::fragment_binding_from_expr(
                                    arg,
                                    locals,
                                    current_dot,
                                    context,
                                    seen,
                                ) {
                                    bindings.push(binding);
                                }
                            }
                            FragmentBinding::merge_all(bindings)
                        }
                        "default" => {
                            let mut choices = Vec::new();
                            if let Some(current) = current {
                                choices.push(current);
                            }
                            for arg in args {
                                if let Some(binding) = Self::fragment_binding_from_expr(
                                    arg,
                                    locals,
                                    current_dot,
                                    context,
                                    seen,
                                ) {
                                    choices.push(binding);
                                }
                            }
                            FragmentBinding::choice(choices)
                        }
                        "ternary" => {
                            let mut choices = Vec::new();
                            if let Some(current) = current {
                                choices.push(current);
                            }
                            for arg in args {
                                if let Some(binding) = Self::fragment_binding_from_expr(
                                    arg,
                                    locals,
                                    current_dot,
                                    context,
                                    seen,
                                ) {
                                    choices.push(binding);
                                }
                            }
                            FragmentBinding::choice(choices)
                        }
                        _ => return None,
                    };
                }
                current
            }
            _ => None,
        }
    }

    fn parse_helper_assignment(text: &str) -> Option<(String, bool, String)> {
        let owned;
        let trimmed = if text.trim_start().starts_with("{{") {
            owned = strip_template_action_wrapping(text)?;
            owned.trim()
        } else {
            text.trim()
        };
        if let Some(index) = trimmed.find(":=") {
            let var = trimmed[..index].trim().strip_prefix('$')?.to_string();
            return Some((var, true, trimmed[index + 2..].trim().to_string()));
        }
        if let Some(index) = trimmed.find(" = ") {
            let var = trimmed[..index].trim().strip_prefix('$')?.to_string();
            return Some((var, false, trimmed[index + 3..].trim().to_string()));
        }
        None
    }

    fn merge_fragment_locals(
        mut base: HashMap<String, FragmentBinding>,
        other: HashMap<String, FragmentBinding>,
    ) -> HashMap<String, FragmentBinding> {
        for (key, value) in other {
            let merged = FragmentBinding::union(base.remove(&key), Some(value));
            if let Some(merged) = merged {
                base.insert(key, merged);
            }
        }
        base
    }

    fn shadow_fragment_binding_keys(
        binding: FragmentBinding,
        keys: BTreeSet<String>,
    ) -> FragmentBinding {
        if keys.is_empty() {
            return binding;
        }
        let new_entries: BTreeMap<String, FragmentBinding> = keys
            .into_iter()
            .map(|key| (key, FragmentBinding::Unknown))
            .collect();
        match binding {
            FragmentBinding::Overlay {
                mut entries,
                fallback,
            } => {
                entries.extend(new_entries);
                FragmentBinding::Overlay { entries, fallback }
            }
            other => FragmentBinding::Overlay {
                entries: new_entries,
                fallback: Box::new(other),
            },
        }
    }

    fn local_set_mutation_target_and_keys(
        text: &str,
        local_bindings: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> Vec<(String, BTreeSet<String>)> {
        let mut out = Vec::new();
        for expr in Self::parse_expr_text(text) {
            expr.walk(|node| {
                let TemplateExpr::Call { function, args } = node else {
                    return;
                };
                if function != "set" || args.len() < 2 {
                    return;
                }
                let TemplateExpr::Variable(var) = &args[0] else {
                    return;
                };
                if var.is_empty() || !local_bindings.contains_key(var) {
                    return;
                }
                let Some(key_binding) =
                    context.fragment_binding_from_expr(&args[1], local_bindings, current_dot, seen)
                else {
                    return;
                };
                let keys = FragmentBinding::strings(&key_binding);
                if !keys.is_empty() {
                    out.push((var.clone(), keys));
                }
            });
        }
        out
    }

    fn apply_local_set_mutations(
        text: &str,
        local_bindings: &mut HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> bool {
        let mutations = Self::local_set_mutation_target_and_keys(
            text,
            local_bindings,
            current_dot,
            context,
            seen,
        );
        let has_mutation = !mutations.is_empty();
        for (var, keys) in mutations {
            if let Some(binding) = local_bindings.remove(&var) {
                local_bindings.insert(var, Self::shadow_fragment_binding_keys(binding, keys));
            }
        }
        has_mutation
    }

    fn range_variable_item_binding(
        header: &str,
        local_bindings: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> Option<(String, FragmentBinding)> {
        let header = header
            .trim()
            .strip_prefix("range ")
            .unwrap_or_else(|| header.trim());
        let exprs = Self::parse_expr_text(header);
        let [TemplateExpr::VariableDefinition { name, value }] = exprs.as_slice() else {
            return None;
        };
        let binding = Self::fragment_binding_from_range_value_expr(
            value,
            local_bindings,
            current_dot,
            context,
            seen,
        )?;
        let item = FragmentBinding::item_binding(&binding)?;
        Some((name.trim_start_matches('$').to_string(), item))
    }

    fn range_variable_name(header: &str) -> Option<String> {
        let header = header
            .trim()
            .strip_prefix("range ")
            .unwrap_or_else(|| header.trim());
        let exprs = Self::parse_expr_text(header);
        let [TemplateExpr::VariableDefinition { name, .. }] = exprs.as_slice() else {
            return None;
        };
        Some(name.trim_start_matches('$').to_string())
    }

    fn range_iterable_binding(
        header: &str,
        local_bindings: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> Option<FragmentBinding> {
        let header = header
            .trim()
            .strip_prefix("range ")
            .unwrap_or_else(|| header.trim());
        let exprs = Self::parse_expr_text(header);
        let value = match exprs.as_slice() {
            [TemplateExpr::VariableDefinition { value, .. }]
            | [TemplateExpr::Assignment { value, .. }] => value.as_ref(),
            [expr] => expr,
            _ => return None,
        };
        Self::fragment_binding_from_range_value_expr(
            value,
            local_bindings,
            current_dot,
            context,
            seen,
        )
    }

    fn fragment_binding_from_range_value_expr(
        value: &TemplateExpr,
        local_bindings: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> Option<FragmentBinding> {
        context.fragment_binding_from_expr(value, local_bindings, current_dot, seen)
    }

    fn range_header_text_from_source(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
        if let Some(range) = node.child_by_field_name("range") {
            return range
                .utf8_text(source.as_bytes())
                .ok()
                .map(|text| text.trim().to_string());
        }
        let mut walker = node.walk();
        for child in node.named_children(&mut walker) {
            if child.kind() == "range_variable_definition"
                && let Some(range) = child.child_by_field_name("range")
            {
                return range
                    .utf8_text(source.as_bytes())
                    .ok()
                    .map(|text| text.trim().to_string());
            }
        }
        None
    }

    fn range_body_emits_sequence_item_from_source(
        node: tree_sitter::Node<'_>,
        source: &str,
    ) -> bool {
        for body_node in Self::children_with_field(node, "body") {
            let Ok(text) = body_node.utf8_text(source.as_bytes()) else {
                continue;
            };
            for line in text.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("- ") || trimmed == "-" {
                    return true;
                }
            }
        }
        false
    }

    fn range_has_destructured_variable_definition(node: tree_sitter::Node<'_>) -> bool {
        let mut walker = node.walk();
        node.named_children(&mut walker)
            .find(|child| child.kind() == "range_variable_definition")
            .is_some_and(|definition| {
                let mut definition_walker = definition.walk();
                definition
                    .named_children(&mut definition_walker)
                    .filter(|child| child.kind() == "variable")
                    .count()
                    >= 2
            })
    }

    fn fragment_binding_from_text(
        text: &str,
        locals: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> Option<FragmentBinding> {
        let mut bindings = Vec::new();
        for expr in Self::parse_expr_text(text) {
            if let Some(binding) =
                context.fragment_binding_from_expr(&expr, locals, current_dot, seen)
            {
                bindings.push(binding);
            }
        }
        FragmentBinding::choice(bindings)
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
                    if Self::apply_local_set_mutations(text, locals, current_dot, context, seen) {
                        return;
                    }
                    if let Some((var, _declares, rhs)) = Self::parse_helper_assignment(text) {
                        let binding = Self::fragment_binding_from_text(
                            &rhs,
                            locals,
                            current_dot,
                            context,
                            seen,
                        );
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
                        Self::fragment_binding_from_text(text, locals, current_dot, context, seen)
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

                *locals = Self::merge_fragment_locals(then_locals, else_locals);
            }
            "with_action" => {
                let binding = node
                    .child_by_field_name("condition")
                    .and_then(|condition| condition.utf8_text(source.as_bytes()).ok())
                    .and_then(|text| {
                        Self::fragment_binding_from_text(text, locals, current_dot, context, seen)
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
                    Self::range_has_destructured_variable_definition(node);
                let header = Self::range_header_text_from_source(node, source);
                let binding = header.as_deref().and_then(|text| {
                    Self::range_iterable_binding(text, locals, current_dot, context, seen)
                });
                if has_destructured_variable_definition
                    && !Self::range_body_emits_sequence_item_from_source(node, source)
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
                    *locals = Self::merge_fragment_locals(locals.clone(), body_locals);
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

    fn expression_output_use_is_keyed_map_projection(
        output: &HelperFragmentOutputUse,
        expression_base: &YamlPath,
    ) -> bool {
        let suffix = if output.relative_path.0.starts_with(&expression_base.0) {
            &output.relative_path.0[expression_base.0.len()..]
        } else {
            output.relative_path.0.as_slice()
        };
        !suffix.is_empty() && suffix.iter().all(|segment| !segment.ends_with("[*]"))
    }

    fn static_yaml_fragment_output_path(text: &str) -> Option<YamlPath> {
        fn printf_format(expr: &TemplateExpr) -> Option<&str> {
            match expr {
                TemplateExpr::Parenthesized(inner) => printf_format(inner),
                TemplateExpr::Call { function, args } if function == "printf" => {
                    let TemplateExpr::Literal(Literal::String(format) | Literal::RawString(format)) =
                        args.first()?
                    else {
                        return None;
                    };
                    Some(format)
                }
                TemplateExpr::Pipeline(stages) => stages.first().and_then(printf_format),
                _ => None,
            }
        }

        let exprs = Self::parse_expr_text(text);
        let [expr] = exprs.as_slice() else {
            return None;
        };
        let format = printf_format(expr)?;
        let key = parse_yaml_key(format.trim_start())?.into_key();
        Some(YamlPath(vec![key]))
    }

    fn helper_output_meta_with_guards(
        mut meta: HelperOutputMeta,
        active_output_guards: &BTreeSet<String>,
    ) -> HelperOutputMeta {
        meta.guards.extend(active_output_guards.iter().cloned());
        meta
    }

    fn push_helper_fragment_output(
        outputs: &mut Vec<HelperFragmentOutputUse>,
        source_expr: String,
        relative_path: &YamlPath,
        kind: ValueKind,
        meta: HelperOutputMeta,
    ) {
        outputs.push(HelperFragmentOutputUse {
            source_expr,
            relative_path: relative_path.clone(),
            kind,
            meta,
        });
    }

    fn collect_fragment_binding_output_uses(
        outputs: &mut Vec<HelperFragmentOutputUse>,
        binding: &FragmentBinding,
        relative_path: &YamlPath,
        kind: ValueKind,
        active_output_guards: &BTreeSet<String>,
        defaulted_paths: &BTreeSet<String>,
    ) {
        match binding {
            FragmentBinding::ValuesPath(path) => {
                Self::push_helper_fragment_output(
                    outputs,
                    path.clone(),
                    relative_path,
                    kind,
                    HelperOutputMeta {
                        guards: active_output_guards.clone(),
                        defaulted: defaulted_paths.contains(path),
                    },
                );
            }
            FragmentBinding::PathSet(paths) => {
                for path in paths {
                    Self::push_helper_fragment_output(
                        outputs,
                        path.clone(),
                        relative_path,
                        kind,
                        HelperOutputMeta {
                            guards: active_output_guards.clone(),
                            defaulted: defaulted_paths.contains(path),
                        },
                    );
                }
            }
            FragmentBinding::OutputSet(paths) => {
                for path in paths {
                    Self::push_helper_fragment_output(
                        outputs,
                        path.clone(),
                        relative_path,
                        kind,
                        HelperOutputMeta {
                            guards: active_output_guards.clone(),
                            defaulted: defaulted_paths.contains(path),
                        },
                    );
                }
            }
            FragmentBinding::Dict(entries) => {
                for (key, value) in entries {
                    let child_path = output_path::append_relative_path(
                        relative_path,
                        &YamlPath(vec![key.clone()]),
                    );
                    Self::collect_fragment_binding_output_uses(
                        outputs,
                        value,
                        &child_path,
                        value.output_child_kind(),
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            FragmentBinding::Overlay { entries, fallback } => {
                Self::collect_fragment_binding_output_uses(
                    outputs,
                    fallback,
                    relative_path,
                    kind,
                    active_output_guards,
                    defaulted_paths,
                );
                for (key, value) in entries {
                    let child_path = output_path::append_relative_path(
                        relative_path,
                        &YamlPath(vec![key.clone()]),
                    );
                    Self::collect_fragment_binding_output_uses(
                        outputs,
                        value,
                        &child_path,
                        value.output_child_kind(),
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            FragmentBinding::Choice(choices) => {
                for choice in choices {
                    Self::collect_fragment_binding_output_uses(
                        outputs,
                        choice,
                        relative_path,
                        kind,
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            FragmentBinding::List(items) => {
                let item_path = output_path::sequence_item_path(relative_path);
                for item in items {
                    Self::collect_fragment_binding_output_uses(
                        outputs,
                        item,
                        &item_path,
                        item.output_child_kind(),
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            FragmentBinding::ValuesRoot
            | FragmentBinding::RootContext
            | FragmentBinding::Unknown
            | FragmentBinding::StringSet(_) => {}
        }
    }

    fn collect_helper_binding_output_uses(
        outputs: &mut Vec<HelperFragmentOutputUse>,
        binding: &HelperBinding,
        relative_path: &YamlPath,
        kind: ValueKind,
        active_output_guards: &BTreeSet<String>,
        defaulted_paths: &BTreeSet<String>,
    ) {
        match binding {
            HelperBinding::ValuesPath(path) => {
                Self::push_helper_fragment_output(
                    outputs,
                    path.clone(),
                    relative_path,
                    kind,
                    HelperOutputMeta {
                        guards: active_output_guards.clone(),
                        defaulted: defaulted_paths.contains(path),
                    },
                );
            }
            HelperBinding::PathSet(paths) => {
                for path in paths {
                    Self::push_helper_fragment_output(
                        outputs,
                        path.clone(),
                        relative_path,
                        kind,
                        HelperOutputMeta {
                            guards: active_output_guards.clone(),
                            defaulted: defaulted_paths.contains(path),
                        },
                    );
                }
            }
            HelperBinding::OutputSet(outputs_by_path) => {
                for (path, meta) in outputs_by_path {
                    let meta = Self::helper_output_meta_with_guards(
                        HelperOutputMeta {
                            guards: meta.guards.clone(),
                            defaulted: meta.defaulted || defaulted_paths.contains(path),
                        },
                        active_output_guards,
                    );
                    Self::push_helper_fragment_output(
                        outputs,
                        path.clone(),
                        relative_path,
                        kind,
                        meta,
                    );
                }
            }
            HelperBinding::Dict(entries) => {
                for (key, value) in entries {
                    let child_path = output_path::append_relative_path(
                        relative_path,
                        &YamlPath(vec![key.clone()]),
                    );
                    Self::collect_helper_binding_output_uses(
                        outputs,
                        value,
                        &child_path,
                        value.output_child_kind(),
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            HelperBinding::Overlay { entries, fallback } => {
                Self::collect_helper_binding_output_uses(
                    outputs,
                    fallback,
                    relative_path,
                    kind,
                    active_output_guards,
                    defaulted_paths,
                );
                for (key, value) in entries {
                    let child_path = output_path::append_relative_path(
                        relative_path,
                        &YamlPath(vec![key.clone()]),
                    );
                    Self::collect_helper_binding_output_uses(
                        outputs,
                        value,
                        &child_path,
                        value.output_child_kind(),
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            HelperBinding::Choice(choices) => {
                for choice in choices {
                    Self::collect_helper_binding_output_uses(
                        outputs,
                        choice,
                        relative_path,
                        kind,
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            HelperBinding::List(items) => {
                let item_path = output_path::sequence_item_path(relative_path);
                for item in items {
                    Self::collect_helper_binding_output_uses(
                        outputs,
                        item,
                        &item_path,
                        item.output_child_kind(),
                        active_output_guards,
                        defaulted_paths,
                    );
                }
            }
            HelperBinding::RootContext | HelperBinding::Unknown => {}
        }
    }

    fn collect_helper_binding_output_uses_from_expr(
        expr: &TemplateExpr,
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
        relative_path: &YamlPath,
        kind: ValueKind,
        active_output_guards: &BTreeSet<String>,
        defaulted_paths: &BTreeSet<String>,
        outputs: &mut Vec<HelperFragmentOutputUse>,
    ) {
        if expr_contains_helper_call(expr) {
            return;
        }

        if let Some(binding) = Self::binding_from_expr(expr, Some(bindings), current_dot) {
            Self::collect_helper_binding_output_uses(
                outputs,
                &binding,
                relative_path,
                kind,
                active_output_guards,
                defaulted_paths,
            );
            return;
        }

        match expr {
            TemplateExpr::Call { args, .. } => {
                for arg in args {
                    Self::collect_helper_binding_output_uses_from_expr(
                        arg,
                        bindings,
                        current_dot,
                        relative_path,
                        kind,
                        active_output_guards,
                        defaulted_paths,
                        outputs,
                    );
                }
            }
            TemplateExpr::Selector { operand, .. } => {
                Self::collect_helper_binding_output_uses_from_expr(
                    operand,
                    bindings,
                    current_dot,
                    relative_path,
                    kind,
                    active_output_guards,
                    defaulted_paths,
                    outputs,
                );
            }
            TemplateExpr::Pipeline(stages) => {
                for stage in stages {
                    Self::collect_helper_binding_output_uses_from_expr(
                        stage,
                        bindings,
                        current_dot,
                        relative_path,
                        kind,
                        active_output_guards,
                        defaulted_paths,
                        outputs,
                    );
                }
            }
            TemplateExpr::Parenthesized(inner)
            | TemplateExpr::VariableDefinition { value: inner, .. }
            | TemplateExpr::Assignment { value: inner, .. } => {
                Self::collect_helper_binding_output_uses_from_expr(
                    inner,
                    bindings,
                    current_dot,
                    relative_path,
                    kind,
                    active_output_guards,
                    defaulted_paths,
                    outputs,
                );
            }
            TemplateExpr::Literal(_)
            | TemplateExpr::Field(_)
            | TemplateExpr::Variable(_)
            | TemplateExpr::Unknown(_) => {}
        }
    }

    fn collect_bound_fragment_output_uses_from_items(
        items: &[HelmAst],
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
        current_dot_fragment: Option<&FragmentBinding>,
        relative_path: &YamlPath,
        active_output_guards: &BTreeSet<String>,
        local_bindings: &mut HashMap<String, FragmentBinding>,
        local_default_paths: &mut HashMap<String, BTreeSet<String>>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
        outputs: &mut Vec<HelperFragmentOutputUse>,
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
                local_bindings,
                local_default_paths,
                context,
                seen,
                outputs,
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
        local_bindings: &mut HashMap<String, FragmentBinding>,
        local_default_paths: &mut HashMap<String, BTreeSet<String>>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
        outputs: &mut Vec<HelperFragmentOutputUse>,
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
                    local_bindings,
                    local_default_paths,
                    context,
                    seen,
                    outputs,
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
                        local_bindings,
                        local_default_paths,
                        context,
                        seen,
                        outputs,
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
                            local_bindings,
                            local_default_paths,
                            context,
                            seen,
                            outputs,
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
                    local_bindings,
                    local_default_paths,
                    context,
                    seen,
                    outputs,
                );
                if let Some(value) = value {
                    Self::collect_bound_fragment_output_uses_from_ast(
                        value,
                        bindings,
                        current_dot,
                        current_dot_fragment,
                        relative_path,
                        active_output_guards,
                        local_bindings,
                        local_default_paths,
                        context,
                        seen,
                        outputs,
                    );
                }
            }
            HelmAst::HelmExpr { text } => {
                let mut seen_set = HashSet::new();
                if Self::apply_local_set_mutations(
                    text,
                    local_bindings,
                    current_dot_fragment,
                    context,
                    &mut seen_set,
                ) {
                    return;
                }

                if let Some((var, _declares, rhs)) = Self::parse_helper_assignment(text) {
                    let mut seen_rhs = HashSet::new();
                    let mut binding = Self::fragment_binding_from_text(
                        &rhs,
                        local_bindings,
                        current_dot_fragment,
                        context,
                        &mut seen_rhs,
                    );
                    let mut top_level_helper_dependency_paths = BTreeSet::new();
                    if text_starts_with_helper_call(&rhs) {
                        let mut rhs_seen = seen.clone();
                        let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                            &rhs,
                            Some(bindings),
                            current_dot,
                            local_bindings,
                            context,
                            &mut rhs_seen,
                        );
                        top_level_helper_dependency_paths = bound_helper_dependency_paths(&nested);
                        if let Some(nested_binding) =
                            Self::fragment_binding_from_helper_analysis(nested)
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
                        local_bindings.insert(var.clone(), binding);
                    }
                    let mut defaulted_paths = Self::resolved_default_fallback_paths_for_text(
                        &rhs,
                        Some(bindings),
                        current_dot,
                    );
                    defaulted_paths.extend(Self::local_default_paths_from_text(
                        &rhs,
                        local_default_paths,
                    ));
                    if defaulted_paths.is_empty() {
                        local_default_paths.remove(&var);
                    } else {
                        local_default_paths.insert(var.clone(), defaulted_paths);
                    }
                    return;
                }

                let kind = if is_fragment_expr(text) {
                    ValueKind::Fragment
                } else {
                    ValueKind::Scalar
                };
                let output_path = Self::static_yaml_fragment_output_path(text)
                    .map(|output_path| {
                        output_path::append_relative_path(relative_path, &output_path)
                    })
                    .unwrap_or_else(|| relative_path.clone());
                let direct_outputs =
                    Self::direct_bound_paths_from_text_in_context(text, bindings, current_dot);
                let fallback_paths = Self::resolved_default_fallback_paths_for_text(
                    text,
                    Some(bindings),
                    current_dot,
                );
                let local_outputs = Self::local_rendered_paths_from_text(text, local_bindings);
                let handled_outputs: BTreeSet<String> = direct_outputs
                    .iter()
                    .chain(local_outputs.iter())
                    .cloned()
                    .collect();
                let mut direct_output_uses = Vec::new();
                for expr in Self::parse_expr_text(text) {
                    Self::collect_helper_binding_output_uses_from_expr(
                        &expr,
                        bindings,
                        current_dot,
                        &output_path,
                        kind,
                        active_output_guards,
                        &fallback_paths,
                        &mut direct_output_uses,
                    );
                }
                outputs.extend(direct_output_uses);

                let local_fallback_paths =
                    Self::local_default_paths_from_text(text, local_default_paths);
                let mut local_output_uses = Vec::new();
                for expr in Self::parse_expr_text(text) {
                    walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                        let binding = match node {
                            TemplateExpr::Variable(var) if !var.is_empty() => {
                                local_bindings.get(var).cloned()
                            }
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
                            Self::collect_fragment_binding_output_uses(
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
                    local_bindings,
                    context,
                    seen,
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
                        Self::expression_output_use_is_keyed_map_projection(
                            output,
                            &empty_output_path,
                        )
                    })
                    .map(|output| output.source_expr.clone())
                    .collect();
                let nested_scalar_sources: BTreeSet<String> =
                    nested.output.keys().cloned().collect();
                let nested_has_fragment_outputs =
                    !nested.fragment_output.is_empty() || !nested.fragment_output_uses.is_empty();

                let mut expression_output_uses = Vec::new();
                let mut expression_seen = seen.clone();
                for expr in Self::parse_expr_text(text) {
                    if !expr_contains_helper_call(&expr) {
                        continue;
                    }
                    if let Some(binding) = Self::helper_binding_from_expr_with_fragment_locals(
                        &expr,
                        local_bindings,
                        Some(bindings),
                        current_dot,
                        context,
                        &mut expression_seen,
                    ) {
                        Self::collect_helper_binding_output_uses(
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
                    Self::expression_output_use_is_keyed_map_projection(output, &output_path)
                });
                let expression_descendant_sources: BTreeSet<String> = expression_output_uses
                    .iter()
                    .filter(|output| !output.relative_path.0.is_empty())
                    .map(|output| output.source_expr.clone())
                    .collect();

                outputs.extend(local_output_uses);
                for output in expression_output_uses {
                    if output.relative_path.0.is_empty()
                        && (handled_outputs.contains(&output.source_expr)
                            || nested_structured_sources.contains(&output.source_expr)
                            || nested_scalar_sources.contains(&output.source_expr))
                    {
                        continue;
                    }
                    outputs.push(output);
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
                    let meta = Self::helper_output_meta_with_guards(meta, active_output_guards);
                    Self::push_helper_fragment_output(
                        outputs,
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
                    let meta = Self::helper_output_meta_with_guards(
                        nested_output.meta,
                        active_output_guards,
                    );
                    Self::push_helper_fragment_output(
                        outputs,
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
                    Self::direct_bound_paths_from_text_in_context(cond, bindings, current_dot);
                branch_guard_paths.extend(Self::local_bound_paths_from_text(cond, local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    cond,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    context,
                    seen,
                );
                branch_guard_paths.extend(bound_helper_condition_paths(&nested));

                let mut then_guards = active_output_guards.clone();
                then_guards.extend(branch_guard_paths);
                let mut then_bindings = local_bindings.clone();
                let mut then_defaults = local_default_paths.clone();
                Self::collect_bound_fragment_output_uses_from_items(
                    then_branch,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    &then_guards,
                    &mut then_bindings,
                    &mut then_defaults,
                    context,
                    seen,
                    outputs,
                );

                let mut else_bindings = local_bindings.clone();
                let mut else_defaults = local_default_paths.clone();
                Self::collect_bound_fragment_output_uses_from_items(
                    else_branch,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    active_output_guards,
                    &mut else_bindings,
                    &mut else_defaults,
                    context,
                    seen,
                    outputs,
                );
                *local_bindings = Self::merge_fragment_locals(then_bindings, else_bindings);
                *local_default_paths = merge_local_default_paths(then_defaults, else_defaults);
            }
            HelmAst::With {
                header,
                body,
                else_branch,
            } => {
                let mut branch_guard_paths =
                    Self::direct_bound_paths_from_text_in_context(header, bindings, current_dot);
                branch_guard_paths
                    .extend(Self::local_bound_paths_from_text(header, local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    header,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    context,
                    seen,
                );
                branch_guard_paths.extend(bound_helper_condition_paths(&nested));
                let body_dot = Self::computed_with_body_dot(header, bindings, current_dot);

                let mut body_guards = active_output_guards.clone();
                body_guards.extend(branch_guard_paths);
                let mut body_bindings = local_bindings.clone();
                let mut body_defaults = local_default_paths.clone();
                let body_dot_fragment = body_dot.as_ref().map(HelperBinding::to_fragment_binding);
                Self::collect_bound_fragment_output_uses_from_items(
                    body,
                    bindings,
                    body_dot.as_ref(),
                    body_dot_fragment.as_ref(),
                    relative_path,
                    &body_guards,
                    &mut body_bindings,
                    &mut body_defaults,
                    context,
                    seen,
                    outputs,
                );

                let mut else_bindings = local_bindings.clone();
                let mut else_defaults = local_default_paths.clone();
                Self::collect_bound_fragment_output_uses_from_items(
                    else_branch,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    active_output_guards,
                    &mut else_bindings,
                    &mut else_defaults,
                    context,
                    seen,
                    outputs,
                );
                *local_bindings = Self::merge_fragment_locals(body_bindings, else_bindings);
                *local_default_paths = merge_local_default_paths(body_defaults, else_defaults);
            }
            HelmAst::Range {
                header,
                body,
                else_branch,
            } => {
                let mut branch_guard_paths =
                    Self::direct_bound_paths_from_text_in_context(header, bindings, current_dot);
                branch_guard_paths
                    .extend(Self::local_bound_paths_from_text(header, local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    header,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    context,
                    seen,
                );
                branch_guard_paths.extend(bound_helper_condition_paths(&nested));
                let mut seen_range_binding = HashSet::new();
                let range_binding = Self::range_iterable_binding(
                    header,
                    local_bindings,
                    current_dot_fragment,
                    context,
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
                let mut body_bindings = local_bindings.clone();
                let mut body_defaults = local_default_paths.clone();
                if let Some(FragmentBinding::List(items)) = &range_binding {
                    let range_var = Self::range_variable_name(header);
                    for item_binding in items {
                        if let Some(range_var) = &range_var {
                            body_bindings.insert(range_var.clone(), item_binding.clone());
                        }
                        let item_dot = item_binding.to_helper_binding();
                        let mut item_seen = seen.clone();
                        Self::collect_bound_fragment_output_uses_from_items(
                            body,
                            bindings,
                            item_dot.as_ref(),
                            Some(item_binding),
                            relative_path,
                            &body_guards,
                            &mut body_bindings,
                            &mut body_defaults,
                            context,
                            &mut item_seen,
                            outputs,
                        );
                    }
                } else {
                    Self::collect_bound_fragment_output_uses_from_items(
                        body,
                        bindings,
                        body_dot.as_ref(),
                        body_dot_fragment.as_ref(),
                        relative_path,
                        &body_guards,
                        &mut body_bindings,
                        &mut body_defaults,
                        context,
                        seen,
                        outputs,
                    );
                }

                if range_binding
                    .as_ref()
                    .is_some_and(FragmentBinding::definitely_nonempty_iterable)
                {
                    *local_bindings = body_bindings;
                    *local_default_paths = body_defaults;
                } else {
                    let mut else_bindings = local_bindings.clone();
                    let mut else_defaults = local_default_paths.clone();
                    Self::collect_bound_fragment_output_uses_from_items(
                        else_branch,
                        bindings,
                        current_dot,
                        current_dot_fragment,
                        relative_path,
                        active_output_guards,
                        &mut else_bindings,
                        &mut else_defaults,
                        context,
                        seen,
                        outputs,
                    );
                    *local_bindings = Self::merge_fragment_locals(body_bindings, else_bindings);
                    *local_default_paths = merge_local_default_paths(body_defaults, else_defaults);
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
        let bindings = Self::bindings_for_helper_arg_with_fragment_locals(
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
                Self::helper_binding_from_expr_with_fragment_locals(
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
            for node in body {
                Self::collect_bound_helper_values_from_ast(
                    node,
                    &bindings,
                    helper_body_dot.as_ref(),
                    &active_output_guards,
                    &mut local_bindings,
                    &mut local_default_paths,
                    &mut local_output_meta,
                    context,
                    seen,
                    &mut analysis,
                );
            }
        }
        let mut helper_fragment_locals = HashMap::new();
        let helper_dot = arg.and_then(|expr| {
            Self::fragment_binding_from_outer_expr(
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
            Self::collect_bound_fragment_output_uses_from_items(
                body,
                &bindings,
                helper_body_dot.as_ref(),
                helper_dot.as_ref(),
                &YamlPath(Vec::new()),
                &active_output_guards,
                &mut local_bindings,
                &mut local_default_paths,
                context,
                seen,
                &mut fragment_output_uses,
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

    fn direct_bound_paths_from_text_in_context(
        text: &str,
        bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for expr in Self::parse_expr_text(text) {
            walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                if expr_contains_helper_call(node) {
                    return;
                }
                if let Some(binding) = Self::binding_from_expr(node, Some(bindings), current_dot) {
                    out.extend(binding.paths());
                }
            });
        }
        out
    }

    fn local_bound_paths_from_text(
        text: &str,
        locals: &HashMap<String, FragmentBinding>,
    ) -> BTreeSet<String> {
        Self::local_paths_from_text(text, locals, FragmentBinding::paths)
    }

    fn local_rendered_paths_from_text(
        text: &str,
        locals: &HashMap<String, FragmentBinding>,
    ) -> BTreeSet<String> {
        Self::local_paths_from_text(text, locals, FragmentBinding::rendered_paths)
    }

    fn local_paths_from_text(
        text: &str,
        locals: &HashMap<String, FragmentBinding>,
        extract_paths: fn(&FragmentBinding) -> BTreeSet<String>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for expr in Self::parse_expr_text(text) {
            walk_expr_excluding_helper_call_args(&expr, &mut |node| match node {
                TemplateExpr::Variable(var) if !var.is_empty() => {
                    if let Some(binding) = locals.get(var) {
                        out.extend(extract_paths(binding));
                    }
                }
                TemplateExpr::Selector { operand, path } => {
                    let TemplateExpr::Variable(var) = operand.as_ref() else {
                        return;
                    };
                    if var.is_empty() {
                        return;
                    }
                    if let Some(binding) = locals.get(var)
                        && let Some(bound) = binding.apply_to_binding(path)
                    {
                        out.extend(extract_paths(&bound));
                    }
                }
                _ => {}
            });
        }
        out
    }

    fn local_default_paths_from_text(
        text: &str,
        local_default_paths: &HashMap<String, BTreeSet<String>>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for expr in Self::parse_expr_text(text) {
            expr.walk(|node| {
                let TemplateExpr::Variable(var) = node else {
                    return;
                };
                if var.is_empty() {
                    return;
                }
                if let Some(paths) = local_default_paths.get(var) {
                    out.extend(paths.iter().cloned());
                }
            });
        }
        out
    }

    fn local_output_meta_from_text(
        text: &str,
        local_bindings: &HashMap<String, FragmentBinding>,
        local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    ) -> BTreeMap<String, HelperOutputMeta> {
        let mut out: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        for expr in Self::parse_expr_text(text) {
            walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                for (path, meta) in
                    Self::local_output_meta_from_expr(node, local_bindings, local_output_meta)
                {
                    let entry = out.entry(path).or_default();
                    entry.guards.extend(meta.guards);
                    entry.defaulted |= meta.defaulted;
                }
            });
        }
        out
    }

    fn local_output_meta_from_expr(
        expr: &TemplateExpr,
        local_bindings: &HashMap<String, FragmentBinding>,
        local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    ) -> BTreeMap<String, HelperOutputMeta> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => {
                local_output_meta.get(var).cloned().unwrap_or_default()
            }
            TemplateExpr::Selector { operand, path } => {
                let TemplateExpr::Variable(var) = operand.as_ref() else {
                    return BTreeMap::new();
                };
                if var.is_empty() {
                    return BTreeMap::new();
                }
                let Some(binding) = local_bindings.get(var) else {
                    return BTreeMap::new();
                };
                let Some(bound) = binding.apply_to_binding(path) else {
                    return BTreeMap::new();
                };
                let selected_paths = FragmentBinding::paths(&bound);
                local_output_meta
                    .get(var)
                    .into_iter()
                    .flat_map(|meta_by_path| meta_by_path.iter())
                    .filter(|(path, _meta)| selected_paths.contains(*path))
                    .map(|(path, meta)| (path.clone(), meta.clone()))
                    .collect()
            }
            _ => BTreeMap::new(),
        }
    }

    fn helper_output_meta_from_analysis(
        analysis: &BoundHelperAnalysis,
    ) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = analysis.output.clone();
        for output in &analysis.fragment_output_uses {
            let entry = out.entry(output.source_expr.clone()).or_default();
            entry.guards.extend(output.meta.guards.iter().cloned());
            entry.defaulted |= output.meta.defaulted;
        }
        for path in &analysis.fragment_output {
            out.entry(path.clone()).or_default();
        }
        out
    }

    fn helper_dependency_meta_from_analysis(
        analysis: &BoundHelperAnalysis,
    ) -> BTreeMap<String, HelperOutputMeta> {
        let mut out = analysis.dependency_meta.clone();
        for (path, meta) in Self::helper_output_meta_from_analysis(analysis) {
            let entry = out.entry(path).or_default();
            entry.guards.extend(meta.guards);
            entry.defaulted |= meta.defaulted;
        }
        out
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
        local_bindings: &mut HashMap<String, FragmentBinding>,
        local_default_paths: &mut HashMap<String, BTreeSet<String>>,
        local_output_meta: &mut HashMap<String, BTreeMap<String, HelperOutputMeta>>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
        analysis: &mut BoundHelperAnalysis,
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
                        local_bindings,
                        local_default_paths,
                        local_output_meta,
                        context,
                        seen,
                        analysis,
                    );
                }
            }
            HelmAst::Pair { key, value } => {
                Self::collect_bound_helper_values_from_ast(
                    key,
                    bindings,
                    current_dot,
                    active_output_guards,
                    local_bindings,
                    local_default_paths,
                    local_output_meta,
                    context,
                    seen,
                    analysis,
                );
                if let Some(value) = value {
                    Self::collect_bound_helper_values_from_ast(
                        value,
                        bindings,
                        current_dot,
                        active_output_guards,
                        local_bindings,
                        local_default_paths,
                        local_output_meta,
                        context,
                        seen,
                        analysis,
                    );
                }
            }
            HelmAst::HelmExpr { text } => {
                if let Some((var, _declares, rhs)) = Self::parse_helper_assignment(text) {
                    let set_default_paths =
                        Self::set_default_chart_paths_for_text(text, Some(bindings), current_dot);
                    analysis.chart_defaults.extend(set_default_paths);
                    extend_type_hints(
                        &mut analysis.type_hints,
                        Self::resolved_type_is_paths_for_text(&rhs, Some(bindings), current_dot),
                    );
                    extend_type_hints(
                        &mut analysis.type_hints,
                        Self::resolved_string_transform_paths_for_text(
                            &rhs,
                            Some(bindings),
                            current_dot,
                        ),
                    );

                    let current_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
                    let mut seen_set = HashSet::new();
                    if Self::apply_local_set_mutations(
                        text,
                        local_bindings,
                        current_dot_fragment.as_ref(),
                        context,
                        &mut seen_set,
                    ) {
                        return;
                    }

                    let fallback_paths = Self::resolved_default_fallback_paths_for_text(
                        &rhs,
                        Some(bindings),
                        current_dot,
                    );
                    let direct_outputs =
                        Self::direct_bound_paths_from_text_in_context(&rhs, bindings, current_dot);
                    let local_fallback_paths =
                        Self::local_default_paths_from_text(&rhs, local_default_paths);
                    let local_outputs = Self::local_bound_paths_from_text(&rhs, local_bindings);
                    let local_meta_by_path =
                        Self::local_output_meta_from_text(&rhs, local_bindings, local_output_meta);
                    analysis
                        .dependency_paths
                        .extend(direct_outputs.iter().cloned());
                    analysis
                        .dependency_paths
                        .extend(local_outputs.iter().cloned());
                    let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                        &rhs,
                        Some(bindings),
                        current_dot,
                        local_bindings,
                        context,
                        seen,
                    );
                    analysis
                        .chart_defaults
                        .extend(nested.chart_defaults.clone());
                    extend_type_hints(&mut analysis.type_hints, nested.type_hints.clone());
                    analysis
                        .dependency_paths
                        .extend(bound_helper_dependency_paths(&nested));
                    analysis.add_dependency_meta_map(Self::helper_dependency_meta_from_analysis(
                        &nested,
                    ));

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
                    for (output, meta) in Self::helper_output_meta_from_analysis(&nested) {
                        let meta = Self::helper_output_meta_with_guards(meta, active_output_guards);
                        let entry = rhs_output_meta.entry(output).or_default();
                        entry.guards.extend(meta.guards);
                        entry.defaulted |= meta.defaulted;
                    }

                    let mut seen_rhs = HashSet::new();
                    if let Some(binding) = Self::fragment_binding_from_text(
                        &rhs,
                        local_bindings,
                        current_dot_fragment.as_ref(),
                        context,
                        &mut seen_rhs,
                    ) {
                        local_bindings.insert(var.clone(), binding);
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
                        local_default_paths.remove(&var);
                    } else {
                        local_default_paths.insert(var.clone(), defaulted_paths);
                    }
                    if rhs_output_meta.is_empty() {
                        local_output_meta.remove(&var);
                    } else {
                        local_output_meta.insert(var, rhs_output_meta);
                    }
                    return;
                }

                let current_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
                let mut seen_set = HashSet::new();
                if Self::apply_local_set_mutations(
                    text,
                    local_bindings,
                    current_dot_fragment.as_ref(),
                    context,
                    &mut seen_set,
                ) {
                    let set_default_paths =
                        Self::set_default_chart_paths_for_text(text, Some(bindings), current_dot);
                    analysis.chart_defaults.extend(set_default_paths);
                    return;
                }

                let direct_outputs =
                    Self::direct_bound_paths_from_text_in_context(text, bindings, current_dot);
                let fallback_paths = Self::resolved_default_fallback_paths_for_text(
                    text,
                    Some(bindings),
                    current_dot,
                );
                extend_type_hints(
                    &mut analysis.type_hints,
                    Self::resolved_type_is_paths_for_text(text, Some(bindings), current_dot),
                );
                extend_type_hints(
                    &mut analysis.type_hints,
                    Self::resolved_string_transform_paths_for_text(
                        text,
                        Some(bindings),
                        current_dot,
                    ),
                );
                let local_outputs = Self::local_rendered_paths_from_text(text, local_bindings);
                let local_fallback_paths =
                    Self::local_default_paths_from_text(text, local_default_paths);
                let local_meta_by_path =
                    Self::local_output_meta_from_text(text, local_bindings, local_output_meta);
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
                        analysis.add_output_meta(output, meta);
                    }
                    for output in local_outputs {
                        let mut meta = local_meta_by_path.get(&output).cloned().unwrap_or_default();
                        meta.guards.extend(active_output_guards.iter().cloned());
                        meta.defaulted |= local_fallback_paths.contains(&output);
                        analysis.add_output_meta(output, meta);
                    }
                }
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    text,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    context,
                    seen,
                );
                let mut nested = nested;
                if expression_kind == ValueKind::Fragment {
                    for (output, mut meta) in nested.output {
                        meta.guards.extend(active_output_guards.iter().cloned());
                        analysis.add_output_meta(output, meta);
                    }
                    for output in nested.fragment_output {
                        Self::push_helper_fragment_output(
                            &mut analysis.fragment_output_uses,
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
                        analysis.fragment_output_uses.push(output);
                    }
                    analysis.dependency_paths.extend(nested.dependency_paths);
                    analysis.add_dependency_meta_map(nested.dependency_meta);
                    analysis.guard_paths.extend(nested.guard_paths);
                    extend_type_hints(&mut analysis.type_hints, nested.type_hints);
                    analysis.suppress_roots.extend(nested.suppress_roots);
                    analysis.chart_defaults.extend(nested.chart_defaults);
                } else {
                    convert_fragment_outputs_to_dependency_outputs(&mut nested);
                    for meta in nested.output.values_mut() {
                        meta.guards.extend(active_output_guards.iter().cloned());
                    }
                    analysis.extend(nested);
                }
                let set_default_paths =
                    Self::set_default_chart_paths_for_text(text, Some(bindings), current_dot);
                analysis.chart_defaults.extend(set_default_paths);
            }
            HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let mut branch_guard_paths =
                    Self::direct_bound_paths_from_text_in_context(cond, bindings, current_dot);
                branch_guard_paths.extend(Self::local_bound_paths_from_text(cond, local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    cond,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    context,
                    seen,
                );
                branch_guard_paths.extend(bound_helper_condition_paths(&nested));
                analysis
                    .guard_paths
                    .extend(branch_guard_paths.iter().cloned());
                let mut then_output_guards = active_output_guards.clone();
                then_output_guards.extend(branch_guard_paths);
                let mut then_bindings = local_bindings.clone();
                let mut then_default_paths = local_default_paths.clone();
                let mut then_output_meta = local_output_meta.clone();
                for item in then_branch {
                    Self::collect_bound_helper_values_from_ast(
                        item,
                        bindings,
                        current_dot,
                        &then_output_guards,
                        &mut then_bindings,
                        &mut then_default_paths,
                        &mut then_output_meta,
                        context,
                        seen,
                        analysis,
                    );
                }
                let mut else_bindings = local_bindings.clone();
                let mut else_default_paths = local_default_paths.clone();
                let mut else_output_meta = local_output_meta.clone();
                for item in else_branch {
                    Self::collect_bound_helper_values_from_ast(
                        item,
                        bindings,
                        current_dot,
                        active_output_guards,
                        &mut else_bindings,
                        &mut else_default_paths,
                        &mut else_output_meta,
                        context,
                        seen,
                        analysis,
                    );
                }
                *local_bindings = Self::merge_fragment_locals(then_bindings, else_bindings);
                *local_default_paths =
                    merge_local_default_paths(then_default_paths, else_default_paths);
                *local_output_meta =
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
                    Self::direct_bound_paths_from_text_in_context(header, bindings, current_dot);
                branch_guard_paths
                    .extend(Self::local_bound_paths_from_text(header, local_bindings));
                let nested = Self::analyze_bound_helper_calls_with_fragment_locals(
                    header,
                    Some(bindings),
                    current_dot,
                    local_bindings,
                    context,
                    seen,
                );
                branch_guard_paths.extend(bound_helper_condition_paths(&nested));
                analysis
                    .guard_paths
                    .extend(branch_guard_paths.iter().cloned());

                let mut range_fragment_binding = None;
                let mut range_binding = None;
                if !is_with {
                    let current_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
                    let mut seen_range = HashSet::new();
                    range_fragment_binding = Self::range_iterable_binding(
                        header,
                        local_bindings,
                        current_dot_fragment.as_ref(),
                        context,
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
                let mut body_bindings = local_bindings.clone();
                let mut body_default_paths = local_default_paths.clone();
                let mut body_output_meta = local_output_meta.clone();
                if !is_with {
                    let header_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
                    let mut seen_range = HashSet::new();
                    if let Some((var, binding)) = Self::range_variable_item_binding(
                        header,
                        &body_bindings,
                        header_dot_fragment.as_ref(),
                        context,
                        &mut seen_range,
                    ) {
                        body_bindings.insert(var, binding);
                    }
                }
                if !is_with && let Some(FragmentBinding::List(items)) = &range_fragment_binding {
                    let range_var = Self::range_variable_name(header);
                    for item_binding in items {
                        if let Some(range_var) = &range_var {
                            body_bindings.insert(range_var.clone(), item_binding.clone());
                        }
                        let item_dot = item_binding.to_helper_binding();
                        let mut item_seen = seen.clone();
                        for item in body {
                            Self::collect_bound_helper_values_from_ast(
                                item,
                                bindings,
                                item_dot.as_ref(),
                                &body_output_guards,
                                &mut body_bindings,
                                &mut body_default_paths,
                                &mut body_output_meta,
                                context,
                                &mut item_seen,
                                analysis,
                            );
                        }
                    }
                } else {
                    for item in body {
                        Self::collect_bound_helper_values_from_ast(
                            item,
                            bindings,
                            body_dot.as_ref(),
                            &body_output_guards,
                            &mut body_bindings,
                            &mut body_default_paths,
                            &mut body_output_meta,
                            context,
                            seen,
                            analysis,
                        );
                    }
                }
                if !is_with
                    && range_binding
                        .as_ref()
                        .is_some_and(HelperBinding::definitely_nonempty_iterable)
                {
                    *local_bindings = body_bindings;
                    *local_default_paths = body_default_paths;
                    *local_output_meta = body_output_meta;
                } else {
                    let mut else_bindings = local_bindings.clone();
                    let mut else_default_paths = local_default_paths.clone();
                    let mut else_output_meta = local_output_meta.clone();
                    for item in else_branch {
                        // `with ... else ...` else-branch executes with
                        // the outer `.`, not the with-shifted one.
                        Self::collect_bound_helper_values_from_ast(
                            item,
                            bindings,
                            current_dot,
                            active_output_guards,
                            &mut else_bindings,
                            &mut else_default_paths,
                            &mut else_output_meta,
                            context,
                            seen,
                            analysis,
                        );
                    }
                    *local_bindings = Self::merge_fragment_locals(body_bindings, else_bindings);
                    *local_default_paths =
                        merge_local_default_paths(body_default_paths, else_default_paths);
                    *local_output_meta =
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
        // Bare `.Values` — `single_resolved_values_path` returns None
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
        Self::binding_from_expr(expr, Some(bindings), current_dot)
    }

    fn resolved_default_fallback_paths_for_text(
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();

        for expr in Self::parse_expr_text(text) {
            out.extend(Self::resolved_default_fallback_paths_for_expr(
                &expr,
                bindings,
                current_dot,
            ));
        }

        out
    }

    fn resolved_default_fallback_paths_for_expr(
        expr: &TemplateExpr,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        expr.walk(|node| match node {
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                out.extend(Self::resolve_expr_to_values_paths_in_context(
                    &args[1],
                    bindings,
                    current_dot,
                ));
            }
            TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
                for window in stages.windows(2) {
                    let TemplateExpr::Call { function, .. } = &window[1] else {
                        continue;
                    };
                    if function != "default" {
                        continue;
                    }
                    out.extend(Self::resolve_expr_to_values_paths_in_context(
                        &window[0],
                        bindings,
                        current_dot,
                    ));
                }
            }
            _ => {}
        });
        out
    }

    fn resolved_type_is_paths_for_text(
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeMap<String, BTreeSet<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for expr in Self::parse_expr_text(text) {
            expr.walk(|node| {
                let TemplateExpr::Call { function, args } = node else {
                    return;
                };
                if function != "typeIs" || args.len() < 2 {
                    return;
                }
                let Some(schema_type) = Self::type_is_schema_type(args.first()) else {
                    return;
                };
                let Some(binding) = Self::binding_from_expr(&args[1], bindings, current_dot) else {
                    return;
                };
                for path in binding.paths() {
                    if !path.trim().is_empty() {
                        out.entry(path).or_default().insert(schema_type.clone());
                    }
                }
            });
        }
        out
    }

    fn resolved_string_transform_paths_for_text(
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeMap<String, BTreeSet<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for expr in Self::parse_expr_text(text) {
            Self::resolved_string_transform_paths_for_expr(&expr, bindings, current_dot, &mut out);
        }
        out
    }

    fn resolved_string_transform_paths_for_expr(
        expr: &TemplateExpr,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        out: &mut BTreeMap<String, BTreeSet<String>>,
    ) {
        match expr {
            TemplateExpr::Parenthesized(inner) => {
                Self::resolved_string_transform_paths_for_expr(inner, bindings, current_dot, out);
            }
            TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
                let mut current = stages.first();
                for stage in &stages[1..] {
                    let TemplateExpr::Call { function, .. } = stage else {
                        continue;
                    };
                    if Self::is_string_transform_function(function)
                        && let Some(current) = current
                    {
                        for path in Self::resolve_expr_to_values_paths_in_context(
                            current,
                            bindings,
                            current_dot,
                        ) {
                            insert_type_hint(out, path, "string");
                        }
                    }
                    current = Some(stage);
                }
            }
            TemplateExpr::Call { function, args }
                if Self::is_string_transform_function(function) =>
            {
                for arg in args {
                    for path in
                        Self::resolve_expr_to_values_paths_in_context(arg, bindings, current_dot)
                    {
                        insert_type_hint(out, path, "string");
                    }
                    Self::resolved_string_transform_paths_for_expr(arg, bindings, current_dot, out);
                }
            }
            TemplateExpr::Call { args, .. } => {
                for arg in args {
                    Self::resolved_string_transform_paths_for_expr(arg, bindings, current_dot, out);
                }
            }
            TemplateExpr::Selector { operand, .. } => {
                Self::resolved_string_transform_paths_for_expr(operand, bindings, current_dot, out);
            }
            _ => {}
        }
    }

    fn is_string_transform_function(function: &str) -> bool {
        matches!(
            function,
            "quote"
                | "squote"
                | "toString"
                | "trunc"
                | "trim"
                | "trimAll"
                | "trimPrefix"
                | "trimSuffix"
                | "replace"
        )
    }

    fn type_is_schema_type(expr: Option<&TemplateExpr>) -> Option<String> {
        let TemplateExpr::Literal(Literal::String(type_name) | Literal::RawString(type_name)) =
            expr?.deparen()
        else {
            return None;
        };
        let schema_type = match type_name.as_str() {
            "bool" | "boolean" => "boolean",
            "float64" | "number" => "number",
            "int" | "int64" | "integer" => "integer",
            "list" | "slice" | "array" => "array",
            "map" | "dict" | "object" => "object",
            "string" => "string",
            _ => return None,
        };
        Some(schema_type.to_string())
    }

    /// Extract Values-rooted paths that this helper-body text declares
    /// as chart-level defaults via the canonical pattern
    ///
    /// ```text
    /// {{- $_ := set OPERAND "KEY" (OPERAND.KEY | default V) }}
    /// ```
    ///
    /// This is the chart writer asserting "this path is defaulted before
    /// any subsequent read." Matched here (and *only* here, not by the
    /// broader `resolved_default_fallback_paths_for_text` which fires
    /// on every `| default` regardless of context — including condition
    /// fallbacks like `if X | default false` that do not mutate values).
    ///
    /// Requirements for a match:
    ///   - The action is a `set` call with exactly three arguments.
    ///   - The first argument resolves to a Values-rooted path through
    ///     the active bindings and `current_dot` (so `with .Values` shifts
    ///     correctly).
    ///   - The second argument is a string literal — the dict key being
    ///     defaulted.
    ///   - The third argument's expression tree contains a `default` call
    ///     anywhere within it. The chart writer's `| default V` (or
    ///     `default V .` form) is what signals the path is optional; a
    ///     bare `set X "K" V` without a default falls outside this rule
    ///     because it unconditionally overwrites and tells us nothing
    ///     about the values.yaml schema's null-tolerance.
    ///
    /// The emitted path is `<resolved-operand>.<key>` (or just `<key>`
    /// when the operand resolves to the Values root, i.e. inside a bare
    /// `with .Values` block).
    fn set_default_chart_paths_for_text(
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
    ) -> BTreeSet<String> {
        fn literal_string(expr: &TemplateExpr) -> Option<&str> {
            match expr {
                TemplateExpr::Literal(Literal::String(s) | Literal::RawString(s)) => Some(s),
                TemplateExpr::Parenthesized(inner) => literal_string(inner),
                _ => None,
            }
        }

        let mut out = BTreeSet::new();
        for expr in Self::parse_expr_text(text) {
            expr.walk(|node| {
                let TemplateExpr::Call { function, args } = node else {
                    return;
                };
                if function != "set" || args.len() != 3 {
                    return;
                }
                let Some(operand_path) =
                    Self::resolve_expr_to_values_path_in_context(&args[0], bindings, current_dot)
                else {
                    return;
                };
                let Some(key) = literal_string(&args[1]) else {
                    return;
                };
                let target_path = if operand_path.is_empty() {
                    key.to_string()
                } else {
                    format!("{operand_path}.{key}")
                };
                let defaulted_paths =
                    Self::resolved_default_fallback_paths_for_expr(&args[2], bindings, current_dot);
                if !defaulted_paths.contains(&target_path) {
                    return;
                }
                out.insert(target_path);
            });
        }
        out
    }

    fn collect_if_with_guards(&mut self, cond_text: &str) {
        let cond_guards = self.condition_guards_in_context(cond_text);

        for v in self.extract_bound_values(cond_text) {
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
        let cond_guards = self.condition_guards_in_context(cond_text);

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

        for v in self.extract_bound_values(cond_text) {
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
            [expr] => Self::fragment_binding_from_outer_expr(
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
        self.resolved_values_paths_in_context(header_text)
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

        self.single_direct_iterable_range_path(txt)
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
            if let Some((var, binding)) = Self::parse_get_binding(txt) {
                self.get_bindings.insert(var, binding);
            }

            if let Some((var, _declares, rhs)) = Self::parse_helper_assignment(txt) {
                let mut locals = self.template_bindings.clone();
                for (key, value) in &self.root_bindings {
                    locals.insert(key.clone(), value.to_fragment_binding());
                }
                let current_dot = self
                    .current_dot_binding()
                    .map(|binding| binding.to_fragment_binding());
                let context = self.fragment_eval_context();
                let mut seen = HashSet::new();
                if let Some(binding) = Self::fragment_binding_from_text(
                    &rhs,
                    &locals,
                    current_dot.as_ref(),
                    context,
                    &mut seen,
                ) {
                    self.template_bindings.insert(var.clone(), binding);
                }
                let default_paths = self.resolved_default_fallback_paths_in_context(&rhs);
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
            if let Some((var, literals)) = Self::parse_literal_list_range(&txt) {
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
