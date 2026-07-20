use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use helm_schema_ast::{DefineIndex, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::Effects;
use crate::expr_eval::bindings_for_helper_arg_with;
use crate::fragment_eval::BodyEvalFacts;
use crate::fragment_eval::summary::{FragmentSummary, eval_bound_helper_fragment};
use crate::fragment_expr_eval::{
    FragmentEvalContext, context_value_from_outer_expr,
    helper_result_from_expr_with_fragment_locals,
};
use helm_schema_ast::parse_go_template;

pub(crate) struct ParsedHelperBody<'a> {
    pub(crate) source: &'a str,
    pub(crate) source_path: &'a str,
    pub(crate) body_offset: usize,
    pub(crate) tree: tree_sitter::Tree,
}

pub(crate) struct IrAnalysisDb {
    define_bodies: HashMap<String, CachedDefineBody>,
    implicit_template_names: BTreeMap<String, String>,
    /// Raw template file sources by index path (static `files/*` templates
    /// requested through `.Files.Get` resolve here).
    file_sources: HashMap<String, String>,
    chart_default_strings: BTreeMap<String, String>,
    define_trees: RefCell<HashMap<String, tree_sitter::Tree>>,
    /// Source-only evaluation facts per helper body (control headers,
    /// resource spans), shared across memoized-summary misses.
    body_eval_facts: RefCell<HashMap<String, Rc<BodyEvalFacts>>>,
    bound_helper_calls: RefCell<BTreeMap<BoundHelperCallCacheKey, Rc<FragmentSummary>>>,
    custom_merge_helpers: RefCell<HashMap<String, bool>>,
    /// The analysis-policy Kubernetes version (normalized core, e.g.
    /// `1.29.0`): the value `.Capabilities.KubeVersion` renders under this
    /// run's provider policy. `None` abstains every capabilities-version
    /// condition instead of guessing a cluster.
    kubernetes_version: Option<String>,
}

pub(crate) struct BoundHelperCallSummary {
    pub(crate) summary: Rc<FragmentSummary>,
    pub(crate) argument_effects: Effects,
}

impl IrAnalysisDb {
    #[tracing::instrument(skip_all)]
    pub(crate) fn new(defines: &DefineIndex) -> Self {
        Self::with_policy(defines, BTreeMap::new(), None)
    }

    pub(crate) fn with_chart_default_strings(
        defines: &DefineIndex,
        chart_default_strings: BTreeMap<String, String>,
    ) -> Self {
        Self::with_policy(defines, chart_default_strings, None)
    }

    pub(crate) fn with_policy(
        defines: &DefineIndex,
        chart_default_strings: BTreeMap<String, String>,
        kubernetes_version: Option<String>,
    ) -> Self {
        let mut define_bodies = HashMap::new();
        let mut implicit_template_names = BTreeMap::new();
        let mut file_sources = HashMap::new();
        for (path, src) in defines.file_sources() {
            file_sources.insert(path.to_string(), src.to_string());
            if let Some(template_relative_path) = template_relative_path(path) {
                let name = format!("@file:{path}");
                implicit_template_names.insert(template_relative_path, name.clone());
                define_bodies.insert(
                    name,
                    CachedDefineBody {
                        source: src.to_string(),
                        source_path: path.to_string(),
                        body_offset: 0,
                    },
                );
            }
            for block in extract_define_blocks(src) {
                define_bodies.insert(
                    block.name,
                    CachedDefineBody {
                        source: block.body,
                        source_path: path.to_string(),
                        body_offset: block.body_offset,
                    },
                );
            }
        }
        Self {
            define_bodies,
            implicit_template_names,
            file_sources,
            chart_default_strings,
            define_trees: RefCell::new(HashMap::new()),
            body_eval_facts: RefCell::new(HashMap::new()),
            bound_helper_calls: RefCell::new(BTreeMap::new()),
            custom_merge_helpers: RefCell::new(HashMap::new()),
            kubernetes_version,
        }
    }

    pub(crate) fn kubernetes_version(&self) -> Option<&str> {
        self.kubernetes_version.as_deref()
    }

    pub(crate) fn has_helper(&self, name: &str) -> bool {
        self.define_bodies.contains_key(name)
    }

