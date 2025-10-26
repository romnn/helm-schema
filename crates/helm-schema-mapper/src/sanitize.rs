use crate::analyze::Role;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use tree_sitter::Node;

/// Record of a placeholder we inserted while sanitizing
#[derive(Debug, Clone)]
pub struct Placeholder {
    pub id: usize,
    pub role: Role,
    pub action_span: Range<usize>,
    pub values: Vec<String>, // the .Values paths that live under this action
    /// True when this placeholder comes from a fragment-producing expression
    /// like `include` (rendering YAML) or `toYaml ... | nindent`.
    pub is_fragment_output: bool,
}

fn is_control_flow(kind: &str) -> bool {
    matches!(
        kind,
        "if_action" | "with_action" | "range_action" | "else_clause"
    )
}

// Template root is also a container we must descend into
fn is_container(kind: &str) -> bool {
    matches!(kind, "template" | "define_action")
}

// Only these node kinds should be replaced by a single placeholder
fn is_output_expr_kind(kind: &str) -> bool {
    matches!(
        kind,
        "selector_expression" | "function_call" | "chained_pipeline"
    )
}

// Assignment actions shouldn't render; we only record their uses.
fn is_assignment_kind(kind: &str) -> bool {
    matches!(
        kind,
        "short_variable_declaration"
            | "variable_declaration"
            | "assignment"
            | "variable_definition"
    )
    // matches!(
    //     kind,
    //     "short_variable_declaration" | "variable_declaration" | "assignment"
    // )
}

fn function_name<'a>(node: &tree_sitter::Node<'a>, src: &str) -> Option<String> {
    if node.kind() != "function_call" {
        return None;
    }
    let f = node.child_by_field_name("function")?;
    Some(f.utf8_text(src.as_bytes()).ok()?.to_string())
}

fn looks_like_fragment_output(node: &tree_sitter::Node, src: &str) -> bool {
    match node.kind() {
        "function_call" => {
            if let Some(name) = function_name(node, src) {
                // These usually render mappings/sequences, not scalars
                return name == "include" || name == "toYaml";
            }
            false
        }
        "chained_pipeline" => {
            // If any function in the chain is a known fragment producer, treat as fragment
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() && ch.kind() == "function_call" {
                    if let Some(name) = function_name(&ch, src) {
                        if name == "include" || name == "toYaml" {
                            return true;
                        }
                    }
                }
            }
            false
        }
        _ => false,
    }
}

fn is_assignment_node(node: &Node, src: &str) -> bool {
    if is_assignment_kind(node.kind()) {
        return true;
    }
    // fallback: grammar differences — detect ':=' in the action text
    let t = &src[node.byte_range()];
    t.contains(":=")
}

/// Is `node` a guard child inside a control-flow action? (i.e. appears before the first text body)
fn is_guard_child(node: &Node) -> bool {
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
    collect_values: impl Fn(&Node) -> Vec<String> + Copy,
) -> String {
    let mut next_id = 0usize;

    fn emit_placeholder_for(
        node: tree_sitter::Node,
        src: &str,
        buf: &mut String,
        next_id: &mut usize,
        out: &mut Vec<Placeholder>,
        collect_values: impl Fn(&Node) -> Vec<String> + Copy,
        is_fragment_output: bool,
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
            is_fragment_output,
        });
    }

    fn walk(
        node: Node,
        src: &str,
        buf: &mut String,
        next_id: &mut usize,
        out: &mut Vec<Placeholder>,
        collect_values: impl Fn(&Node) -> Vec<String> + Copy,
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
                let parent_is_define = node.kind() == "define_action";

                if !ch.is_named() {
                    continue;
                }
                if ch.kind() == "text" {
                    // Do NOT emit raw text from define bodies — keeps sanitized YAML valid
                    if !parent_is_define {
                        buf.push_str(&src[ch.byte_range()]);
                    }
                    continue;
                }

                // RECORD guards (no YAML emission)
                if is_control_flow(node.kind()) && is_guard_child(&ch) {
                    let id = *next_id;
                    *next_id += 1;
                    let values = collect_values(&ch);
                    out.push(Placeholder {
                        id,
                        role: Role::Guard,
                        action_span: ch.byte_range(),
                        values,
                        is_fragment_output: false,
                    });
                    continue;
                }

                if is_control_flow(ch.kind()) || is_container(ch.kind()) {
                    // Nested container (e.g., else_clause); recurse
                    walk(ch, src, buf, next_id, out, collect_values);
                } else if is_output_expr_kind(ch.kind()) {
                    // single placeholder for this direct child expression
                    let frag = looks_like_fragment_output(&ch, src);
                    if parent_is_define {
                        // In define bodies: record the use, but DO NOT write to buf
                        let id = *next_id;
                        *next_id += 1;
                        let values = collect_values(&ch);
                        out.push(Placeholder {
                            id,
                            role: Role::Fragment, // define content has no concrete YAML site
                            action_span: ch.byte_range(),
                            values,
                            is_fragment_output: frag,
                        });
                    } else {
                        // Normal template files: emit placeholder into YAML
                        emit_placeholder_for(ch, src, buf, next_id, out, collect_values, frag);
                    }
                    // emit_placeholder_for(ch, src, buf, next_id, out, collect_values, frag);
                    // emit_placeholder_for(ch, src, buf, next_id, out, collect_values);
                } else if ch.kind() == "ERROR" {
                    // skip ERROR nodes (often whitespace artifacts) — do not emit
                    continue;
                } else if is_assignment_node(&ch, src) {
                    // RECORD assignment uses (no YAML emission)
                    let id = *next_id;
                    *next_id += 1;
                    let values = collect_values(&ch);
                    out.push(Placeholder {
                        id,
                        role: Role::Fragment, // records a use; no concrete YAML location
                        action_span: ch.byte_range(),
                        values,
                        is_fragment_output: false,
                    });
                    continue;
                } else {
                    // Unknown non-output node at container level — skip to keep YAML valid.
                    continue;
                }
                // else {
                //     // everything else (e.g., ERROR, whitespace trivia): pass through source bytes
                //     buf.push_str(&src[ch.byte_range()]);
                // }
                //     // EXPRESSION at output position → single placeholder
                //     emit_placeholder_for(ch, src, buf, next_id, out, collect_values);
                // }
            }
            return;
        }

        // 3) non-container reached (shouldn’t happen for well-formed trees, but safe fallback)
        if is_output_expr_kind(node.kind()) {
            let frag = looks_like_fragment_output(&node, src);
            emit_placeholder_for(node, src, buf, next_id, out, collect_values, frag);
        } else {
            unreachable!("should not happen?");
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
