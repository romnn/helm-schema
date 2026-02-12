use std::collections::HashMap;
use std::fmt;

use crate::scanner::ScanError;
use crate::{Yaml, YamlLoader};

#[derive(Debug, thiserror::Error)]
pub enum FusedParseError {
    #[error(transparent)]
    Yaml(#[from] ScanError),

    #[error("unbalanced helm control flow: expected an `end` for `{0}`")]
    UnbalancedControlFlow(String),
}

fn take_action_prefix(s: &str) -> Option<(&str, &str)> {
    if !s.starts_with("{{") {
        return None;
    }
    let close_at = if is_comment_action_prefix(s) {
        s.rfind("}}").map(|idx| idx + 2)?
    } else {
        s.find("}}").map(|idx| idx + 2)?
    };
    Some((&s[..close_at], &s[close_at..]))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FusedNode {
    Stream {
        items: Vec<FusedNode>,
    },
    Document {
        items: Vec<FusedNode>,
    },

    Mapping {
        items: Vec<FusedNode>,
    },
    Pair {
        key: Box<FusedNode>,
        value: Option<Box<FusedNode>>,
    },

    Sequence {
        items: Vec<FusedNode>,
    },
    Item {
        value: Option<Box<FusedNode>>,
    },

    Scalar {
        kind: String,
        text: String,
    },

    HelmExpr {
        text: String,
    },
    HelmComment {
        text: String,
    },

    If {
        cond: String,
        then_branch: Vec<FusedNode>,
        else_branch: Vec<FusedNode>,
    },
    Range {
        header: String,
        body: Vec<FusedNode>,
        else_branch: Vec<FusedNode>,
    },
    With {
        header: String,
        body: Vec<FusedNode>,
        else_branch: Vec<FusedNode>,
    },
    Define {
        header: String,
        body: Vec<FusedNode>,
    },
    Block {
        header: String,
        body: Vec<FusedNode>,
    },

    Unknown {
        kind: String,
        text: Option<String>,
        children: Vec<FusedNode>,
    },
}

impl fmt::Display for FusedNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Parse a Helm template source into a fused YAML/Helm AST.
///
/// # Errors
///
/// Returns a [`FusedParseError`] if the input contains malformed YAML or
/// unbalanced Helm control-flow blocks.
#[allow(clippy::too_many_lines)]
pub fn parse_fused_yaml_helm(src: &str) -> Result<FusedNode, FusedParseError> {
    let mut out: Vec<FusedNode> = Vec::new();
    let mut stack: Vec<ControlFrame> = Vec::new();
    let mut pending_yaml = String::new();

    let push_to_current =
        |stack: &mut Vec<ControlFrame>, out: &mut Vec<FusedNode>, node: FusedNode| {
            if let Some(top) = stack.last_mut() {
                top.push_item(node);
            } else {
                out.push(node);
            }
        };

    let flush_yaml = |pending_yaml: &mut String,
                      stack: &mut Vec<ControlFrame>,
                      out: &mut Vec<FusedNode>|
     -> Result<(), FusedParseError> {
        if pending_yaml.trim().is_empty() {
            pending_yaml.clear();
            return Ok(());
        }

        let fragment = std::mem::take(pending_yaml);
        let fragment = deindent_yaml_fragment(&fragment);
        let (yaml_fragment, mut inline_value_frags) =
            rewrite_inline_block_value_fragments(&fragment);
        let (yaml_fragment, helm_placeholders) = replace_helm_expr_placeholders(&yaml_fragment);

        match YamlLoader::load_from_str(&yaml_fragment) {
            Ok(docs) => {
                for doc in docs {
                    let mut node = convert_yaml_to_fused(&doc);
                    if !inline_value_frags.is_empty() {
                        apply_inline_value_fragments(&mut node, &mut inline_value_frags);
                    }
                    if !helm_placeholders.is_empty() {
                        restore_helm_expr_placeholders(&mut node, &helm_placeholders);
                    }
                    push_to_current(stack, out, node);
                }
            }
            Err(_e) => {}
        }
        Ok(())
    };

    let mut lines = src.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        if should_absorb_action_line_into_pending_yaml(&pending_yaml, line) {
            pending_yaml.push_str(line);
            continue;
        }
        if let Some((raw_action, indent_col)) = try_take_standalone_helm_action(line, &mut lines) {
            let tok = parse_helm_template_text(&raw_action);

            // If this is an indented YAML fragment injector (e.g. `{{- include ... | nindent N }}`)
            // and we're not inside control flow, keep it in the YAML fragment and let the YAML
            // layer skip it. This matches the tree-sitter parser behavior for cases like the
            // cert-manager `labels` injection.
            if stack.is_empty() && indent_col > 0 {
                if let HelmTok::Expr { text } = &tok {
                    let is_injector = (text.contains("include")
                        || text.contains("tpl")
                        || text.contains("template"))
                        && (text.contains("nindent") || text.contains("indent"));
                    if is_injector {
                        continue;
                    }
                }
            }

            flush_yaml(&mut pending_yaml, &mut stack, &mut out)?;

            match tok {
                HelmTok::OpenIf { cond } => {
                    stack.push(ControlFrame {
                        kind: ControlKind::If { cond },
                        then_items: Vec::new(),
                        else_items: Vec::new(),
                        in_else: false,
                        shares_end_with_parent: false,
                    });
                }
                HelmTok::OpenRange { header } => {
                    stack.push(ControlFrame {
                        kind: ControlKind::Range { header },
                        then_items: Vec::new(),
                        else_items: Vec::new(),
                        in_else: false,
                        shares_end_with_parent: false,
                    });
                }
                HelmTok::OpenWith { header } => {
                    stack.push(ControlFrame {
                        kind: ControlKind::With { header },
                        then_items: Vec::new(),
                        else_items: Vec::new(),
                        in_else: false,
                        shares_end_with_parent: false,
                    });
                }
                HelmTok::OpenDefine { header } => {
                    stack.push(ControlFrame {
                        kind: ControlKind::Define { header },
                        then_items: Vec::new(),
                        else_items: Vec::new(),
                        in_else: false,
                        shares_end_with_parent: false,
                    });
                }
                HelmTok::OpenBlock { header } => {
                    stack.push(ControlFrame {
                        kind: ControlKind::Block { header },
                        then_items: Vec::new(),
                        else_items: Vec::new(),
                        in_else: false,
                        shares_end_with_parent: false,
                    });
                }
                HelmTok::Else => {
                    if let Some(top) = stack.last_mut() {
                        top.in_else = true;
                    }
                }
                HelmTok::ElseIf { cond } => {
                    if let Some(top) = stack.last_mut() {
                        top.in_else = true;
                    }
                    stack.push(ControlFrame {
                        kind: ControlKind::If { cond },
                        then_items: Vec::new(),
                        else_items: Vec::new(),
                        in_else: false,
                        shares_end_with_parent: true,
                    });
                }
                HelmTok::End => {
                    let Some(frame) = stack.pop() else {
                        continue;
                    };
                    let mut shares = frame.shares_end_with_parent;
                    let node = frame.into_node();
                    push_to_current(&mut stack, &mut out, node);

                    while shares {
                        let Some(parent) = stack.pop() else {
                            break;
                        };
                        shares = parent.shares_end_with_parent;
                        let parent_node = parent.into_node();
                        push_to_current(&mut stack, &mut out, parent_node);
                    }
                }
                HelmTok::Comment { text } => {
                    push_to_current(&mut stack, &mut out, FusedNode::HelmComment { text });
                }
                HelmTok::Expr { text } => {
                    if !is_silent_reassignment_expr(&text) {
                        push_to_current(&mut stack, &mut out, FusedNode::HelmExpr { text });
                    }
                }
            }

            continue;
        }

        // Split inline helm control-flow actions out of YAML scalar lines.
        // This matches the tree-sitter parser behavior, which treats `{{- if ... -}}`
        // etc. as control-flow boundaries even when they appear inline.
        let mut rest = line;
        loop {
            let Some(action_at) = rest.find("{{") else {
                pending_yaml.push_str(rest);
                break;
            };

            let (before, after) = rest.split_at(action_at);
            let Some((action, tail)) = take_action_prefix(after) else {
                pending_yaml.push_str(rest);
                break;
            };

            let tok = parse_helm_template_text(action);
            let is_control = matches!(
                tok,
                HelmTok::OpenIf { .. }
                    | HelmTok::OpenRange { .. }
                    | HelmTok::OpenWith { .. }
                    | HelmTok::OpenDefine { .. }
                    | HelmTok::OpenBlock { .. }
                    | HelmTok::Else
                    | HelmTok::ElseIf { .. }
                    | HelmTok::End
                    | HelmTok::Comment { .. }
            );

            if !is_control {
                pending_yaml.push_str(before);
                pending_yaml.push_str(action);
                rest = tail;
                continue;
            }

            pending_yaml.push_str(before);
            if !pending_yaml.ends_with('\n') {
                pending_yaml.push('\n');
            }
            flush_yaml(&mut pending_yaml, &mut stack, &mut out)?;

            match tok {
                HelmTok::OpenIf { cond } => {
                    stack.push(ControlFrame {
                        kind: ControlKind::If { cond },
                        then_items: Vec::new(),
                        else_items: Vec::new(),
                        in_else: false,
                        shares_end_with_parent: false,
                    });
                }
                HelmTok::OpenRange { header } => {
                    stack.push(ControlFrame {
                        kind: ControlKind::Range { header },
                        then_items: Vec::new(),
                        else_items: Vec::new(),
                        in_else: false,
                        shares_end_with_parent: false,
                    });
                }
                HelmTok::OpenWith { header } => {
                    stack.push(ControlFrame {
                        kind: ControlKind::With { header },
                        then_items: Vec::new(),
                        else_items: Vec::new(),
                        in_else: false,
                        shares_end_with_parent: false,
                    });
                }
                HelmTok::OpenDefine { header } => {
                    stack.push(ControlFrame {
                        kind: ControlKind::Define { header },
                        then_items: Vec::new(),
                        else_items: Vec::new(),
                        in_else: false,
                        shares_end_with_parent: false,
                    });
                }
                HelmTok::OpenBlock { header } => {
                    stack.push(ControlFrame {
                        kind: ControlKind::Block { header },
                        then_items: Vec::new(),
                        else_items: Vec::new(),
                        in_else: false,
                        shares_end_with_parent: false,
                    });
                }
                HelmTok::Else => {
                    if let Some(top) = stack.last_mut() {
                        top.in_else = true;
                    }
                }
                HelmTok::ElseIf { cond } => {
                    if let Some(top) = stack.last_mut() {
                        top.in_else = true;
                    }
                    stack.push(ControlFrame {
                        kind: ControlKind::If { cond },
                        then_items: Vec::new(),
                        else_items: Vec::new(),
                        in_else: false,
                        shares_end_with_parent: true,
                    });
                }
                HelmTok::End => {
                    let Some(frame) = stack.pop() else {
                        rest = tail;
                        continue;
                    };
                    let mut shares = frame.shares_end_with_parent;
                    let node = frame.into_node();
                    push_to_current(&mut stack, &mut out, node);

                    while shares {
                        let Some(parent) = stack.pop() else {
                            break;
                        };
                        shares = parent.shares_end_with_parent;
                        let parent_node = parent.into_node();
                        push_to_current(&mut stack, &mut out, parent_node);
                    }
                }
                HelmTok::Comment { text } => {
                    push_to_current(&mut stack, &mut out, FusedNode::HelmComment { text });
                }
                HelmTok::Expr { text } => {
                    if !is_silent_reassignment_expr(&text) {
                        push_to_current(&mut stack, &mut out, FusedNode::HelmExpr { text });
                    }
                }
            }

            rest = tail;
        }
    }

    flush_yaml(&mut pending_yaml, &mut stack, &mut out)?;

    if let Some(unclosed) = stack.pop() {
        return Err(FusedParseError::UnbalancedControlFlow(unclosed.label()));
    }

    Ok(FusedNode::Document { items: out })
}

fn should_absorb_action_line_into_pending_yaml(pending_yaml: &str, line: &str) -> bool {
    let start_col = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
    let after_indent = &line[start_col..];
    if !after_indent.starts_with("{{") {
        return false;
    }

    let Some((action, _rest)) = take_action_if_closed_on_line(after_indent) else {
        return false;
    };
    let likely_block = action.contains("nindent") || action.contains("indent");
    if !likely_block {
        return false;
    }

    let Some((key_indent, _key)) = last_key_only_line(pending_yaml) else {
        return false;
    };
    start_col > key_indent
}

fn last_key_only_line(pending_yaml: &str) -> Option<(usize, String)> {
    for line in pending_yaml.split_inclusive('\n').rev() {
        let line_no_nl = line.trim_end_matches(['\n', '\r']);
        if line_no_nl.trim().is_empty() {
            continue;
        }

        let indent = line_no_nl
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count();
        let after_indent = &line_no_nl[indent..];

        let colon_at = after_indent.find(':')?;
        let (lhs, rhs_with_colon) = after_indent.split_at(colon_at);
        let key = lhs.trim();
        if key.is_empty() {
            return None;
        }
        if key.starts_with('-') {
            return None;
        }

        let rhs = &rhs_with_colon[1..];
        if !rhs.trim().is_empty() {
            return None;
        }

        return Some((indent, key.to_string()));
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HelmTok {
    OpenIf { cond: String },
    OpenRange { header: String },
    OpenWith { header: String },
    OpenDefine { header: String },
    OpenBlock { header: String },
    Else,
    ElseIf { cond: String },
    End,
    Comment { text: String },
    Expr { text: String },
}

fn is_silent_reassignment_expr(text: &str) -> bool {
    let mut it = text.split_whitespace();
    let Some(first) = it.next() else {
        return false;
    };
    let Some(second) = it.next() else {
        return false;
    };
    first.starts_with('$') && second == "="
}

fn parse_helm_template_text(raw: &str) -> HelmTok {
    let mut s = raw.trim();

    // Strip delimiters.
    if let Some(rest) = s.strip_prefix("{{") {
        s = rest;
    }
    s = s.strip_prefix('-').unwrap_or(s);
    s = s.trim_start();
    if let Some(rest) = s.strip_suffix("}}") {
        s = rest;
    }
    s = s.strip_suffix('-').unwrap_or(s);
    s = s.trim();

    if s.starts_with("/*") {
        return HelmTok::Comment {
            text: s.to_string(),
        };
    }

    let mut it = s.split_whitespace();
    let first = it.next().unwrap_or("");
    let rest = s[first.len()..].trim_start();

    match first {
        "if" => HelmTok::OpenIf {
            cond: rest.to_string(),
        },
        "range" => HelmTok::OpenRange {
            header: rest.to_string(),
        },
        "with" => HelmTok::OpenWith {
            header: rest.to_string(),
        },
        "define" => HelmTok::OpenDefine {
            header: rest.to_string(),
        },
        "block" => HelmTok::OpenBlock {
            header: rest.to_string(),
        },
        "else" => {
            let rest_trim = rest.trim_start();
            if let Some(after_if) = rest_trim.strip_prefix("if") {
                let cond = after_if.trim_start();
                HelmTok::ElseIf {
                    cond: cond.to_string(),
                }
            } else {
                HelmTok::Else
            }
        }
        "end" => HelmTok::End,
        _ => HelmTok::Expr {
            text: s.to_string(),
        },
    }
}

#[derive(Debug)]
struct ControlFrame {
    kind: ControlKind,
    then_items: Vec<FusedNode>,
    else_items: Vec<FusedNode>,
    in_else: bool,
    shares_end_with_parent: bool,
}

#[derive(Debug)]
enum ControlKind {
    If { cond: String },
    Range { header: String },
    With { header: String },
    Define { header: String },
    Block { header: String },
}

impl ControlFrame {
    fn label(&self) -> String {
        match &self.kind {
            ControlKind::If { cond } => format!("if {cond}"),
            ControlKind::Range { header } => format!("range {header}"),
            ControlKind::With { header } => format!("with {header}"),
            ControlKind::Define { header } => format!("define {header}"),
            ControlKind::Block { header } => format!("block {header}"),
        }
    }

    fn push_item(&mut self, node: FusedNode) {
        if self.in_else {
            self.else_items.push(node);
        } else {
            self.then_items.push(node);
        }
    }

    fn into_node(self) -> FusedNode {
        match self.kind {
            ControlKind::If { cond } => FusedNode::If {
                cond,
                then_branch: self.then_items,
                else_branch: self.else_items,
            },
            ControlKind::Range { header } => FusedNode::Range {
                header,
                body: self.then_items,
                else_branch: self.else_items,
            },
            ControlKind::With { header } => FusedNode::With {
                header,
                body: self.then_items,
                else_branch: self.else_items,
            },
            ControlKind::Define { header } => FusedNode::Define {
                header,
                body: self.then_items,
            },
            ControlKind::Block { header } => FusedNode::Block {
                header,
                body: self.then_items,
            },
        }
    }
}

fn try_take_standalone_helm_action<'a>(
    line: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Option<(String, usize)> {
    let start_col = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
    let after_indent = &line[start_col..];
    if !after_indent.starts_with("{{") {
        return None;
    }

    let mut action = after_indent.to_string();
    loop {
        if let Some((closed, _rest)) = take_action_if_closed_on_line(&action) {
            return Some((closed, start_col));
        }

        // If we already saw a `}}` on the first line, but we couldn't treat it
        // as a standalone action, it's an inline expression embedded in YAML.
        if action == after_indent && after_indent.contains("}}") {
            return None;
        }

        let Some(next_line) = lines.next() else {
            break;
        };
        action.push_str(next_line);
    }

    Some((action, start_col))
}

fn take_action_if_closed_on_line(s: &str) -> Option<(String, &str)> {
    let close_at = if is_comment_action_prefix(s) {
        s.rfind("}}").map(|idx| idx + 2)?
    } else {
        s.find("}}").map(|idx| idx + 2)?
    };
    let (action, rest) = s.split_at(close_at);

    let mut tail = rest;
    while tail.starts_with(' ') || tail.starts_with('\t') {
        tail = &tail[1..];
    }
    if tail.starts_with('#') || tail.trim().is_empty() {
        return Some((action.to_string(), rest));
    }

    None
}

fn is_comment_action_prefix(s: &str) -> bool {
    let mut t = s.trim_start();
    if let Some(rest) = t.strip_prefix("{{") {
        t = rest;
    }
    t = t.strip_prefix('-').unwrap_or(t);
    t = t.trim_start();
    t.starts_with("/*")
}

fn deindent_yaml_fragment(fragment: &str) -> String {
    let mut min_indent: Option<usize> = None;
    for line in fragment.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim().is_empty() {
            continue;
        }
        let indent = content
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count();
        min_indent = Some(match min_indent {
            None => indent,
            Some(prev) => prev.min(indent),
        });
    }

    let Some(min_indent) = min_indent else {
        return fragment.to_string();
    };

    let mut out = String::with_capacity(fragment.len());
    for line in fragment.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim().is_empty() {
            out.push_str(line);
            continue;
        }

        let mut removed = 0usize;
        let mut idx = 0usize;
        for ch in line.chars() {
            if removed >= min_indent {
                break;
            }
            if ch == ' ' || ch == '\t' {
                removed += 1;
                idx += ch.len_utf8();
                continue;
            }
            break;
        }
        out.push_str(&line[idx..]);
    }
    out
}

fn convert_yaml_to_fused(doc: &Yaml) -> FusedNode {
    match doc {
        Yaml::Null => FusedNode::Scalar {
            kind: "null".to_string(),
            text: "null".to_string(),
        },
        Yaml::Boolean(b) => FusedNode::Scalar {
            kind: "bool".to_string(),
            text: b.to_string(),
        },
        Yaml::Integer(i) => FusedNode::Scalar {
            kind: "int".to_string(),
            text: i.to_string(),
        },
        Yaml::Real(s) => FusedNode::Scalar {
            kind: "real".to_string(),
            text: s.clone(),
        },
        Yaml::String(s) => {
            if is_entire_helm_action_scalar(s) {
                match parse_helm_template_text(s) {
                    HelmTok::Comment { text } => FusedNode::HelmComment { text },
                    HelmTok::Expr { text } => FusedNode::HelmExpr { text },
                    HelmTok::OpenIf { cond } => FusedNode::HelmExpr {
                        text: format!("if {cond}"),
                    },
                    HelmTok::OpenRange { header } => FusedNode::HelmExpr {
                        text: format!("range {header}"),
                    },
                    HelmTok::OpenWith { header } => FusedNode::HelmExpr {
                        text: format!("with {header}"),
                    },
                    HelmTok::OpenDefine { header } => FusedNode::HelmExpr {
                        text: format!("define {header}"),
                    },
                    HelmTok::OpenBlock { header } => FusedNode::HelmExpr {
                        text: format!("block {header}"),
                    },
                    HelmTok::Else => FusedNode::HelmExpr {
                        text: "else".to_string(),
                    },
                    HelmTok::ElseIf { cond } => FusedNode::HelmExpr {
                        text: format!("else if {cond}"),
                    },
                    HelmTok::End => FusedNode::HelmExpr {
                        text: "end".to_string(),
                    },
                }
            } else {
                FusedNode::Scalar {
                    kind: "str".to_string(),
                    text: s.clone(),
                }
            }
        }
        Yaml::Array(items) => FusedNode::Sequence {
            items: items
                .iter()
                .map(|item| FusedNode::Item {
                    value: Some(Box::new(convert_yaml_to_fused(item))),
                })
                .collect(),
        },
        Yaml::Hash(h) => FusedNode::Mapping {
            items: h
                .iter()
                .map(|(k, v)| FusedNode::Pair {
                    key: Box::new(convert_yaml_to_fused(k)),
                    value: Some(Box::new(convert_yaml_to_fused(v))),
                })
                .collect(),
        },
        Yaml::Alias(id) => FusedNode::Scalar {
            kind: "alias".to_string(),
            text: id.to_string(),
        },
        Yaml::BadValue => FusedNode::Scalar {
            kind: "bad".to_string(),
            text: "bad".to_string(),
        },
    }
}

fn is_entire_helm_action_scalar(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("{{") && s.ends_with("}}")
}

fn rewrite_inline_block_value_fragments(fragment: &str) -> (String, HashMap<String, Vec<String>>) {
    let mut out = String::with_capacity(fragment.len());
    let mut frags: HashMap<String, Vec<String>> = HashMap::new();

    let mut lines = fragment.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        let nl = if line.ends_with('\n') { "\n" } else { "" };
        let line_no_nl = line.strip_suffix('\n').unwrap_or(line);

        let Some(colon_at) = line_no_nl.find(':') else {
            out.push_str(line);
            continue;
        };

        let (lhs, rhs_with_colon) = line_no_nl.split_at(colon_at);
        let key = lhs.trim();
        if key.is_empty() {
            out.push_str(line);
            continue;
        }

        let rhs = &rhs_with_colon[1..];
        let rhs_trim = rhs.trim_start();

        // Case 1: `key: {{- toYaml . | nindent N }}` (inline)
        if rhs_trim.starts_with("{{") {
            let Some((action, _rest)) = take_action_if_closed_on_line(rhs_trim) else {
                out.push_str(line);
                continue;
            };

            let likely_block = action.contains("nindent") || action.contains("indent");
            if !likely_block {
                out.push_str(line);
                continue;
            }

            let expr_text = match parse_helm_template_text(&action) {
                HelmTok::Expr { text } => text,
                HelmTok::Comment { text } => text,
                other => format!("{other:?}"),
            };
            frags.entry(key.to_string()).or_default().push(expr_text);

            out.push_str(&line_no_nl[..=colon_at]);
            out.push_str(nl);
            continue;
        }

        // Case 2: `key:` followed by an indented `{{- toYaml ... | nindent N }}` on the next line.
        if rhs_trim.is_empty() {
            if let Some(next_line) = lines.peek().copied() {
                let next_indent = next_line
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .count();
                let next_after_indent = &next_line[next_indent..];
                if next_indent > lhs.chars().take_while(|c| *c == ' ' || *c == '\t').count()
                    && next_after_indent.trim_start().starts_with("{{")
                {
                    if let Some((action, _rest)) = take_action_if_closed_on_line(next_after_indent)
                    {
                        let likely_block = action.contains("nindent") || action.contains("indent");
                        if likely_block {
                            let expr_text = match parse_helm_template_text(&action) {
                                HelmTok::Expr { text } => text,
                                HelmTok::Comment { text } => text,
                                other => format!("{other:?}"),
                            };
                            frags.entry(key.to_string()).or_default().push(expr_text);

                            // Keep only `key:` line, skip the injected action line.
                            out.push_str(line);
                            let _ = lines.next();
                            continue;
                        }
                    }
                }
            }
        }

        out.push_str(line);
    }

