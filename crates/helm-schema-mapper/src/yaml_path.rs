use crate::Role;
use std::collections::BTreeMap;
use thiserror::Error;
use tree_sitter::{Node, Parser};

fn is_mapping_pair(n: &Node) -> bool {
    matches!(n.kind(), "block_mapping_pair" | "flow_pair")
}

fn is_sequence(n: &Node) -> bool {
    matches!(n.kind(), "block_sequence" | "flow_sequence")
}

fn is_key_node(node: Node, pair: Node) -> bool {
    // find the node that is the direct child under `pair`
    let n_under_pair = ascend_to_child_of(node, pair);

    if let Some(k) = pair.child_by_field_name("key") {
        return n_under_pair.id() == k.id();
    }
    // fallback: first named child is typically the key
    if let Some(first) = pair.named_child(0) {
        return n_under_pair.id() == first.id();
    }
    false
}

fn nearest_ancestor<F>(mut n: Node, pred: F) -> Option<Node>
where
    F: Fn(&Node) -> bool,
{
    let mut p = n.parent();
    while let Some(node) = p {
        if pred(&node) {
            return Some(node);
        }
        p = node.parent();
    }
    None
}

/// Ascend from `n` until its **direct** parent is `ancestor`, returning that child.
fn ascend_to_child_of<'tree>(mut n: Node<'tree>, ancestor: Node<'tree>) -> Node<'tree> {
    let mut cur = n;
    while let Some(p) = cur.parent() {
        if p.id() == ancestor.id() {
            return cur;
        }
        cur = p;
    }
    n
}

/// Extract a mapping key from a mapping pair.
/// Fallbacks make this robust across grammar changes.
fn mapping_key_text(pair: Node, src: &str) -> Option<String> {
    if let Some(k) = pair.child_by_field_name("key") {
        return Some(
            k.utf8_text(src.as_bytes())
                .ok()?
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string(),
        );
    }
    // Fallback: first named child is typically the key
    let k = pair.named_child(0)?;
    Some(
        k.utf8_text(src.as_bytes())
            .ok()?
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string(),
    )
}

fn index_in_sequence(item_or_descendant: Node, seq: Node) -> usize {
    // find the direct child of `seq` we belong to
    let item = ascend_to_child_of(item_or_descendant, seq);
    let mut idx = 0usize;
    let mut c = seq.walk();
    for ch in seq.named_children(&mut c) {
        if ch.id() == item.id() {
            return idx;
        }
        idx += 1;
    }
    idx
}

fn compute_path_for_scalar(mut leaf: Node, src: &str) -> Option<YamlPath> {
    let mut elems = Vec::<PathElem>::new();

    // We’ll move upward in hops: nearest mapping-pair OR nearest sequence, whichever comes first.
    loop {
        // Find nearest mapping pair and nearest sequence
        let near_pair = nearest_ancestor(leaf, |n| is_mapping_pair(n));
        let near_seq = nearest_ancestor(leaf, |n| is_sequence(n));

        match (near_pair, near_seq) {
            (Some(p), Some(s)) => {
                // pick the one **closer** to `leaf`
                let d_pair = depth_between(leaf, p);
                let d_seq = depth_between(leaf, s);
                if d_pair <= d_seq {
                    // mapping pair is closer
                    let key = mapping_key_text(p, src)?;
                    elems.push(PathElem::Key(key));
                    leaf = p; // continue from the pair
                } else {
                    // sequence is closer
                    let idx = index_in_sequence(leaf, s);
                    elems.push(PathElem::Index(idx));
                    leaf = s; // continue from the sequence
                }
            }
            (Some(p), None) => {
                let key = mapping_key_text(p, src)?;
                elems.push(PathElem::Key(key));
                leaf = p;
            }
            (None, Some(s)) => {
                let idx = index_in_sequence(leaf, s);
                elems.push(PathElem::Index(idx));
                leaf = s;
            }
            (None, None) => break, // reached top (document/stream)
        }

        // Stop at document root-ish nodes
        if matches!(leaf.kind(), "document" | "stream" | "program") {
            break;
        }
    }

    elems.reverse();
    Some(YamlPath(elems))
}

