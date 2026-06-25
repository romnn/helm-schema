use std::collections::HashMap;

use helm_schema_ast::{TemplateExpr, parse_action_expressions};

use crate::fragment_range_scope::range_body_mapping_entry_indent_from_source;
use crate::yaml_syntax::first_mapping_colon_offset;
use crate::{ValueKind, YamlPath};

use super::{ControlSite, OutputSlot, OutputSlotKind};

const PLACEHOLDER_PREFIX: &str = "__HS";

#[derive(Clone, Debug, Default)]
pub(super) struct ResolvedNodeContext {
    pub(super) current_path: YamlPath,
    pub(super) output_path: YamlPath,
    pub(super) mapping_entry_path: YamlPath,
    pub(super) in_mapping_key: bool,
    pub(super) entire_scalar_value: bool,
    pub(super) inside_block_scalar: bool,
}

#[derive(Default)]
pub(super) struct AttributionIndex {
    output_slots: HashMap<(usize, usize), OutputSlot>,
    control_sites: HashMap<(usize, usize), ControlSite>,
}

impl AttributionIndex {
    pub(super) fn output_slot_for_node(
        &self,
        mut node: tree_sitter::Node<'_>,
    ) -> Option<OutputSlot> {
        loop {
            if let Some(slot) = self.output_slots.get(&(node.start_byte(), node.end_byte())) {
                return Some(slot.clone());
            }
            node = node.parent()?;
        }
    }

