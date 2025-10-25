use std::ops::Range;
use thiserror::Error;
use tree_sitter::Node;
use vfs::VfsPath;

use helm_schema_template::{
    parse::parse_gotmpl_document,
    values::{ValuePath as TmplValuePath, extract_values_paths},
};

use crate::sanitize::{Placeholder, build_sanitized_with_placeholders};
use crate::yaml_path::{YamlPath, compute_yaml_paths_for_placeholders};

#[derive(Debug, Clone, PartialEq, Eq)]
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

    // // (4) functions that usually emit YAML fragments
    // if snippet.contains("toYaml") || snippet.contains("include ") || snippet.contains("tpl ") {
    //     // could still be scalar, but often multi-line; keep as fragment for now
    //     return Role::Fragment;
    // }

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

    // Collect .Values paths + *which action child* they live under
    // (we do one more pass to map each selector/index occurrence to its top-level action child)
    let mut occurrences: Vec<(String, Node)> = Vec::new();
    {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == "selector_expression" || node.kind() == "function_call" {
                // Reuse template extractor to decide if this node contributes a .Values path
                if node.kind() == "selector_expression" {
                    if let Some(segs) =
                        helm_schema_template::values::parse_selector_expression(&node, &source)
                    {
                        let full = segs.join(".");
                        if let Some(owner) = owning_action_child(tree, node.byte_range()) {
                            occurrences.push((full, owner));
                        }
                    }
                } else if node.kind() == "function_call" {
                    if let Some(segs) =
                        helm_schema_template::values::parse_index_call(&node, &source)
                    {
                        let full = segs.join(".");
                        if let Some(owner) = owning_action_child(tree, node.byte_range()) {
                            occurrences.push((full, owner));
                        }
                    }
                }
            }
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() {
                    stack.push(ch);
                }
            }
        }
    }

    // Group by owning action
    use std::collections::{BTreeMap, BTreeSet};
    let mut per_action: BTreeMap<usize, (Node, BTreeSet<String>)> = BTreeMap::new();
    for (vp, act) in occurrences {
        per_action
            .entry(act.id())
            .or_insert_with(|| (act, BTreeSet::new()))
            .1
            .insert(vp);
    }

    // (2) Recursively sanitize: insert placeholders at *any* action in value position,
    //     and attach .Values paths collected from that action's subtree.
    let mut placeholders: Vec<crate::sanitize::Placeholder> = Vec::new();
    let sanitized = crate::sanitize::build_sanitized_with_placeholders(
        &source,
        tree,
        &mut placeholders,
        // |src, act| classify_action(&source, &act.byte_range()),
        |act| collect_values_in_subtree(act, &source),
    );

    println!("SANITIZED YAML:\n\n{sanitized}");

    // Let YAML AST decide roles and paths
    let bindings = compute_yaml_paths_for_placeholders(&sanitized).map_err(|_| Error::YamlParse)?;

    // // (4) parse YAML and map placeholders to YAML paths
    // let ph_to_yaml =
    //     compute_yaml_paths_for_placeholders(&sanitized).map_err(|_| Error::YamlParse)?;

    let mut out = Vec::new();
    for ph in placeholders {
        let binding = bindings.get(&ph.id).cloned().unwrap_or_default();
        // let Binding { role, path } = bindings.get(&ph.id).cloned().unwrap_or(Binding {
        //     role: Role::Unknown,
        //     path: None,
        // });
        for v in ph.values {
            out.push(ValueUse {
                value_path: v,
                role: binding.role.clone(),
                action_span: ph.action_span.clone(),
                yaml_path: binding.path.clone(),
            });
        }
    }

    // // (5) collect ValueUses
    // let mut out = Vec::new();
    // for ph in placeholders {
    //     let role = ph.role.clone();
    //     let yaml_path = ph_to_yaml.get(&ph.id).cloned();
    //     for v in ph.values {
    //         out.push(ValueUse {
    //             value_path: v,
    //             role: role.clone(),
    //             action_span: ph.action_span.clone(),
    //             yaml_path: yaml_path.clone(),
    //         });
    //     }
    // }

    Ok(out)
}

fn collect_values_in_subtree(node: &Node, src: &str) -> Vec<String> {
    let mut out = std::collections::BTreeSet::<String>::new();
    let mut stack = vec![*node];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "selector_expression" if helm_schema_template::values::is_terminal_selector(&n) => {
                if let Some(segs) = helm_schema_template::values::parse_selector_expression(&n, src)
                {
                    if !segs.is_empty() {
                        out.insert(segs.join("."));
                    }
                }
            }
            "function_call" => {
                if let Some(segs) = helm_schema_template::values::parse_index_call(&n, src) {
                    if !segs.is_empty() {
                        out.insert(segs.join("."));
                    }
                }
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

// // Build sanitized YAML, inserting placeholders ONLY for scalar-value actions
// let mut placeholders: Vec<Placeholder> = Vec::new();
// let sanitized = build_sanitized_with_placeholders(
//     &source,
//     &tree,
//     &per_action,
//     &mut placeholders,
//     |src, act| classify_action(src, &act.byte_range()),
// );
//
// // Parse YAML and map placeholders to YAML paths
// let ph_to_yaml =
//     compute_yaml_paths_for_placeholders(&sanitized).map_err(|_| Error::YamlParse)?;
//
// // Collect ValueUses
// let mut out = Vec::new();
// for ph in placeholders {
//     let role = ph.role.clone();
//     let yaml_path = ph_to_yaml.get(&ph.id);
//     for v in ph.values {
//         out.push(ValueUse {
//             value_path: v,
//             role: role.clone(),
//             action_span: ph.action_span.clone(),
//             yaml_path: yaml_path.cloned(),
//         });
//     }
// }