fn depth_between(mut from: Node, to: Node) -> usize {
    let mut d = 0usize;
    let mut p = from.parent();
    while let Some(n) = p {
        d += 1;
        if n.id() == to.id() {
            return d;
        }
        p = n.parent();
    }
    usize::MAX // shouldn't happen if `to` is ancestor of `from`
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathElem {
    Key(String),
    Index(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct YamlPath(pub Vec<PathElem>);

impl std::fmt::Display for YamlPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for e in &self.0 {
            match e {
                PathElem::Key(k) => {
                    if first {
                        write!(f, "{}", k)?;
                        first = false;
                    } else {
                        write!(f, ".{}", k)?;
                    }
                }
                PathElem::Index(i) => {
                    write!(f, "[{}]", i)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("parse yaml")]
    Parse,
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub role: Role,
    pub path: Option<YamlPath>, // None for mapping keys
}

impl Default for Binding {
    fn default() -> Self {
        Self {
            role: Role::Unknown,
            path: None,
        }
    }
}

pub fn compute_yaml_paths_for_placeholders(
    sanitized: &str,
) -> Result<BTreeMap<usize, Binding>, Error> {
    // Parse YAML
    let mut parser = Parser::new();
    let language = tree_sitter::Language::new(helm_schema_template_grammar::yaml::language());
    parser.set_language(&language).map_err(|_| Error::Parse)?;
    let tree = parser.parse(sanitized, None).ok_or(Error::Parse)?;

    // Find scalars with "__TSG_PLACEHOLDER_{n}__"
    let mut map = BTreeMap::new();
    let root = tree.root_node();
    let mut stack = vec![root];

    while let Some(n) = stack.pop() {
        // scalar kinds in tree-sitter-yaml
        let k = n.kind();
        let is_scalar = matches!(
            k,
            "plain_scalar" | "single_quote_scalar" | "double_quote_scalar"
        );
        if is_scalar {
            let text = &sanitized[n.byte_range()];
            if let Some(id) = parse_placeholder_id(text) {
                // Is this scalar a mapping key?
                if let Some(pair) = nearest_ancestor(n, is_mapping_pair) {
                    if is_key_node(n, pair) {
                        map.insert(
                            id,
                            Binding {
                                role: Role::MappingKey,
                                path: None,
                            },
                        );
                        // don't compute value path for keys
                    } else {
                        let path = compute_path_for_scalar(n, sanitized);
                        map.insert(
                            id,
                            Binding {
                                role: Role::ScalarValue,
                                path,
                            },
                        );
                    }
                } else {
                    // Not under a pair => treat as value and compute path if possible (e.g., sequence item)
                    let path = compute_path_for_scalar(n, sanitized);
                    map.insert(
                        id,
                        Binding {
                            role: Role::ScalarValue,
                            path,
                        },
                    );
                }
            }
        }

        let mut c = n.walk();
        for ch in n.children(&mut c) {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }

    Ok(map)
}

fn parse_placeholder_id(text: &str) -> Option<usize> {
    // matches "__TSG_PLACEHOLDER_{n}__" with or without quotes
    let t = text.trim().trim_matches('"');
    t.strip_prefix("__TSG_PLACEHOLDER_")
        .and_then(|rest| rest.strip_suffix("__"))
        .and_then(|num| num.parse::<usize>().ok())
}

// if let Some(path) = compute_path_for_scalar(n, sanitized) {
//     map.insert(id, path);
// }

#[cfg(false)]
pub mod v1 {
    use super::{PathElem, YamlPath};
    use std::collections::BTreeMap;
    use thiserror::Error;
    use tree_sitter::{Node, Parser};

    /// compute YAML path for a scalar node by walking up mapping/sequence parents
    fn compute_path_for_scalar(mut node: Node, src: &str) -> Option<YamlPath> {
        let mut elems = Vec::<PathElem>::new();

        loop {
            let parent = node.parent()?;
            match parent.kind() {
                "block_mapping_pair" | "flow_pair" => {
                    // get key text
                    let key = parent
                        .child_by_field_name("key")
                        .unwrap_or_else(|| parent.named_child(0).unwrap());
                    let key_text = key.utf8_text(src.as_bytes()).ok()?;
                    // sanitize key text to identifier-ish (strip quotes if any)
                    let key_norm = key_text
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                    elems.push(PathElem::Key(key_norm));
                    // move to the pair's parent
                    node = parent;
                }
                "block_sequence_item" | "flow_node" | "block_node" => {
                    // compute index among siblings if parent is a sequence
                    if let Some(seq) = find_sequence_parent(node) {
                        let idx = index_in_sequence(node, seq);
                        elems.push(PathElem::Index(idx));
                        node = seq;
                    } else {
                        node = parent;
                    }
                }
                _ => {
                    node = parent;
                }
            }

            // stop at document root-ish nodes
            if matches!(node.kind(), "document" | "stream" | "program") {
                break;
            }
        }

        elems.reverse();
        Some(YamlPath(elems))
    }

    fn find_sequence_parent(mut node: Node) -> Option<Node> {
        let mut p = node.parent();
        while let Some(pp) = p {
            if pp.kind().contains("sequence") {
                return Some(pp);
            }
            p = pp.parent();
        }
        None
    }

    fn index_in_sequence(item_or_descendant: Node, seq: Node) -> usize {
        // find nearest ancestor that is a child of seq
        let mut node = item_or_descendant;
        while let Some(p) = node.parent() {
            if p.id() == seq.id() {
                break;
            }
            node = p;
        }
        let mut idx = 0usize;
        let mut c = seq.walk();
        for ch in seq.named_children(&mut c) {
            if ch.id() == node.id() {
                return idx;
            }
            idx += 1;
        }
        idx
    }
}
