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

pub(crate) fn is_control_flow(kind: &str) -> bool {
    matches!(
        kind,
        "if_action" | "with_action" | "range_action" | "else_clause"
    )
}

// Template root is also a container we must descend into
pub(crate) fn is_container(kind: &str) -> bool {
    matches!(kind, "template" | "define_action")
}

// Only these node kinds should be replaced by a single placeholder
fn is_output_expr_kind(kind: &str) -> bool {
    matches!(
        kind,
        "selector_expression" | "function_call" | "chained_pipeline" | "variable" | "dot"
    )
}

// Assignment actions shouldn't render; we only record their uses.
pub(crate) fn is_assignment_kind(kind: &str) -> bool {
    matches!(
        kind,
        "short_variable_declaration"
            | "variable_declaration"
            | "assignment"
            | "variable_definition"
    )
}

fn variable_ident(node: &Node, src: &str) -> Option<String> {
    if node.kind() != "variable" {
        return None;
    }
    // prefer identifier child if present
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.is_named() && ch.kind() == "identifier" {
            return ch.utf8_text(src.as_bytes()).ok().map(|s| s.to_string());
        }
    }
    // fallback: strip leading '$'
    node.utf8_text(src.as_bytes())
        .ok()
        .map(|t| t.trim().trim_start_matches('$').to_string())
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
                // return matches!(name.as_str(), "toYaml");
                return name == "include" || name == "toYaml";
            }
        }
        "chained_pipeline" => {
            // If any function in the chain is a known fragment producer, treat as fragment
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.is_named() && ch.kind() == "function_call" {
                    if let Some(name) = function_name(&ch, src) {
                        if name == "include" || name == "toYaml" {
                            // if matches!(name.as_str(), "toYaml") {
                            return true;
                        }
                    }
                }
            }
        }
        _ => {}
    };
    false
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

pub fn validate_yaml_strict_all_docs(src: &str) -> Result<(), serde_yaml::Error> {
    use serde::de::Deserialize;
    let mut de = serde_yaml::Deserializer::from_str(src);
    // Deserialize the whole stream (YAML can contain multiple documents)
    while let Some(doc) = de.next() {
        serde_yaml::Value::deserialize(doc)?; // parse or error with location
    }
    Ok(())
}

pub fn pretty_yaml_error(src: &str, err: &serde_yaml::Error) -> String {
    if let Some(loc) = err.location() {
        let (line0, col0) = (loc.line().saturating_sub(1), loc.column().saturating_sub(1));
        let line_txt = src.lines().nth(line0).unwrap_or("");
        let caret = " ".repeat(col0) + "^";
        format!(
            "YAML error at {}:{}: {}\n{}\n{}",
            loc.line(),
            loc.column(),
            err,
            line_txt,
            caret
        )
    } else {
        err.to_string()
    }
}

// Where would a node go *syntactically* if we rendered something here?
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Slot {
    MappingValue, // previous non-empty line ends with ':'
    SequenceItem, // current line (after indentation) starts with "- "
    Plain,        // anywhere else
}

fn current_slot_in_buf(buf: &str) -> Slot {
    // Look at the current and the previous non-empty line
    let mut it = buf.rsplit('\n');

    let cur = it.next().unwrap_or(""); // current (possibly empty) line
    let cur_trim = cur.trim_start();

    // If current line already starts with "- " → list item context
    if cur_trim.starts_with("- ") {
        return Slot::SequenceItem;
    }

    let prev_non_empty = it.find(|l| !l.trim().is_empty()).unwrap_or("");

    let prev_trim = prev_non_empty.trim_end();

    // Definitely mapping value if the previous non-empty line ends with ':'
    if prev_trim.ends_with(':') {
        return Slot::MappingValue;
    }

    // NEW: stay in mapping if the previous non-empty line is a placeholder mapping entry
    // e.g.   "  "__TSG_PLACEHOLDER_3__": 0"
    if prev_trim.contains("__TSG_PLACEHOLDER_") && prev_trim.contains("\": 0") {
        return Slot::MappingValue;
    }

    Slot::Plain
}

fn write_fragment_placeholder(out: &mut String, id: usize, slot: Slot) {
    use std::fmt::Write as _;
    match slot {
        Slot::MappingValue => {
            // becomes an entry *inside* the surrounding block mapping,
            // can repeat multiple times safely:
            let _ = write!(out, "\"__TSG_PLACEHOLDER_{id}__\": 0\n");
        }
        Slot::SequenceItem => {
            // fill the list item in-line, do NOT add a leading '- ' here; it's already present
            let _ = write!(out, "\"__TSG_PLACEHOLDER_{id}__\"\n");
        }
        Slot::Plain => {
            // standalone scalar is fine anywhere else
            let _ = write!(out, "\"__TSG_PLACEHOLDER_{id}__\"\n");
        }
    }
}

