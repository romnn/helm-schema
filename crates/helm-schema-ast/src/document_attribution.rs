use std::collections::HashMap;

use helm_schema_core::{ResourceRef, ValueKind, YamlPath, sequence_item_path};

use crate::{TemplateExpr, parse_expr_text, range_body_mapping_entry_indent_from_source};

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

/// Build the output-slot and control-site tables for one template source.
/// Action and control spans come from the tree-sitter template walk; the
/// open-slot context of every span is answered by the `helm-schema-syntax`
/// layout parse of the same source.
pub fn build_attribution_index(source: &str, root: tree_sitter::Node<'_>) -> AttributionIndex {
    let mut outputs = Vec::new();
    let mut controls = Vec::new();
    collect_spans(source, root, &mut outputs, &mut controls);
    outputs.sort_by_key(|output| output.start);
    controls.sort_by_key(|control| control.span_start);

    let document = helm_schema_syntax::TemplatedDocument::parse_with_root(source, root);
    let mut attribution = AttributionIndex::default();

    for output in outputs {
        let context = output_context(&document, &output);
        let slot = output_slot_from_context(&output, &context);
        attribution
            .output_slots
            .insert((output.start, output.end), slot.clone());
        attribution
            .output_slots
            .insert((output.node_start, output.node_end), slot);
    }

    for control in controls {
        let control_context = document.slot_context_at(control.context_byte, None);
        let range_mapping_entry_path = control.mapping_entry_indent.map(|indent| {
            document
                .open_slot_path_before(control.context_byte, indent)
                .map_or_else(
                    || fold_segments(&control_context.path),
                    |segments| fold_segments(&segments),
                )
        });
        let control_path = if control_context.inside_block_scalar {
            YamlPath(Vec::new())
        } else {
            fold_segments(&control_context.path)
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

/// The resolved context of one output action, from the CST queries.
struct OutputContext {
    path: YamlPath,
    in_mapping_key: bool,
    entire_scalar_value: bool,
    inside_block_scalar: bool,
    on_comment_line: bool,
}

fn output_context(
    document: &helm_schema_syntax::TemplatedDocument<'_>,
    output: &OutputSpan,
) -> OutputContext {
    let line = document.slot_context_at(output.start, Some((output.start, output.end)));
    if !line.inside_block_scalar
        && let Some(indent) = output.structural_indent
        && let Some(segments) = document.open_slot_path_before(output.start, indent)
    {
        // Structured fragments (`… | nindent N`) land in the open slot at
        // their rendered indent; the line-shape flags do not apply there.
        return OutputContext {
            path: fold_segments(&segments),
            in_mapping_key: false,
            entire_scalar_value: false,
            inside_block_scalar: false,
            on_comment_line: line.on_comment_line,
        };
    }
    OutputContext {
        path: fold_segments(&line.path),
        in_mapping_key: line.in_mapping_key,
        entire_scalar_value: line.entire_scalar_value,
        inside_block_scalar: line.inside_block_scalar,
        on_comment_line: line.on_comment_line,
    }
}

fn output_slot_from_context(output: &OutputSpan, context: &OutputContext) -> OutputSlot {
    let mut path = if context.in_mapping_key || context.inside_block_scalar {
        YamlPath(Vec::new())
    } else {
        context.path.clone()
    };
    if output.kind == ValueKind::Fragment
        && let Some(last) = path.0.last_mut()
        && let Some(stripped) = last.strip_suffix("[*]")
    {
        *last = stripped.to_string();
    }

    let slot = if context.in_mapping_key {
        OutputSlotKind::MappingKey
    } else if context.on_comment_line {
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

/// Fold typed path segments into the `YamlPath` convention (`[*]` appended
/// to the enclosing key for sequence items, collapsing repeats).
fn fold_segments(segments: &[helm_schema_syntax::PathSegment]) -> YamlPath {
    let mut path = YamlPath(Vec::new());
    for segment in segments {
        match segment {
            helm_schema_syntax::PathSegment::Key(key) => path.0.push(key.clone()),
            helm_schema_syntax::PathSegment::Item => path = sequence_item_path(&path),
        }
    }
    path
}

fn line_bounds(source: &str, byte: usize) -> (usize, usize) {
    let start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let end = source[byte..]
        .find('\n')
        .map_or(source.len(), |offset| byte + offset);
    (start, end)
}

fn first_nonblank_byte(bytes: &[u8], start: usize, end: usize) -> Option<usize> {
    let end = end.min(bytes.len());
    let start = start.min(end);
    bytes[start..end]
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        .map(|offset| start + offset)
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
