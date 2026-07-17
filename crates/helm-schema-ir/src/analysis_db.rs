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
}

pub(crate) struct BoundHelperCallSummary {
    pub(crate) summary: Rc<FragmentSummary>,
    pub(crate) argument_effects: Effects,
}

impl IrAnalysisDb {
    #[tracing::instrument(skip_all)]
    pub(crate) fn new(defines: &DefineIndex) -> Self {
        Self::with_chart_default_strings(defines, BTreeMap::new())
    }

    pub(crate) fn with_chart_default_strings(
        defines: &DefineIndex,
        chart_default_strings: BTreeMap<String, String>,
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
        }
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
    /// an engine.
    pub(crate) fn program_wrapper_sentinels(&self, entry: &str) -> BTreeSet<String> {
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
            .cloned()
            .collect()
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
}

struct ResolvedBoundHelperCall {
    resolution: BoundHelperCallResolution,
    argument_effects: Effects,
}

struct ResolveBoundHelperCallParams<'a, 'context> {
    helper_name: &'a str,
    arg: Option<&'a TemplateExpr>,
    outer_bindings: Option<&'a HashMap<String, AbstractValue>>,
    current_dot: Option<&'a AbstractValue>,
    fragment_locals: &'a HashMap<String, AbstractValue>,
    context: FragmentEvalContext<'context>,
    seen: &'a HashSet<String>,
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

    ResolvedBoundHelperCall {
        resolution: BoundHelperCallResolution {
            bindings,
            dot: DotFrame {
                helper: helper_body_dot,
                fragment: helper_fragment_dot,
            },
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

#[cfg(test)]
#[path = "tests/analysis_db.rs"]
mod tests;
