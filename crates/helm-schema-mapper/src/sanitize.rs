use crate::analyze::Role;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

/// Record of a placeholder we inserted while sanitizing
#[derive(Debug, Clone)]
pub struct Placeholder {
    pub id: usize,
    pub role: Role,
    pub action_span: Range<usize>,
    pub values: Vec<String>, // the .Values paths that live under this action
}

fn is_control_flow(kind: &str) -> bool {
    matches!(
        kind,
        "if_action" | "with_action" | "range_action" | "else_clause"
    )
}

// Template root is also a container we must descend into
fn is_container(kind: &str) -> bool {
    matches!(kind, "template")
}

// Only these node kinds should be replaced by a single placeholder
fn is_output_expr_kind(kind: &str) -> bool {
    matches!(
        kind,
        "selector_expression" | "function_call" | "chained_pipeline"
    )
}

/// Is `node` a guard child inside a control-flow action? (i.e. appears before the first text body)
fn is_guard_child(node: &tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if !is_control_flow(parent.kind()) {
        return false;
    }

    // find first named `text` child in the control-flow node
    let mut c = parent.walk();
    let mut first_text_start: Option<usize> = None;
    for ch in parent.children(&mut c) {
        if ch.is_named() && ch.kind() == "text" {
            first_text_start = Some(ch.start_byte());
            break;
        }
    }
    match first_text_start {
        Some(body_start) => node.end_byte() <= body_start,
        None => true, // no text body at all → everything is guard-ish
    }
}

pub fn build_sanitized_with_placeholders(
    src: &str,
    gtree: &tree_sitter::Tree,
    out_placeholders: &mut Vec<Placeholder>,
    collect_values: impl Fn(&tree_sitter::Node) -> Vec<String> + Copy,
) -> String {
    let mut next_id = 0usize;

    fn emit_placeholder_for(
        node: tree_sitter::Node,
        src: &str,
        buf: &mut String,
        next_id: &mut usize,
        out: &mut Vec<Placeholder>,
        collect_values: impl Fn(&tree_sitter::Node) -> Vec<String> + Copy,
    ) {
        let id = *next_id;
        *next_id += 1;
        buf.push('"');
        buf.push_str(&format!("__TSG_PLACEHOLDER_{}__", id));
        buf.push('"');

        let values = collect_values(&node);
        out.push(Placeholder {
            id,
            role: Role::Unknown, // decided later from YAML AST
            action_span: node.byte_range(),
            values,
        });
    }

    fn walk(
        node: tree_sitter::Node,
        src: &str,
        buf: &mut String,
        next_id: &mut usize,
        out: &mut Vec<Placeholder>,
        collect_values: impl Fn(&tree_sitter::Node) -> Vec<String> + Copy,
    ) {
        // 1) pass through raw text
        if node.kind() == "text" {
            buf.push_str(&src[node.byte_range()]);
            return;
        }

        // 2) containers: descend into their DIRECT children only
        if is_container(node.kind()) || is_control_flow(node.kind()) {
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if !ch.is_named() {
                    continue;
                }
                if ch.kind() == "text" {
                    buf.push_str(&src[ch.byte_range()]);
                    continue;
                }

                // Guard nodes inside control-flow are skipped entirely
                if is_control_flow(node.kind()) && is_guard_child(&ch) {
                    continue;
                }

                if is_control_flow(ch.kind()) || is_container(ch.kind()) {
                    // Nested container (e.g., else_clause); recurse
                    walk(ch, src, buf, next_id, out, collect_values);
                } else if is_output_expr_kind(ch.kind()) {
                    // single placeholder for this direct child expression
                    emit_placeholder_for(ch, src, buf, next_id, out, collect_values);
                } else {
                    // everything else (e.g., ERROR, whitespace trivia): pass through source bytes
                    buf.push_str(&src[ch.byte_range()]);
                }
                //     // EXPRESSION at output position → single placeholder
                //     emit_placeholder_for(ch, src, buf, next_id, out, collect_values);
                // }
            }
            return;
        }

        // 3) non-container reached (shouldn’t happen for well-formed trees, but safe fallback)
        if is_output_expr_kind(node.kind()) {
            emit_placeholder_for(node, src, buf, next_id, out, collect_values);
        } else {
            // trivia/unknown — pass through
            buf.push_str(&src[node.byte_range()]);
        }
    }

    let mut out = String::new();
    walk(
        gtree.root_node(),
        src,
        &mut out,
        &mut next_id,
        out_placeholders,
        collect_values,
    );
    out
}