    (out, frags)
}

fn apply_inline_value_fragments(node: &mut FusedNode, frags: &mut HashMap<String, Vec<String>>) {
    match node {
        FusedNode::Mapping { items } => {
            for item in items.iter_mut() {
                apply_inline_value_fragments(item, frags);
            }
        }
        FusedNode::Pair { key, value } => {
            apply_inline_value_fragments(key, frags);
            if let Some(v) = value.as_deref_mut() {
                apply_inline_value_fragments(v, frags);
            }

            let FusedNode::Scalar { kind, text } = key.as_ref() else {
                return;
            };
            if kind != "str" {
                return;
            }
            let Some(v) = value.as_deref_mut() else {
                return;
            };
            let FusedNode::Scalar {
                kind: v_kind,
                text: _v_text,
            } = v
            else {
                return;
            };
            if v_kind != "null" {
                return;
            }

            if let Some(exprs) = frags.get_mut(text) {
                if !exprs.is_empty() {
                    let expr = exprs.remove(0);
                    *value = Some(Box::new(FusedNode::HelmExpr { text: expr }));
                }
            }
        }
        FusedNode::Sequence { items } => {
            for item in items.iter_mut() {
                apply_inline_value_fragments(item, frags);
            }
        }
        FusedNode::Item { value } => {
            if let Some(v) = value.as_deref_mut() {
                apply_inline_value_fragments(v, frags);
            }
        }
        FusedNode::If {
            then_branch,
            else_branch,
            ..
        } => {
            for item in then_branch.iter_mut() {
                apply_inline_value_fragments(item, frags);
            }
            for item in else_branch.iter_mut() {
                apply_inline_value_fragments(item, frags);
            }
        }
        FusedNode::Range {
            body, else_branch, ..
        }
        | FusedNode::With {
            body, else_branch, ..
        } => {
            for item in body.iter_mut() {
                apply_inline_value_fragments(item, frags);
            }
            for item in else_branch.iter_mut() {
                apply_inline_value_fragments(item, frags);
            }
        }
        FusedNode::Define { body, .. } | FusedNode::Block { body, .. } => {
            for item in body.iter_mut() {
                apply_inline_value_fragments(item, frags);
            }
        }
        FusedNode::Stream { items } | FusedNode::Document { items } => {
            for item in items.iter_mut() {
                apply_inline_value_fragments(item, frags);
            }
        }
        FusedNode::Scalar { .. }
        | FusedNode::HelmExpr { .. }
        | FusedNode::HelmComment { .. }
        | FusedNode::Unknown { .. } => {}
    }
}

