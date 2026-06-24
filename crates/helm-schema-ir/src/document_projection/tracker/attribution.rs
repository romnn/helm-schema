use std::collections::{HashMap, HashSet};

use helm_schema_ast::{TemplateExpr, parse_action_expressions};

use crate::YamlPath;
use crate::fragment_range_scope::range_body_mapping_entry_indent_from_source;
use crate::tree_sitter_utils::parse_go_template;
use crate::yaml_syntax::{first_mapping_colon_offset, parse_yaml_key};

use super::yaml_tree::{
    is_scalar_like, parse_yaml_tree, scalar_text, strip_scalar_quotes, unwrap_yaml_node,
};

const PLACEHOLDER_PREFIX: &str = "__HS";
const INLINE_PLACEHOLDER: &str = "__HSINLINE__";

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
    sanitized: String,
    output_nodes: HashMap<(usize, usize), ResolvedNodeContext>,
    output_spans: HashSet<(usize, usize)>,
    control_nodes: HashMap<(usize, usize), ResolvedNodeContext>,
    rendered_output_nodes: HashMap<(usize, usize, usize), ResolvedNodeContext>,
    mapping_entry_nodes: HashMap<(usize, usize, usize), ResolvedNodeContext>,
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

    pub(super) fn virtual_indent_context_for_node(
        &self,
        node: tree_sitter::Node<'_>,
        indent: usize,
    ) -> Option<ResolvedNodeContext> {
        let (span_start, span_end) = self.output_span_for_node_or_ancestor(node)?;
        self.rendered_output_nodes
            .get(&(span_start, span_end, indent))
            .cloned()
            .or_else(|| {
                resolve_full_document_fragment_output_context(
                    &self.sanitized,
                    span_start,
                    span_end,
                    indent,
                )
            })
    }

    pub(super) fn mapping_entry_context_in_span_at_indent(
        &self,
        start: usize,
        end: usize,
        indent: usize,
    ) -> Option<ResolvedNodeContext> {
        self.mapping_entry_nodes
            .get(&(start, end, indent))
            .cloned()
            .or_else(|| {
                let insertion_byte =
                    first_nonblank_byte(self.sanitized.as_bytes(), start, end).unwrap_or(start);
                resolve_full_document_mapping_entry_context(&self.sanitized, insertion_byte, indent)
            })
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

    fn output_span_for_node_or_ancestor(
        &self,
        mut node: tree_sitter::Node<'_>,
    ) -> Option<(usize, usize)> {
        loop {
            let span = (node.start_byte(), node.end_byte());
            if self.output_spans.contains(&span) {
                return Some(span);
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
    let mut attribution = AttributionIndex {
        sanitized: sanitized.clone(),
        ..AttributionIndex::default()
    };
    let mut previous_output_paths = Vec::new();

    for output in outputs {
        attribution
            .output_spans
            .insert((output.action_start, output.action_end));
        attribution
            .output_spans
            .insert((output.node_start, output.node_end));

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
        let inline_context = resolve_inline_output_probe_context(
            &sanitized,
            output.action_start,
            output.action_end,
            &output.placeholder,
        );
        let rendered_context = output.structural_indent.and_then(|indent| {
            let rendered = resolve_full_document_fragment_output_context(
                &sanitized,
                output.action_start,
                output.action_end,
                indent,
            );
            let mapping = resolve_full_document_mapping_entry_context(
                &sanitized,
                output.action_start,
                indent,
            );
            if let Some(context) = rendered.clone() {
                attribution.rendered_output_nodes.insert(
                    (output.action_start, output.action_end, indent),
                    context.clone(),
                );
                attribution
                    .rendered_output_nodes
                    .insert((output.node_start, output.node_end, indent), context);
            }
            if let Some(context) = mapping.clone() {
                attribution.mapping_entry_nodes.insert(
                    (output.action_start, output.action_end, indent),
                    context.clone(),
                );
                attribution
                    .mapping_entry_nodes
                    .insert((output.node_start, output.node_end, indent), context);
            }
            rendered.or(mapping)
        });

        let context = if output.structural_indent.is_some() {
            let base_context = merge_resolved_contexts(base_context, inline_context);
            if rendered_context_is_better_base(&base_context, &rendered_context) {
                rendered_context.clone().or(base_context)
            } else {
                base_context.or(rendered_context.clone())
            }
        } else {
            merge_resolved_contexts(base_context, inline_context)
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
        for control in controls {
            if let Some(context) = resolve_yaml_context(
                root,
                &sanitized,
                control.context_byte,
                ContextMode::Control,
                &YamlPath(Vec::new()),
            ) {
                attribution
                    .control_nodes
                    .insert((control.span_start, control.span_end), context);
            }
            if let Some(indent) = control.mapping_entry_indent
                && let Some(context) = resolve_full_document_mapping_entry_context(
                    &sanitized,
                    control.context_byte,
                    indent,
                )
            {
                attribution
                    .mapping_entry_nodes
                    .insert((control.span_start, control.span_end, indent), context);
            }
        }
    }

    attribution
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

fn merge_resolved_contexts(
    global: Option<ResolvedNodeContext>,
    local: Option<ResolvedNodeContext>,
) -> Option<ResolvedNodeContext> {
    match (global, local) {
        (Some(mut global), Some(local)) => {
            global.current_path = prefer_context_path(global.current_path, local.current_path);
            global.output_path = prefer_context_path(global.output_path, local.output_path);
            global.mapping_entry_path =
                prefer_context_path(global.mapping_entry_path, local.mapping_entry_path);
            global.in_mapping_key |= local.in_mapping_key;
            global.entire_scalar_value |= local.entire_scalar_value;
            global.inside_block_scalar |= local.inside_block_scalar;
            global.explicit_mapping_value_slot |= local.explicit_mapping_value_slot;
            global.sequence_item_slot |= local.sequence_item_slot;
            Some(global)
        }
        (Some(global), None) => Some(global),
        (None, Some(local)) => Some(local),
        (None, None) => None,
    }
}

fn prefer_context_path(left: YamlPath, right: YamlPath) -> YamlPath {
    match (left.0.is_empty(), right.0.is_empty()) {
        (true, true) => YamlPath(Vec::new()),
        (true, false) => right,
        (false, true) => left,
        (false, false) => {
            if path_is_relative_sequence(&left.0) && !path_is_relative_sequence(&right.0) {
                right
            } else if path_is_relative_sequence(&right.0) && !path_is_relative_sequence(&left.0) {
                left
            } else if path_looks_like_scalar_header_artifact(&left.0)
                && !path_looks_like_scalar_header_artifact(&right.0)
            {
                right
            } else if (path_looks_like_scalar_header_artifact(&right.0)
                && !path_looks_like_scalar_header_artifact(&left.0))
                || (path_has_equivalent_suffix(&left.0, &right.0) && left.0.len() > right.0.len())
            {
                left
            } else if (path_has_equivalent_suffix(&right.0, &left.0)
                && right.0.len() > left.0.len())
                || (right.0.len() > left.0.len()
                    && !path_looks_like_scalar_header_artifact(&right.0))
            {
                right
            } else {
                left
            }
        }
    }
}

fn resolve_inline_output_probe_context(
    sanitized: &str,
    action_start: usize,
    action_end: usize,
    placeholder: &str,
) -> Option<ResolvedNodeContext> {
    let mut probe = sanitized.as_bytes().to_vec();
    fill_placeholder(&mut probe, action_start, action_end, placeholder);
    let probe = String::from_utf8(probe).ok()?;
    let tree = parse_yaml_tree(&probe)?;
    resolve_yaml_context(
        tree.root_node(),
        &probe,
        action_start.min(probe.len()),
        ContextMode::Output { placeholder },
        &YamlPath(Vec::new()),
    )
}

fn resolve_full_document_fragment_output_context(
    sanitized: &str,
    insertion_start: usize,
    insertion_end: usize,
    indent: usize,
) -> Option<ResolvedNodeContext> {
    let mapping = resolve_full_document_mapping_entry_context(sanitized, insertion_start, indent);
    let sequence = resolve_full_document_sequence_item_context(
        sanitized,
        insertion_start,
        insertion_end,
        indent,
    );
    match (mapping, sequence) {
        (Some(mapping), Some(_sequence)) if mapping.explicit_mapping_value_slot => Some(mapping),
        (Some(mapping), Some(sequence))
            if sequence.output_path.0.len() < mapping.output_path.0.len()
                && path_has_equivalent_prefix(&mapping.output_path.0, &sequence.output_path.0) =>
        {
            Some(sequence)
        }
        (Some(mapping), Some(sequence)) => Some(rebase_virtual_context(mapping, sequence)),
        (Some(mapping), None) => Some(mapping),
        (None, Some(sequence)) => Some(sequence),
        (None, None) => None,
    }
}

fn resolve_full_document_mapping_entry_context(
    sanitized: &str,
    insertion_byte: usize,
    indent: usize,
) -> Option<ResolvedNodeContext> {
    let structural = resolve_structural_gap_context(
        sanitized,
        insertion_byte,
        indent,
        ProbeContextKind::Mapping,
    );
    let value_slot = resolve_full_document_mapping_value_context(sanitized, insertion_byte, indent);
    match (structural, value_slot) {
        (Some(structural), Some(value_slot))
            if value_slot.output_path.0.len() > structural.output_path.0.len()
                && path_has_equivalent_prefix(
                    &value_slot.output_path.0,
                    &structural.output_path.0,
                ) =>
        {
            Some(value_slot)
        }
        (Some(structural), _) => Some(structural),
        (None, Some(value_slot)) => Some(value_slot),
        (None, None) => None,
    }
}

fn resolve_full_document_mapping_value_context(
    sanitized: &str,
    insertion_byte: usize,
    indent: usize,
) -> Option<ResolvedNodeContext> {
    if line_has_inline_prefix(sanitized, insertion_byte) {
        return None;
    }
    let key = previous_open_mapping_key(sanitized, insertion_byte, indent)?;
    let mut context = resolve_structural_gap_context(
        sanitized,
        insertion_byte,
        indent + 1,
        ProbeContextKind::Mapping,
    )?;
    let last = context.output_path.0.last()?;
    if !path_segments_equivalent(last, &key) {
        return None;
    }
    context.explicit_mapping_value_slot = true;
    Some(context)
}

fn previous_open_mapping_key(
    sanitized: &str,
    insertion_byte: usize,
    indent: usize,
) -> Option<String> {
    let line_start = sanitized[..insertion_byte.min(sanitized.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let previous = sanitized[..line_start]
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())?;
    let previous_indent = previous.chars().take_while(|&ch| ch == ' ').count();
    if previous_indent != indent {
        return None;
    }
    let text = previous.trim_start();
    let key = parse_yaml_key(text)?.into_key();
    let colon = first_mapping_colon_offset(text)?;
    text[colon + 1..].trim().is_empty().then_some(key)
}

fn resolve_full_document_sequence_item_context(
    sanitized: &str,
    insertion_start: usize,
    insertion_end: usize,
    indent: usize,
) -> Option<ResolvedNodeContext> {
    let insertion_byte = first_nonblank_byte(sanitized.as_bytes(), insertion_start, insertion_end)
        .unwrap_or(insertion_start);
    if let Some(context) = resolve_exact_sequence_indent_context(sanitized, insertion_byte, indent)
    {
        return Some(context);
    }
    resolve_structural_gap_context(
        sanitized,
        insertion_byte,
        indent,
        ProbeContextKind::Sequence,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProbeContextKind {
    Mapping,
    Sequence,
}

fn resolve_structural_gap_context(
    sanitized: &str,
    insertion_byte: usize,
    indent: usize,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    if line_has_inline_prefix(sanitized, insertion_byte) {
        return None;
    }
    let tree = parse_yaml_tree(sanitized)?;
    let root = tree.root_node();
    let insertion_byte = insertion_byte.min(root.end_byte());
    resolve_structural_gap_context_in_node(
        root,
        sanitized,
        insertion_byte,
        indent,
        &YamlPath(Vec::new()),
        kind,
    )
}

fn resolve_exact_sequence_indent_context(
    sanitized: &str,
    insertion_byte: usize,
    indent: usize,
) -> Option<ResolvedNodeContext> {
    let tree = parse_yaml_tree(sanitized)?;
    let mut best = None;
    collect_exact_sequence_indent_slots(
        tree.root_node(),
        sanitized,
        insertion_byte,
        indent,
        &YamlPath(Vec::new()),
        &mut best,
    );
    best.map(|(_start, path)| {
        let mut context = probe_context_for_path(&path, ProbeContextKind::Sequence);
        context.sequence_item_slot = true;
        context
    })
}

fn collect_exact_sequence_indent_slots(
    node: tree_sitter::Node<'_>,
    source: &str,
    insertion_byte: usize,
    indent: usize,
    path: &YamlPath,
    best: &mut Option<(usize, YamlPath)>,
) {
    if node.start_byte() > insertion_byte {
        return;
    }

    match node.kind() {
        "stream" | "document" | "block_node" | "flow_node" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor).filter(|child| child.is_named()) {
                collect_exact_sequence_indent_slots(
                    child,
                    source,
                    insertion_byte,
                    indent,
                    path,
                    best,
                );
            }
        }
        "block_mapping" | "flow_mapping" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor).filter(|child| child.is_named()) {
                collect_exact_sequence_indent_slots(
                    child,
                    source,
                    insertion_byte,
                    indent,
                    path,
                    best,
                );
            }
        }
        "block_mapping_pair" | "flow_pair" => {
            let child_path = mapping_pair_child_path(node, source, path);
            if let Some(value) = node.child_by_field_name("value") {
                collect_exact_sequence_indent_slots(
                    value,
                    source,
                    insertion_byte,
                    indent,
                    &child_path,
                    best,
                );
            }
        }
        "block_sequence" | "flow_sequence" => {
            if sequence_item_indent(node) == Some(indent)
                && sequence_scope_is_open_at_insertion(node, source, insertion_byte)
                && best.as_ref().is_none_or(|(best_start, best_path)| {
                    path.0.len() > best_path.0.len()
                        || (path.0.len() == best_path.0.len() && node.start_byte() > *best_start)
                })
            {
                *best = Some((node.start_byte(), path.clone()));
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor).filter(|child| child.is_named()) {
                collect_exact_sequence_indent_slots(
                    child,
                    source,
                    insertion_byte,
                    indent,
                    path,
                    best,
                );
            }
        }
        "block_sequence_item" => {
            if let Some(child) = node.named_child(0).map(unwrap_yaml_node) {
                let item_path = append_sequence_segment(path);
                collect_exact_sequence_indent_slots(
                    child,
                    source,
                    insertion_byte,
                    indent,
                    &item_path,
                    best,
                );
            }
        }
        _ => {}
    }
}

fn sequence_scope_is_open_at_insertion(
    node: tree_sitter::Node<'_>,
    source: &str,
    insertion_byte: usize,
) -> bool {
    if insertion_byte <= node.end_byte() {
        return true;
    }
    let parent_indent = sequence_parent_indent(node);
    !has_dedent_before_insertion(source, node.end_byte(), insertion_byte, parent_indent)
}

fn sequence_parent_indent(node: tree_sitter::Node<'_>) -> usize {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if matches!(parent.kind(), "block_mapping_pair" | "flow_pair") {
            return parent.start_position().column;
        }
        current = parent;
    }
    node.start_position().column.saturating_sub(2)
}

fn has_dedent_before_insertion(
    source: &str,
    start: usize,
    insertion_byte: usize,
    parent_indent: usize,
) -> bool {
    let start = start.min(source.len());
    let insertion_byte = insertion_byte.min(source.len());
    if start >= insertion_byte {
        return false;
    }
    source[start..insertion_byte]
        .lines()
        .filter(|line| !line.trim().is_empty())
        .any(|line| line.chars().take_while(|&ch| ch == ' ').count() <= parent_indent)
}

fn resolve_structural_gap_context_in_node(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    indent: usize,
    path: &YamlPath,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    if !node_covers_gap(node, byte) {
        return None;
    }

    match node.kind() {
        "stream" | "document" | "block_node" | "flow_node" => {
            resolve_structural_gap_in_children(node, source, byte, indent, path, kind)
        }
        "block_mapping" | "flow_mapping" => {
            resolve_structural_gap_in_mapping(node, source, byte, indent, path, kind)
        }
        "block_mapping_pair" | "flow_pair" => {
            resolve_structural_gap_in_mapping_pair(node, source, byte, indent, path, kind)
        }
        "block_sequence" | "flow_sequence" => {
            resolve_structural_gap_in_sequence(node, source, byte, indent, path, kind)
        }
        "block_sequence_item" => {
            resolve_structural_gap_in_sequence_item(node, source, byte, indent, path, kind)
        }
        _ => None,
    }
}

fn resolve_structural_gap_in_children(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    indent: usize,
    path: &YamlPath,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    let mut previous = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if contains_byte(child, byte) {
            return resolve_structural_gap_context_in_node(child, source, byte, indent, path, kind);
        }
        if byte < child.start_byte() {
            return previous.and_then(|child| {
                resolve_trailing_structural_gap_from_child(child, source, byte, indent, path, kind)
            });
        }
        previous = Some(child);
    }

    previous.and_then(|child| {
        resolve_trailing_structural_gap_from_child(child, source, byte, indent, path, kind)
    })
}

fn resolve_structural_gap_in_mapping(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    indent: usize,
    path: &YamlPath,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    let pair_kind = if node.kind() == "block_mapping" {
        "block_mapping_pair"
    } else {
        "flow_pair"
    };
    let mut previous_pair = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() || child.kind() != pair_kind {
            continue;
        }
        if contains_byte(child, byte) {
            return resolve_structural_gap_in_mapping_pair(child, source, byte, indent, path, kind);
        }
        if byte < child.start_byte() {
            return resolve_mapping_gap_after_pair(previous_pair, source, indent, path, kind)
                .or_else(|| mapping_container_gap_context(path, kind));
        }
        previous_pair = Some(child);
    }

    resolve_mapping_gap_after_pair(previous_pair, source, indent, path, kind)
        .or_else(|| mapping_container_gap_context(path, kind))
}