    pub(super) fn control_site_for_node(
        &self,
        mut node: tree_sitter::Node<'_>,
    ) -> Option<ControlSite> {
        loop {
            if let Some(site) = self
                .control_sites
                .get(&(node.start_byte(), node.end_byte()))
            {
                return Some(site.clone());
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
    structural_indent: Option<usize>,
    kind: ValueKind,
}

#[derive(Clone, Copy)]
struct OutputActionShape {
    kind: ValueKind,
    structural_indent: Option<usize>,
    may_inject_yaml_structure: bool,
    uses_structural_indent_filter: bool,
}

struct ControlSpan {
    span_start: usize,
    span_end: usize,
    context_byte: usize,
    mapping_entry_indent: Option<usize>,
}

pub(super) fn build_attribution_index(
    source: &str,
    root: tree_sitter::Node<'_>,
) -> AttributionIndex {
    let mut sanitized = source.as_bytes().to_vec();
    let mut outputs = Vec::<OutputSpan>::new();
    let mut controls = Vec::<ControlSpan>::new();
    sanitize_stream(
        source,
        &direct_children(root),
        &mut sanitized,
        &mut outputs,
        &mut controls,
    );
    outputs.sort_by_key(|output| output.action_start);

    let sanitized = String::from_utf8(sanitized).expect("sanitized template is utf-8");
    let mut attribution = AttributionIndex::default();

    for output in outputs {
        let context = output_context(&sanitized, &output);

        if let Some(context) = context {
            let context = if output.node_start >= output.action_start
                && output.node_end <= output.action_end
            {
                context
            } else {
                ResolvedNodeContext::default()
            };
            let action_slot =
                output_slot_from_context(&output, output.action_start, &context, source);
            let node_slot = output_slot_from_context(&output, output.node_start, &context, source);
            attribution
                .output_slots
                .insert((output.action_start, output.action_end), action_slot);
            attribution
                .output_slots
                .insert((output.node_start, output.node_end), node_slot);
        }
    }

    for control in controls {
        let control_context = line_context_at(&sanitized, control.context_byte, None);

        let range_mapping_entry_path = control.mapping_entry_indent.and_then(|indent| {
            structural_context_before(&sanitized, control.context_byte, indent)
                .or_else(|| control_context.clone())
                .map(|context| context.mapping_entry_path)
        });

        let control_path = control_context.map(|context| {
            if context.inside_block_scalar {
                YamlPath(Vec::new())
            } else {
                context.current_path
            }
        });

        if control_path.is_some() || range_mapping_entry_path.is_some() {
            attribution.control_sites.insert(
                (control.span_start, control.span_end),
                ControlSite {
                    path: control_path.unwrap_or_else(|| YamlPath(Vec::new())),
                    range_mapping_entry_path,
                },
            );
        }
    }

    attribution
}

fn output_slot_from_context(
    output: &OutputSpan,
    start: usize,
    context: &ResolvedNodeContext,
    source: &str,
) -> OutputSlot {
    let mut path = if context.in_mapping_key || context.inside_block_scalar {
        YamlPath(Vec::new())
    } else {
        context.output_path.clone()
    };
    if output.kind == ValueKind::Fragment
        && let Some(last) = path.0.last_mut()
        && let Some(stripped) = last.strip_suffix("[*]")
    {
        *last = stripped.to_string();
    }

    let in_yaml_comment = document_site_is_yaml_comment_part(source, start);
    let slot = output_slot_kind(output.kind, &path, context, in_yaml_comment);

    OutputSlot {
        kind: output.kind,
        path,
        resource: None,
        slot,
    }
}

fn output_slot_kind(
    output_kind: ValueKind,
    path: &YamlPath,
    context: &ResolvedNodeContext,
    in_yaml_comment: bool,
) -> OutputSlotKind {
    if context.in_mapping_key {
        OutputSlotKind::MappingKey
    } else if in_yaml_comment {
        OutputSlotKind::YamlComment
    } else if context.inside_block_scalar {
        OutputSlotKind::BlockScalarSuppressed
    } else if output_kind == ValueKind::Fragment {
        OutputSlotKind::FragmentInsertion
    } else if context.entire_scalar_value {
        OutputSlotKind::WholeScalar
    } else if !path.0.is_empty() {
        OutputSlotKind::PartialScalar
    } else {
        OutputSlotKind::Opaque
    }
}

fn document_site_is_yaml_comment_part(source: &str, start: usize) -> bool {
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    source[line_start..start].trim_start().starts_with('#')
}

fn output_context(sanitized: &str, output: &OutputSpan) -> Option<ResolvedNodeContext> {
    let line_context = line_context_at(sanitized, output.node_start, Some(&output.placeholder));
    if line_context
        .as_ref()
        .is_some_and(|context| context.inside_block_scalar)
    {
        return line_context;
    }
    output
        .structural_indent
        .and_then(|indent| structural_context_before(sanitized, output.action_start, indent))
        .or(line_context)
}

#[derive(Clone)]
struct StructuralSlot {
    indent: usize,
    path: YamlPath,
    allow_same_indent_output: bool,
    block_scalar: bool,
}

fn structural_context_before(
    sanitized: &str,
    insertion_byte: usize,
    output_indent: usize,
) -> Option<ResolvedNodeContext> {
    if output_indent == 0 {
        return Some(default_context(&YamlPath(Vec::new())));
    }

    let insertion_byte = insertion_byte.min(sanitized.len());
    let slots = structural_slot_stack_before(&sanitized[..insertion_byte]);
    let slot = slots
        .iter()
        .rev()
        .find(|slot| {
            slot.indent < output_indent
                || (slot.indent == output_indent && slot.allow_same_indent_output)
        })
        .or_else(|| slots.last())?;
    Some(default_context(&slot.path))
}

fn structural_slot_stack_before(prefix: &str) -> Vec<StructuralSlot> {
    let mut slots = Vec::new();
    for line in prefix.lines() {
        push_structural_line(line, &mut slots);
    }
    slots
}

fn push_structural_line(line: &str, slots: &mut Vec<StructuralSlot>) {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return;
    }

    let indent = line.len() - trimmed.len();
    if slots
        .last()
        .is_some_and(|slot| slot.block_scalar && indent > slot.indent)
    {
        return;
    }
    let Some(after_dash) = trimmed.strip_prefix('-') else {
        pop_closed_slots(slots, indent);
        mark_parent_slot_with_child(slots, indent);
        push_mapping_slot(trimmed, indent, slots);
        return;
    };

    pop_closed_slots_before_sequence_item(slots, indent);
    if !after_dash.is_empty() && !after_dash.starts_with(char::is_whitespace) {
        return;
    }

    mark_parent_slot_with_child(slots, indent);
    let parent_path = slots
        .last()
        .map(|slot| slot.path.clone())
        .unwrap_or_else(|| YamlPath(Vec::new()));
    let item_path = append_sequence_segment(&parent_path);
    let nested = after_dash.trim_start();
    let block_scalar = nested.starts_with('|') || nested.starts_with('>');
    slots.push(StructuralSlot {
        indent,
        path: item_path,
        allow_same_indent_output: false,
        block_scalar,
    });

    if !nested.is_empty() && !block_scalar {
        push_mapping_slot(nested, indent + 2, slots);
    }
}

fn pop_closed_slots(slots: &mut Vec<StructuralSlot>, indent: usize) {
    while slots.last().is_some_and(|slot| slot.indent >= indent) {
        slots.pop();
    }
}

fn pop_closed_slots_before_sequence_item(slots: &mut Vec<StructuralSlot>, indent: usize) {
    while slots.last().is_some_and(|slot| {
        slot.indent > indent || (slot.indent == indent && !slot.allow_same_indent_output)
    }) {
        slots.pop();
    }
}

fn mark_parent_slot_with_child(slots: &mut [StructuralSlot], indent: usize) {
    if let Some(slot) = slots.iter_mut().rev().find(|slot| slot.indent < indent) {
        slot.allow_same_indent_output = false;
    }
}

fn push_mapping_slot(text: &str, indent: usize, slots: &mut Vec<StructuralSlot>) {
    let Some(colon) = first_mapping_colon_offset(text) else {
        return;
    };
    if !mapping_colon_is_structural(text, colon) {
        return;
    }
    let value = text[colon + 1..].trim();
    let block_scalar = value.starts_with('|') || value.starts_with('>');
    if !value.is_empty() && !block_scalar {
        return;
    }
    let key = strip_scalar_quotes(text[..colon].trim());
    if key.is_empty() || key.contains(PLACEHOLDER_PREFIX) {
        return;
    }
    let parent_path = slots
        .last()
        .map(|slot| slot.path.clone())
        .unwrap_or_else(|| YamlPath(Vec::new()));
    slots.push(StructuralSlot {
        indent,
        path: append_mapping_segment(&parent_path, key),
        allow_same_indent_output: value.is_empty(),
        block_scalar,
    })
}

fn line_context_at(
    sanitized: &str,
    byte: usize,
    placeholder: Option<&str>,
) -> Option<ResolvedNodeContext> {
    let byte = byte.min(sanitized.len());
    let (line_start, line_end) = line_bounds(sanitized, byte);
    let line = &sanitized[line_start..line_end];
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let prior_slots = structural_slot_stack_before(&sanitized[..line_start]);
    if let Some(slot) = prior_slots
        .last()
        .filter(|slot| slot.block_scalar && indent > slot.indent)
    {
        return Some(block_scalar_context(&slot.path));
    }
    if trimmed.starts_with('#') {
        return Some(default_context(&YamlPath(Vec::new())));
    }
    if trimmed.is_empty() {
        return structural_context_for_line(sanitized, line_start, 0, false);
    }

    let is_sequence_item = valid_sequence_item(trimmed);
    let parent_context =
        structural_context_for_line(sanitized, line_start, indent, is_sequence_item)
            .unwrap_or_else(|| default_context(&YamlPath(Vec::new())));
    let parent_path = parent_context.current_path;

    if is_sequence_item {
        let item_path = append_sequence_segment(&parent_path);
        let nested = trimmed[1..].trim_start();
        if placeholder.is_none() && !starts_with_inline_mapping(nested) {
            return Some(default_context(&parent_path));
        }
        return Some(context_from_line_text(
            nested,
            &item_path,
            placeholder,
            line_start + indent + 1 + trimmed[1..].len() - nested.len(),
            byte,
        ));
    }

    Some(context_from_line_text(
        trimmed,
        &parent_path,
        placeholder,
        line_start + indent,
        byte,
    ))
}

fn line_bounds(source: &str, byte: usize) -> (usize, usize) {
    let start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let end = source[byte..]
        .find('\n')
        .map_or(source.len(), |offset| byte + offset);
    (start, end)
}

fn structural_context_for_line(
    sanitized: &str,
    line_start: usize,
    indent: usize,
    is_sequence_item: bool,
) -> Option<ResolvedNodeContext> {
    let mut slots = structural_slot_stack_before(&sanitized[..line_start]);
    if is_sequence_item {
        pop_closed_slots_before_sequence_item(&mut slots, indent);
    } else {
        pop_closed_slots(&mut slots, indent);
    }
    slots.last().map(|slot| default_context(&slot.path))
}

fn valid_sequence_item(trimmed: &str) -> bool {
    let Some(after_dash) = trimmed.strip_prefix('-') else {
        return false;
    };
    after_dash.is_empty() || after_dash.starts_with(char::is_whitespace)
}

fn starts_with_inline_mapping(text: &str) -> bool {
    first_mapping_colon_offset(text).is_some_and(|colon| mapping_colon_is_structural(text, colon))
}

fn context_from_line_text(
    text: &str,
    parent_path: &YamlPath,
    placeholder: Option<&str>,
    text_start: usize,
    byte: usize,
) -> ResolvedNodeContext {
    let Some(colon) = first_mapping_colon_offset(text) else {
        return scalar_line_context(text, parent_path, placeholder);
    };
    if !mapping_colon_is_structural(text, colon) {
        return scalar_line_context(text, parent_path, placeholder);
    }

    let key_start = text_start;
    let key_end = text_start + colon;
    let key_text = strip_scalar_quotes(text[..colon].trim());
    let key_path = if key_text.contains(PLACEHOLDER_PREFIX) {
        parent_path.clone()
    } else {
        append_mapping_segment(parent_path, key_text)
    };

    if placeholder.is_some() && byte >= key_start && byte <= key_end {
        return mapping_key_context(parent_path, Some(key_text), placeholder.unwrap());
    }

    let value_text = text[colon + 1..].trim();
    ResolvedNodeContext {
        current_path: key_path.clone(),
        output_path: key_path.clone(),
        mapping_entry_path: key_path,
        in_mapping_key: false,
        entire_scalar_value: placeholder
            .is_some_and(|placeholder| strip_scalar_quotes(value_text) == placeholder),
        inside_block_scalar: false,
    }
}

fn scalar_line_context(
    text: &str,
    path: &YamlPath,
    placeholder: Option<&str>,
) -> ResolvedNodeContext {
    let value = text.trim();
    ResolvedNodeContext {
        current_path: path.clone(),
        output_path: path.clone(),
        mapping_entry_path: path.clone(),
        in_mapping_key: false,
        entire_scalar_value: placeholder
            .is_some_and(|placeholder| strip_scalar_quotes(value) == placeholder),
        inside_block_scalar: false,
    }
}

fn mapping_colon_is_structural(text: &str, colon: usize) -> bool {
    text[colon + 1..]
        .chars()
        .next()
        .is_none_or(char::is_whitespace)
}

fn inline_mapping_value_key_offset(prefix: &str) -> Option<usize> {
    let text = prefix.trim_end();
    let colon = first_mapping_colon_offset(text)?;
    if !text[colon + 1..].trim().is_empty() {
        return None;
    }

    if let Some(after_dash) = text.strip_prefix('-') {
        let whitespace = after_dash.len() - after_dash.trim_start().len();
        if whitespace == 0 {
            None
        } else {
            Some(1 + whitespace)
        }
    } else {
        Some(0)
    }
}

fn direct_children<'tree>(node: tree_sitter::Node<'tree>) -> Vec<tree_sitter::Node<'tree>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

fn enclosing_template_action_span(mut node: tree_sitter::Node<'_>) -> (usize, usize) {
    loop {
        if node.kind() == "template_action" {
            return (node.start_byte(), node.end_byte());
        }
        let Some(parent) = node.parent() else {
            return (node.start_byte(), node.end_byte());
        };
        node = parent;
    }
}

fn sanitize_output_action(
    sanitized: &mut [u8],
    start: usize,
    end: usize,
    token: &str,
    shape: OutputActionShape,
) {
    if action_is_root_standalone_line(sanitized, start, end)
        || (action_is_standalone_line(sanitized, start, end) && shape.may_inject_yaml_structure)
        || (shape.uses_structural_indent_filter
            && (action_is_standalone_line(sanitized, start, end)
                || action_is_inline_mapping_value(sanitized, start)))
    {
        blank_range(sanitized, start, end);
    } else {
        fill_placeholder(sanitized, start, end, token);
    }
}

fn output_action_shape(sanitized: &[u8], start: usize, end: usize) -> OutputActionShape {
    let Ok(text) =
        std::str::from_utf8(&sanitized[start.min(sanitized.len())..end.min(sanitized.len())])
    else {
        return OutputActionShape {
            kind: ValueKind::Scalar,
            structural_indent: None,
            may_inject_yaml_structure: false,
            uses_structural_indent_filter: false,
        };
    };
    let exprs = parse_action_expressions(text);
    OutputActionShape {
        kind: if exprs.iter().any(TemplateExpr::renders_yaml_fragment) {
            ValueKind::Fragment
        } else {
            ValueKind::Scalar
        },
        structural_indent: exprs
            .iter()
            .rev()
            .find_map(TemplateExpr::fragment_indent_width),
        may_inject_yaml_structure: exprs.iter().any(TemplateExpr::may_inject_yaml_structure),
        uses_structural_indent_filter: exprs.iter().any(expr_uses_indent_filter),
    }
}

fn action_is_standalone_line(sanitized: &[u8], start: usize, end: usize) -> bool {
    let start = start.min(sanitized.len());
    let end = end.min(sanitized.len());
    let line_start = sanitized[..start]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let line_end = sanitized[end..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(sanitized.len(), |index| end + index);

    sanitized[line_start..start]
        .iter()
        .all(|byte| matches!(byte, b' ' | b'\t'))
        && sanitized[end..line_end]
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t'))
}

fn action_is_root_standalone_line(sanitized: &[u8], start: usize, end: usize) -> bool {
    let start = start.min(sanitized.len());
    let line_start = sanitized[..start]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    line_start == start && action_is_standalone_line(sanitized, start, end)
}

fn action_is_inline_mapping_value(sanitized: &[u8], start: usize) -> bool {
    let start = start.min(sanitized.len());
    let line_start = sanitized[..start]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let prefix = &sanitized[line_start..start];
    if prefix.iter().all(|byte| matches!(byte, b' ' | b'\t')) {
        return false;
    }

    let Ok(prefix) = std::str::from_utf8(prefix) else {
        return false;
    };
    let indent = prefix.chars().take_while(|&ch| ch == ' ').count();
    inline_mapping_value_key_offset(&prefix[indent..]).is_some()
}

fn expr_uses_indent_filter(expr: &TemplateExpr) -> bool {
    let mut found = false;
    expr.walk(|node| {
        if found {
            return;
        }
        if let TemplateExpr::Call { function, .. } = node
            && matches!(function.as_str(), "indent" | "nindent")
        {
            found = true;
        }
    });
    found
}

fn sanitize_stream(
    source: &str,
    children: &[tree_sitter::Node<'_>],
    sanitized: &mut [u8],
    outputs: &mut Vec<OutputSpan>,
    controls: &mut Vec<ControlSpan>,
) {
    let mut index = 0usize;
    while index < children.len() {
        let node = children[index];

        if matches!(
            node.kind(),
            "if_action" | "with_action" | "range_action" | "define_action" | "block_action"
        ) {
            sanitize_control_node(source, node, sanitized, outputs, controls);
            index += 1;
            continue;
        }

        if node.is_named() && is_output_root_kind(node.kind()) {
            let (action_start, action_end) = enclosing_template_action_span(node);
            let shape = output_action_shape(sanitized, action_start, action_end);
            let token = placeholder_token(outputs.len(), action_end.saturating_sub(action_start));
            sanitize_output_action(sanitized, action_start, action_end, &token, shape);
            outputs.push(OutputSpan {
                node_start: node.start_byte(),
                node_end: node.end_byte(),
                action_start,
                action_end,
                placeholder: token,
                structural_indent: shape.structural_indent,
                kind: shape.kind,
            });
            index += 1;
            continue;
        }

        if is_template_delim_start(node.kind()) {
            let mut end_index = index + 1;
            while end_index < children.len() && !is_template_delim_end(children[end_index].kind()) {
                end_index += 1;
            }

            if end_index < children.len() {
                let start = node.start_byte();
                let end = children[end_index].end_byte();
                let named_inner = children[index + 1..end_index]
                    .iter()
                    .find(|child| {
                        child.is_named()
                            && child.kind() != "comment"
                            && is_output_root_kind(child.kind())
                    })
                    .copied();
                if let Some(output_root) = named_inner {
                    let shape = output_action_shape(sanitized, start, end);
                    let token = placeholder_token(outputs.len(), end.saturating_sub(start));
                    sanitize_output_action(sanitized, start, end, &token, shape);
                    outputs.push(OutputSpan {
                        node_start: output_root.start_byte(),
                        node_end: output_root.end_byte(),
                        action_start: start,
                        action_end: end,
                        placeholder: token,
                        structural_indent: shape.structural_indent,
                        kind: shape.kind,
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
    source: &str,
    node: tree_sitter::Node<'_>,
    sanitized: &mut [u8],
    outputs: &mut Vec<OutputSpan>,
    controls: &mut Vec<ControlSpan>,
) {
    let kept_fields: &[&str] = match node.kind() {
        "if_action" => &["consequence", "alternative", "option"],
        "with_action" => &["consequence", "alternative"],
        "range_action" => &["body", "alternative"],
        "define_action" | "block_action" => &["body"],
        _ => &[],
    };

    let mut kept_children = Vec::new();
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            let keep = cursor
                .field_name()
                .is_some_and(|field| kept_fields.contains(&field));
            if !keep {
                blank_range(sanitized, child.start_byte(), child.end_byte());
            } else {
                kept_children.push(child);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    sanitize_stream(source, &kept_children, sanitized, outputs, controls);

    let context_byte = kept_children
        .iter()
        .find_map(|child| first_nonblank_byte(sanitized, child.start_byte(), child.end_byte()))
        .unwrap_or_else(|| node.start_byte());
    controls.push(ControlSpan {
        span_start: node.start_byte(),
        span_end: node.end_byte(),
        context_byte,
        mapping_entry_indent: (node.kind() == "range_action")
            .then(|| range_body_mapping_entry_indent_from_source(node, source))
            .flatten(),
    });
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

fn first_nonblank_byte(sanitized: &[u8], start: usize, end: usize) -> Option<usize> {
    let end = end.min(sanitized.len());
    let start = start.min(end);
    sanitized[start..end]
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        .map(|offset| start + offset)
}

fn mapping_key_context(
    path: &YamlPath,
    key_text: Option<&str>,
    placeholder: &str,
) -> ResolvedNodeContext {
    ResolvedNodeContext {
        current_path: path.clone(),
        output_path: if key_text == Some(placeholder) {
            path.clone()
        } else {
            YamlPath(Vec::new())
        },
        mapping_entry_path: path.clone(),
        in_mapping_key: key_text != Some(placeholder),
        entire_scalar_value: key_text == Some(placeholder),
        inside_block_scalar: false,
    }
}

fn block_scalar_context(path: &YamlPath) -> ResolvedNodeContext {
    ResolvedNodeContext {
        current_path: path.clone(),
        output_path: YamlPath(Vec::new()),
        mapping_entry_path: path.clone(),
        in_mapping_key: false,
        entire_scalar_value: false,
        inside_block_scalar: true,
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

fn strip_scalar_quotes(text: &str) -> &str {
    if text.len() >= 2
        && ((text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\'')))
    {
        &text[1..text.len() - 1]
    } else {
        text
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

fn is_template_delim_start(kind: &str) -> bool {
    matches!(kind, "{{" | "{{-")
}

fn is_template_delim_end(kind: &str) -> bool {
    matches!(kind, "}}" | "-}}")
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
#[path = "../../tests/document_projection/tracker/attribution.rs"]
mod tests;
