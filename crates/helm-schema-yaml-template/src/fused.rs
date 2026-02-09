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

pub fn parse_fused_yaml_helm(src: &str) -> Result<FusedNode, FusedParseError> {
    let mut out: Vec<FusedNode> = Vec::new();
    let mut stack: Vec<ControlFrame> = Vec::new();
    let mut pending_yaml = String::new();

    let mut push_to_current =
        |stack: &mut Vec<ControlFrame>, out: &mut Vec<FusedNode>, node: FusedNode| {
            if let Some(top) = stack.last_mut() {
                top.push_item(node);
            } else {
                out.push(node);
            }
        };

    let mut flush_yaml = |pending_yaml: &mut String,
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
            Err(_e) => {
                push_to_current(
                    stack,
                    out,
                    FusedNode::Unknown {
                        kind: "yaml_parse_error".to_string(),
                        text: Some(fragment),
                        children: Vec::new(),
                    },
                );
            }
        }
        Ok(())
    };

    let mut lines = src.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        if let Some((raw_action, _indent_col)) = try_take_standalone_helm_action(line, &mut lines) {
            flush_yaml(&mut pending_yaml, &mut stack, &mut out)?;

            let tok = parse_helm_template_text(&raw_action);
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
                    push_to_current(&mut stack, &mut out, FusedNode::HelmExpr { text });
                }
            }

            continue;
        }

        pending_yaml.push_str(line);
    }

    flush_yaml(&mut pending_yaml, &mut stack, &mut out)?;

    if let Some(unclosed) = stack.pop() {
        return Err(FusedParseError::UnbalancedControlFlow(unclosed.label()));
    }

    Ok(FusedNode::Document { items: out })
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

    if let Some((action, _rest)) = take_action_if_closed_on_line(after_indent) {
        return Some((action, start_col));
    }

    // If the line already contains `}}` but `take_action_if_closed_on_line`
    // rejected it, there is trailing content after `}}` (e.g.
    // `{{ template "foo" . }}-client: "true"`). This is an inline expression
    // embedded in YAML, not a standalone Helm action.
    if after_indent.contains("}}") {
        return None;
    }

    let mut action = after_indent.to_string();
    while !action.contains("}}") {
        let Some(next_line) = lines.next() else {
            break;
        };
        action.push_str(next_line);
    }
    Some((action, start_col))
}

fn take_action_if_closed_on_line(s: &str) -> Option<(String, &str)> {
    let close_at = s.find("}}").map(|idx| idx + 2)?;
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

    for line in fragment.split_inclusive('\n') {
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

        if !rhs_trim.starts_with("{{") {
            out.push_str(line);
            continue;
        }

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
fn replace_helm_expr_placeholders(fragment: &str) -> (String, Vec<String>) {
    let mut out = String::with_capacity(fragment.len());
    let mut exprs: Vec<String> = Vec::new();
    let mut rest = fragment;

    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];
        if let Some(end) = after_open.find("}}") {
            let full_expr = &rest[start..start + 2 + end + 2];
            let placeholder = format!("__HELM_PLACEHOLDER_{}__", exprs.len());
            exprs.push(full_expr.to_string());
            out.push_str(&placeholder);
            rest = &rest[start + 2 + end + 2..];
        } else {
            // No closing }}, pass through as-is.
            out.push_str("{{");
            rest = after_open;
        }
    }
    out.push_str(rest);
    (out, exprs)
}

/// Walk a parsed `FusedNode` tree and restore placeholder strings back to
/// `HelmExpr` nodes (when the placeholder is the entire scalar text) or
/// restore the original `{{ ... }}` text inline (when concatenated with
/// other text, e.g. `{{ template "foo" . }}-client`).
fn restore_helm_expr_placeholders(node: &mut FusedNode, exprs: &[String]) {
    match node {
        FusedNode::Scalar { text, .. } => {
            // Check if the entire text is exactly one placeholder.
            if let Some(idx) = parse_sole_placeholder(text) {
                if let Some(original) = exprs.get(idx) {
                    let inner = extract_helm_expr_inner(original);
                    *node = FusedNode::HelmExpr {
                        text: inner.to_string(),
                    };
                }
                return;
            }
            // Otherwise, restore any embedded placeholders back to their
            // original `{{ ... }}` text so the scalar preserves them inline.
            for (i, original) in exprs.iter().enumerate() {
                let placeholder = format!("__HELM_PLACEHOLDER_{}__", i);
                if text.contains(&placeholder) {
                    *text = text.replace(&placeholder, original);
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