fn resolve_structural_gap_in_mapping_pair(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    indent: usize,
    path: &YamlPath,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    let key = node.child_by_field_name("key");
    let value = node.child_by_field_name("value");
    let child_path = mapping_pair_child_path(node, source, path);

    if let Some(key) = key
        && contains_byte(key, byte)
    {
        return None;
    }

    if let Some(value) = value {
        if contains_byte(value, byte) {
            if is_scalar_like(value.kind()) || value.kind() == "block_scalar" {
                return None;
            }
            return resolve_structural_gap_context_in_node(
                value,
                source,
                byte,
                indent,
                &child_path,
                kind,
            );
        }
        if byte >= value.end_byte() && indent > node.start_position().column {
            return resolve_trailing_structural_gap_from_child(
                value,
                source,
                byte,
                indent,
                &child_path,
                kind,
            );
        }
        return None;
    }

    (indent > node.start_position().column).then(|| probe_context_for_path(&child_path, kind))
}

fn resolve_structural_gap_in_sequence(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    indent: usize,
    path: &YamlPath,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    let mut previous = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if contains_byte(child, byte) {
            if let Some(context) = sequence_container_gap_context(node, indent, path, kind)
                && indent <= child.start_position().column
            {
                return Some(context);
            }
            return resolve_structural_gap_in_sequence_child(
                child, source, byte, indent, path, kind,
            );
        }
        if byte < child.start_byte() {
            return sequence_gap_context(node, previous, source, byte, indent, path, kind);
        }
        previous = Some(child);
    }

    sequence_gap_context(node, previous, source, byte, indent, path, kind)
}

