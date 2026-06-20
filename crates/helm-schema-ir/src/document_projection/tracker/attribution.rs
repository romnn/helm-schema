use std::collections::HashMap;

use crate::YamlPath;

use super::yaml_tree::{
    is_scalar_like, parse_yaml_tree, scalar_text, strip_scalar_quotes, unwrap_yaml_node,
};

const PLACEHOLDER_PREFIX: &str = "__HS";

#[derive(Clone, Debug)]
pub(super) struct ResolvedNodeContext {
    pub(super) current_path: YamlPath,
    pub(super) output_path: YamlPath,
    pub(super) mapping_entry_path: YamlPath,
    pub(super) in_mapping_key: bool,
    pub(super) entire_scalar_value: bool,
    pub(super) inside_block_scalar: bool,
}

impl Default for ResolvedNodeContext {
    fn default() -> Self {
        Self {
            current_path: YamlPath(Vec::new()),
            output_path: YamlPath(Vec::new()),
            mapping_entry_path: YamlPath(Vec::new()),
            in_mapping_key: false,
            entire_scalar_value: false,
            inside_block_scalar: false,
        }
    }
}

#[derive(Default)]
pub(super) struct AttributionIndex {
    output_nodes: HashMap<(usize, usize), ResolvedNodeContext>,
    control_nodes: HashMap<(usize, usize), ResolvedNodeContext>,
}

impl AttributionIndex {
    pub(super) fn output_context_for_node(
        &self,
        node: tree_sitter::Node<'_>,
    ) -> Option<ResolvedNodeContext> {
        self.context_for_node_or_ancestor(&self.output_nodes, node)
    }

    pub(super) fn control_context_for_node(
        &self,
        node: tree_sitter::Node<'_>,
    ) -> Option<ResolvedNodeContext> {
        self.context_for_node_or_ancestor(&self.control_nodes, node)
    }

    fn context_for_node_or_ancestor(
        &self,
        contexts: &HashMap<(usize, usize), ResolvedNodeContext>,
        mut node: tree_sitter::Node<'_>,
    ) -> Option<ResolvedNodeContext> {
        loop {
            if let Some(context) = contexts.get(&(node.start_byte(), node.end_byte())) {
                return Some(context.clone());
            }
            node = node.parent()?;
        }
    }
}

#[derive(Clone)]
struct OutputSpan {
    node_start: usize,
    node_end: usize,
    action_start: usize,
    action_end: usize,
    placeholder: String,
}

pub(super) fn build_attribution_index(
    source: &str,
    root: tree_sitter::Node<'_>,
) -> AttributionIndex {
    let mut sanitized = source.as_bytes().to_vec();
    let mut outputs = Vec::<OutputSpan>::new();
    let mut controls = Vec::<(usize, usize)>::new();
    sanitize_stream(
        &direct_children(root),
        &mut sanitized,
        &mut outputs,
        &mut controls,
    );

    let sanitized = String::from_utf8(sanitized).expect("sanitized template is utf-8");
    let tree = parse_yaml_tree(&sanitized);
    let mut attribution = AttributionIndex::default();

    for output in outputs {
        let global_context = tree.as_ref().and_then(|tree| {
            resolve_output_context(
                tree.root_node(),
                &sanitized,
                output.node_start,
                &output.placeholder,
                &YamlPath(Vec::new()),
            )
        });
        let local_context =
            resolve_local_output_context(&sanitized, output.action_start, &output.placeholder);

        if let Some(context) = merge_resolved_contexts(global_context, local_context) {
            let context = if output.node_start >= output.action_start
                && output.node_end <= output.action_end
            {
                context
            } else {
                ResolvedNodeContext::default()
            };
            attribution
                .output_nodes
                .insert((output.action_start, output.action_end), context.clone());
            attribution
                .output_nodes
                .insert((output.node_start, output.node_end), context);
        }
    }

    if let Some(tree) = tree.as_ref() {
        let root = tree.root_node();
        for (start, end) in controls {
            if let Some(context) =
                resolve_control_context(root, &sanitized, start, &YamlPath(Vec::new()))
            {
                attribution.control_nodes.insert((start, end), context);
            }
        }
    }

    attribution
}

