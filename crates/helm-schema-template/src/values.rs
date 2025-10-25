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
    // // leaf heuristic (matches your test expectation)
    // if let Some(last) = segs.last() {
    //     paths.insert(ValuePath(last.clone()));
    // }
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
    // Expect something like: (selector_expression (selector_expression (field (identifier "Values")) (field_identifier "ingress")) (field_identifier "enabled"))
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

// fn parse_index_call(node: Node, src: &str) -> Option<Vec<String>> {
//     debug_assert_eq!(node.kind(), "function_call");
//     // (function_call (identifier "index") (argument_list <arg1> <arg2> ...))
//     // We handle: index .Values "ingress" "pathType" and also index (.Values.foo) "bar"
//     let func = node.child_by_field_name("function")?;
//     if func.kind() != "identifier" || func.utf8_text(src.as_bytes()).ok()? != "index" {
//         return None;
//     }
//     let args = node.child_by_field_name("arguments")?;
//     let mut segs = Vec::<String>::new();
//     // first arg must be a .Values chain of some sort
//     let mut cursor = args.walk();
//     let mut children = Vec::new();
//     for ch in args.named_children(&mut cursor) {
//         children.push(ch);
//     }
//     if children.is_empty() {
//         return None;
//     }
//
//     // arg0: either "field" (.Values) or selector_expression that resolves to Values.* chain
//     if let Some(head) = children.first() {
//         match head.kind() {
//             "field" | "selector_expression" => {
//                 if let Some(mut base) = match head.kind() {
//                     "selector_expression" => parse_selector_expression(*head, src),
//                     "field" => {
//                         let id = head.child_by_field_name("name")?;
//                         (id.kind() == "identifier"
//                             && id.utf8_text(src.as_bytes()).ok()? == "Values")
//                             .then(|| Vec::<String>::new())
//                     }
//                     _ => None,
//                 } {
//                     // remaining args are path segments (string or identifiers)
//                     for ch in children.iter().skip(1) {
//                         match ch.kind() {
//                             "interpreted_string_literal" => {
//                                 let raw = ch.utf8_text(src.as_bytes()).ok()?.to_string();
//                                 // strip quotes
//                                 let s = raw.trim_matches('"').to_string();
//                                 base.push(s);
//                             }
//                             "identifier" | "field_identifier" => {
//                                 base.push(ch.utf8_text(src.as_bytes()).ok()?.to_string());
//                             }
//                             _ => {}
//                         }
//                     }
//                     if !base.is_empty() {
//                         return Some(base);
//                     }
//                 }
//             }
//             _ => {}
//         }
//     }
//     None
// }

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

// /// Walk the go-template tree and collect all `.Values` paths (full + leaf).
// pub fn extract_values_paths(
//     tree: &tree_sitter::Tree,
//     src: &str,
// ) -> std::collections::BTreeSet<ValuePath> {
//     let mut paths = std::collections::BTreeSet::<ValuePath>::new();
//     let root = tree.root_node();
//
//     let mut stack = vec![root];
//     while let Some(node) = stack.pop() {
//         // selector chains
//         if node.kind() == "selector_expression" {
//             if let Some(segs) = parse_selector_expression(node, src) {
//                 push_path(&mut paths, &segs);
//             }
//         }
//
//         // index(...) calls
//         if node.kind() == "function_call" {
//             if let Some(segs) = parse_index_call(node, src) {
//                 push_path(&mut paths, &segs);
//             }
//             // for wrappers like tpl/default/nindent/include… traverse args
//             // (we already traverse generically by pushing children below)
//         }
//
//         // generic DFS
//         let mut c = node.walk();
//         for ch in node.children(&mut c) {
//             if ch.is_named() {
//                 stack.push(ch);
//             }
//         }
//     }
//     paths
// }

// --------------------------------------------------------------------------