fn sequence_gap_context(
    sequence: tree_sitter::Node<'_>,
    previous: Option<tree_sitter::Node<'_>>,
    source: &str,
    byte: usize,
    indent: usize,
    path: &YamlPath,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    let previous_item = previous.and_then(|child| {
        resolve_trailing_structural_gap_from_child(child, source, byte, indent, path, kind)
    });
    let sequence_container = sequence_container_gap_context(sequence, indent, path, kind);
    if indent > sequence_item_indent(sequence).unwrap_or(sequence.start_position().column) {
        previous_item.or(sequence_container)
    } else {
        sequence_container.or(previous_item)
    }
}

fn resolve_structural_gap_in_sequence_child(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    indent: usize,
    path: &YamlPath,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    let is_block_sequence_item = node.kind() == "block_sequence_item";
    let Some(child) = (if is_block_sequence_item {
        node.named_child(0).map(unwrap_yaml_node)
    } else {
        Some(unwrap_yaml_node(node))
    }) else {
        return sequence_container_gap_context(node, indent, path, kind);
    };

    let item_path = append_sequence_segment(path);
    if contains_byte(child, byte) {
        if is_scalar_like(child.kind()) || child.kind() == "block_scalar" {
            return sequence_container_gap_context(node, indent, path, kind);
        }
        return resolve_structural_gap_context_in_node(
            child, source, byte, indent, &item_path, kind,
        )
        .or_else(|| sequence_container_gap_context(node, indent, path, kind));
    }

    resolve_trailing_structural_gap_from_child(child, source, byte, indent, &item_path, kind)
        .or_else(|| sequence_container_gap_context(node, indent, path, kind))
}