fn merge_resolved_contexts(
    global: Option<ResolvedNodeContext>,
    local: Option<ResolvedNodeContext>,
) -> Option<ResolvedNodeContext> {
    match (global, local) {
        (Some(mut global), Some(local)) => {
            global.in_mapping_key |= local.in_mapping_key;
            global.entire_scalar_value |= local.entire_scalar_value;
            global.inside_block_scalar |= local.inside_block_scalar;
            Some(global)
        }
        (Some(global), None) => Some(global),
        (None, Some(local)) => Some(local),
        (None, None) => None,
    }
}

fn resolve_local_output_context(
    sanitized: &str,
    action_start: usize,
    placeholder: &str,
) -> Option<ResolvedNodeContext> {
    let line_start = sanitized[..action_start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line_end = sanitized[action_start..]
        .find('\n')
        .map_or(sanitized.len(), |index| action_start + index);
    let line = &sanitized[line_start..line_end];
    let placeholder_byte = line.find(placeholder)?;
    let mut snippet = line.to_string();
    snippet.push('\n');
    let tree = parse_yaml_tree(&snippet)?;
    let context = resolve_output_context(
        tree.root_node(),
        &snippet,
        placeholder_byte,
        placeholder,
        &YamlPath(Vec::new()),
    )?;
    Some(ResolvedNodeContext {
        current_path: YamlPath(Vec::new()),
        output_path: YamlPath(Vec::new()),
        mapping_entry_path: YamlPath(Vec::new()),
        in_mapping_key: context.in_mapping_key,
        entire_scalar_value: context.entire_scalar_value,
        inside_block_scalar: context.inside_block_scalar,
    })
}

#[derive(Clone)]
struct ChildNode<'tree> {
    node: tree_sitter::Node<'tree>,
    field_name: Option<String>,
}

fn direct_children<'tree>(node: tree_sitter::Node<'tree>) -> Vec<ChildNode<'tree>> {
    let mut children = Vec::new();
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            children.push(ChildNode {
                node: cursor.node(),
                field_name: cursor.field_name().map(std::string::ToString::to_string),
            });
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    children
}

fn sanitize_stream(
    children: &[ChildNode<'_>],
    sanitized: &mut [u8],
    outputs: &mut Vec<OutputSpan>,
    controls: &mut Vec<(usize, usize)>,
) {
    let mut index = 0usize;
    while index < children.len() {
        let child = &children[index];
        let node = child.node;

        if matches!(
            node.kind(),
            "if_action" | "with_action" | "range_action" | "define_action" | "block_action"
        ) {
            sanitize_control_node(node, sanitized, outputs, controls);
            index += 1;
            continue;
        }

        if is_template_delim_start(node.kind()) {
            let mut end_index = index + 1;
            while end_index < children.len()
                && !is_template_delim_end(children[end_index].node.kind())
            {
                end_index += 1;
            }

            if end_index < children.len() {
                let start = node.start_byte();
                let end = children[end_index].node.end_byte();
                let named_inner = children[index + 1..end_index]
                    .iter()
                    .find(|child| {
                        child.node.is_named()
                            && child.node.kind() != "comment"
                            && is_output_root_kind(child.node.kind())
                    })
                    .map(|child| child.node);
                if let Some(output_root) = named_inner {
                    let token = placeholder_token(outputs.len(), end.saturating_sub(start));
                    fill_placeholder(sanitized, start, end, &token);
                    outputs.push(OutputSpan {
                        node_start: output_root.start_byte(),
                        node_end: output_root.end_byte(),
                        action_start: start,
                        action_end: end,
                        placeholder: token,
                    });
                } else {
                    blank_range(sanitized, start, end);
                }
                index = end_index + 1;
                continue;
            }
        }

        if node.is_named() && node.kind() == "comment" {
            blank_range(sanitized, node.start_byte(), node.end_byte());
        }

        index += 1;
    }
}

fn sanitize_control_node(
    node: tree_sitter::Node<'_>,
    sanitized: &mut [u8],
    outputs: &mut Vec<OutputSpan>,
    controls: &mut Vec<(usize, usize)>,
) {
    controls.push((node.start_byte(), node.end_byte()));
    let kept_fields: &[&str] = match node.kind() {
        "if_action" => &["consequence", "alternative", "option"],
        "with_action" => &["consequence", "alternative"],
        "range_action" => &["body", "alternative"],
        "define_action" | "block_action" => &["body"],
        _ => &[],
    };

    let children = direct_children(node);
    for child in &children {
        let start = child.node.start_byte();
        let end = child.node.end_byte();
        let keep = child
            .field_name
            .as_deref()
            .is_some_and(|field| kept_fields.contains(&field));
        if !keep {
            blank_range(sanitized, start, end);
        }
    }

    let kept_children = children
        .into_iter()
        .filter(|child| {
            child
                .field_name
                .as_deref()
                .is_some_and(|field| kept_fields.contains(&field))
        })
        .collect::<Vec<_>>();
    sanitize_stream(&kept_children, sanitized, outputs, controls);
}

fn placeholder_token(index: usize, len: usize) -> String {
    let base = format!("{PLACEHOLDER_PREFIX}{}_", base36(index));
    if base.len() >= len {
        base[..len].to_string()
    } else {
        let mut token = base;
        token.push_str(&"x".repeat(len - token.len()));
        token
    }
}

fn base36(mut value: usize) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }

    let mut out = Vec::new();
    while value > 0 {
        out.push(DIGITS[value % 36]);
        value /= 36;
    }
    out.reverse();
    String::from_utf8(out).expect("base36 output is ascii")
}