// /// Extract `.Values` paths from a parsed go-template expression Tree.
// /// This handles:
// ///   - direct selectors: `.Values.foo.bar`
// ///   - index form: `index .Values "foo" "bar"`
// ///   - values passed as args to functions (e.g., `toYaml .Values.x`)
// pub fn extract_values_paths(root: &Tree, source: &str) -> Vec<ValuePath> {
//     let mut out = Vec::new();
//     let mut cursor = root.root_node().walk();
//     visit(&mut cursor, source, &mut out);
//     out.sort();
//     out.dedup();
//     out
// }
//
// fn visit(cur: &mut TreeCursor, source: &str, out: &mut Vec<ValuePath>) {
//     let node = cur.node();
//     match node.kind() {
//         // .Values.foo.bar
//         "selector_expression" => {
//             if let Some(parts) = selector_to_values_path(node, source) {
//                 out.push(ValuePath(parts.join(".")));
//             }
//         }
//         // index .Values "foo" "bar"
//         "function_call" => {
//             if let Some(parts) = index_call_to_values_path(node, source) {
//                 out.push(ValuePath(parts.join(".")));
//             }
//         }
//         _ => {}
//     }
//
//     if cur.goto_first_child() {
//         loop {
//             visit(cur, source, out);
//             if !cur.goto_next_sibling() {
//                 break;
//             }
//         }
//         cur.goto_parent();
//     }
// }
//
// /// Convert a nested selector_expression into a Values path:
// /// (selector_expression
// ///   (selector_expression
// ///     (field ".Values")
// ///     (field_identifier "ingress"))
// ///   (field_identifier "enabled"))
// /// => ["ingress", "enabled"]
// fn selector_to_values_path(node: Node, source: &str) -> Option<Vec<String>> {
//     if node.kind() != "selector_expression" {
//         return None;
//     }
//     let mut parts: Vec<String> = Vec::new();
//     let mut cur = node;
//
//     loop {
//         // selector_expression must have exactly 2 named children:
//         // left: selector_expression | field
//         // right: field_identifier
//         let mut iter = cur.walk();
//         let mut it = cur.named_children(&mut iter);
//         let left = it.next()?;
//         let right = it.next()?;
//         if right.kind() != "field_identifier" {
//             return None;
//         }
//         parts.push(text(source, right).to_string());
//         match left.kind() {
//             "selector_expression" => {
//                 cur = left;
//                 continue;
//             }
//             "field" => {
//                 // Expect .Values as the root
//                 let t = text(source, left);
//                 if t.trim() == ".Values" {
//                     parts.reverse();
//                     return Some(parts);
//                 } else {
//                     return None;
//                 }
//             }
//             _ => return None,
//         }
//     }
// }
//
// /// Convert `function_call` of form `index .Values "a" "b"` into ["a","b"].
// /// Supports interpreted and raw string literals.
// fn index_call_to_values_path(node: Node, source: &str) -> Option<Vec<String>> {
//     if node.kind() != "function_call" {
//         return None;
//     }
//     // (function_call
//     //   (identifier "index")
//     //   (argument_list
//     //     (field ".Values")
//     //     (interpreted_string_literal "\"a\"")
//     //     (raw_string_literal "`b`")))
//     let mut cur = node.walk();
//     let mut it = node.named_children(&mut cur);
//     let ident = it.next()?;
//     if ident.kind() != "identifier" || text(source, ident) != "index" {
//         return None;
//     }
//     let args = it.next()?;
//     if args.kind() != "argument_list" {
//         return None;
//     }
//     let mut cur = args.walk();
//     let mut aiter = args.named_children(&mut cur);
//     let first = aiter.next()?;
//     if first.kind() != "field" || text(source, first).trim() != ".Values" {
//         return None;
//     }
//     let mut parts = Vec::new();
//     for a in aiter {
//         match a.kind() {
//             "interpreted_string_literal" | "raw_string_literal" => {
//                 let seg = unquote(text(source, a));
//                 if !seg.is_empty() {
//                     parts.push(seg.to_string());
//                 }
//             }
//             // Be conservative; bail if non-string arg
//             _ => break,
//         }
//     }
//     if parts.is_empty() { None } else { Some(parts) }
// }
//
// fn text<'a>(src: &'a str, node: Node) -> &'a str {
//     let r = node.byte_range();
//     &src[r]
// }
//
// fn unquote(s: &str) -> &str {
//     s.trim_matches('"').trim_matches('\'').trim_matches('`')
// }
//
// #[cfg(false)]
// mod v1 {
//     fn visit_node(cur: &mut TreeCursor, source: &str, out: &mut Vec<ValuePath>) {
//         let node = cur.node();
//         // Heuristic: collect `field`-like chains and "index" calls
//         // Since node kinds vary across grammar versions, we rely on text checks on leaves.
//         // This keeps us robust to minor grammar changes.
//
//         let kind = node.kind();
//         // dbg!(kind);
//         if kind == "field" || kind == "identifier" || kind == "command" || kind == "pipeline" {
//             // Inspect text for '.Values' or 'index .Values ...'
//             // NOTE: this is textual fallback over the AST node; keeps tree-sitter responsibility for segmentation.
//             let text = node
//                 .utf8_text(source.as_bytes())
//                 //     cur.node().byte_range(),
//                 //     cur.node().tree().source_code().unwrap_or(b""),
//                 // )
//                 .unwrap_or("");
//
//             if text.contains(".Values") {
//                 // collect chained selectors: .Values.foo.bar -> foo.bar
//                 if let Some(idx) = text.find(".Values") {
//                     let tail = &text[idx + ".Values".len()..];
//                     let cleaned = tail.trim();
//                     let path = if cleaned.starts_with('.') {
//                         cleaned
//                             .trim_start_matches('.')
//                             .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
//                             .next()
//                             .unwrap_or("")
//                             .trim_end_matches('.')
//                             .to_string()
//                     } else {
//                         "".into()
//                     };
//                     if !path.is_empty() {
//                         out.push(ValuePath(path));
//                     }
//                 }
//             }
//             if text.starts_with("index ") || text.contains(" index ") {
//                 if let Some(p) = parse_index_chain(text) {
//                     out.push(ValuePath(p));
//                 }
//             }
//         }
//
//         if cur.goto_first_child() {
//             loop {
//                 visit_node(cur, source, out);
//                 if !cur.goto_next_sibling() {
//                     break;
//                 }
//             }
//             cur.goto_parent();
//         }
//     }
//
//     fn parse_index_chain(s: &str) -> Option<String> {
//         // very small parser: index .Values "a" "b" -> a.b ; supports mixed quoting
//         let mut toks = s.split_whitespace();
//         let first = toks.next()?;
//         if first != "index" {
//             return None;
//         }
//         let second = toks.next()?;
//         if second != ".Values" {
//             return None;
//         }
//         let mut parts = Vec::new();
//         for t in toks {
//             let t = t.trim_matches('"').trim_matches('\'');
//             if t.is_empty() || t.starts_with('.') || t.starts_with('|') {
//                 break;
//             }
//             // stop on closing braces heuristically
//             if t.contains('}') {
//                 break;
//             }
//             parts.push(t.to_string());
//         }
//         if parts.is_empty() {
//             None
//         } else {
//             Some(parts.join("."))
//         }
//     }
// }
