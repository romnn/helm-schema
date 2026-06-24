use std::collections::HashMap;

use helm_schema_ast::{TemplateExpr, parse_action_expressions};

use crate::fragment_range_scope::range_body_mapping_entry_indent_from_source;
use crate::tree_sitter_utils::parse_go_template;
use crate::yaml_syntax::{first_mapping_colon_offset, parse_yaml_key};
use crate::{SourceSpan, ValueKind, YamlPath};

use super::yaml_tree::{
    is_scalar_like, parse_yaml_tree, scalar_text, strip_scalar_quotes, unwrap_yaml_node,
};
use super::{ControlSite, OutputSlot, OutputSlotKind};

const PLACEHOLDER_PREFIX: &str = "__HS";

#[derive(Clone, Debug)]
pub(super) struct ResolvedNodeContext {
    pub(super) current_path: YamlPath,
    pub(super) output_path: YamlPath,
    pub(super) mapping_entry_path: YamlPath,
    pub(super) in_mapping_key: bool,
    pub(super) entire_scalar_value: bool,
    pub(super) inside_block_scalar: bool,
    explicit_mapping_value_slot: bool,
    pub(super) sequence_item_slot: bool,
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
            explicit_mapping_value_slot: false,
            sequence_item_slot: false,
        }
    }
}

#[derive(Default)]
pub(super) struct AttributionIndex {
    output_slots: HashMap<(usize, usize), OutputSlot>,
    control_sites: HashMap<(usize, usize), ControlSite>,
}

