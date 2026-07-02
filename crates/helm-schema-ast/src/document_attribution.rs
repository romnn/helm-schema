use std::collections::HashMap;

use helm_schema_core::{ResourceRef, ValueKind, YamlPath, sequence_item_path};

use crate::{
    TemplateExpr, parse_expr_text, range_body_mapping_entry_indent_from_source,
    structural_mapping_colon, unquote_yaml_scalar,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputSlot {
    pub kind: ValueKind,
    pub path: YamlPath,
    pub resource: Option<ResourceRef>,
    pub slot: OutputSlotKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputSlotKind {
    MappingKey,
    YamlComment,
    WholeScalar,
    PartialScalar,
    FragmentInsertion,
    BlockScalarSuppressed,
    Opaque,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlSite {
    pub path: YamlPath,
    pub range_mapping_entry_path: Option<YamlPath>,
}

#[derive(Clone, Debug)]
pub struct ResourceSpan {
    pub start: usize,
    pub end: usize,
    pub resource: ResourceRef,
    pub path_prefix: Vec<String>,
}

impl OutputSlot {
    pub fn suppresses_fragment_output(&self) -> bool {
        self.slot == OutputSlotKind::MappingKey
    }

    pub fn direct_value_kind(&self) -> ValueKind {
        if self.kind == ValueKind::Scalar
            && self.slot == OutputSlotKind::PartialScalar
            && !self.path.0.is_empty()
        {
            ValueKind::PartialScalar
        } else {
            self.kind
        }
    }

    pub fn direct_value_path(&self, source_expr: &str) -> YamlPath {
        if source_expr.ends_with(".*") && !self.in_sequence_item() {
            YamlPath(Vec::new())
        } else {
            self.path.clone()
        }
    }

    pub fn can_project_scalar_helper_to_caller_path(&self) -> bool {
        !self.path.0.is_empty()
            && self.kind == ValueKind::Scalar
            && self.slot == OutputSlotKind::WholeScalar
    }

    pub fn can_project_structured_helper_to_caller_path(&self) -> bool {
        !self.path.0.is_empty()
            && (self.kind == ValueKind::Fragment
                || (self.kind == ValueKind::Scalar && self.slot == OutputSlotKind::WholeScalar))
    }

    fn in_sequence_item(&self) -> bool {
        self.path
            .0
            .last()
            .map(std::string::String::as_str)
            .is_some_and(|segment| segment.ends_with("[*]"))
    }
}

impl Default for OutputSlot {
    fn default() -> Self {
        Self {
            kind: ValueKind::Scalar,
            path: YamlPath(Vec::new()),
            resource: None,
            slot: OutputSlotKind::Opaque,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct ResolvedNodeContext {
    current_path: YamlPath,
    in_mapping_key: bool,
    entire_scalar_value: bool,
    inside_block_scalar: bool,
}

#[derive(Clone, Default)]
pub struct AttributionIndex {
    output_slots: HashMap<(usize, usize), OutputSlot>,
    control_sites: HashMap<(usize, usize), ControlSite>,
    resource_spans: Vec<ResourceSpan>,
}

impl AttributionIndex {
    pub fn with_resource_spans(mut self, resource_spans: Vec<ResourceSpan>) -> Self {
        self.resource_spans = resource_spans;
        self
    }

    pub fn output_slot_for_node(&self, mut node: tree_sitter::Node<'_>) -> Option<OutputSlot> {
        let output_byte = node.start_byte();
        loop {
            if let Some(slot) = self.output_slots.get(&(node.start_byte(), node.end_byte())) {
                return Some(self.resource_scoped_slot(output_byte, slot.clone()));
            }
            let Some(parent) = node.parent() else {
                break;
            };
            node = parent;
        }
        self.resource_at(output_byte)
            .is_some()
            .then(|| self.resource_scoped_slot(output_byte, OutputSlot::default()))
    }

    pub fn control_site_for_node(&self, mut node: tree_sitter::Node<'_>) -> Option<ControlSite> {
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

    pub fn resource_at(&self, byte: usize) -> Option<&ResourceRef> {
        self.resource_span_at(byte).map(|span| &span.resource)
    }

    pub fn single_resource_in_span(&self, start: usize, end: usize) -> Option<&ResourceRef> {
        let mut resource = None;
        for span in &self.resource_spans {
            if span.start >= end || start >= span.end {
                continue;
            }
            match resource {
                Some(existing) if existing != &span.resource => return None,
                Some(_) => {}
                None => resource = Some(&span.resource),
            }
        }
        resource
    }

    pub fn rebase_path_at(&self, byte: usize, path: YamlPath) -> YamlPath {
        let Some(span) = self.resource_span_at(byte) else {
            return path;
        };
        if span.path_prefix.is_empty() || !path.0.starts_with(&span.path_prefix) {
            return path;
        }
        YamlPath(path.0[span.path_prefix.len()..].to_vec())
    }

    fn resource_scoped_slot(&self, byte: usize, mut slot: OutputSlot) -> OutputSlot {
        slot.path = self.rebase_path_at(byte, slot.path);
        slot.resource = self.resource_at(byte).cloned();
        slot
    }

    fn resource_span_at(&self, byte: usize) -> Option<&ResourceSpan> {
        self.resource_spans
            .iter()
            .filter(|span| span.start <= byte && byte < span.end)
            .min_by(|left, right| {
                let left_len = left.end.saturating_sub(left.start);
                let right_len = right.end.saturating_sub(right.start);
                left_len
                    .cmp(&right_len)
                    .then_with(|| right.start.cmp(&left.start))
            })
    }
}

#[derive(Clone, Copy)]
struct OutputSpan {
    node_start: usize,
    node_end: usize,
    start: usize,
    end: usize,
    kind: ValueKind,
    structural_indent: Option<usize>,
}

#[derive(Clone, Copy)]
struct ControlSpan {
    span_start: usize,
    span_end: usize,
    context_byte: usize,
    mapping_entry_indent: Option<usize>,
}

pub fn build_attribution_index(source: &str, root: tree_sitter::Node<'_>) -> AttributionIndex {
    let mut outputs = Vec::new();
    let mut controls = Vec::new();
    collect_spans(source, root, &mut outputs, &mut controls);
    outputs.sort_by_key(|output| output.start);
    controls.sort_by_key(|control| control.span_start);

    let document = StructuralDocument::new(source);
    let mut attribution = AttributionIndex::default();

    for output in outputs {
        let context = document.output_context(&output);
        let slot = output_slot_from_context(source, &output, &context);
        attribution
            .output_slots
            .insert((output.start, output.end), slot.clone());
        attribution
            .output_slots
            .insert((output.node_start, output.node_end), slot);
    }

    for control in controls {
        let control_context = document.line_context_at(control.context_byte, None);
        let range_mapping_entry_path = control.mapping_entry_indent.map(|indent| {
            document
                .structural_path_before(control.context_byte, indent)
                .unwrap_or_else(|| control_context.current_path.clone())
        });
        let control_path = if control_context.inside_block_scalar {
            YamlPath(Vec::new())
        } else {
            control_context.current_path
        };

        if !control_path.0.is_empty() || range_mapping_entry_path.is_some() {
            attribution.control_sites.insert(
                (control.span_start, control.span_end),
                ControlSite {
                    path: control_path,
                    range_mapping_entry_path,
                },
            );
        }
    }

    attribution
}

fn collect_spans(
    source: &str,
    node: tree_sitter::Node<'_>,
    outputs: &mut Vec<OutputSpan>,
    controls: &mut Vec<ControlSpan>,
) {
    let control_kept_fields: Option<&[&str]> = match node.kind() {
        "if_action" => Some(&["consequence", "alternative", "option"]),
        "with_action" => Some(&["consequence", "alternative"]),
        "range_action" => Some(&["body", "alternative"]),
        "define_action" | "block_action" => Some(&["body"]),
        _ => None,
    };
    if let Some(kept_fields) = control_kept_fields {
        let mut kept_children = Vec::new();
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if cursor
                    .field_name()
                    .is_some_and(|field| kept_fields.contains(&field))
                {
                    kept_children.push(child);
                    collect_spans(source, child, outputs, controls);
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        let context_byte = kept_children
            .iter()
            .find_map(|child| {
                first_nonblank_byte(source.as_bytes(), child.start_byte(), child.end_byte())
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
        return;
    }

    if node.is_named() && is_output_root_kind(node.kind()) {
        let (start, end) = template_action_span_for_node(source, node);
        if outputs
            .iter()
            .any(|output| output.start == start && output.end == end)
        {
            return;
        }
        if let Some((kind, structural_indent)) = output_action_shape(source, start, end) {
            outputs.push(OutputSpan {
                node_start: node.start_byte(),
                node_end: node.end_byte(),
                start,
                end,
                kind,
                structural_indent,
            });
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_spans(source, child, outputs, controls);
    }
}

fn output_action_shape(
    source: &str,
    start: usize,
    end: usize,
) -> Option<(ValueKind, Option<usize>)> {
    let text = source.get(start.min(source.len())..end.min(source.len()))?;
    let exprs = parse_expr_text(text);
    if exprs.is_empty() {
        return None;
    }
    let kind = if exprs.iter().any(TemplateExpr::renders_yaml_fragment) {
        ValueKind::Fragment
    } else {
        ValueKind::Scalar
    };
    let structural_indent = exprs
        .iter()
        .rev()
        .find_map(TemplateExpr::fragment_indent_width);
    Some((kind, structural_indent))
}

fn template_action_span_for_node(source: &str, mut node: tree_sitter::Node<'_>) -> (usize, usize) {
    let original_start = node.start_byte();
    let original_end = node.end_byte();
    loop {
        if node.kind() == "template_action" {
            return (node.start_byte(), node.end_byte());
        }
        let Some(parent) = node.parent() else {
            return delimited_action_span(source, original_start, original_end)
                .unwrap_or((original_start, original_end));
        };
        if parent.kind() == "source_file" {
            return delimited_action_span(source, original_start, original_end)
                .unwrap_or((original_start, original_end));
        }
        node = parent;
    }
}

fn delimited_action_span(source: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let (line_start, _) = line_bounds(source, start);
    let (_, line_end) = line_bounds(source, end);
    let action_start = source[line_start..start]
        .rfind("{{")
        .map(|offset| line_start + offset)?;
    let action_end = source[end..line_end]
        .find("}}")
        .map(|offset| end + offset + 2)?;
    Some((action_start, action_end))
}

fn output_slot_from_context(
    source: &str,
    output: &OutputSpan,
    context: &ResolvedNodeContext,
) -> OutputSlot {
    let mut path = if context.in_mapping_key || context.inside_block_scalar {
        YamlPath(Vec::new())
    } else {
        context.current_path.clone()
    };
    if output.kind == ValueKind::Fragment
        && let Some(last) = path.0.last_mut()
        && let Some(stripped) = last.strip_suffix("[*]")
    {
        *last = stripped.to_string();
    }

    let slot = if context.in_mapping_key {
        OutputSlotKind::MappingKey
    } else if document_site_is_yaml_comment_part(source, output.start) {
        OutputSlotKind::YamlComment
    } else if context.inside_block_scalar {
        OutputSlotKind::BlockScalarSuppressed
    } else if output.kind == ValueKind::Fragment {
        OutputSlotKind::FragmentInsertion
    } else if context.entire_scalar_value {
        OutputSlotKind::WholeScalar
    } else if !path.0.is_empty() {
        OutputSlotKind::PartialScalar
    } else {
        OutputSlotKind::Opaque
    };
    OutputSlot {
        kind: output.kind,
        path,
        resource: None,
        slot,
    }
}

fn document_site_is_yaml_comment_part(source: &str, start: usize) -> bool {
    let (line_start, _) = line_bounds(source, start);
    source[line_start..start].trim_start().starts_with('#')
}

struct StructuralDocument<'a> {
    source: &'a str,
}

#[derive(Clone)]
struct StructuralSlot {
    indent: usize,
    path: YamlPath,
    allow_same_indent_output: bool,
    block_scalar: bool,
}

impl<'a> StructuralDocument<'a> {
    fn new(source: &'a str) -> Self {
        Self { source }
    }

    fn output_context(&self, output: &OutputSpan) -> ResolvedNodeContext {
        let line_context = self.line_context_at(output.start, Some((output.start, output.end)));
        if line_context.inside_block_scalar {
            return line_context;
        }
        output
            .structural_indent
            .and_then(|indent| self.structural_path_before(output.start, indent))
            .map_or(line_context, |path| path_context(&path))
    }

    fn line_context_at(
        &self,
        byte: usize,
        action_span: Option<(usize, usize)>,
    ) -> ResolvedNodeContext {
        let byte = byte.min(self.source.len());
        let (line_start, line_end) = line_bounds(self.source, byte);
        let line = &self.source[line_start..line_end];
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let prior_slots = self.structural_slot_stack_before(line_start);
        if let Some(slot) = prior_slots
            .last()
            .filter(|slot| slot.block_scalar && indent > slot.indent)
        {
            return ResolvedNodeContext {
                inside_block_scalar: true,
                ..path_context(&slot.path)
            };
        }
        // A `#` line never carries an output slot; a blank line pops every
        // open slot (indent 0 closes all), so both resolve to the empty
        // default context.
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return ResolvedNodeContext::default();
        }

        let is_sequence_item = valid_sequence_item(trimmed);
        let parent_path = self
            .structural_path_for_line(line_start, indent, is_sequence_item)
            .unwrap_or_else(|| YamlPath(Vec::new()));

        if is_sequence_item {
            let item_path = sequence_item_path(&parent_path);
            let after_dash = &trimmed[1..];
            let nested = after_dash.trim_start();
            if action_span.is_none() && structural_mapping_colon(nested).is_none() {
                return path_context(&parent_path);
            }
            let nested_start =
                line_start + indent + 1 + after_dash.len().saturating_sub(nested.len());
            return context_from_line_text(nested, &item_path, action_span, nested_start);
        }

        context_from_line_text(trimmed, &parent_path, action_span, line_start + indent)
    }

    fn structural_path_before(
        &self,
        insertion_byte: usize,
        output_indent: usize,
    ) -> Option<YamlPath> {
        if output_indent == 0 {
            return Some(YamlPath(Vec::new()));
        }
        let slots = self.structural_slot_stack_before(insertion_byte.min(self.source.len()));
        let slot = slots
            .iter()
            .rev()
            .find(|slot| {
                slot.indent < output_indent
                    || (slot.indent == output_indent && slot.allow_same_indent_output)
            })
            .or_else(|| slots.last())?;
        Some(slot.path.clone())
    }

    fn structural_path_for_line(
        &self,
        line_start: usize,
        indent: usize,
        is_sequence_item: bool,
    ) -> Option<YamlPath> {
        let mut slots = self.structural_slot_stack_before(line_start);
        if is_sequence_item {
            pop_closed_slots_before_sequence_item(&mut slots, indent);
        } else {
            pop_closed_slots(&mut slots, indent);
        }
        slots.pop().map(|slot| slot.path)
    }

    fn structural_slot_stack_before(&self, byte: usize) -> Vec<StructuralSlot> {
        let mut slots = Vec::new();
        for line in self.source[..byte].lines() {
            push_structural_line(line, &mut slots);
        }
        slots
    }
}

fn line_bounds(source: &str, byte: usize) -> (usize, usize) {
    let start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let end = source[byte..]
        .find('\n')
        .map_or(source.len(), |offset| byte + offset);
    (start, end)
}

fn push_structural_line(line: &str, slots: &mut Vec<StructuralSlot>) {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("{{") {
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
    let item_path = sequence_item_path(&parent_path);
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
    let Some(colon) = structural_mapping_colon(text) else {
        return;
    };
    let value = text[colon + 1..].trim();
    let block_scalar = value.starts_with('|') || value.starts_with('>');
    let template_value = value.contains("{{");
    if !value.is_empty() && !block_scalar && !template_value {
        return;
    }
    let key = unquote_yaml_scalar(text[..colon].trim());
    if key.is_empty() || key.contains("{{") || key.contains("}}") {
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

fn context_from_line_text(
    text: &str,
    parent_path: &YamlPath,
    action_span: Option<(usize, usize)>,
    text_start: usize,
) -> ResolvedNodeContext {
    let Some(colon) = structural_mapping_colon(text) else {
        return scalar_line_context(text, parent_path, action_span, text_start);
    };

    if action_span.is_some_and(|(start, _)| start >= text_start && start <= text_start + colon) {
        return ResolvedNodeContext {
            in_mapping_key: true,
            ..path_context(parent_path)
        };
    }

    let key_text = unquote_yaml_scalar(text[..colon].trim());
    let key_path = if key_text.contains("{{") || key_text.contains("}}") {
        parent_path.clone()
    } else {
        append_mapping_segment(parent_path, key_text)
    };
    scalar_line_context(
        &text[colon + 1..],
        &key_path,
        action_span,
        text_start + colon + 1,
    )
}

fn scalar_line_context(
    text: &str,
    path: &YamlPath,
    action_span: Option<(usize, usize)>,
    text_start: usize,
) -> ResolvedNodeContext {
    let value = text.trim();
    let value_start = text_start + text.len().saturating_sub(text.trim_start().len());
    ResolvedNodeContext {
        entire_scalar_value: action_span
            .is_some_and(|span| span_is_entire_scalar(value, value_start, span)),
        ..path_context(path)
    }
}

fn span_is_entire_scalar(text: &str, text_start: usize, (start, end): (usize, usize)) -> bool {
    let trimmed_end = text_start + text.len();
    if start == text_start && end == trimmed_end {
        return true;
    }
    if unquote_yaml_scalar(text).len() != text.len() {
        return start == text_start + 1 && end == trimmed_end - 1;
    }
    false
}

fn valid_sequence_item(trimmed: &str) -> bool {
    let Some(after_dash) = trimmed.strip_prefix('-') else {
        return false;
    };
    after_dash.is_empty() || after_dash.starts_with(char::is_whitespace)
}

fn first_nonblank_byte(bytes: &[u8], start: usize, end: usize) -> Option<usize> {
    let end = end.min(bytes.len());
    let start = start.min(end);
    bytes[start..end]
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        .map(|offset| start + offset)
}

fn path_context(path: &YamlPath) -> ResolvedNodeContext {
    ResolvedNodeContext {
        current_path: path.clone(),
        ..ResolvedNodeContext::default()
    }
}

fn append_mapping_segment(path: &YamlPath, key: &str) -> YamlPath {
    let mut path = path.clone();
    if !key.is_empty() {
        path.0.push(key.to_string());
    }
    path
}

pub fn is_output_root_kind(kind: &str) -> bool {
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