fn resolve_structural_gap_in_sequence_item(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    indent: usize,
    path: &YamlPath,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    let Some(child) = node.named_child(0).map(unwrap_yaml_node) else {
        return sequence_container_gap_context(node, indent, path, kind);
    };
    let item_path = append_sequence_segment(path);

    if contains_byte(child, byte) {
        if is_scalar_like(child.kind()) || child.kind() == "block_scalar" {
            return sequence_container_gap_context(node, indent, path, kind);
        }
        return resolve_structural_gap_context_in_node(
            child, source, byte, indent, &item_path, kind,
        )
        .or_else(|| sequence_container_gap_context(node, indent, path, kind));
    }

    resolve_trailing_structural_gap_from_child(child, source, byte, indent, &item_path, kind)
        .or_else(|| sequence_container_gap_context(node, indent, path, kind))
}

fn resolve_trailing_structural_gap_from_child(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    indent: usize,
    path: &YamlPath,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    match node.kind() {
        "block_node" | "flow_node" => node.named_child(0).and_then(|child| {
            resolve_trailing_structural_gap_from_child(child, source, byte, indent, path, kind)
        }),
        "block_mapping" | "flow_mapping" => {
            resolve_structural_gap_in_mapping(node, source, byte, indent, path, kind)
        }
        "block_mapping_pair" | "flow_pair" => {
            resolve_mapping_gap_after_pair(Some(node), source, indent, path, kind)
        }
        "block_sequence" | "flow_sequence" => {
            if kind == ProbeContextKind::Sequence {
                sequence_container_gap_context(node, indent, path, kind)
            } else {
                resolve_structural_gap_in_sequence(node, source, byte, indent, path, kind)
            }
        }
        "block_sequence_item" => {
            resolve_structural_gap_in_sequence_item(node, source, byte, indent, path, kind)
        }
        _ => None,
    }
}