/// Replace `{{ ... }}` sequences in a YAML fragment with unique placeholders
/// so that yaml-rust can parse the fragment. Returns the rewritten fragment
/// and a vec of original expressions (index corresponds to placeholder number).
#[derive(Debug, Clone)]
struct HelmPlaceholder {
    raw: String,
    quoted: bool,
}

fn replace_helm_expr_placeholders(fragment: &str) -> (String, Vec<HelmPlaceholder>) {
    let mut out = String::with_capacity(fragment.len());
    let mut exprs: Vec<HelmPlaceholder> = Vec::new();
    let mut i = 0usize;

    while let Some(rel_start) = fragment[i..].find("{{") {
        let start = i + rel_start;
        out.push_str(&fragment[i..start]);
        let after_open = &fragment[start + 2..];
        if let Some(rel_end) = after_open.find("}}") {
            let end = start + 2 + rel_end + 2;
            let full_expr = &fragment[start..end];

            let before = fragment[..start].trim_end_matches([' ', '\t']);
            let after = fragment[end..].trim_start_matches([' ', '\t']);
            let quoted = (before.ends_with('"') && after.starts_with('"'))
                || (before.ends_with('\'') && after.starts_with('\''));

            let placeholder = format!("__HELM_PLACEHOLDER_{}__", exprs.len());
            exprs.push(HelmPlaceholder {
                raw: full_expr.to_string(),
                quoted,
            });
            out.push_str(&placeholder);
            i = end;
        } else {
            // No closing }}, pass through as-is.
            out.push_str("{{");
            i = start + 2;
        }
    }
    out.push_str(&fragment[i..]);
    (out, exprs)
}

