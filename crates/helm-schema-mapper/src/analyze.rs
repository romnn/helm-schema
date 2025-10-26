use helm_schema_template::{
    parse::parse_gotmpl_document,
    values::{ValuePath as TmplValuePath, extract_values_paths},
};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use thiserror::Error;
use tree_sitter::Node;
use vfs::VfsPath;

use crate::sanitize::{Placeholder, build_sanitized_with_placeholders};
use crate::yaml_path::{YamlPath, compute_yaml_paths_for_placeholders};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// This template action appears at a mapping/scalar "value position" (after a colon),
    /// so we can place a scalar placeholder and map to a YAML path.
    ScalarValue,
    /// Appears before the colon in a key position (rare). Placeholder not inserted (for now).
    MappingKey,
    /// Control flow or renders YAML fragments (e.g. include/toYaml) — we don't insert a scalar placeholder.
    Fragment,
    /// Used only in `if/with/range` guard; no scalar emission.
    Guard,
    /// Unknown/ambiguous – no placeholder.
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ValueUse {
    pub value_path: String, // e.g., "ingress.pathType"
    pub role: Role,
    pub action_span: Range<usize>, // byte range in original source for the owning template action
    pub yaml_path: Option<YamlPath>, // present if role == ScalarValue and we found a placeholder target
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("VFS: {0}")]
    Vfs(#[from] vfs::VfsError),
    #[error("Parse go-template failed")]
    GtmplParse,
    #[error("Parse YAML failed")]
    YamlParse,
}

/// Heuristic: classify a template action by looking at the original source line content
fn classify_action(src: &str, action_span: &Range<usize>) -> Role {
    // Get line slice
    let start = action_span.start;
    let line_start = src[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = src[start..]
        .find('\n')
        .map(|i| start + i)
        .unwrap_or(src.len());
    let line = &src[line_start..line_end];

    // position of action within the line
    let rel = start - line_start;

    // If there's a ':' before action and no ':' after -> value position
    if let Some(_) = line[..rel].rfind(':') {
        if line[rel..].find(':').is_none() {
            return Role::ScalarValue;
        }
    }

    // If action appears before ':' on the same line -> key position
    if let Some(_) = line[rel..].find(':') {
        return Role::MappingKey;
    }

    // Control flow keywords at start of action often not scalar
    let snippet = &src[action_span.clone()];
    let is_ctrl = ["if ", "with ", "range ", "end", "else"]
        .iter()
        .any(|kw| snippet.contains(kw));
    if is_ctrl {
        return Role::Guard;
    }

    Role::Unknown
}

fn root_children_in_order<'tree>(tree: &'tree tree_sitter::Tree) -> Vec<Node<'tree>> {
    let root = tree.root_node();
    let mut c = root.walk();
    root.children(&mut c).collect()
}

/// Find the smallest *top-level* template child that contains a node range.
/// Top-level means immediate child of the root `template` node that is not a `text` node.
fn owning_action_child<'tree>(
    tree: &'tree tree_sitter::Tree,
    byte_range: Range<usize>,
) -> Option<Node<'tree>> {
    for ch in root_children_in_order(tree) {
        if ch.kind() == "text" {
            continue;
        }
        let r = ch.byte_range();
        if r.start <= byte_range.start && byte_range.end <= r.end {
            return Some(ch);
        }
    }
    None
}

/// Analyze a *single* template file from VFS.
pub fn analyze_template_file(path: &VfsPath) -> Result<Vec<ValueUse>, Error> {
    let source = path.read_to_string()?;

    // Parse whole document as go-template
    let parsed = parse_gotmpl_document(&source).ok_or(Error::GtmplParse)?;
    let tree = &parsed.tree;
    let root = tree.root_node();

    let ast = helm_schema_template::fmt::SExpr::parse_tree(&root, &source);
    println!("{}", ast.to_string_pretty());

    // Index defines in same directory and compute closure
    let defines = index_defines_in_dir(&path.parent())?;
    let define_closure = compute_define_closure(&defines);

    // // Wrapped collector that augments include() nodes with define-closure values
    // let collect = |n: &Node| -> Vec<String> {
    //     let mut set: BTreeSet<String> = collect_values_in_subtree(n, &source).into_iter().collect();
    //
    //     if n.kind() == "function_call" {
    //         if let Some(name) = include_call_name(n, &source) {
    //             if let Some(vals) = define_closure.get(&name) {
    //                 set.extend(vals.iter().cloned());
    //             }
    //         }
    //     }
    //     set.into_iter().collect()
    // };

    // Wrapped collector that augments include() found anywhere in the subtree.
    let collect = |n: &Node| -> Vec<String> {
        // Start with the regular .Values found under this subtree
        let mut set: BTreeSet<String> = collect_values_in_subtree(n, &source).into_iter().collect();

        // NEW: scan the subtree for any include() and merge its define-closure values
        let mut stack = vec![*n];
        while let Some(q) = stack.pop() {
            if q.kind() == "function_call" {
                if let Some(name) = include_call_name(&q, &source) {
                    if let Some(vals) = define_closure.get(&name) {
                        set.extend(vals.iter().cloned());
                    }
                }
            }
            let mut c = q.walk();
            for ch in q.children(&mut c) {
                if ch.is_named() {
                    stack.push(ch);
                }
            }
        }

        set.into_iter().collect()
    };

    // Build sanitized YAML with placeholders (roles inferred later from YAML structure)
    let mut placeholders: Vec<crate::sanitize::Placeholder> = Vec::new();
    let sanitized = build_sanitized_with_placeholders(&source, tree, &mut placeholders, collect);

    // Map placeholders back to YAML paths (+ roles)
    let ph_to_yaml =
        compute_yaml_paths_for_placeholders(&sanitized).map_err(|_| Error::YamlParse)?;

    // Collect output uses
    let mut out = Vec::new();
    for ph in placeholders {
        let role = ph.role.clone();
        let yaml_path = ph_to_yaml.get(&ph.id).and_then(|b| b.path.clone()).clone();
        for v in ph.values {
            out.push(ValueUse {
                value_path: v,
                role: ph_to_yaml
                    .get(&ph.id)
                    .map(|b| b.role.clone())
                    .unwrap_or(role.clone()),
                action_span: ph.action_span.clone(),
                yaml_path: yaml_path.clone(),
            });
        }
    }

    Ok(out)
}