    pub(crate) fn implicit_template_name(&self, suffix: &str) -> Option<&str> {
        let suffix = suffix.trim_start_matches('/');
        let mut matches = self
            .implicit_template_names
            .iter()
            .filter(|(path, _)| path.as_str() == suffix)
            .map(|(_, name)| name.as_str());
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }

    pub(crate) fn file_source(&self, path: &str) -> Option<&str> {
        self.file_sources.get(path).map(String::as_str)
    }

    pub(crate) fn chart_default_string(&self, path: &str) -> Option<&str> {
        self.chart_default_strings.get(path).map(String::as_str)
    }

    /// Indexed chart file paths (templates plus `.Files.Get` sources),
    /// sorted for deterministic enumeration.
    pub(crate) fn file_source_paths(&self) -> Vec<&str> {
        let mut paths: Vec<&str> = self.file_sources.keys().map(String::as_str).collect();
        paths.sort_unstable();
        paths
    }

    #[tracing::instrument(skip_all)]
    fn define_tree(&self, name: &str) -> Option<tree_sitter::Tree> {
        if let Some(tree) = self.define_trees.borrow().get(name) {
            return Some(tree.clone());
        }

        let src = self.define_bodies.get(name)?.source.as_str();
        let tree = parse_go_template(src)?;
        self.define_trees
            .borrow_mut()
            .insert(name.to_string(), tree.clone());
        Some(tree)
    }

    /// The source-only evaluation facts of one helper body, computed once.
    pub(crate) fn helper_body_eval_facts(
        &self,
        name: &str,
        build: impl FnOnce() -> BodyEvalFacts,
    ) -> Rc<BodyEvalFacts> {
        if let Some(facts) = self.body_eval_facts.borrow().get(name) {
            return Rc::clone(facts);
        }
        let facts = Rc::new(build());
        self.body_eval_facts
            .borrow_mut()
            .insert(name.to_string(), Rc::clone(&facts));
        facts
    }

    /// Sentinel keys of a chart-authored values-program wrapper engine
    /// rooted at `entry`: within the define family (the entry plus its
    /// transitive includes, bounded), a sentinel is a literal key that the
    /// family both TESTS with `hasKey` and READS with `get` into a value
    /// that feeds `tpl` — the structural shape of an engine that replaces
    /// singleton `{KEY: PROGRAM}` maps with rendered program results
    /// (nats' `tplYaml`/`tplYamlItr`). Empty when the family is not such
    /// an engine. The value marks SPREAD sentinels: a sentinel whose
    /// `hasKey` test guards a `fail` terminal is the engine's
    /// spread-into-parent form (nats' `$tplYamlSpread` root guard) rather
    /// than a plain node replacement.
    pub(crate) fn program_wrapper_sentinels(&self, entry: &str) -> BTreeMap<String, bool> {
        const MAX_FAMILY: usize = 16;
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut queue = vec![entry.to_string()];
        let mut has_key_literals: BTreeSet<String> = BTreeSet::new();
        let mut tpl_fed_literals: BTreeSet<String> = BTreeSet::new();
        while let Some(name) = queue.pop() {
            if visited.len() >= MAX_FAMILY || !visited.insert(name.clone()) {
                continue;
            }
            let Some(body) = self.define_bodies.get(&name) else {
                continue;
            };
            // Per body: `get`-bound variables feeding a later `tpl` call.
            let mut get_bound: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            let mut tpl_variables: BTreeSet<String> = BTreeSet::new();
            for expr in helm_schema_ast::parse_action_expressions(&body.source) {
                expr.walk(|inner| match inner {
                    TemplateExpr::Call { function, args } => match function.as_str() {
                        "hasKey" => {
                            if let Some(key) = literal_string_argument(args.get(1)) {
                                has_key_literals.insert(key);
                            }
                        }
                        "tpl" => match args.first().map(TemplateExpr::deparen) {
                            Some(TemplateExpr::Variable(variable)) => {
                                tpl_variables.insert(variable.clone());
                            }
                            Some(TemplateExpr::Call {
                                function: inner_function,
                                args: inner_args,
                            }) if inner_function == "get" => {
                                if let Some(key) = literal_string_argument(inner_args.get(1)) {
                                    tpl_fed_literals.insert(key);
                                }
                            }
                            _ => {}
                        },
                        "include" | "template" => {
                            if let Some(name) = literal_string_argument(args.first()) {
                                queue.push(name);
                            }
                        }
                        _ => {}
                    },
                    TemplateExpr::VariableDefinition { name, value }
                    | TemplateExpr::Assignment { name, value } => {
                        if let TemplateExpr::Call { function, args } = value.deparen()
                            && function == "get"
                            && let Some(key) = literal_string_argument(args.get(1))
                        {
                            // Definitions spell the `$`, uses do not.
                            get_bound
                                .entry(name.trim_start_matches('$').to_string())
                                .or_default()
                                .insert(key);
                        }
                    }
                    _ => {}
                });
            }
            for variable in &tpl_variables {
                if let Some(keys) = get_bound.get(variable) {
                    tpl_fed_literals.extend(keys.iter().cloned());
                }
            }
        }
        has_key_literals
            .intersection(&tpl_fed_literals)
            .map(|key| {
                let spread = visited
                    .iter()
                    .filter_map(|name| self.parsed_helper_body(name))
                    .any(|body| if_has_key_guards_fail(body.source, body.tree.root_node(), key));
                (key.clone(), spread)
            })
            .collect()
    }