// pub fn build_sanitized_with_placeholders(
//     src: &str,
//     gtree: &tree_sitter::Tree,
//     out_placeholders: &mut Vec<Placeholder>,
//     collect_values: impl Fn(&tree_sitter::Node) -> Vec<String> + Copy,
// ) -> String {
//     let mut next_id = 0usize;
//
//     fn walk(
//         node: tree_sitter::Node,
//         src: &str,
//         buf: &mut String,
//         next_id: &mut usize,
//         out: &mut Vec<Placeholder>,
//         collect_values: impl Fn(&tree_sitter::Node) -> Vec<String> + Copy,
//     ) {
//         if node.kind() == "text" {
//             buf.push_str(&src[node.byte_range()]);
//             return;
//         }
//
//         // Always recurse into control-flow containers; skip guard children
//         if is_container(node.kind()) || is_control_flow(node.kind()) {
//             let mut c = node.walk();
//             for ch in node.children(&mut c) {
//                 if !ch.is_named() {
//                     continue;
//                 }
//                 if is_control_flow(node.kind()) && is_guard_child(&ch) {
//                     // skip guard expressions
//                     continue;
//                 }
//                 walk(ch, src, buf, next_id, out, collect_values);
//             }
//             return;
//         }
//
//         // For every other non-text node (selector, function_call, chained_pipeline, …),
//         // insert a quoted scalar placeholder and record its values.
//         let id = *next_id;
//         *next_id += 1;
//         buf.push('"');
//         buf.push_str(&format!("__TSG_PLACEHOLDER_{}__", id));
//         buf.push('"');
//
//         let values = collect_values(&node);
//         out.push(Placeholder {
//             id,
//             role: Role::Unknown, // filled later from YAML AST
//             action_span: node.byte_range(),
//             values,
//         });
//     }
//
//     let mut out = String::new();
//     walk(
//         gtree.root_node(),
//         src,
//         &mut out,
//         &mut next_id,
//         out_placeholders,
//         collect_values,
//     );
//     out
// }

// if {
//     let mut c = node.walk();
//     for ch in node.children(&mut c) {
//         if ch.is_named() {
//             if is_guard_child(&ch) {
//                 // don't emit anything; also don't record a use for guards
//                 continue;
//             }
//             walk(ch, src, buf, next_id, out, collect_values);
//         }
//     }
//     return;
// }