impl AttributionIndex {
    pub(super) fn output_slot_for_node(&self, node: tree_sitter::Node<'_>) -> Option<OutputSlot> {
        self.output_slot_for_node_or_ancestor(node)
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

    fn output_slot_for_node_or_ancestor(
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
    let tree = parse_yaml_tree(&sanitized);
    let mut attribution = AttributionIndex::default();
    let mut previous_output_paths = Vec::new();

    for output in outputs {
        let base_context = tree.as_ref().and_then(|tree| {
            resolve_yaml_context(
                tree.root_node(),
                &sanitized,
                output.node_start,
                ContextMode::Output {
                    placeholder: &output.placeholder,
                },
                &YamlPath(Vec::new()),
            )
        });
        let rendered_context = output.structural_indent.and_then(|indent| {
            resolve_structural_output_context(&sanitized, output.action_start, indent)
        });

        let context = if output.structural_indent.is_some() {
            if rendered_context_is_better_base(&base_context, &rendered_context) {
                rendered_context.clone().or(base_context)
            } else {
                base_context.or(rendered_context.clone())
            }
        } else {
            base_context
        };

        if let Some(context) = context {
            let context = if output.node_start >= output.action_start
                && output.node_end <= output.action_end
            {
                context
            } else {
                ResolvedNodeContext::default()
            };
            let context = rebase_relative_sequence_context(context, &previous_output_paths);
            if !context.output_path.0.is_empty() {
                previous_output_paths.push(context.output_path.clone());
            }
            let action_slot = output_slot_from_context(
                &output,
                output.action_start,
                output.action_end,
                &context,
                rendered_context.as_ref(),
                source,
            );
            let node_slot = output_slot_from_context(
                &output,
                output.node_start,
                output.node_end,
                &context,
                rendered_context.as_ref(),
                source,
            );
            attribution
                .output_slots
                .insert((output.action_start, output.action_end), action_slot);
            attribution
                .output_slots
                .insert((output.node_start, output.node_end), node_slot);
        }
    }

    if let Some(tree) = tree.as_ref() {
        let root = tree.root_node();
        for control in controls {
            let control_context = resolve_yaml_context(
                root,
                &sanitized,
                control.context_byte,
                ContextMode::Control,
                &YamlPath(Vec::new()),
            );

            let range_mapping_entry_path = control.mapping_entry_indent.and_then(|indent| {
                resolve_structural_output_context(&sanitized, control.context_byte, indent)
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
    }

    attribution
}

fn output_slot_from_context(
    output: &OutputSpan,
    start: usize,
    end: usize,
    context: &ResolvedNodeContext,
    rendered_context: Option<&ResolvedNodeContext>,
    source: &str,
) -> OutputSlot {
    let mut path = if context.in_mapping_key || context.inside_block_scalar {
        YamlPath(Vec::new())
    } else if output.kind == ValueKind::Fragment {
        prefer_fragment_output_path(context, rendered_context)
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
        source_span: SourceSpan::new(start, end),
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

fn prefer_fragment_output_path(
    current: &ResolvedNodeContext,
    rendered: Option<&ResolvedNodeContext>,
) -> YamlPath {
    let current_path = &current.output_path;
    let Some(rendered) = rendered else {
        return current_path.clone();
    };
    let rendered_path = &rendered.output_path;
    if current_path.0.is_empty() {
        return rendered_path.clone();
    }
    if rendered_path.0.is_empty() {
        return current_path.clone();
    }
    if path_has_equivalent_prefix(&current_path.0, &rendered_path.0) {
        if rendered.sequence_item_slot {
            return rendered_path.clone();
        }
        return current_path.clone();
    }
    if path_has_equivalent_prefix(&rendered_path.0, &current_path.0) {
        return preserve_specific_prefix(current_path, rendered_path);
    }
    rendered_path.clone()
}

fn preserve_specific_prefix(prefix: &YamlPath, path: &YamlPath) -> YamlPath {
    if prefix.0.is_empty() || prefix.0.len() > path.0.len() {
        return path.clone();
    }

    let mut merged = prefix.0.clone();
    merged.extend(path.0.iter().skip(prefix.0.len()).cloned());
    YamlPath(merged)
}

fn document_site_is_yaml_comment_part(source: &str, start: usize) -> bool {
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    source[line_start..start].trim_start().starts_with('#')
}

fn rendered_context_is_better_base(
    base: &Option<ResolvedNodeContext>,
    rendered: &Option<ResolvedNodeContext>,
) -> bool {
    let Some(rendered) = rendered else {
        return false;
    };
    if rendered.explicit_mapping_value_slot {
        return true;
    }
    let Some(base) = base else {
        return true;
    };
    rendered.output_path.0.len() > base.output_path.0.len()
        && path_has_equivalent_prefix(&rendered.output_path.0, &base.output_path.0)
}

struct SourceSlot {
    indent: usize,
    path: YamlPath,
    kind: SourceSlotKind,
    has_same_indent_sequence_value: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceSlotKind {
    MappingValue,
    SequenceItem,
}

fn resolve_structural_output_context(
    sanitized: &str,
    insertion_byte: usize,
    output_indent: usize,
) -> Option<ResolvedNodeContext> {
    let insertion_byte = insertion_byte.min(sanitized.len());
    if output_indent == 0 {
        return Some(default_context(&YamlPath(Vec::new())));
    }

    let line_start = sanitized[..insertion_byte]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let mut slots = Vec::new();
    for line in sanitized[..line_start].lines() {
        observe_source_slot_line(line, &mut slots);
    }

    let prefix = &sanitized[line_start..insertion_byte];
    let prefix_produced_slot = observe_source_slot_line(prefix, &mut slots);
    if prefix.trim().is_empty() {
        close_dedented_slots_for_action_prefix(prefix, output_indent, &mut slots);
    }
    if !prefix.trim().is_empty() && !prefix_produced_slot {
        return None;
    }

    let slot = slots
        .iter()
        .rev()
        .find(|slot| {
            slot.indent < output_indent
                || (slot.indent == output_indent && slot.kind == SourceSlotKind::MappingValue)
        })
        .or_else(|| slots.last())?;
    Some(context_for_source_slot(slot))
}

fn observe_source_slot_line(line: &str, slots: &mut Vec<SourceSlot>) -> bool {
    let trimmed = line.trim_start_matches(' ');
    let text = trimmed.trim_end();
    if text.is_empty() || text.starts_with('#') || text.starts_with(PLACEHOLDER_PREFIX) {
        return false;
    }
    if matches!(text, "---" | "...") {
        slots.clear();
        return false;
    }

    let indent = line.len() - trimmed.len();
    if let Some(after_dash) = text.strip_prefix('-') {
        let same_indent_parent_index = slots
            .iter()
            .rev()
            .position(|slot| slot.indent == indent && slot.kind == SourceSlotKind::MappingValue)
            .map(|index| slots.len() - 1 - index);
        let same_indent_parent = same_indent_parent_index.map(|index| {
            slots[index].has_same_indent_sequence_value = true;
            slots[index].path.clone()
        });
        slots.retain(|slot| {
            slot.indent < indent
                || (same_indent_parent.is_some()
                    && slot.indent == indent
                    && slot.kind == SourceSlotKind::MappingValue)
        });
        observe_sequence_slot(indent, after_dash, same_indent_parent, slots)
    } else {
        slots.retain(|slot| slot.indent < indent);
        observe_mapping_slot(indent, text, slots)
    }
}

fn observe_sequence_slot(
    indent: usize,
    after_dash: &str,
    same_indent_parent: Option<YamlPath>,
    slots: &mut Vec<SourceSlot>,
) -> bool {
    let parent = same_indent_parent.unwrap_or_else(|| current_source_slot_path(slots, indent));
    let item_path = append_sequence_segment(&parent);
    slots.push(SourceSlot {
        indent,
        path: item_path.clone(),
        kind: SourceSlotKind::SequenceItem,
        has_same_indent_sequence_value: false,
    });

    let after_dash = after_dash.trim_start();
    if after_dash.is_empty() {
        return true;
    }
    observe_mapping_slot_with_parent(indent + 2, after_dash, item_path, slots);
    true
}

fn observe_mapping_slot(indent: usize, text: &str, slots: &mut Vec<SourceSlot>) -> bool {
    let parent = current_source_slot_path(slots, indent);
    observe_mapping_slot_with_parent(indent, text, parent, slots)
}

fn observe_mapping_slot_with_parent(
    indent: usize,
    text: &str,
    parent: YamlPath,
    slots: &mut Vec<SourceSlot>,
) -> bool {
    let Some(colon) = first_mapping_colon_offset(text) else {
        return false;
    };
    if !text[colon + 1..].trim().is_empty() {
        return false;
    }
    let Some(key) = parse_yaml_key(text).map(|key| key.into_key()) else {
        return false;
    };
    if key.contains(PLACEHOLDER_PREFIX) {
        return false;
    }
    slots.push(SourceSlot {
        indent,
        path: append_mapping_segment(&parent, &key),
        kind: SourceSlotKind::MappingValue,
        has_same_indent_sequence_value: false,
    });
    true
}

fn close_dedented_slots_for_action_prefix(
    prefix: &str,
    output_indent: usize,
    slots: &mut Vec<SourceSlot>,
) {
    let indent = prefix
        .chars()
        .take_while(|&ch| ch == ' ')
        .count()
        .max(output_indent);
    if !slots.iter().any(|slot| slot.indent > indent) {
        return;
    }

    slots.retain(|slot| {
        slot.indent < indent
            || (slot.indent == indent
                && slot.kind == SourceSlotKind::MappingValue
                && slot.has_same_indent_sequence_value)
    });
}

fn current_source_slot_path(slots: &[SourceSlot], indent: usize) -> YamlPath {
    slots
        .iter()
        .rev()
        .find(|slot| slot.indent < indent)
        .map(|slot| slot.path.clone())
        .unwrap_or_else(|| YamlPath(Vec::new()))
}

fn context_for_source_slot(slot: &SourceSlot) -> ResolvedNodeContext {
    let mut context = default_context(&slot.path);
    context.explicit_mapping_value_slot = slot.kind == SourceSlotKind::MappingValue;
    context.sequence_item_slot = slot.kind == SourceSlotKind::SequenceItem;
    context
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

fn rebase_relative_sequence_context(
    mut context: ResolvedNodeContext,
    previous_output_paths: &[YamlPath],
) -> ResolvedNodeContext {
    let Some(suffix) = context.output_path.0.strip_prefix(&["[*]".to_string()]) else {
        return context;
    };
    let Some(base) = previous_output_paths
        .iter()
        .rev()
        .find(|path| path_has_equivalent_suffix(&path.0, suffix))
    else {
        return context;
    };
    context.current_path = base.clone();
    context.output_path = base.clone();
    context.mapping_entry_path = base.clone();
    context
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
    children: &[ChildNode<'_>],
    sanitized: &mut [u8],
    outputs: &mut Vec<OutputSpan>,
    controls: &mut Vec<ControlSpan>,
) {
    let mut index = 0usize;
    while index < children.len() {
        let child = &children[index];
        let node = child.node;

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
        } else if matches!(node.kind(), "text" | "yaml_no_injection_text") {
            sanitize_embedded_template_actions(node, sanitized);
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
    sanitize_stream(source, &kept_children, sanitized, outputs, controls);

    let context_byte = kept_children
        .iter()
        .find_map(|child| {
            first_nonblank_byte(sanitized, child.node.start_byte(), child.node.end_byte())
        })
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

fn sanitize_embedded_template_actions(node: tree_sitter::Node<'_>, sanitized: &mut [u8]) {
    let Ok(text) = node.utf8_text(sanitized) else {
        return;
    };
    let text = text.to_string();
    if !text.contains("{{") {
        return;
    }

    let Some(tree) = parse_go_template(&text) else {
        return;
    };
    let mut ranges = Vec::new();
    collect_template_action_ranges(tree.root_node(), &mut ranges);

    for (local_start, local_end) in ranges {
        if local_start >= local_end || local_end > text.len() {
            continue;
        }
        let start = node.start_byte() + local_start;
        let end = node.start_byte() + local_end;
        let action_text = &text[local_start..local_end];
        if parse_action_expressions(action_text).is_empty() {
            blank_range(sanitized, start, end);
        } else {
            let token = embedded_placeholder_token(local_start, end - start);
            fill_placeholder(sanitized, start, end, &token);
        }
    }
}

fn collect_template_action_ranges(node: tree_sitter::Node<'_>, ranges: &mut Vec<(usize, usize)>) {
    let children = direct_children(node);
    let mut index = 0usize;
    while index < children.len() {
        let child = children[index].node;
        if is_template_delim_start(child.kind()) {
            let mut end_index = index + 1;
            while end_index < children.len()
                && !is_template_delim_end(children[end_index].node.kind())
            {
                end_index += 1;
            }
            if end_index < children.len() {
                ranges.push((child.start_byte(), children[end_index].node.end_byte()));
                index = end_index + 1;
                continue;
            }
        }

        collect_template_action_ranges(child, ranges);
        index += 1;
    }
}

fn embedded_placeholder_token(offset: usize, len: usize) -> String {
    let base = format!("__HSE{}_", base36(offset));
    if base.len() >= len {
        base[..len].to_string()
    } else {
        let mut token = base;
        token.push_str(&"x".repeat(len - token.len()));
        token
    }
}

#[derive(Clone, Copy)]
enum ContextMode<'a> {
    Output { placeholder: &'a str },
    Control,
}

fn resolve_yaml_context(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    mode: ContextMode<'_>,
    path: &YamlPath,
) -> Option<ResolvedNodeContext> {
    if !contains_byte(node, byte) {
        return None;
    }

    match node.kind() {
        "stream" | "document" | "block_node" | "flow_node" => {
            resolve_yaml_context_in_children(node, source, byte, mode, path)
        }
        "block_mapping" | "flow_mapping" => {
            resolve_yaml_context_in_mapping(node, source, byte, mode, path)
        }
        "block_mapping_pair" | "flow_pair" => {
            resolve_yaml_context_in_mapping_pair(node, source, byte, mode, path)
        }
        "block_sequence" | "flow_sequence" => {
            resolve_yaml_context_in_sequence(node, source, byte, mode, path)
        }
        "block_sequence_item" => {
            resolve_yaml_context_in_sequence_item(node, source, byte, mode, path)
                .or_else(|| Some(default_context(path)))
        }
        "block_scalar" => Some(block_scalar_context(path)),
        kind if is_scalar_like(kind) => Some(match mode {
            ContextMode::Output { placeholder } => {
                resolve_scalar_context(node, source, placeholder, path)
            }
            ContextMode::Control => default_context(path),
        }),
        _ => match mode {
            ContextMode::Output { .. } => {
                resolve_yaml_context_in_children(node, source, byte, mode, path)
            }
            ContextMode::Control => Some(default_context(path)),
        },
    }
}

fn resolve_yaml_context_in_children(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    mode: ContextMode<'_>,
    path: &YamlPath,
) -> Option<ResolvedNodeContext> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if let Some(context) = resolve_yaml_context(child, source, byte, mode, path) {
            return Some(context);
        }
    }
    Some(default_context(path))
}

fn resolve_yaml_context_in_mapping(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    mode: ContextMode<'_>,
    path: &YamlPath,
) -> Option<ResolvedNodeContext> {
    let pair_kind = if node.kind() == "block_mapping" {
        "block_mapping_pair"
    } else {
        "flow_pair"
    };
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named()
            && child.kind() == pair_kind
            && let Some(context) = resolve_yaml_context(child, source, byte, mode, path)
        {
            return Some(context);
        }
    }
    Some(default_context(path))
}

fn resolve_yaml_context_in_mapping_pair(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    mode: ContextMode<'_>,
    path: &YamlPath,
) -> Option<ResolvedNodeContext> {
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

    if let ContextMode::Output { placeholder } = mode
        && let Some(key) = key
        && contains_byte(key, byte)
    {
        return Some(output_mapping_key_context(
            path,
            key_text.as_deref(),
            placeholder,
        ));
    }

    if let Some(value) = value
        && contains_byte(value, byte)
    {
        if is_scalar_like(value.kind()) {
            return Some(match mode {
                ContextMode::Output { placeholder } => {
                    resolve_scalar_context(value, source, placeholder, &child_path)
                }
                ContextMode::Control => default_context(&child_path),
            });
        }
        if let Some(context) = resolve_yaml_context(value, source, byte, mode, &child_path) {
            return Some(context);
        }
    }

    Some(default_context(&child_path))
}

fn resolve_yaml_context_in_sequence(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    mode: ContextMode<'_>,
    path: &YamlPath,
) -> Option<ResolvedNodeContext> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if matches!(
            child.kind(),
            "block_sequence_item" | "flow_node" | "flow_pair"
        ) && let Some(context) = match mode {
            ContextMode::Output { .. } => {
                resolve_yaml_context_in_sequence_item(child, source, byte, mode, path)
            }
            ContextMode::Control => resolve_yaml_context(child, source, byte, mode, path),
        } {
            return Some(context);
        }
    }
    Some(default_context(path))
}

fn resolve_yaml_context_in_sequence_item(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    mode: ContextMode<'_>,
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
        return Some(match mode {
            ContextMode::Output { placeholder } => {
                let mut context = resolve_scalar_context(child, source, placeholder, path);
                if is_block_sequence_item || context.entire_scalar_value {
                    let item_path = append_sequence_segment(path);
                    context.current_path = item_path.clone();
                    context.output_path = item_path.clone();
                    context.mapping_entry_path = item_path;
                }
                context
            }
            ContextMode::Control => default_context(path),
        });
    }

    if child.kind() == "block_scalar" {
        return Some(block_scalar_context(path));
    }

    let seq_path = append_sequence_segment(path);
    resolve_yaml_context(child, source, byte, mode, &seq_path)
}

fn output_mapping_key_context(
    path: &YamlPath,
    key_text: Option<&str>,
    placeholder: &str,
) -> ResolvedNodeContext {
    if key_text == Some(placeholder) {
        return ResolvedNodeContext {
            current_path: path.clone(),
            output_path: path.clone(),
            mapping_entry_path: path.clone(),
            in_mapping_key: false,
            entire_scalar_value: true,
            inside_block_scalar: false,
            explicit_mapping_value_slot: false,
            sequence_item_slot: false,
        };
    }

    ResolvedNodeContext {
        current_path: path.clone(),
        output_path: YamlPath(Vec::new()),
        mapping_entry_path: path.clone(),
        in_mapping_key: true,
        entire_scalar_value: false,
        inside_block_scalar: false,
        explicit_mapping_value_slot: false,
        sequence_item_slot: false,
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
        explicit_mapping_value_slot: false,
        sequence_item_slot: false,
    }
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
        explicit_mapping_value_slot: false,
        sequence_item_slot: false,
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
        explicit_mapping_value_slot: false,
        sequence_item_slot: false,
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

fn path_has_equivalent_suffix(path: &[String], suffix: &[String]) -> bool {
    if suffix.len() > path.len() {
        return false;
    }
    path[path.len() - suffix.len()..]
        .iter()
        .zip(suffix)
        .all(|(left, right)| path_segments_equivalent(left, right))
}

fn path_has_equivalent_prefix(path: &[String], prefix: &[String]) -> bool {
    if prefix.len() > path.len() {
        return false;
    }
    path.iter()
        .zip(prefix)
        .all(|(left, right)| path_segments_equivalent(left, right))
}

fn path_segments_equivalent(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_suffix("[*]")
            .is_some_and(|stripped| stripped == right)
        || right
            .strip_suffix("[*]")
            .is_some_and(|stripped| stripped == left)
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
#[path = "../../tests/document_projection/tracker/attribution.rs"]
mod tests;