fn fill_placeholder(sanitized: &mut [u8], start: usize, end: usize, token: &str) {
    blank_range(sanitized, start, end);
    let end = end.min(sanitized.len());
    let start = start.min(end);
    for (offset, byte) in token.as_bytes().iter().enumerate() {
        if start + offset >= end {
            break;
        }
        sanitized[start + offset] = *byte;
    }
}

fn blank_range(sanitized: &mut [u8], start: usize, end: usize) {
    let end = end.min(sanitized.len());
    let start = start.min(end);
    for byte in &mut sanitized[start..end] {
        if *byte != b'\n' && *byte != b'\r' {
            *byte = b' ';
        }
    }
}

fn resolve_output_context(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    placeholder: &str,
    path: &YamlPath,
) -> Option<ResolvedNodeContext> {
    if !contains_byte(node, byte) {
        return None;
    }

    match node.kind() {
        "stream" | "document" | "block_node" | "flow_node" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if !child.is_named() {
                    continue;
                }
                if let Some(context) =
                    resolve_output_context(child, source, byte, placeholder, path)
                {
                    return Some(context);
                }
            }
            Some(default_context(path))
        }
        "block_mapping" | "flow_mapping" => {
            let pair_kind = if node.kind() == "block_mapping" {
                "block_mapping_pair"
            } else {
                "flow_pair"
            };
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named()
                    && child.kind() == pair_kind
                    && let Some(context) =
                        resolve_output_context(child, source, byte, placeholder, path)
                {
                    return Some(context);
                }
            }
            Some(default_context(path))
        }
        "block_mapping_pair" | "flow_pair" => {
            let key = node.child_by_field_name("key");
            let value = node.child_by_field_name("value");
            let key_text = key.and_then(|node| scalar_text(node, source));
            let child_path = if key_text
                .as_deref()
                .is_some_and(|text| text.contains(PLACEHOLDER_PREFIX))
            {
                path.clone()
            } else if let Some(key_text) = key_text.as_deref() {
                append_mapping_segment(path, key_text)
            } else {
                path.clone()
            };

            if let Some(key) = key
                && contains_byte(key, byte)
            {
                if key_text.as_deref() == Some(placeholder) {
                    return Some(ResolvedNodeContext {
                        current_path: path.clone(),
                        output_path: path.clone(),
                        mapping_entry_path: path.clone(),
                        in_mapping_key: false,
                        entire_scalar_value: true,
                        inside_block_scalar: false,
                    });
                }
                return Some(ResolvedNodeContext {
                    current_path: path.clone(),
                    output_path: YamlPath(Vec::new()),
                    mapping_entry_path: path.clone(),
                    in_mapping_key: true,
                    entire_scalar_value: false,
                    inside_block_scalar: false,
                });
            }

            if let Some(value) = value
                && contains_byte(value, byte)
            {
                if is_scalar_like(value.kind()) {
                    return Some(resolve_scalar_context(
                        value,
                        source,
                        placeholder,
                        &child_path,
                    ));
                }
                if let Some(context) =
                    resolve_output_context(value, source, byte, placeholder, &child_path)
                {
                    return Some(context);
                }
            }

            Some(ResolvedNodeContext {
                current_path: child_path.clone(),
                output_path: child_path.clone(),
                mapping_entry_path: child_path,
                in_mapping_key: false,
                entire_scalar_value: false,
                inside_block_scalar: false,
            })
        }
        "block_sequence" | "flow_sequence" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if !child.is_named() {
                    continue;
                }
                if matches!(
                    child.kind(),
                    "block_sequence_item" | "flow_node" | "flow_pair"
                ) && let Some(context) =
                    resolve_output_sequence_child(child, source, byte, placeholder, path)
                {
                    return Some(context);
                }
            }
            Some(default_context(path))
        }
        "block_sequence_item" => {
            resolve_output_sequence_child(node, source, byte, placeholder, path)
                .or_else(|| Some(default_context(path)))
        }
        "block_scalar" => Some(ResolvedNodeContext {
            current_path: path.clone(),
            output_path: YamlPath(Vec::new()),
            mapping_entry_path: path.clone(),
            in_mapping_key: false,
            entire_scalar_value: false,
            inside_block_scalar: true,
        }),
        kind if is_scalar_like(kind) => {
            Some(resolve_scalar_context(node, source, placeholder, path))
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if !child.is_named() {
                    continue;
                }
                if let Some(context) =
                    resolve_output_context(child, source, byte, placeholder, path)
                {
                    return Some(context);
                }
            }
            Some(default_context(path))
        }
    }
}