pub fn build_sanitized_with_placeholders(
    src: &str,
    gtree: &tree_sitter::Tree,
    out_placeholders: &mut Vec<Placeholder>,
    collect_values: impl Fn(&Node) -> Vec<String> + Copy,
) -> String {
    let mut next_id = 0usize;

    // Variable environment: var name -> Values set
    type Env = BTreeMap<String, BTreeSet<String>>;
    let mut env: Env = Env::new();

    fn emit_placeholder_for(
        node: tree_sitter::Node,
        src: &str,
        buf: &mut String,
        next_id: &mut usize,
        out: &mut Vec<Placeholder>,
        collect_values: impl Fn(&Node) -> Vec<String> + Copy,
        env: &BTreeMap<String, BTreeSet<String>>,
        is_fragment_output: bool,
    ) {
        let id = *next_id;
        *next_id += 1;
        buf.push('"');
        buf.push_str(&format!("__TSG_PLACEHOLDER_{}__", id));
        buf.push('"');

        let mut values: Vec<String> = if node.kind() == "variable" {
            // Pull values from env if we know the variable
            if let Some(name) = variable_ident(&node, src) {
                env.get(&name)
                    .map(|s| s.iter().cloned().collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            collect_values(&node)
        };
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
        env: &mut BTreeMap<String, BTreeSet<String>>,
    ) {
        // 1) pass through raw text
        if node.kind() == "text" {
            buf.push_str(&src[node.byte_range()]);
            return;
        }

        // 2) containers: descend into their DIRECT children only
        if is_container(node.kind()) || is_control_flow(node.kind()) {
            let mut c = node.walk();
            let parent_is_define = node.kind() == "define_action";
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

                // // RECORD guards (no YAML emission)
                if is_control_flow(node.kind()) && is_guard_child(&ch) {
                    let id = *next_id;
                    *next_id += 1;
                    // let values = collect_values(&ch);
                    // out.push(Placeholder {
                    //     id,
                    //     role: Role::Guard,
                    //     action_span: ch.byte_range(),
                    //     values,
                    //     is_fragment_output: false,
                    // });
                    continue;
                }

                if is_control_flow(ch.kind()) || is_container(ch.kind()) {
                    // Nested container (e.g., else_clause); recurse
                    walk(ch, src, buf, next_id, out, collect_values, env);
                } else if is_output_expr_kind(ch.kind()) {
                    // single placeholder for this direct child expression
                    let frag = looks_like_fragment_output(&ch, src);
                    if parent_is_define || frag {
                        // Record the placeholder, but DO NOT write anything into the buffer.
                        // This keeps the surrounding YAML valid.
                        let id = *next_id;
                        *next_id += 1;

                        let values = if ch.kind() == "variable" {
                            if let Some(name) = variable_ident(&ch, src) {
                                env.get(&name)
                                    .map(|s| s.iter().cloned().collect())
                                    .unwrap_or_default()
                            } else {
                                Vec::new()
                            }
                        } else {
                            collect_values(&ch)
                        };
                        out.push(Placeholder {
                            id,
                            role: Role::Fragment,
                            action_span: ch.byte_range(),
                            values,
                            is_fragment_output: true,
                        });

                        // // In define bodies: record the use, but DO NOT write to buf
                        // let id = *next_id;
                        // *next_id += 1;
                        // let values = if ch.kind() == "variable" {
                        //     if let Some(name) = variable_ident(&ch, src) {
                        //         env.get(&name)
                        //             .map(|s| s.iter().cloned().collect())
                        //             .unwrap_or_default()
                        //     } else {
                        //         Vec::new()
                        //     }
                        // } else {
                        //     collect_values(&ch)
                        // };
                        // out.push(Placeholder {
                        //     id,
                        //     role: Role::Fragment, // define content has no concrete YAML site
                        //     action_span: ch.byte_range(),
                        //     values,
                        //     is_fragment_output: frag,
                        // });

                        // IMPORTANT: only write to the buffer when not suppressing define body text
                        if !parent_is_define {
                            let slot = current_slot_in_buf(buf);
                            write_fragment_placeholder(buf, id, slot);
                        }
                    } else {
                        // Normal template files: emit placeholder into YAML
                        emit_placeholder_for(ch, src, buf, next_id, out, collect_values, env, frag);
                    }
                } else if ch.kind() == "ERROR" {
                    // These commonly carry indentation/spacing that YAML needs.
                    // Preserve them verbatim.
                    // buf.push_str(&src[ch.byte_range()]);
                    // skip (often whitespace artifacts)
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
                        values: values.clone(),
                        is_fragment_output: false,
                    });
                    // Try to extract `$var` name and bind collected values
                    // child "variable" holds the LHS
                    let mut vc = ch.walk();
                    for sub in ch.children(&mut vc) {
                        if sub.is_named() && sub.kind() == "variable" {
                            if let Some(name) = variable_ident(&sub, src) {
                                let mut set = BTreeSet::<String>::new();
                                set.extend(values.into_iter());
                                env.insert(name, set);
                            }
                            break;
                        }
                    }
                    continue;
                } else {
                    // Unknown non-output node at container level — skip to keep YAML valid.
                    continue;
                }
            }
            return;
        }

        // 3) non-container reached (shouldn’t happen for well-formed trees, but safe fallback)
        if is_output_expr_kind(node.kind()) {
            let frag = looks_like_fragment_output(&node, src);
            emit_placeholder_for(node, src, buf, next_id, out, collect_values, env, frag);
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
        &mut env,
    );
    out
}