fn collect_values_in_subtree(node: &Node, src: &str) -> Vec<String> {
    // NOTE: we capture a global/static? no; we’ll override this via a closure in analyze_template_file (see below)
    // This plain version remains here; the closure will wrap it to add include-closure values.
    let mut out = BTreeSet::<String>::new();
    let mut stack = vec![*node];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "selector_expression" => {
                let parent_is_selector = n
                    .parent()
                    .map(|p| p.kind() == "selector_expression")
                    .unwrap_or(false);
                if !parent_is_selector {
                    if let Some(segs) =
                        helm_schema_template::values::parse_selector_expression(&n, src)
                    {
                        if !segs.is_empty() {
                            out.insert(segs.join("."));
                        }
                    }
                }
            }
            "function_call" => {
                if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
                    if !segs.is_empty() {
                        out.insert(segs.join("."));
                    }
                }
                // NOTE: include-closure injection is handled by the wrapper closure in analyze_template_file
            }
            _ => {}
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }
    out.into_iter().collect()
}

// ADD (small util) somewhere private in this module:
fn function_name_of(node: &Node, src: &str) -> Option<String> {
    if node.kind() != "function_call" {
        return None;
    }
    let f = node.child_by_field_name("function")?;
    Some(f.utf8_text(src.as_bytes()).ok()?.to_string())
}

// Info we index for every {{ define "name" }}...{{ end }}
#[derive(Debug, Clone, Default)]
pub struct DefineInfo {
    pub values: BTreeSet<String>,   // .Values.* used inside this define body
    pub includes: BTreeSet<String>, // nested {{ include "..." ... }} calls
}

// Best-effort string unquote ( "foo", `foo` )
fn unquote(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('`') && t.ends_with('`')) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

// Find include name if this is {{ include "name" ... }}
fn include_call_name(node: &Node, src: &str) -> Option<String> {
    if node.kind() != "function_call" {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    if !(func.kind() == "identifier" && func.utf8_text(src.as_bytes()).ok()? == "include") {
        return None;
    }
    let args = node.child_by_field_name("arguments")?;
    let mut c = args.walk();
    for ch in args.named_children(&mut c) {
        // first string literal is the template name
        if ch.kind() == "interpreted_string_literal" || ch.kind() == "raw_string_literal" {
            let name = unquote(&src[ch.byte_range()]);
            return Some(name);
        }
    }
    None
}

// Get define name for {{ define "name" }} body node
fn define_name(node: &Node, src: &str) -> Option<String> {
    if node.kind() != "define_action" {
        return None;
    }
    if let Some(n) = node.child_by_field_name("name") {
        return Some(unquote(&src[n.byte_range()]));
    }
    // fallback: first string literal child
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if !ch.is_named() {
            continue;
        }
        if ch.kind() == "interpreted_string_literal" || ch.kind() == "raw_string_literal" {
            return Some(unquote(&src[ch.byte_range()]));
        }
    }
    None
}