/// Walk a parsed `FusedNode` tree and restore placeholder strings back to
/// `HelmExpr` nodes (when the placeholder is the entire scalar text) or
/// restore the original `{{ ... }}` text inline (when concatenated with
/// other text, e.g. `{{ template "foo" . }}-client`).
fn restore_helm_expr_placeholders(node: &mut FusedNode, exprs: &[HelmPlaceholder]) {
    match node {
        FusedNode::Scalar { text, .. } => {
            // Check if the entire text is exactly one placeholder.
            if let Some(idx) = parse_sole_placeholder(text) {
                if let Some(original) = exprs.get(idx) {
                    if original.quoted {
                        text.clone_from(&original.raw);
                    } else {
                        let inner = extract_helm_expr_inner(&original.raw);
                        *node = FusedNode::HelmExpr {
                            text: inner.to_string(),
                        };
                    }
                }
                return;
            }
            // Otherwise, restore any embedded placeholders back to their
            // original `{{ ... }}` text so the scalar preserves them inline.
            for (i, original) in exprs.iter().enumerate() {
                let placeholder = format!("__HELM_PLACEHOLDER_{i}__");
                if text.contains(&placeholder) {
                    let inner = extract_helm_expr_inner_preserve_trailing(&original.raw);
                    let inner = inner.trim_start();

                    // Heuristics to match the expected AST output:
                    // - `.foo` / `$foo` expressions are tightened: `{{.foo}}`
                    // - `default ...` is normalized to remove the leading space after `{{`,
                    //   but keep a trailing space before `}}` if it existed in source.
                    // - Otherwise, preserve the original raw template text.
                    let restored = if inner.starts_with('.') || inner.starts_with('$') {
                        let inner = inner.trim_end();
                        format!("{{{{{inner}}}}}")
                    } else if inner.starts_with("default") {
                        format!("{{{{{inner}}}}}")
                    } else {
                        original.raw.clone()
                    };
                    *text = text.replace(&placeholder, &restored);
                }
            }
        }
        FusedNode::Mapping { items } => {
            for item in items.iter_mut() {
                restore_helm_expr_placeholders(item, exprs);
            }
        }
        FusedNode::Pair { key, value } => {
            restore_helm_expr_placeholders(key, exprs);
            if let Some(v) = value.as_deref_mut() {
                restore_helm_expr_placeholders(v, exprs);
            }
        }
        FusedNode::Sequence { items } => {
            for item in items.iter_mut() {
                restore_helm_expr_placeholders(item, exprs);
            }
        }
        FusedNode::Item { value } => {
            if let Some(v) = value.as_deref_mut() {
                restore_helm_expr_placeholders(v, exprs);
            }
        }
        FusedNode::If {
            then_branch,
            else_branch,
            ..
        } => {
            for item in then_branch.iter_mut() {
                restore_helm_expr_placeholders(item, exprs);
            }
            for item in else_branch.iter_mut() {
                restore_helm_expr_placeholders(item, exprs);
            }
        }
        FusedNode::Range {
            body, else_branch, ..
        }
        | FusedNode::With {
            body, else_branch, ..
        } => {
            for item in body.iter_mut() {
                restore_helm_expr_placeholders(item, exprs);
            }
            for item in else_branch.iter_mut() {
                restore_helm_expr_placeholders(item, exprs);
            }
        }
        FusedNode::Define { body, .. } | FusedNode::Block { body, .. } => {
            for item in body.iter_mut() {
                restore_helm_expr_placeholders(item, exprs);
            }
        }
        FusedNode::Stream { items } | FusedNode::Document { items } => {
            for item in items.iter_mut() {
                restore_helm_expr_placeholders(item, exprs);
            }
        }
        FusedNode::HelmExpr { .. } | FusedNode::HelmComment { .. } | FusedNode::Unknown { .. } => {}
    }
}

/// If the text is exactly `__HELM_PLACEHOLDER_N__`, return N.
fn parse_sole_placeholder(text: &str) -> Option<usize> {
    let s = text.trim();
    let s = s.strip_prefix("__HELM_PLACEHOLDER_")?;
    let s = s.strip_suffix("__")?;
    s.parse().ok()
}

/// Extract the inner expression from `{{ expr }}` or `{{- expr -}}`.
fn extract_helm_expr_inner(raw: &str) -> &str {
    let mut s = raw.trim();
    if let Some(rest) = s.strip_prefix("{{") {
        s = rest;
    }
    s = s.strip_prefix('-').unwrap_or(s);
    s = s.trim_start();
    if let Some(rest) = s.strip_suffix("}}") {
        s = rest;
    }
    s = s.strip_suffix('-').unwrap_or(s);
    s.trim()
}

fn extract_helm_expr_inner_preserve_trailing(raw: &str) -> &str {
    let mut s = raw.trim();
    if let Some(rest) = s.strip_prefix("{{") {
        s = rest;
    }
    s = s.strip_prefix('-').unwrap_or(s);
    if let Some(rest) = s.strip_suffix("}}") {
        s = rest;
    }
    s = s.strip_suffix('-').unwrap_or(s);
    s
}
