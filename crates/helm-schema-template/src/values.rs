use std::collections::BTreeSet;
use tree_sitter::{Node, Tree, TreeCursor};

/// A `.Values` path referenced from templates, normalized to dot-path form:
///   - `.Values.foo.bar`       -> "foo.bar"
///   - `index .Values "a" "b"` -> "a.b"
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValuePath(pub String);

pub fn push_path(paths: &mut BTreeSet<ValuePath>, segs: &[String]) {
    if segs.is_empty() {
        return;
    }
    // full path
    paths.insert(ValuePath(segs.join(".")));
}

// A selector is terminal when it is NOT the left/operand child of a parent selector_expression
pub fn is_terminal_selector(node: &Node) -> bool {
    if node.kind() != "selector_expression" {
        return false;
    }
    if let Some(parent) = node.parent() {
        if parent.kind() == "selector_expression" {
            if let Some(op) = parent.child_by_field_name("operand") {
                // same node id ⇒ this node is the left side of a longer chain
                if op.id() == node.id() {
                    return false;
                }
            }
        }
    }
    true
}

pub fn parse_selector_expression(node: &Node, src: &str) -> Option<Vec<String>> {
    // Expect something like:
    // (selector_expression (selector_expression
    //   (field (identifier "Values"))
    //   (field_identifier "ingress"))
    //   (field_identifier "enabled"))
    //
    // Walk leftward to ensure base is .Values or $.Values
    let mut segs = Vec::<String>::new();
    let mut cur = *node;
    loop {
        match cur.kind() {
            "selector_expression" => {
                let left = cur.child_by_field_name("operand")?;
                let right = cur.child_by_field_name("field")?; // field_identifier
                if right.kind() == "field_identifier" {
                    segs.push(right.utf8_text(src.as_bytes()).ok()?.to_string());
                }
                cur = left;
            }
            "field" => {
                // ".Values" -> (field (identifier "Values"))
                let id = cur.child_by_field_name("name")?;
                if id.kind() == "identifier" && id.utf8_text(src.as_bytes()).ok()? == "Values" {
                    segs.reverse(); // collected from right to left
                    return Some(segs);
                } else {
                    return None;
                }
            }
            "variable" | "dot" => {
                // not a .Values chain
                return None;
            }
            _ => return None,
        }
    }
}

pub fn parse_index_call(node: &Node, src: &str) -> Option<Vec<String>> {
    debug_assert_eq!(node.kind(), "function_call");

    // 1) Get (identifier, argument_list): try fields, else positional fallback.
    let (ident, args) = match (
        node.child_by_field_name("function"),
        node.child_by_field_name("arguments"),
    ) {
        (Some(f), Some(a)) => (f, a),
        _ => {
            let mut cursor = node.walk();
            let mut it = node.named_children(&mut cursor);
            let f = it.next()?; // identifier
            let a = it.next()?; // argument_list
            (f, a)
        }
    };

    if ident.kind() != "identifier" || ident.utf8_text(src.as_bytes()).ok()? != "index" {
        return None;
    }
    if args.kind() != "argument_list" {
        return None;
    }

    // 2) Collect all named children of the argument_list.
    let mut kids = Vec::new();
    let mut aw = args.walk();
    for ch in args.named_children(&mut aw) {
        kids.push(ch);
    }
    if kids.is_empty() {
        return None;
    }

    // 3) Head must be .Values or a selector rooted at .Values
    let mut segs = match kids[0].kind() {
        "field" => {
            let name = kids[0].child_by_field_name("name")?;
            (name.utf8_text(src.as_bytes()).ok()? == "Values").then(|| Vec::<String>::new())
        }
        "selector_expression" => parse_selector_expression(&kids[0], src),
        _ => None,
    }?;

    // 4) Remaining args become path segments (support raw + interpreted strings, and idents).
    for ch in kids.into_iter().skip(1) {
        match ch.kind() {
            "interpreted_string_literal" | "raw_string_literal" => {
                let raw = ch.utf8_text(src.as_bytes()).ok()?;
                let seg = raw
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim_matches('`')
                    .to_string();
                if !seg.is_empty() {
                    segs.push(seg);
                }
            }
            "identifier" | "field_identifier" => {
                segs.push(ch.utf8_text(src.as_bytes()).ok()?.to_string());
            }
            _ => {}
        }
    }

    if segs.is_empty() { None } else { Some(segs) }
}

pub fn extract_values_paths(tree: &Tree, src: &str) -> BTreeSet<ValuePath> {
    let mut paths = BTreeSet::<ValuePath>::new();
    let root = tree.root_node();

    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "selector_expression" && is_terminal_selector(&node) {
            if let Some(segs) = parse_selector_expression(&node, src) {
                push_path(&mut paths, &segs);
            }
        }

        if node.kind() == "function_call" {
            if let Some(segs) = parse_index_call(&node, src) {
                push_path(&mut paths, &segs);
            }
        }

        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }
    paths
}