// Collect .Values + nested include names inside a define’s subtree
fn collect_define_info(node: &Node, src: &str) -> DefineInfo {
    let mut values = BTreeSet::<String>::new();
    let mut includes = BTreeSet::<String>::new();

    let root_id = node.id(); // <-- NEW: remember which define we were called with
    let mut stack = vec![*node];

    while let Some(n) = stack.pop() {
        match n.kind() {
            "selector_expression" => {
                // OUTERMOST selector only
                let parent_is_selector = n
                    .parent()
                    .map(|p| p.kind() == "selector_expression")
                    .unwrap_or(false);
                if !parent_is_selector {
                    if let Some(segs) =
                        helm_schema_template::values::parse_selector_expression(&n, src)
                    {
                        if !segs.is_empty() {
                            values.insert(segs.join("."));
                        }
                    }
                }
            }
            "function_call" => {
                if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
                    if !segs.is_empty() {
                        values.insert(segs.join("."));
                    }
                }
                if let Some(name) = include_call_name(&n, src) {
                    includes.insert(name);
                }
            }
            "define_action" => {
                // If this is a nested define (not the root define), do not recurse into it.
                if n.id() != root_id {
                    continue;
                }
                // otherwise (the root define) fall through to push its children below
            }
            _ => {}
        }

        let mut c = n.walk();
        for ch in n.children(&mut c) {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }

    DefineInfo { values, includes }
}

// fn collect_define_info(node: &Node, src: &str) -> DefineInfo {
//     let mut values = BTreeSet::<String>::new();
//     let mut includes = BTreeSet::<String>::new();
//
//     let mut stack = vec![*node];
//     while let Some(n) = stack.pop() {
//         match n.kind() {
//             "selector_expression" => {
//                 // OUTERMOST selector only
//                 let parent_is_selector = n
//                     .parent()
//                     .map(|p| p.kind() == "selector_expression")
//                     .unwrap_or(false);
//                 if !parent_is_selector {
//                     if let Some(segs) =
//                         helm_schema_template::values::parse_selector_expression(&n, src)
//                     {
//                         if !segs.is_empty() {
//                             values.insert(segs.join("."));
//                         }
//                     }
//                 }
//             }
//             "function_call" => {
//                 if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
//                     if !segs.is_empty() {
//                         values.insert(segs.join("."));
//                     }
//                 }
//                 if let Some(name) = include_call_name(&n, src) {
//                     includes.insert(name);
//                 }
//             }
//             "define_action" => {
//                 // don’t recurse into nested defines
//                 continue;
//             }
//             _ => {}
//         }
//         let mut c = n.walk();
//         for ch in n.children(&mut c) {
//             if ch.is_named() {
//                 stack.push(ch);
//             }
//         }
//     }
//
//     DefineInfo { values, includes }
// }

// Index all defines in a directory (e.g., templates/_*.tpl)
pub fn index_defines_in_dir(dir: &VfsPath) -> Result<BTreeMap<String, DefineInfo>, Error> {
    let mut out = BTreeMap::<String, DefineInfo>::new();
    for path in dir.read_dir()? {
        let name = path.filename();
        // keep it simple: process *.tpl only
        if !name.ends_with(".tpl") {
            continue;
        }
        dbg!(&path);
        let src = path.read_to_string()?;
        if let Some(parsed) = helm_schema_template::parse::parse_gotmpl_document(&src) {
            let root = parsed.tree.root_node();
            let mut stack = vec![root];
            while let Some(n) = stack.pop() {
                if n.kind() == "define_action" {
                    if let Some(define_name) = define_name(&n, &src) {
                        let info = collect_define_info(&n, &src);
                        out.entry(define_name.clone())
                            .or_default()
                            .values
                            .extend(info.values);
                        out.entry(define_name)
                            .or_default()
                            .includes
                            .extend(info.includes);
                    }
                    // don’t recurse into define bodies again; continue
                    continue;
                }
                let mut c = n.walk();
                for ch in n.children(&mut c) {
                    if ch.is_named() {
                        stack.push(ch);
                    }
                }
            }
        }
    }
    Ok(out)
}

// Transitive closure of .Values per define (follows nested includes)
pub fn compute_define_closure(
    defs: &BTreeMap<String, DefineInfo>,
) -> BTreeMap<String, BTreeSet<String>> {
    fn dfs<'a>(
        name: &str,
        defs: &'a BTreeMap<String, DefineInfo>,
        seen: &mut BTreeSet<String>,
        memo: &mut BTreeMap<String, BTreeSet<String>>,
    ) -> BTreeSet<String> {
        if let Some(m) = memo.get(name) {
            return m.clone();
        }
        if !seen.insert(name.to_string()) {
            return BTreeSet::new();
        }

        let mut acc = BTreeSet::<String>::new();
        if let Some(info) = defs.get(name) {
            acc.extend(info.values.clone());
            for inc in &info.includes {
                acc.extend(dfs(inc, defs, seen, memo));
            }
        }
        memo.insert(name.to_string(), acc.clone());
        acc
    }

    let mut memo = BTreeMap::<String, BTreeSet<String>>::new();
    for key in defs.keys() {
        let mut seen = BTreeSet::new();
        let _ = dfs(key, defs, &mut seen, &mut memo);
    }
    memo
}