fn resolve_control_context(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    path: &YamlPath,
) -> Option<ResolvedNodeContext> {
    if !contains_byte(node, byte) {
        return None;
    }

    match node.kind() {
        "stream" | "document" | "block_node" | "flow_node" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if !child.is_named() {
                    continue;
                }
                if let Some(context) = resolve_control_context(child, source, byte, path) {
                    return Some(context);
                }
            }
            Some(default_context(path))
        }
        "block_mapping" | "flow_mapping" => {
            let pair_kind = if node.kind() == "block_mapping" {
                "block_mapping_pair"
            } else {
                "flow_pair"
            };
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named()
                    && child.kind() == pair_kind
                    && let Some(context) = resolve_control_context(child, source, byte, path)
                {
                    return Some(context);
                }
            }
            Some(default_context(path))
        }
        "block_mapping_pair" | "flow_pair" => {
            let key = node.child_by_field_name("key");
            let value = node.child_by_field_name("value");
            let key_text = key.and_then(|node| scalar_text(node, source));
            let child_path = if key_text
                .as_deref()
                .is_some_and(|text| text.contains(PLACEHOLDER_PREFIX))
            {
                path.clone()
            } else if let Some(key_text) = key_text {
                append_mapping_segment(path, &key_text)
            } else {
                path.clone()
            };

            if let Some(value) = value
                && contains_byte(value, byte)
            {
                if is_scalar_like(value.kind()) {
                    return Some(default_context(&child_path));
                }
                if let Some(context) = resolve_control_context(value, source, byte, &child_path) {
                    return Some(context);
                }
            }

            Some(default_context(&child_path))
        }
        "block_sequence" | "flow_sequence" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if !child.is_named() {
                    continue;
                }
                if matches!(
                    child.kind(),
                    "block_sequence_item" | "flow_node" | "flow_pair"
                ) && let Some(context) = resolve_control_context(child, source, byte, path)
                {
                    return Some(context);
                }
            }
            Some(default_context(path))
        }
        "block_sequence_item" => {
            if let Some(child) = node.named_child(0) {
                let child = unwrap_yaml_node(child);
                if is_scalar_like(child.kind()) && contains_byte(child, byte) {
                    return Some(default_context(path));
                }
                if child.kind() == "block_scalar" {
                    return Some(ResolvedNodeContext {
                        current_path: path.clone(),
                        output_path: YamlPath(Vec::new()),
                        mapping_entry_path: path.clone(),
                        in_mapping_key: false,
                        entire_scalar_value: false,
                        inside_block_scalar: true,
                    });
                }
                let seq_path = append_sequence_segment(path);
                if let Some(context) = resolve_control_context(child, source, byte, &seq_path) {
                    return Some(context);
                }
            }
            Some(default_context(path))
        }
        "block_scalar" => Some(ResolvedNodeContext {
            current_path: path.clone(),
            output_path: YamlPath(Vec::new()),
            mapping_entry_path: path.clone(),
            in_mapping_key: false,
            entire_scalar_value: false,
            inside_block_scalar: true,
        }),
        _ => Some(default_context(path)),
    }
}