fn resolve_mapping_gap_after_pair(
    pair: Option<tree_sitter::Node<'_>>,
    source: &str,
    indent: usize,
    path: &YamlPath,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    let pair = pair?;
    let child_path = mapping_pair_child_path(pair, source, path);
    if let Some(value) = pair.child_by_field_name("value") {
        if indent <= pair.start_position().column {
            return None;
        }
        return resolve_trailing_structural_gap_from_child(
            value,
            source,
            value.end_byte(),
            indent,
            &child_path,
            kind,
        );
    }

    Some(probe_context_for_path(&child_path, kind))
}

fn mapping_pair_child_path(node: tree_sitter::Node<'_>, source: &str, path: &YamlPath) -> YamlPath {
    let key = node.child_by_field_name("key");
    let key_text = key.and_then(|node| scalar_text(node, source));
    if key_text
        .as_deref()
        .is_some_and(|text| text.contains(PLACEHOLDER_PREFIX))
    {
        path.clone()
    } else if let Some(key_text) = key_text.as_deref() {
        append_mapping_segment(path, key_text)
    } else {
        path.clone()
    }
}

fn mapping_container_gap_context(
    path: &YamlPath,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    (kind == ProbeContextKind::Mapping).then(|| default_context(path))
}

fn sequence_container_gap_context(
    node: tree_sitter::Node<'_>,
    indent: usize,
    path: &YamlPath,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    if kind != ProbeContextKind::Sequence {
        return None;
    }
    let expected_indent = sequence_item_indent(node)?;
    (indent >= expected_indent).then(|| probe_context_for_path(path, kind))
}

