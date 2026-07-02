use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::{AttributionIndex, DefineIndex, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_body_analysis::{
    ResolveBoundHelperCallParams, interpret_bound_helper_body, resolve_bound_helper_call,
};
use crate::helper_summary::HelperSummary;
use crate::{ContractProvenance, SourceSpan};
use helm_schema_ast::parse_go_template;

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
    define_attributions: RefCell<HashMap<String, AttributionIndex>>,
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
            define_attributions: RefCell::new(HashMap::new()),
            bound_helper_calls: RefCell::new(BTreeMap::new()),
        }
    }

    pub(crate) fn has_helper(&self, name: &str) -> bool {
        self.define_bodies.contains_key(name)
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

    pub(crate) fn parsed_helper_body(&self, name: &str) -> Option<ParsedHelperBody<'_>> {
        let body = self.define_bodies.get(name)?;
        Some(ParsedHelperBody {
            source: body.source.as_str(),
            source_path: body.source_path.as_str(),
            body_offset: body.body_offset,
            tree: self.define_tree(name)?,
        })
    }

    pub(crate) fn helper_attribution(&self, name: &str) -> Option<AttributionIndex> {
        if let Some(attribution) = self.define_attributions.borrow().get(name) {
            return Some(attribution.clone());
        }

        let body = self.define_bodies.get(name)?;
        let tree = self.define_tree(name)?;
        let attribution = crate::resource_identity::attributed_document(
            body.source.as_str(),
            tree.root_node(),
            self,
        );
        self.define_attributions
            .borrow_mut()
            .insert(name.to_string(), attribution.clone());
        Some(attribution)
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
        let seen_key = seen.iter().cloned().collect();
        let key = BoundHelperCallCacheKey::from_resolution(name, &resolution, seen_key);

        if let Some(cached) = self.bound_helper_calls.borrow().get(&key) {
            seen.remove(name);
            return cached.clone();
        }

        let mut summary = interpret_bound_helper_body(name, &resolution, context, seen);
        summary.mark_suppressed_roots_for_bound_outputs(&resolution.bindings);
        self.bound_helper_calls
            .borrow_mut()
            .insert(key, summary.clone());
        seen.remove(name);
        summary
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
    helper_body_dot: Option<AbstractValue>,
    helper_fragment_dot: Option<AbstractValue>,
    seen: BTreeSet<String>,
}

impl BoundHelperCallCacheKey {
    fn from_resolution(
        name: &str,
        resolution: &crate::helper_body_analysis::BoundHelperCallResolution,
        seen: BTreeSet<String>,
    ) -> Self {
        Self {
            name: name.to_string(),
            bindings: resolution
                .bindings
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            helper_body_dot: resolution.helper_body_dot.clone(),
            helper_fragment_dot: resolution.helper_fragment_dot.clone(),
            seen,
        }
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
