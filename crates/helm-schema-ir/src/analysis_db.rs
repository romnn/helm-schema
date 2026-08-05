use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use helm_schema_ast::{DefineIndex, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::Effects;
use crate::eval_env::EvalEnv;
use crate::expr_eval::bindings_for_helper_arg_with;
use crate::fragment_eval::summary::{FragmentSummary, eval_bound_helper_fragment};
use crate::fragment_eval::{BodyEvalFacts, ValueRead};
use crate::fragment_expr_eval::{
    FragmentEvalContext, context_value_from_outer_expr, document_result_from_expr,
};
use crate::scalar_value::ScalarValueDispatch;
use crate::symbolic::SymbolicPolicy;
use helm_schema_ast::parse_go_template;

pub(crate) struct ParsedHelperBody<'a> {
    pub(crate) source: &'a str,
    pub(crate) source_path: &'a str,
    pub(crate) body_offset: usize,
    pub(crate) tree: tree_sitter::Tree,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CustomMergeHelper {
    /// A recursive merge of two map arguments, with the second taking precedence.
    Pair,
    /// A list of rendered values decoded as maps and merged in list order.
    ParsedMapList,
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
    custom_merge_helpers: RefCell<HashMap<String, Option<CustomMergeHelper>>>,
    nil_scrub_helpers: RefCell<HashMap<String, bool>>,
    /// Exact immutable Helm root fields, represented separately from values.
    static_root_fields: HashMap<String, AbstractValue>,
}

pub(crate) struct BoundHelperCallSummary {
    pub(crate) summary: Rc<FragmentSummary>,
    pub(crate) argument_effects: Effects,
}

fn static_root_fields(strings: BTreeMap<Vec<String>, String>) -> HashMap<String, AbstractValue> {
    let mut roots = BTreeMap::new();
    for (path, value) in strings {
        insert_static_root_string(&mut roots, &path, value);
    }
    roots.into_iter().collect()
}

fn insert_kubernetes_version_fields(strings: &mut BTreeMap<Vec<String>, String>, version: &str) {
    let version = version.trim_start_matches('v');
    let helm_version = format!("v{version}");
    for field in ["Version", "GitVersion"] {
        strings.insert(
            vec![
                "Capabilities".to_string(),
                "KubeVersion".to_string(),
                field.to_string(),
            ],
            helm_version.clone(),
        );
    }

    let mut components = version.split('.');
    for field in ["Major", "Minor"] {
        let Some(component) = components.next() else {
            break;
        };
        strings.insert(
            vec![
                "Capabilities".to_string(),
                "KubeVersion".to_string(),
                field.to_string(),
            ],
            component.to_string(),
        );
    }
}

fn insert_static_root_string(
    fields: &mut BTreeMap<String, AbstractValue>,
    path: &[String],
    value: String,
) {
    let Some((head, tail)) = path.split_first() else {
        return;
    };
    if tail.is_empty() {
        fields.insert(
            head.clone(),
            AbstractValue::StringSet(BTreeSet::from([value])),
        );
        return;
    }
    let entry = fields
        .entry(head.clone())
        .or_insert_with(|| AbstractValue::Dict(BTreeMap::new()));
    if let AbstractValue::Dict(nested) = entry {
        insert_static_root_string(nested, tail, value);
    }
}

fn allowed_custom_merge_set_value(
    expr: &TemplateExpr,
    nested_vars: &BTreeSet<String>,
    is_map_member: &impl Fn(&TemplateExpr) -> bool,
) -> bool {
    if is_map_member(expr) {
        return true;
    }
    match expr.deparen() {
        TemplateExpr::Variable(variable) => nested_vars.contains(variable.trim_start_matches('$')),
        TemplateExpr::Call { function, args } if function == "or" => args
            .iter()
            .all(|arg| allowed_custom_merge_set_value(arg, nested_vars, is_map_member)),
        _ => false,
    }
}

impl IrAnalysisDb {
    #[tracing::instrument(skip_all)]
    pub(crate) fn new(defines: &DefineIndex) -> Self {
        Self::with_policy(defines, SymbolicPolicy::default())
    }

    pub(crate) fn with_policy(defines: &DefineIndex, policy: SymbolicPolicy) -> Self {
        let SymbolicPolicy {
            chart_default_strings,
            kubernetes_version,
            mut static_root_strings,
        } = policy;
        if let Some(version) = kubernetes_version.as_deref() {
            insert_kubernetes_version_fields(&mut static_root_strings, version);
        }
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
            nil_scrub_helpers: RefCell::new(HashMap::new()),
            static_root_fields: static_root_fields(static_root_strings),
        }
    }

    pub(crate) fn static_root_fields(&self) -> &HashMap<String, AbstractValue> {
        &self.static_root_fields
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

    /// Chart-authored defaults that one exact or wildcard values provenance
    /// can select at a `tpl` boundary.
    ///
    /// A wildcard names one mapping member at that depth. Enumerating its
    /// finite matching defaults recovers the exact programs Helm executes
    /// through ranged configuration maps while retaining each concrete
    /// source path for selection guards.
    pub(crate) fn chart_default_programs_matching<'a>(
        &'a self,
        path_pattern: &str,
    ) -> Vec<(&'a str, &'a str)> {
        let pattern = helm_schema_core::split_value_path(path_pattern);
        let has_wildcard = pattern.iter().any(|segment| segment == "*");
        self.chart_default_strings
            .iter()
            .filter_map(|(path, value)| {
                let segments = helm_schema_core::split_value_path(path);
                let matches = segments.len() == pattern.len()
                    && segments
                        .iter()
                        .zip(&pattern)
                        .all(|(segment, expected)| expected == "*" || segment == expected);
                if !matches
                    || (has_wildcard
                        && !matches!(helm_schema_ast::contains_template_action(value), Ok(true)))
                {
                    return None;
                }
                Some((path.as_str(), value.as_str()))
            })
            .collect()
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
    pub(crate) fn custom_merge_helper(&self, name: &str) -> Option<CustomMergeHelper> {
        if let Some(cached) = self.custom_merge_helpers.borrow().get(name) {
            return *cached;
        }
        let recognized = self.classify_custom_merge_helper(name);
        self.custom_merge_helpers
            .borrow_mut()
            .insert(name.to_string(), recognized);
        recognized
    }

    /// Recognize the nil-scrub define shape (airflow's `removeNilFields`):
    /// one destructured range over DOT that copies each member into a
    /// fresh dict accumulator — map members through the self-recursive
    /// scrub (kept only when nonempty), other members only when not nil —
    /// and renders exactly `toYaml ACC`. Under those rules the output IS
    /// the input map minus its (recursively) nil members, so the call
    /// site can substitute the input identity with a scrubbed marker
    /// instead of evaluating the recursion.
    pub(crate) fn nil_scrub_helper(&self, name: &str) -> Option<()> {
        if let Some(cached) = self.nil_scrub_helpers.borrow().get(name) {
            return cached.then_some(());
        }
        let recognized = self.classify_nil_scrub_helper(name).is_some();
        self.nil_scrub_helpers
            .borrow_mut()
            .insert(name.to_string(), recognized);
        recognized.then_some(())
    }

    /// The recognizer matches the canonical action sequence exactly — any
    /// extra action (another condition, another write, another render)
    /// rejects, so a helper that drops more than nil members or rewrites
    /// values can never claim the scrubbed identity.
    #[expect(
        clippy::too_many_lines,
        reason = "keeping this semantic operation together makes its state transitions easier to audit"
    )]
    fn classify_nil_scrub_helper(&self, name: &str) -> Option<()> {
        use helm_schema_ast::Literal;

        let body = self.parsed_helper_body(name)?;
        let mut ranges = Vec::new();
        if !collect_dot_ranges(body.tree.root_node(), body.source, &mut ranges) {
            return None;
        }
        let [(key_var, val_var)] = ranges.as_slice() else {
            return None;
        };

        let exprs = helm_schema_ast::parse_action_expressions(body.source);
        let [
            accumulator_init,
            range_header,
            map_test,
            nested_init,
            nonempty_test,
            nested_write,
            not_nil_test,
            member_write,
            render,
        ] = exprs.as_slice()
        else {
            return None;
        };

        let var_is = |expr: &TemplateExpr, name: &str| {
            matches!(
                expr.deparen(),
                TemplateExpr::Variable(variable) if variable.trim_start_matches('$') == name
            )
        };
        let TemplateExpr::VariableDefinition {
            name: accumulator,
            value,
        } = accumulator_init
        else {
            return None;
        };
        let accumulator = accumulator.trim_start_matches('$');
        if !matches!(
            value.deparen(),
            TemplateExpr::Call { function, args } if function == "dict" && args.is_empty()
        ) {
            return None;
        }
        if !matches!(range_header.deparen(), TemplateExpr::Field(path) if path.is_empty()) {
            return None;
        }
        if !matches!(
            map_test.deparen(),
            TemplateExpr::Call { function, args }
                if function == "kindIs"
                    && matches!(
                        args.first().map(TemplateExpr::deparen),
                        Some(TemplateExpr::Literal(Literal::String(kind))) if kind == "map"
                    )
                    && args.get(1).is_some_and(|arg| var_is(arg, val_var))
        ) {
            return None;
        }
        let TemplateExpr::VariableDefinition {
            name: nested,
            value,
        } = nested_init
        else {
            return None;
        };
        let nested = nested.trim_start_matches('$');
        let TemplateExpr::Pipeline(stages) = value.deparen() else {
            return None;
        };
        let [head, tail] = stages.as_slice() else {
            return None;
        };
        if !matches!(
            head.deparen(),
            TemplateExpr::Call { function, args }
                if (function == "include" || function == "template")
                    && matches!(
                        args.first().map(TemplateExpr::deparen),
                        Some(TemplateExpr::Literal(Literal::String(callee))) if callee == name
                    )
                    && args.len() == 2
                    && args.get(1).is_some_and(|arg| var_is(arg, val_var))
        ) || !matches!(
            tail.deparen(),
            TemplateExpr::Call { function, args } if function == "fromYaml" && args.is_empty()
        ) {
            return None;
        }
        if !matches!(
            nonempty_test.deparen(),
            TemplateExpr::Call { function, args }
                if function == "gt"
                    && matches!(
                        args.first().map(TemplateExpr::deparen),
                        Some(TemplateExpr::Call { function: len_fn, args: len_args })
                            if len_fn == "len"
                                && matches!(len_args.as_slice(), [arg] if var_is(arg, nested))
                    )
                    && matches!(
                        args.get(1).map(TemplateExpr::deparen),
                        Some(TemplateExpr::Literal(Literal::Int(0)))
                    )
        ) {
            return None;
        }
        let accumulator_write = |expr: &TemplateExpr, member: &str| {
            matches!(
                expr,
                TemplateExpr::VariableDefinition { value, .. }
                    if matches!(
                        value.deparen(),
                        TemplateExpr::Call { function, args }
                            if function == "set"
                                && matches!(args.as_slice(), [target, key, value]
                                    if var_is(target, accumulator)
                                        && var_is(key, key_var)
                                        && var_is(value, member))
                    )
            )
        };
        if !accumulator_write(nested_write, nested) {
            return None;
        }
        if !matches!(
            not_nil_test.deparen(),
            TemplateExpr::Call { function, args }
                if function == "not"
                    && matches!(
                        args.first().map(TemplateExpr::deparen),
                        Some(TemplateExpr::Call { function: kind_fn, args: kind_args })
                            if kind_fn == "kindIs"
                                && matches!(
                                    kind_args.first().map(TemplateExpr::deparen),
                                    Some(TemplateExpr::Literal(Literal::String(kind)))
                                        if kind == "invalid"
                                )
                                && kind_args.get(1).is_some_and(|arg| var_is(arg, val_var))
                    )
        ) {
            return None;
        }
        if !accumulator_write(member_write, val_var) {
            return None;
        }
        matches!(
            render.deparen(),
            TemplateExpr::Call { function, args }
                if function == "toYaml"
                    && matches!(args.as_slice(), [arg] if var_is(arg, accumulator))
        )
        .then_some(())
    }

    fn classify_custom_merge_helper(&self, name: &str) -> Option<CustomMergeHelper> {
        if self.classify_pair_merge_helper(name).is_some() {
            return Some(CustomMergeHelper::Pair);
        }
        self.classify_parsed_map_list_merge_helper(name)
            .map(|()| CustomMergeHelper::ParsedMapList)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "keeping this semantic recognition operation together makes its invariants easier to audit"
    )]
    fn classify_pair_merge_helper(&self, name: &str) -> Option<()> {
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
                            || !targets_out
                            || !keyed_by_range
                            || args.get(2).is_none_or(|value| {
                                !allowed_custom_merge_set_value(value, &nested_vars, &is_map_member)
                            })
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
                                && matches!(
                                    list_args.as_slice(),
                                    [first, second, ..]
                                        if is_map_member(first) && is_map_member(second)
                                )
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

    /// Recognizes the Bitnami-style rendered-map merge exactly.
    ///
    /// Each `.values` member is rendered by a typed string-or-`toYaml`
    /// helper, decoded with Helm's map-only `fromYaml`, and merged into a
    /// fresh accumulator before that accumulator is serialized. Therefore
    /// only mapping source shapes retain identity at the output.
    fn classify_parsed_map_list_merge_helper(&self, name: &str) -> Option<()> {
        let body = self.parsed_helper_body(name)?;
        let expressions = helm_schema_ast::parse_action_expressions(body.source);
        let [init, range_subject, assignment, render] = expressions.as_slice() else {
            return None;
        };

        let accumulator = match init.deparen() {
            TemplateExpr::VariableDefinition { name, value }
                if matches!(
                    value.deparen(),
                    TemplateExpr::Call { function, args }
                        if function == "dict" && args.is_empty()
                ) =>
            {
                name.trim_start_matches('$')
            }
            _ => return None,
        };
        if !matches!(range_subject.deparen(), TemplateExpr::Field(path) if *path == ["values"]) {
            return None;
        }

        let TemplateExpr::Assignment {
            name: assigned,
            value,
        } = assignment
        else {
            return None;
        };
        if assigned.trim_start_matches('$') != accumulator {
            return None;
        }
        let TemplateExpr::Pipeline(stages) = value.deparen() else {
            return None;
        };
        let [include, from_yaml, merge] = stages.as_slice() else {
            return None;
        };
        let renderer = parsed_map_renderer_call(include)?;
        if !self.classify_typed_yaml_renderer(renderer)
            || !matches!(
                from_yaml.deparen(),
                TemplateExpr::Call { function, args }
                    if function == "fromYaml" && args.is_empty()
            )
            || !matches!(
                merge.deparen(),
                TemplateExpr::Call { function, args }
                    if function == "merge"
                        && matches!(
                            args.as_slice(),
                            [TemplateExpr::Variable(variable)]
                                if variable.trim_start_matches('$') == accumulator
                        )
            )
            || !matches!(
                render.deparen(),
                TemplateExpr::Pipeline(stages)
                    if matches!(
                        stages.as_slice(),
                        [TemplateExpr::Variable(variable), TemplateExpr::Call { function, args }]
                            if variable.trim_start_matches('$') == accumulator
                                && function == "toYaml"
                                && args.is_empty()
                    )
            )
        {
            return None;
        }

        let mut ranges = Vec::new();
        collect_nodes_of_kind(body.tree.root_node(), "range_action", &mut ranges);
        let [range] = ranges.as_slice() else {
            return None;
        };
        let range_source = body.source.get(range.byte_range())?;
        let range_expressions = helm_schema_ast::parse_action_expressions(range_source);
        matches!(
            range_expressions.as_slice(),
            [range_subject, range_assignment]
                if range_subject == expressions.get(1)?
                    && range_assignment == expressions.get(2)?
        )
        .then_some(())
    }

    fn classify_typed_yaml_renderer(&self, name: &str) -> bool {
        let Some(body) = self.parsed_helper_body(name) else {
            return false;
        };
        let expressions = helm_schema_ast::parse_action_expressions(body.source);
        let [binding, contains, scope, scoped_tpl, tpl, output] = expressions.as_slice() else {
            return false;
        };
        let TemplateExpr::VariableDefinition {
            name: binding_name,
            value,
        } = binding
        else {
            return false;
        };
        let binding_name = binding_name.trim_start_matches('$');
        typed_yaml_renderer_binding(value)
            && contains_tests_render_subject(contains)
            && matches!(scope.deparen(), TemplateExpr::Field(path) if *path == ["scope"])
            && tpl_consumes_binding(scoped_tpl, binding_name)
            && tpl_consumes_binding(tpl, binding_name)
            && matches!(
                output.deparen(),
                TemplateExpr::Variable(variable)
                    if variable.trim_start_matches('$') == binding_name
            )
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
    #[expect(
        clippy::too_many_arguments,
        reason = "helper evaluation needs the complete lexical and fragment context"
    )]
    pub(crate) fn summarize_bound_helper_call(
        &self,
        name: &str,
        arg: Option<&TemplateExpr>,
        outer_bindings: Option<&HashMap<String, AbstractValue>>,
        current_dot: Option<&AbstractValue>,
        eval_env: &EvalEnv,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> BoundHelperCallSummary {
        if !seen.insert(name.to_string()) {
            return BoundHelperCallSummary {
                summary: Rc::new(FragmentSummary::default()),
                argument_effects: Effects::default(),
            };
        }

        let resolved = resolve_bound_helper_call(&ResolveBoundHelperCallParams {
            helper_name: name,
            arg,
            outer_bindings,
            current_dot,
            eval_env,
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
    /// Scalar facts visible through the helper's dot-relative root fields:
    /// caller root facts for a root-passthrough call, or evaluated field
    /// dispatches for a statically constructed argument mapping.
    pub(crate) root_truthy_predicates: HashMap<String, helm_schema_core::Predicate>,
    pub(crate) root_value_dispatches: HashMap<String, ScalarValueDispatch>,
}

struct ResolvedBoundHelperCall {
    resolution: BoundHelperCallResolution,
    argument_effects: Effects,
}

#[derive(Clone, Copy)]
struct ResolveBoundHelperCallParams<'a, 'context> {
    helper_name: &'a str,
    arg: Option<&'a TemplateExpr>,
    outer_bindings: Option<&'a HashMap<String, AbstractValue>>,
    current_dot: Option<&'a AbstractValue>,
    eval_env: &'a EvalEnv,
    context: FragmentEvalContext<'context>,
    seen: &'a HashSet<String>,
}

fn resolve_bound_helper_call(
    params: &ResolveBoundHelperCallParams<'_, '_>,
) -> ResolvedBoundHelperCall {
    let mut argument_effects = Effects::default();
    let mut eval_arg = |expr: &TemplateExpr, seen: &mut HashSet<String>| {
        let result = document_result_from_expr(
            expr,
            params.eval_env,
            params.outer_bindings,
            params.current_dot,
            params.context,
            seen,
        );
        argument_effects.merge(result.effects.clone().execution_only());
        result
    };
    let mut binding_seen = params.seen.clone();
    let arg_resolution = bindings_for_helper_arg_with(params.arg, params.outer_bindings, |expr| {
        eval_arg(expr, &mut binding_seen)
    });
    let mut bindings = arg_resolution.bindings;
    let argument_scalar_dispatches = arg_resolution
        .scalar_dispatches
        .into_iter()
        .collect::<HashMap<_, _>>();

    // The binding resolution already evaluated the whole arg unless the arg
    // was a dot/root or merge call; only those shapes still need their own
    // helper-dot evaluation here (same evaluation, fresh seen set).
    let mut helper_body_dot = arg_resolution
        .value
        .or_else(|| {
            let mut dot_seen = params.seen.clone();
            params
                .arg
                .and_then(|expr| eval_arg(expr, &mut dot_seen).value)
        })
        .or_else(|| params.current_dot.cloned());

    let mut helper_fragment_dot = params.arg.and_then(|expr| {
        context_value_from_outer_expr(
            expr,
            Some(&params.eval_env.locals),
            Some(&params.eval_env.local_output_meta),
            params.outer_bindings,
            params.current_dot,
        )
    });

    let mut widened_paths = BTreeSet::new();
    widen_large_bound_values(&mut bindings, params.helper_name, &mut widened_paths);
    helper_body_dot = helper_body_dot
        .map(|binding| widen_large_bound_value(binding, params.helper_name, &mut widened_paths));
    helper_fragment_dot = helper_fragment_dot
        .map(|binding| widen_large_bound_value(binding, params.helper_name, &mut widened_paths));
    // Widening discards member-to-path correspondence, not the fact that the
    // eager argument read those paths. Total-shape dependency rows keep a
    // closed root schema from turning that resource abstention into a false
    // rejection; they do not claim that YAML serialization actually ran.
    argument_effects
        .helper_reads
        .extend(widened_paths.into_iter().map(|values_path| ValueRead {
            values_path,
            kind: crate::ValueKind::WidenedDependency,
            condition: helm_schema_core::GuardDnf::default(),
            resource: None,
            provenance: Vec::new(),
            dependency: true,
        }));

    // Root condition facts apply only when the body's dot IS the caller's
    // root context: only then does a body-level `.field` read resolve
    // against the caller's root `set` state.
    let root_passthrough = matches!(helper_body_dot, Some(AbstractValue::RootContext));
    if root_passthrough {
        for (field, value) in params.context.analysis_db.static_root_fields() {
            bindings
                .entry(field.clone())
                .or_insert_with(|| value.clone());
        }
    }
    let (root_truthy_predicates, root_value_dispatches) = if root_passthrough {
        (
            params.eval_env.root_truthy_predicates.clone(),
            params.eval_env.root_value_dispatches.clone(),
        )
    } else {
        (HashMap::new(), argument_scalar_dispatches)
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

/// Caps bound helper values before recursive structure causes combinatorial expansion.
///
/// Corpus measurement placed the nearest precise value at width 31 and the first widened value at
/// width 58.
/// A synthetic 256-leaf value reproduced the preflight blowup this bound prevents.
pub(crate) const BOUND_HELPER_STRUCTURAL_WIDTH_LIMIT: usize = 32;

pub(crate) fn widen_large_bound_values(
    bindings: &mut HashMap<String, AbstractValue>,
    helper_name: &str,
    widened_paths: &mut BTreeSet<String>,
) {
    for binding in bindings.values_mut() {
        let Some(widened) = widen_large_bound_value_ref(binding, helper_name, widened_paths) else {
            continue;
        };
        *binding = widened;
    }
}

pub(crate) fn widen_large_bound_value_ref(
    binding: &AbstractValue,
    helper_name: &str,
    widened_paths: &mut BTreeSet<String>,
) -> Option<AbstractValue> {
    let width = binding.structural_width();
    if width <= BOUND_HELPER_STRUCTURAL_WIDTH_LIMIT {
        return None;
    }

    tracing::debug!(
        helper = helper_name,
        structural_width = width,
        structural_width_limit = BOUND_HELPER_STRUCTURAL_WIDTH_LIMIT,
        "widening bound helper value"
    );
    widened_paths.extend(binding.paths());
    Some(AbstractValue::Top)
}

fn widen_large_bound_value(
    binding: AbstractValue,
    helper_name: &str,
    widened_paths: &mut BTreeSet<String>,
) -> AbstractValue {
    widen_large_bound_value_ref(&binding, helper_name, widened_paths).unwrap_or(binding)
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
    root_value_dispatches: BTreeMap<String, ScalarValueDispatch>,
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

/// The `(name, body)` pairs a template source `define`s, for include-graph
/// walks that need to follow helper calls through helper bodies and for
/// chart-ownership queries over the define index's files.
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

fn parsed_map_renderer_call(expression: &TemplateExpr) -> Option<&str> {
    let TemplateExpr::Call { function, args } = expression.deparen() else {
        return None;
    };
    if function != "include" && function != "template" {
        return None;
    }
    let [
        TemplateExpr::Literal(helm_schema_ast::Literal::String(renderer)),
        argument,
    ] = args.as_slice()
    else {
        return None;
    };
    let TemplateExpr::Call {
        function: dict,
        args: entries,
    } = argument.deparen()
    else {
        return None;
    };
    if dict != "dict" {
        return None;
    }
    entries
        .chunks_exact(2)
        .any(|entry| {
            matches!(
                entry,
                [
                    TemplateExpr::Literal(helm_schema_ast::Literal::String(key)),
                    TemplateExpr::Field(path),
                ] if key == "value" && path.is_empty()
            )
        })
        .then_some(renderer)
}

fn typed_yaml_renderer_binding(expression: &TemplateExpr) -> bool {
    let TemplateExpr::Pipeline(stages) = expression.deparen() else {
        return false;
    };
    matches!(
        stages.as_slice(),
        [
            TemplateExpr::Call {
                function: type_is,
                args: type_args,
            },
            TemplateExpr::Call {
                function: ternary,
                args: ternary_args,
            },
        ] if type_is == "typeIs"
            && matches!(
                type_args.as_slice(),
                [
                    TemplateExpr::Literal(helm_schema_ast::Literal::String(kind)),
                    TemplateExpr::Field(subject),
                ] if kind == "string" && *subject == ["value"]
            )
            && matches!(
                ternary_args.as_slice(),
                [TemplateExpr::Field(raw), serialized]
                    if *raw == ["value"] && matches!(
                        serialized.deparen(),
                        TemplateExpr::Pipeline(serialized_stages)
                            if matches!(
                                serialized_stages.as_slice(),
                                [
                                    TemplateExpr::Field(subject),
                                    TemplateExpr::Call { function, args },
                                ] if *subject == ["value"]
                                    && function == "toYaml"
                                    && args.is_empty()
                            )
                    )
            )
    )
}

fn contains_tests_render_subject(expression: &TemplateExpr) -> bool {
    matches!(
        expression.deparen(),
        TemplateExpr::Call { function, args }
            if function == "contains"
                && matches!(
                    args.as_slice(),
                    [
                        TemplateExpr::Literal(helm_schema_ast::Literal::String(open)),
                        encoded,
                    ] if open == "{{" && matches!(
                        encoded.deparen(),
                        TemplateExpr::Call { function, args }
                            if function == "toJson"
                                && matches!(
                                    args.as_slice(),
                                    [TemplateExpr::Field(path)] if *path == ["value"]
                                )
                    )
                )
    )
}

fn tpl_consumes_binding(expression: &TemplateExpr, binding: &str) -> bool {
    let TemplateExpr::Call { function, args } = expression.deparen() else {
        return false;
    };
    if function != "tpl" {
        return false;
    }
    let Some(program) = args.first() else {
        return false;
    };
    let mut consumes = false;
    program.walk(|inner| {
        if matches!(
            inner,
            TemplateExpr::Variable(variable)
                if variable.trim_start_matches('$') == binding
        ) {
            consumes = true;
        }
    });
    consumes
}

fn collect_nodes_of_kind<'tree>(
    node: tree_sitter::Node<'tree>,
    kind: &str,
    out: &mut Vec<tree_sitter::Node<'tree>>,
) {
    if node.kind() == kind {
        out.push(node);
    }
    let mut walker = node.walk();
    for child in node.named_children(&mut walker) {
        collect_nodes_of_kind(child, kind, out);
    }
}

#[expect(
    clippy::struct_field_names,
    reason = "the suffix distinguishes the three template variables from their resolved values"
)]
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

/// Collect `(key_var, value_var)` for destructured ranges whose source is
/// the helper's own DOT, rejecting any other range in the subtree.
fn collect_dot_ranges(
    node: tree_sitter::Node<'_>,
    source: &str,
    out: &mut Vec<(String, String)>,
) -> bool {
    if node.kind() == "range_action" {
        let source_is_dot =
            helm_schema_ast::range_header_from_source(node, source).is_some_and(|header| {
                matches!(header.expr().deparen(), TemplateExpr::Field(path) if path.is_empty())
            });
        let key_var = helm_schema_ast::range_destructured_key_variable(node, source);
        let value_var = helm_schema_ast::range_destructured_value_variable(node, source);
        match (source_is_dot, key_var, value_var) {
            (true, Some(key_var), Some(value_var)) => out.push((key_var, value_var)),
            _ => return false,
        }
    }
    let mut walker = node.walk();
    node.named_children(&mut walker)
        .all(|child| collect_dot_ranges(child, source, out))
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