fn sequence_item_indent(node: tree_sitter::Node<'_>) -> Option<usize> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.is_named())
        .map(|child| child.start_position().column)
        .or_else(|| Some(node.start_position().column + 2))
}

fn probe_context_for_path(path: &YamlPath, kind: ProbeContextKind) -> ResolvedNodeContext {
    let output_path = match kind {
        ProbeContextKind::Mapping => path.clone(),
        ProbeContextKind::Sequence => append_sequence_segment(path),
    };
    ResolvedNodeContext {
        current_path: path.clone(),
        output_path,
        mapping_entry_path: path.clone(),
        in_mapping_key: false,
        entire_scalar_value: false,
        inside_block_scalar: false,
        explicit_mapping_value_slot: kind == ProbeContextKind::Mapping,
        sequence_item_slot: false,
    }
}

fn rebase_virtual_context(
    mut full: ResolvedNodeContext,
    local: ResolvedNodeContext,
) -> ResolvedNodeContext {
    if local.output_path.0.is_empty()
        || path_looks_like_scalar_header_artifact(&local.output_path.0)
        || path_is_relative_sequence(&local.output_path.0)
    {
        return full;
    }
    if full.output_path.0.is_empty()
        || path_looks_like_scalar_header_artifact(&full.output_path.0)
        || path_is_relative_sequence(&full.output_path.0)
    {
        return local;
    }

    full.current_path = rebase_virtual_path(&full.current_path, &local.current_path);
    full.output_path = rebase_virtual_path(&full.output_path, &local.output_path);
    full.mapping_entry_path =
        rebase_virtual_path(&full.mapping_entry_path, &local.mapping_entry_path);
    full.in_mapping_key |= local.in_mapping_key;
    full.entire_scalar_value |= local.entire_scalar_value;
    full.inside_block_scalar |= local.inside_block_scalar;
    full.explicit_mapping_value_slot |= local.explicit_mapping_value_slot;
    full.sequence_item_slot |= local.sequence_item_slot;
    full
}