fn resolve_output_sequence_child(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    placeholder: &str,
    path: &YamlPath,
) -> Option<ResolvedNodeContext> {
    if !contains_byte(node, byte) {
        return None;
    }

    let is_block_sequence_item = node.kind() == "block_sequence_item";
    let child = if is_block_sequence_item {
        node.named_child(0).map(unwrap_yaml_node)?
    } else {
        unwrap_yaml_node(node)
    };

    if is_scalar_like(child.kind()) && contains_byte(child, byte) {
        let mut context = resolve_scalar_context(child, source, placeholder, path);
        if is_block_sequence_item || context.entire_scalar_value {
            let item_path = append_sequence_segment(path);
            context.current_path = item_path.clone();
            context.output_path = item_path.clone();
            context.mapping_entry_path = item_path;
        }
        return Some(context);
    }

    if child.kind() == "block_scalar" {
        return Some(ResolvedNodeContext {
            current_path: path.clone(),
            output_path: YamlPath(Vec::new()),
            mapping_entry_path: path.clone(),
            in_mapping_key: false,
            entire_scalar_value: false,
            inside_block_scalar: true,
        });
    }

    let seq_path = append_sequence_segment(path);
    resolve_output_context(child, source, byte, placeholder, &seq_path)
}

fn resolve_scalar_context(
    node: tree_sitter::Node<'_>,
    source: &str,
    placeholder: &str,
    path: &YamlPath,
) -> ResolvedNodeContext {
    let text = node.utf8_text(source.as_bytes()).unwrap_or("").trim();
    let text = strip_scalar_quotes(text);
    ResolvedNodeContext {
        current_path: path.clone(),
        output_path: path.clone(),
        mapping_entry_path: path.clone(),
        in_mapping_key: false,
        entire_scalar_value: text == placeholder,
        inside_block_scalar: false,
    }
}

fn default_context(path: &YamlPath) -> ResolvedNodeContext {
    ResolvedNodeContext {
        current_path: path.clone(),
        output_path: path.clone(),
        mapping_entry_path: path.clone(),
        in_mapping_key: false,
        entire_scalar_value: false,
        inside_block_scalar: false,
    }
}

fn append_mapping_segment(path: &YamlPath, key: &str) -> YamlPath {
    let mut path = path.clone();
    if !key.is_empty() {
        path.0.push(key.to_string());
    }
    path
}

fn append_sequence_segment(path: &YamlPath) -> YamlPath {
    let mut path = path.clone();
    if let Some(last) = path.0.last_mut() {
        if !last.ends_with("[*]") {
            last.push_str("[*]");
        }
    } else {
        path.0.push("[*]".to_string());
    }
    path
}

fn contains_byte(node: tree_sitter::Node<'_>, byte: usize) -> bool {
    node.start_byte() <= byte && byte < node.end_byte()
}

fn is_template_delim_start(kind: &str) -> bool {
    kind == "{{" || kind == "{{-"
}

fn is_template_delim_end(kind: &str) -> bool {
    kind == "}}" || kind == "-}}"
}

pub(super) fn is_output_root_kind(kind: &str) -> bool {
    matches!(
        kind,
        "template_action"
            | "dot"
            | "variable"
            | "field"
            | "chained_pipeline"
            | "parenthesized_pipeline"
            | "selector_expression"
            | "function_call"
            | "method_call"
    )
}

#[cfg(test)]
mod tests {
    use super::placeholder_token;

    #[test]
    fn short_placeholder_tokens_remain_distinct_for_dense_inline_actions() {
        let tokens = (0..36)
            .map(|index| placeholder_token(index, 5))
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(tokens.len(), 36);
        assert!(tokens.iter().all(|token| token.starts_with("__HS")));
    }
}