    /// Classifies `name` as a bounded chart-authored merge helper, memoized.
    ///
    /// The recognized shape is airflow's `workersMergeValues` engine: the
    /// define destructures `(list INPUT OVERWRITE …)` through `index`,
    /// builds an empty `dict` accumulator, declares a literal
    /// full-overwrite key list probed with `has`, ranges only the two
    /// maps with destructured `key, val` variables, writes accumulator
    /// members only from the two maps' members (`$val`, `get MAP $key`,
    /// `or` of those, or the self-recursive merge of those members), and
    /// renders exactly `toYaml ACC`. Under those rules the output is a
    /// merge of OVERWRITE over INPUT, so the call site can substitute the
    /// layered value without evaluating the recursion.
    pub(crate) fn custom_merge_helper(&self, name: &str) -> Option<()> {
        if let Some(cached) = self.custom_merge_helpers.borrow().get(name) {
            return cached.then_some(());
        }
        let recognized = self.classify_custom_merge_helper(name).is_some();
        self.custom_merge_helpers
            .borrow_mut()
            .insert(name.to_string(), recognized);
        recognized.then_some(())
    }

    fn classify_custom_merge_helper(&self, name: &str) -> Option<()> {
        let body = self.parsed_helper_body(name)?;

        let mut ranges = Vec::new();
        if !collect_destructured_ranges(body.tree.root_node(), body.source, &mut ranges) {
            return None;
        }
        if ranges.is_empty() {
            return None;
        }

        let exprs = helm_schema_ast::parse_action_expressions(body.source);
        let mut indexed_params: BTreeMap<i64, String> = BTreeMap::new();
        let mut accumulator: Option<String> = None;
        let mut literal_lists: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut nested_vars: BTreeSet<String> = BTreeSet::new();
        for expr in &exprs {
            let TemplateExpr::VariableDefinition {
                name: var_name,
                value,
            } = expr
            else {
                continue;
            };
            let var_name = var_name.trim_start_matches('$').to_string();
            match value.deparen() {
                TemplateExpr::Call { function, args } if function == "index" => {
                    if let [
                        subject,
                        TemplateExpr::Literal(helm_schema_ast::Literal::Int(n)),
                    ] = args.as_slice()
                        && matches!(subject.deparen(), TemplateExpr::Field(path) if path.is_empty())
                        && indexed_params.insert(*n, var_name).is_some()
                    {
                        return None;
                    }
                }
                TemplateExpr::Call { function, args } if function == "dict" && args.is_empty() => {
                    if accumulator.replace(var_name).is_some() {
                        return None;
                    }
                }
                TemplateExpr::Call { function, args } if function == "list" => {
                    let keys = args
                        .iter()
                        .map(|arg| match arg.deparen() {
                            TemplateExpr::Literal(helm_schema_ast::Literal::String(key)) => {
                                Some(key.clone())
                            }
                            _ => None,
                        })
                        .collect::<Option<BTreeSet<String>>>();
                    if let Some(keys) = keys {
                        literal_lists.insert(var_name, keys);
                    }
                }
                _ => {
                    if is_self_merge_recursion(value, name) {
                        nested_vars.insert(var_name);
                    }
                }
            }
        }

        let input_var = indexed_params.get(&0)?.clone();
        let overwrite_var = indexed_params.get(&1)?.clone();
        let out_var = accumulator?;
        if input_var == overwrite_var || out_var == input_var || out_var == overwrite_var {
            return None;
        }
        let mut key_vars: BTreeSet<String> = BTreeSet::new();
        let mut val_vars: BTreeSet<String> = BTreeSet::new();
        for range in &ranges {
            if range.source_var != input_var && range.source_var != overwrite_var {
                return None;
            }
            key_vars.insert(range.key_var.clone());
            val_vars.insert(range.value_var.clone());
        }

        let is_map_member = |expr: &TemplateExpr| -> bool {
            match expr.deparen() {
                TemplateExpr::Variable(variable) => {
                    val_vars.contains(variable.trim_start_matches('$'))
                }
                TemplateExpr::Call { function, args } if function == "get" => {
                    matches!(
                        args.first().map(TemplateExpr::deparen),
                        Some(TemplateExpr::Variable(base))
                            if base.trim_start_matches('$') == input_var
                                || base.trim_start_matches('$') == overwrite_var
                    ) && matches!(
                        args.get(1).map(TemplateExpr::deparen),
                        Some(TemplateExpr::Variable(key))
                            if key_vars.contains(key.trim_start_matches('$'))
                    )
                }
                _ => false,
            }
        };
        fn allowed_set_value(
            expr: &TemplateExpr,
            nested_vars: &BTreeSet<String>,
            is_map_member: &impl Fn(&TemplateExpr) -> bool,
        ) -> bool {
            if is_map_member(expr) {
                return true;
            }
            match expr.deparen() {
                TemplateExpr::Variable(variable) => {
                    nested_vars.contains(variable.trim_start_matches('$'))
                }
                TemplateExpr::Call { function, args } if function == "or" => args
                    .iter()
                    .all(|arg| allowed_set_value(arg, nested_vars, is_map_member)),
                _ => false,
            }
        }

        let mut full_overwrite_sources: BTreeSet<String> = BTreeSet::new();
        let mut disciplined = true;
        for expr in &exprs {
            expr.walk(|inner| match inner {
                TemplateExpr::Call { function, args } => match function.as_str() {
                    "has" => {
                        if let (
                            Some(TemplateExpr::Variable(subject)),
                            Some(TemplateExpr::Variable(list_var)),
                        ) = (
                            args.first().map(TemplateExpr::deparen),
                            args.get(1).map(TemplateExpr::deparen),
                        ) && key_vars.contains(subject.trim_start_matches('$'))
                            && literal_lists.contains_key(list_var.trim_start_matches('$'))
                        {
                            full_overwrite_sources.insert(list_var.trim_start_matches('$').into());
                        }
                    }
                    "set" | "unset" => {
                        let targets_out = matches!(
                            args.first().map(TemplateExpr::deparen),
                            Some(TemplateExpr::Variable(target))
                                if target.trim_start_matches('$') == out_var
                        );
                        let keyed_by_range = matches!(
                            args.get(1).map(TemplateExpr::deparen),
                            Some(TemplateExpr::Variable(key))
                                if key_vars.contains(key.trim_start_matches('$'))
                        );
                        if function == "unset"
                            || args.len() != 3
                            || !targets_out
                            || !keyed_by_range
                            || !allowed_set_value(&args[2], &nested_vars, &is_map_member)
                        {
                            disciplined = false;
                        }
                    }
                    "include" | "template" => {
                        match args.first().map(TemplateExpr::deparen) {
                            Some(TemplateExpr::Literal(helm_schema_ast::Literal::String(
                                callee,
                            ))) if callee == name => {}
                            _ => disciplined = false,
                        }
                        let recursion_operands_are_members = matches!(
                            args.get(1).map(TemplateExpr::deparen),
                            Some(TemplateExpr::Call {
                                function: list_fn,
                                args: list_args,
                            }) if list_fn == "list"
                                && list_args.len() >= 2
                                && is_map_member(&list_args[0])
                                && is_map_member(&list_args[1])
                        );
                        if !recursion_operands_are_members {
                            disciplined = false;
                        }
                    }
                    _ => {}
                },
                TemplateExpr::Assignment { name: target, .. } => {
                    let target = target.trim_start_matches('$');
                    if target == input_var || target == overwrite_var || target == out_var {
                        disciplined = false;
                    }
                }
                _ => {}
            });
        }
        if !disciplined {
            return None;
        }

        let renders_accumulator_yaml = matches!(
            exprs.last().map(TemplateExpr::deparen),
            Some(TemplateExpr::Call { function, args })
                if function == "toYaml"
                    && matches!(
                        args.first().map(TemplateExpr::deparen),
                        Some(TemplateExpr::Variable(subject))
                            if subject.trim_start_matches('$') == out_var
                    )
        );
        if !renders_accumulator_yaml {
            return None;
        }

        let mut sources = full_overwrite_sources.into_iter();
        let (Some(source), None) = (sources.next(), sources.next()) else {
            return None;
        };
        literal_lists.remove(&source).map(|_keys| ())
    }