fn rebase_virtual_path(base: &YamlPath, local: &YamlPath) -> YamlPath {
    if local.0.is_empty() {
        return base.clone();
    }
    if base.0.is_empty() {
        return local.clone();
    }
    if path_has_equivalent_prefix(&local.0, &base.0) {
        return local.clone();
    }
    if path_has_equivalent_prefix(&base.0, &local.0) {
        return base.clone();
    }
    if path_has_equivalent_suffix(&base.0, &local.0) {
        return base.clone();
    }

    let mut path = base.0.clone();
    if let (Some(base_last), Some(local_first)) = (path.last(), local.0.first())
        && path_segments_equivalent(base_last, local_first)
    {
        path.extend(local.0.iter().skip(1).cloned());
        return YamlPath(path);
    }
    path.extend(local.0.iter().cloned());
    YamlPath(path)
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

fn sanitize_output_action(sanitized: &mut [u8], start: usize, end: usize, token: &str) {
    if action_is_root_standalone_line(sanitized, start, end)
        || (action_is_standalone_line(sanitized, start, end)
            && action_may_inject_yaml_structure(sanitized, start, end))
        || action_uses_structural_indent_filter(sanitized, start, end)
    {
        blank_range(sanitized, start, end);
    } else {
        fill_placeholder(sanitized, start, end, token);
    }
}

fn action_uses_structural_indent_filter(sanitized: &[u8], start: usize, end: usize) -> bool {
    let Ok(text) =
        std::str::from_utf8(&sanitized[start.min(sanitized.len())..end.min(sanitized.len())])
    else {
        return false;
    };
    parse_action_expressions(text)
        .iter()
        .any(expr_uses_indent_filter)
        && (action_is_standalone_line(sanitized, start, end)
            || action_is_inline_mapping_value(sanitized, start))
}

fn action_may_inject_yaml_structure(sanitized: &[u8], start: usize, end: usize) -> bool {
    let Ok(text) =
        std::str::from_utf8(&sanitized[start.min(sanitized.len())..end.min(sanitized.len())])
    else {
        return false;
    };
    parse_action_expressions(text)
        .iter()
        .any(TemplateExpr::may_inject_yaml_structure)
}

fn action_structural_indent_width(sanitized: &[u8], start: usize, end: usize) -> Option<usize> {
    let text =
        std::str::from_utf8(&sanitized[start.min(sanitized.len())..end.min(sanitized.len())])
            .ok()?;
    let exprs = parse_action_expressions(text);
    exprs
        .iter()
        .rev()
        .find_map(TemplateExpr::fragment_indent_width)
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
    let snippet = format!("{prefix}{INLINE_PLACEHOLDER}\n");
    let Some(tree) = parse_yaml_tree(&snippet) else {
        return false;
    };
    resolve_yaml_context(
        tree.root_node(),
        &snippet,
        prefix.len(),
        ContextMode::Output {
            placeholder: INLINE_PLACEHOLDER,
        },
        &YamlPath(Vec::new()),
    )
    .is_some_and(|context| context.entire_scalar_value && !context.output_path.0.is_empty())
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
            let structural_indent =
                action_structural_indent_width(sanitized, action_start, action_end);
            let token = placeholder_token(outputs.len(), action_end.saturating_sub(action_start));
            sanitize_output_action(sanitized, action_start, action_end, &token);
            outputs.push(OutputSpan {
                node_start: node.start_byte(),
                node_end: node.end_byte(),
                action_start,
                action_end,
                placeholder: token,
                structural_indent,
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
                    let structural_indent = action_structural_indent_width(sanitized, start, end);
                    let token = placeholder_token(outputs.len(), end.saturating_sub(start));
                    sanitize_output_action(sanitized, start, end, &token);
                    outputs.push(OutputSpan {
                        node_start: output_root.start_byte(),
                        node_end: output_root.end_byte(),
                        action_start: start,
                        action_end: end,
                        placeholder: token,
                        structural_indent,
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

fn line_has_inline_prefix(sanitized: &str, byte: usize) -> bool {
    let byte = byte.min(sanitized.len());
    let line_start = sanitized[..byte].rfind('\n').map_or(0, |index| index + 1);
    !sanitized[line_start..byte].trim().is_empty()
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

fn path_looks_like_scalar_header_artifact(path: &[String]) -> bool {
    path.len() > 1 && matches!(path[0].as_str(), "apiVersion" | "kind")
}

fn path_is_relative_sequence(path: &[String]) -> bool {
    path.first().is_some_and(|segment| segment == "[*]")
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

fn node_covers_gap(node: tree_sitter::Node<'_>, byte: usize) -> bool {
    node.start_byte() <= byte && byte <= node.end_byte()
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