#[cfg(false)]
pub mod v1 {
    pub fn build_sanitized_with_placeholders(
        src: &str,
        gtree: &tree_sitter::Tree,
        out_placeholders: &mut Vec<Placeholder>,
        classify: impl Fn(&str, &tree_sitter::Node) -> Role + Copy,
        collect_values: impl Fn(&tree_sitter::Node) -> Vec<String> + Copy,
    ) -> String {
        let mut next_id = 0usize;

        fn walk(
            node: tree_sitter::Node,
            src: &str,
            buf: &mut String,
            next_id: &mut usize,
            out: &mut Vec<Placeholder>,
            classify: impl Fn(&str, &tree_sitter::Node) -> Role + Copy,
            collect_values: impl Fn(&tree_sitter::Node) -> Vec<String> + Copy,
        ) {
            if node.kind() == "text" {
                buf.push_str(&src[node.byte_range()]);
                return;
            }

            // For any non-text node, check if it's in a scalar value position.
            let role = classify(src, &node);
            dbg!(node.utf8_text(src.as_bytes()), &role);

            match role {
                Role::ScalarValue => {
                    let id = *next_id;
                    *next_id += 1;
                    buf.push('"');
                    buf.push_str(&format!("__TSG_PLACEHOLDER_{}__", id));
                    buf.push('"');

                    let values = collect_values(&node);
                    out.push(Placeholder {
                        id,
                        role: Role::ScalarValue,
                        action_span: node.byte_range(),
                        values,
                    });
                    return; // we don't descend further; the action is replaced as a whole
                }
                Role::MappingKey => {
                    // DO NOT insert a placeholder (we’re not mapping keys yet).
                    // Just record the usage so tests can see Role::MappingKey.
                    let id = *next_id;
                    *next_id += 1;

                    let values = collect_values(&node);
                    out.push(Placeholder {
                        id,
                        role: Role::MappingKey,
                        action_span: node.byte_range(),
                        values,
                    });
                    return; // drop the action text for sanitized YAML
                }
                _ => {
                    // Descend and aggregate children (e.g., inside if/with/range/include)
                    let mut c = node.walk();
                    for ch in node.children(&mut c) {
                        if ch.is_named() {
                            walk(ch, src, buf, next_id, out, classify, collect_values);
                        }
                    }
                }
            };

            // // Otherwise, descend and emit whatever the children contribute.
            // let mut c = node.walk();
            // for ch in node.children(&mut c) {
            //     if ch.is_named() {
            //         walk(ch, src, buf, next_id, out, classify, collect_values);
            //     }
            // }
        }

        let mut out = String::new();
        let root = gtree.root_node();
        walk(
            root,
            src,
            &mut out,
            &mut next_id,
            out_placeholders,
            classify,
            collect_values,
        );
        out
    }
}

// /// Build sanitized YAML from the go-template tree by:
// ///  - appending `text` nodes verbatim,
// ///  - for each non-text top-level action: insert a scalar placeholder **if** classifier says ScalarValue.
// ///    Otherwise, insert nothing (effectively removing the action).
// ///
// /// `per_action` groups action-node-id -> (node, set_of_value_paths)
// pub fn build_sanitized_with_placeholders(
//     src: &str,
//     gtree: &tree_sitter::Tree,
//     per_action: &BTreeMap<usize, (tree_sitter::Node, BTreeSet<String>)>,
//     out_placeholders: &mut Vec<Placeholder>,
//     classify: impl Fn(&str, &tree_sitter::Node) -> Role,
// ) -> String {
//     let mut out = String::new();
//     let root = gtree.root_node();
//     let mut children = {
//         let mut c = root.walk();
//         root.children(&mut c).collect::<Vec<_>>()
//     };
//
//     let mut next_id = 0usize;
//
//     for ch in children {
//         if ch.kind() == "text" {
//             let r = ch.byte_range();
//             out.push_str(&src[r]);
//             continue;
//         }
//
//         // This is a top-level template action (if/with/range/function etc.)
//         let role = classify(src, &ch);
//         let (values, span) = per_action
//             .get(&ch.id())
//             .map(|(_, set)| (set.iter().cloned().collect::<Vec<_>>(), ch.byte_range()))
//             .unwrap_or((Vec::new(), ch.byte_range()));
//
//         match role {
//             Role::ScalarValue => {
//                 // Insert a quoted scalar placeholder; always valid YAML in value position.
//                 let id = {
//                     let i = next_id;
//                     next_id += 1;
//                     i
//                 };
//                 let token = format!("\"__TSG_PLACEHOLDER_{}__\"", id);
//                 out.push_str(&token);
//                 out_placeholders.push(Placeholder {
//                     id,
//                     role: Role::ScalarValue,
//                     action_span: span.clone(),
//                     values,
//                 });
//             }
//             _ => {
//                 // Drop the action; it’s control flow, key position, or fragment.
//                 // The surrounding YAML (concat of text) remains parseable.
//             }
//         }
//     }
//
//     out
// }