    pub(crate) fn parsed_helper_body(&self, name: &str) -> Option<ParsedHelperBody<'_>> {
        let body = self.define_bodies.get(name)?;
        Some(ParsedHelperBody {
            source: body.source.as_str(),
            source_path: body.source_path.as_str(),
            body_offset: body.body_offset,
            tree: self.define_tree(name)?,
        })
    }

    /// Evaluate one bound helper call in the fragment domain, memoized per
    /// (helper, bindings, dot, call chain).
    #[tracing::instrument(skip_all, fields(helper = name))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn summarize_bound_helper_call(
        &self,
        name: &str,
        arg: Option<&TemplateExpr>,
        outer_bindings: Option<&HashMap<String, AbstractValue>>,
        outer_root_facts: OuterRootFacts<'_>,
        current_dot: Option<&AbstractValue>,
        fragment_locals: &HashMap<String, AbstractValue>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> BoundHelperCallSummary {
        if !seen.insert(name.to_string()) {
            return BoundHelperCallSummary {
                summary: Rc::new(FragmentSummary::default()),
                argument_effects: Effects::default(),
            };
        }

        let resolved = resolve_bound_helper_call(ResolveBoundHelperCallParams {
            helper_name: name,
            arg,
            outer_bindings,
            outer_root_facts,
            current_dot,
            fragment_locals,
            context,
            seen,
        });
        let seen_key = seen.iter().cloned().collect();
        let key = BoundHelperCallCacheKey::from_resolution(name, &resolved.resolution, seen_key);

        if let Some(cached) = self.bound_helper_calls.borrow().get(&key) {
            seen.remove(name);
            return BoundHelperCallSummary {
                summary: Rc::clone(cached),
                argument_effects: resolved.argument_effects,
            };
        }

        let summary = Rc::new(eval_bound_helper_fragment(
            name,
            &resolved.resolution,
            self,
            seen,
        ));
        self.bound_helper_calls
            .borrow_mut()
            .insert(key, Rc::clone(&summary));
        seen.remove(name);
        BoundHelperCallSummary {
            summary,
            argument_effects: resolved.argument_effects,
        }
    }
}

