use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::{DefineIndex, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_body_analysis::{
    ResolveBoundHelperCallParams, interpret_bound_helper_body, resolve_bound_helper_call,
};
use crate::helper_summary::HelperSummary;
use crate::tree_sitter_utils::parse_go_template;
use crate::{ContractProvenance, SourceSpan};

pub(crate) struct ParsedHelperBody<'a> {
    pub(crate) source: &'a str,
    pub(crate) source_path: &'a str,
    pub(crate) body_offset: usize,
    pub(crate) tree: tree_sitter::Tree,
}

impl ParsedHelperBody<'_> {
    pub(crate) fn provenance(&self, helper_name: &str) -> ContractProvenance {
        ContractProvenance::new(
            self.source_path,
            SourceSpan::new(self.body_offset, self.body_offset + self.source.len()),
            vec![helper_name.to_string()],
        )
    }
}

pub(crate) struct IrAnalysisDb {
    define_bodies: HashMap<String, CachedDefineBody>,
    define_trees: RefCell<HashMap<String, tree_sitter::Tree>>,
    bound_helper_calls: RefCell<BTreeMap<BoundHelperCallCacheKey, HelperSummary>>,
}

impl IrAnalysisDb {
    #[tracing::instrument(skip_all)]
    pub(crate) fn new(defines: &DefineIndex) -> Self {
        let mut define_bodies = HashMap::new();
        for (path, src) in defines.file_sources() {
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
            define_trees: RefCell::new(HashMap::new()),
            bound_helper_calls: RefCell::new(BTreeMap::new()),
        }
    }

    pub(crate) fn has_helper(&self, name: &str) -> bool {
        self.define_bodies.contains_key(name)
    }

    fn define_source(&self, name: &str) -> Option<&str> {
        self.define_bodies
            .get(name)
            .map(|body| body.source.as_str())
    }

    #[tracing::instrument(skip_all)]
    fn define_tree(&self, name: &str) -> Option<tree_sitter::Tree> {
        if let Some(tree) = self.define_trees.borrow().get(name) {
            return Some(tree.clone());
        }

        let src = self.define_source(name)?;
        let tree = parse_go_template(src)?;
        self.define_trees
            .borrow_mut()
            .insert(name.to_string(), tree.clone());
        Some(tree)
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
    ) -> HelperSummary {
        let outer_bindings_key: BTreeMap<String, AbstractValue> = outer_bindings
            .into_iter()
            .flatten()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let fragment_locals_key: BTreeMap<String, AbstractValue> = fragment_locals
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let key = BoundHelperCallCacheKey {
            name: name.to_string(),
            arg: format!("{arg:?}"),
            current_dot: current_dot.cloned(),
            outer_bindings: outer_bindings_key,
            fragment_locals: fragment_locals_key,
            seen: seen.iter().cloned().collect(),
        };

        if let Some(cached) = self.bound_helper_calls.borrow().get(&key) {
            return cached.clone();
        }

        let summary = analyze_bound_helper_call_with_fragment_locals(
            name,
            arg,
            outer_bindings,
            current_dot,
            fragment_locals,
            context,
            seen,
        );
        self.bound_helper_calls
            .borrow_mut()
            .insert(key, summary.clone());
        summary
    }
}

#[tracing::instrument(skip_all, fields(helper = name))]
fn analyze_bound_helper_call_with_fragment_locals(
    name: &str,
    arg: Option<&TemplateExpr>,
    outer_bindings: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    fragment_locals: &HashMap<String, AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HelperSummary {
    if !seen.insert(name.to_string()) {
        return HelperSummary::default();
    }

    let resolution = resolve_bound_helper_call(ResolveBoundHelperCallParams {
        helper_name: name,
        arg,
        outer_bindings,
        current_dot,
        fragment_locals,
        context,
        seen,
    });
    let mut analysis = interpret_bound_helper_body(name, &resolution, context, seen);
    analysis.mark_suppressed_roots_for_bound_outputs(&resolution.bindings);

    seen.remove(name);
    analysis
}

struct CachedDefineBody {
    source: String,
    source_path: String,
    body_offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BoundHelperCallCacheKey {
    name: String,
    arg: String,
    current_dot: Option<AbstractValue>,
    outer_bindings: BTreeMap<String, AbstractValue>,
    fragment_locals: BTreeMap<String, AbstractValue>,
    seen: BTreeSet<String>,
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
