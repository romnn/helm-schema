use std::cell::RefCell;
use std::collections::HashMap;

use helm_schema_ast::DefineIndex;

use crate::tree_sitter_utils::parse_go_template;

pub(crate) struct DefineBodyCache {
    bodies: HashMap<String, CachedDefineBody>,
    trees: RefCell<HashMap<String, tree_sitter::Tree>>,
}

impl DefineBodyCache {
    #[tracing::instrument(skip_all)]
    pub(crate) fn new(defines: &DefineIndex) -> Self {
        let mut bodies = HashMap::new();
        for (path, src) in defines.file_sources() {
            for block in extract_define_blocks(src) {
                bodies.insert(
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
            bodies,
            trees: RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn source(&self, name: &str) -> Option<&str> {
        self.bodies.get(name).map(|body| body.source.as_str())
    }

    pub(crate) fn source_path(&self, name: &str) -> Option<&str> {
        self.bodies.get(name).map(|body| body.source_path.as_str())
    }

    pub(crate) fn body_offset(&self, name: &str) -> Option<usize> {
        self.bodies.get(name).map(|body| body.body_offset)
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn tree(&self, name: &str) -> Option<tree_sitter::Tree> {
        if let Some(tree) = self.trees.borrow().get(name) {
            return Some(tree.clone());
        }

        let src = self.source(name)?;
        let tree = parse_go_template(src)?;
        self.trees
            .borrow_mut()
            .insert(name.to_string(), tree.clone());
        Some(tree)
    }
}

struct CachedDefineBody {
    source: String,
    source_path: String,
    body_offset: usize,
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
#[path = "tests/define_body_cache.rs"]
mod tests;