fn template_relative_path(path: &str) -> Option<String> {
    let marker = "templates/";
    let index = path.rfind(marker)?;
    Some(path[(index + marker.len())..].to_string())
}

/// One dot (`.`) binding as the two evaluation flavors see it: value
/// analysis reads the context-value projection (`helper`), fragment
/// evaluation reads the raw fragment shape (`fragment`). The flavors
/// interpret the same binding differently on purpose: collapsing to a
/// single projection loses information the other flavor needs.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DotFrame {
    pub(crate) helper: Option<AbstractValue>,
    pub(crate) fragment: Option<AbstractValue>,
}

pub(crate) struct BoundHelperCallResolution {
    pub(crate) bindings: HashMap<String, AbstractValue>,
    pub(crate) dot: DotFrame,
    /// The caller's root-field truth predicates and value dispatches,
    /// threaded only when the helper dot IS the caller's root context: a
    /// helper body reading `.mode` then decodes the caller's `set`-key
    /// facts (vault's `ne .mode "dev"` volume-claim gates). A dict-bound
    /// call keeps them empty — its "root" fields are the argument's.
    pub(crate) root_truthy_predicates: HashMap<String, helm_schema_core::Predicate>,
    pub(crate) root_value_dispatches: HashMap<String, crate::eval_effect::RootValueDispatch>,
}

struct ResolvedBoundHelperCall {
    resolution: BoundHelperCallResolution,
    argument_effects: Effects,
}

struct ResolveBoundHelperCallParams<'a, 'context> {
    helper_name: &'a str,
    arg: Option<&'a TemplateExpr>,
    outer_bindings: Option<&'a HashMap<String, AbstractValue>>,
    outer_root_facts: OuterRootFacts<'a>,
    current_dot: Option<&'a AbstractValue>,
    fragment_locals: &'a HashMap<String, AbstractValue>,
    context: FragmentEvalContext<'context>,
    seen: &'a HashSet<String>,
}

/// The caller's root-field condition facts (truth predicates and value
/// dispatches), passed alongside the plain bindings.
#[derive(Clone, Copy, Default)]
pub(crate) struct OuterRootFacts<'a> {
    pub(crate) truthy_predicates: Option<&'a HashMap<String, helm_schema_core::Predicate>>,
    pub(crate) value_dispatches: Option<&'a HashMap<String, crate::eval_effect::RootValueDispatch>>,
}

fn resolve_bound_helper_call(
    params: ResolveBoundHelperCallParams<'_, '_>,
) -> ResolvedBoundHelperCall {
    let mut argument_effects = Effects::default();
    let mut eval_arg_value = |expr: &TemplateExpr, seen: &mut HashSet<String>| {
        let result = helper_result_from_expr_with_fragment_locals(
            expr,
            params.fragment_locals,
            params.outer_bindings,
            params.current_dot,
            params.context,
            seen,
        );
        argument_effects.merge(result.effects.execution_only());
        result.value
    };
    let mut binding_seen = params.seen.clone();
    let arg_resolution = bindings_for_helper_arg_with(params.arg, params.outer_bindings, |expr| {
        eval_arg_value(expr, &mut binding_seen)
    });
    let mut bindings = arg_resolution.bindings;

    // The binding resolution already evaluated the whole arg unless the arg
    // was a dot/root or merge call; only those shapes still need their own
    // helper-dot evaluation here (same evaluation, fresh seen set).
    let mut helper_body_dot = arg_resolution
        .value
        .or_else(|| {
            let mut dot_seen = params.seen.clone();
            params
                .arg
                .and_then(|expr| eval_arg_value(expr, &mut dot_seen))
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

    // Root condition facts apply only when the body's dot IS the caller's
    // root context: only then does a body-level `.field` read resolve
    // against the caller's root `set` state.
    let root_passthrough = matches!(helper_body_dot, Some(AbstractValue::RootContext));
    let (root_truthy_predicates, root_value_dispatches) = if root_passthrough {
        (
            params
                .outer_root_facts
                .truthy_predicates
                .cloned()
                .unwrap_or_default(),
            params
                .outer_root_facts
                .value_dispatches
                .cloned()
                .unwrap_or_default(),
        )
    } else {
        (HashMap::new(), HashMap::new())
    };
    ResolvedBoundHelperCall {
        resolution: BoundHelperCallResolution {
            bindings,
            dot: DotFrame {
                helper: helper_body_dot,
                fragment: helper_fragment_dot,
            },
            root_truthy_predicates,
            root_value_dispatches,
        },
        argument_effects,
    }
}

fn helper_uses_large_config_arg(name: &str) -> bool {
    name.starts_with("opentelemetry-collector.apply")
}

fn abstract_config_binding(binding: AbstractValue) -> AbstractValue {
    // `path_choices` yields `None` only for an empty path set, so a pathless
    // config binding widens straight to `Top`.
    AbstractValue::path_choices(binding.paths()).unwrap_or(AbstractValue::Top)
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

struct CachedDefineBody {
    source: String,
    source_path: String,
    body_offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BoundHelperCallCacheKey {
    name: String,
    bindings: BTreeMap<String, AbstractValue>,
    dot: DotFrame,
    root_truthy_predicates: BTreeMap<String, helm_schema_core::Predicate>,
    root_value_dispatches: BTreeMap<String, crate::eval_effect::RootValueDispatch>,
    seen: BTreeSet<String>,
}

impl BoundHelperCallCacheKey {
    fn from_resolution(
        name: &str,
        resolution: &BoundHelperCallResolution,
        seen: BTreeSet<String>,
    ) -> Self {
        Self {
            name: name.to_string(),
            bindings: resolution
                .bindings
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            dot: resolution.dot.clone(),
            root_truthy_predicates: resolution
                .root_truthy_predicates
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            root_value_dispatches: resolution
                .root_value_dispatches
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            seen,
        }
    }
}

fn literal_string_argument(argument: Option<&TemplateExpr>) -> Option<String> {
    match argument.map(TemplateExpr::deparen) {
        Some(TemplateExpr::Literal(helm_schema_ast::Literal::String(text))) => Some(text.clone()),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DefineBlock {
    name: String,
    body: String,
    body_offset: usize,
}

#[tracing::instrument(skip_all)]
/// The helper names a template source `define`s, for chart-ownership
/// queries over the define index's files.
#[must_use]
pub fn define_names_in_source(src: &str) -> Vec<String> {
    extract_define_blocks(src)
        .into_iter()
        .map(|block| block.name)
        .collect()
}

/// The `(name, body)` pairs a template source `define`s, for include-graph
/// walks that need to follow helper calls through helper bodies.
#[must_use]
pub fn define_bodies_in_source(src: &str) -> Vec<(String, String)> {
    extract_define_blocks(src)
        .into_iter()
        .map(|block| (block.name, block.body))
        .collect()
}

fn extract_define_blocks(src: &str) -> Vec<DefineBlock> {
    let Some(tree) = parse_go_template(src) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    collect_define_blocks(tree.root_node(), src, &mut out);
    out.sort_by_key(|block| block.body_offset);
    out
}

fn collect_define_blocks(node: tree_sitter::Node<'_>, src: &str, out: &mut Vec<DefineBlock>) {
    if node.kind() == "define_action"
        && let Some(block) = define_block_from_node(node, src)
    {
        out.push(block);
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_define_blocks(child, src, out);
    }
}

fn define_block_from_node(node: tree_sitter::Node<'_>, src: &str) -> Option<DefineBlock> {
    let name = define_name(node, src)?;
    let body_children = children_with_field(node, "body");
    let end_action_start = find_end_action_start(node);

    let body_end = end_action_start.unwrap_or_else(|| {
        body_children
            .last()
            .map(tree_sitter::Node::end_byte)
            .unwrap_or_else(|| node.end_byte())
    });
    let body_start = body_children
        .first()
        .map(tree_sitter::Node::start_byte)
        .unwrap_or(body_end);
    let body_range = body_start..body_end;
    let body = src.get(body_range.clone())?.to_string();

    Some(DefineBlock {
        name,
        body,
        body_offset: body_range.start,
    })
}

fn define_name(node: tree_sitter::Node<'_>, src: &str) -> Option<String> {
    let raw = node
        .child_by_field_name("name")?
        .utf8_text(src.as_bytes())
        .ok()?
        .trim();
    let quoted = raw
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            raw.strip_prefix('`')
                .and_then(|rest| rest.strip_suffix('`'))
        })
        .or_else(|| {
            raw.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(raw)
        .trim();
    if quoted.is_empty() {
        return None;
    }
    Some(quoted.to_string())
}

fn find_end_action_start(node: tree_sitter::Node<'_>) -> Option<usize> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == "end_action")
        .map(|child| child.start_byte())
}

fn children_with_field<'node>(
    node: tree_sitter::Node<'node>,
    field: &str,
) -> Vec<tree_sitter::Node<'node>> {
    let mut cursor = node.walk();
    node.children_by_field_name(field, &mut cursor).collect()
}

/// Whether any `if` whose condition tests `hasKey … "key"` — and no other
/// literal key — guards a `fail` terminal in its consequence subtree: the
/// structural shape of a wrapper engine's spread sentinel, whose semantics
/// carry extra failure rules (nats' `$tplYamlSpread`: no spread at the
/// values root, and the program result must match the parent collection's
/// kind). The engine's outer dispatch tests every sentinel in one `or`
/// condition, so requiring a single-key test keeps the plain replace
/// sentinel out of the classification.
fn if_has_key_guards_fail(source: &str, node: tree_sitter::Node<'_>, key: &str) -> bool {
    let mut walker = node.walk();
    for child in node.named_children(&mut walker) {
        if child.kind() == "if_action"
            && let Some(header) = crate::node_eval::control_header(source, child)
            && header_has_key_literals(&header)
                .is_some_and(|literals| literals.len() == 1 && literals.contains(key))
            && children_with_field(child, "consequence")
                .into_iter()
                .any(|consequence| subtree_contains_fail(source, consequence))
        {
            return true;
        }
        if if_has_key_guards_fail(source, child, key) {
            return true;
        }
    }
    false
}

/// Literal keys the header tests with `hasKey`; `None` when it tests none.
fn header_has_key_literals(header: &helm_schema_ast::TemplateHeader) -> Option<BTreeSet<String>> {
    let mut literals = BTreeSet::new();
    header.expr().walk(|inner| {
        if let TemplateExpr::Call { function, args } = inner
            && function == "hasKey"
            && let Some(literal) = literal_string_argument(args.get(1))
        {
            literals.insert(literal);
        }
    });
    if literals.is_empty() {
        None
    } else {
        Some(literals)
    }
}

struct DestructuredRange {
    source_var: String,
    key_var: String,
    value_var: String,
}

/// Collects every `range` in the body, requiring each to be the
/// destructured `range $key, $val := $VAR` form.
///
/// That is the only shape whose member writes the merge recognizer can
/// attribute. Returns `false` when any range deviates.
fn collect_destructured_ranges(
    node: tree_sitter::Node<'_>,
    source: &str,
    out: &mut Vec<DestructuredRange>,
) -> bool {
    if node.kind() == "range_action" {
        let source_var =
            helm_schema_ast::range_header_from_source(node, source).and_then(|header| match header
                .expr()
                .deparen()
            {
                TemplateExpr::Variable(variable) if !variable.is_empty() => {
                    Some(variable.trim_start_matches('$').to_string())
                }
                _ => None,
            });
        let key_var = helm_schema_ast::range_destructured_key_variable(node, source);
        let value_var = helm_schema_ast::range_destructured_value_variable(node, source);
        match (source_var, key_var, value_var) {
            (Some(source_var), Some(key_var), Some(value_var)) => out.push(DestructuredRange {
                source_var,
                key_var,
                value_var,
            }),
            _ => return false,
        }
    }
    let mut walker = node.walk();
    node.named_children(&mut walker)
        .all(|child| collect_destructured_ranges(child, source, out))
}

/// Whether a binding's value is the helper's own recursive merge of two
/// members (`include SELF (list …) | fromYaml`).
///
/// The value discipline treats such a recursion result as another
/// map-member source.
fn is_self_merge_recursion(value: &TemplateExpr, helper_name: &str) -> bool {
    let TemplateExpr::Pipeline(stages) = value.deparen() else {
        return false;
    };
    let [head, tail] = stages.as_slice() else {
        return false;
    };
    let head_is_self_include = matches!(
        head.deparen(),
        TemplateExpr::Call { function, args }
            if (function == "include" || function == "template")
                && matches!(
                    args.first().map(TemplateExpr::deparen),
                    Some(TemplateExpr::Literal(helm_schema_ast::Literal::String(callee)))
                        if callee == helper_name
                )
    );
    head_is_self_include
        && matches!(
            tail.deparen(),
            TemplateExpr::Call { function, args } if function == "fromYaml" && args.is_empty()
        )
}

fn subtree_contains_fail(source: &str, node: tree_sitter::Node<'_>) -> bool {
    match crate::node_eval::node_action(source, node) {
        crate::node_eval::NodeAction::Output(Some(exprs))
        | crate::node_eval::NodeAction::Assignment(Some(exprs)) => {
            let mut found = false;
            for expr in &exprs {
                expr.walk(|inner| {
                    if let TemplateExpr::Call { function, .. } = inner
                        && function == "fail"
                    {
                        found = true;
                    }
                });
            }
            if found {
                return true;
            }
        }
        _ => {}
    }
    let mut walker = node.walk();
    node.named_children(&mut walker)
        .any(|child| subtree_contains_fail(source, child))
}

#[cfg(test)]
#[path = "tests/analysis_db.rs"]
mod tests;
