use std::collections::HashMap;

use helm_schema_ast::{TemplateExpr, parse_action_expressions};

use crate::YamlPath;
use crate::fragment_range_scope::range_body_mapping_entry_indent_from_source;
use crate::yaml_syntax::{first_mapping_colon_offset, parse_yaml_key};

use super::fragment_indent::fragment_indent_width_from_exprs;
use super::yaml_tree::{
    is_scalar_like, parse_yaml_tree, scalar_text, strip_scalar_quotes, unwrap_yaml_node,
};

const PLACEHOLDER_PREFIX: &str = "__HS";
const VIRTUAL_MAPPING_KEY: &str = "__HS_MAPPING_KEY__";

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
    sanitized: String,
    output_nodes: HashMap<(usize, usize), ResolvedNodeContext>,
    output_placeholders: HashMap<(usize, usize), String>,
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
        let (span_start, span_end, placeholder) = self.placeholder_for_node_or_ancestor(node)?;
        self.rendered_output_nodes
            .get(&(span_start, span_end, indent))
            .cloned()
            .or_else(|| {
                resolve_full_document_fragment_output_context(
                    &self.sanitized,
                    span_start,
                    span_end,
                    indent,
                    &placeholder,
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

    fn placeholder_for_node_or_ancestor(
        &self,
        mut node: tree_sitter::Node<'_>,
    ) -> Option<(usize, usize, String)> {
        loop {
            let span = (node.start_byte(), node.end_byte());
            if let Some(placeholder) = self.output_placeholders.get(&span) {
                return Some((span.0, span.1, placeholder.clone()));
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
        attribution.output_placeholders.insert(
            (output.action_start, output.action_end),
            output.placeholder.clone(),
        );
        attribution.output_placeholders.insert(
            (output.node_start, output.node_end),
            output.placeholder.clone(),
        );

        let base_context = tree.as_ref().and_then(|tree| {
            resolve_output_context(
                tree.root_node(),
                &sanitized,
                output.node_start,
                &output.placeholder,
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
                &output.placeholder,
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
            base_context.or(rendered_context.clone())
        } else {
            merge_output_context_candidates(base_context, inline_context, None)
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
            if let Some(context) = resolve_control_context(
                root,
                &sanitized,
                control.context_byte,
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

fn merge_output_context_candidates(
    base: Option<ResolvedNodeContext>,
    inline: Option<ResolvedNodeContext>,
    rendered: Option<ResolvedNodeContext>,
) -> Option<ResolvedNodeContext> {
    let merged = merge_resolved_contexts(base, inline);
    match (merged, rendered) {
        (Some(merged), Some(rendered)) => Some(rebase_virtual_context(merged, rendered)),
        (Some(merged), None) => Some(merged),
        (None, Some(rendered)) => Some(rendered),
        (None, None) => None,
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
    resolve_output_context(
        tree.root_node(),
        &probe,
        action_start.min(probe.len()),
        placeholder,
        &YamlPath(Vec::new()),
    )
}

fn resolve_full_document_fragment_output_context(
    sanitized: &str,
    insertion_start: usize,
    insertion_end: usize,
    indent: usize,
    placeholder: &str,
) -> Option<ResolvedNodeContext> {
    let mapping = resolve_full_document_mapping_entry_context(sanitized, insertion_start, indent);
    let sequence = resolve_full_document_sequence_item_context(
        sanitized,
        insertion_start,
        insertion_end,
        indent,
        placeholder,
    );
    match (mapping, sequence) {
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
    )
    .or_else(|| {
        resolve_full_document_probe_output_context(
            sanitized,
            insertion_byte,
            indent,
            &format!("{VIRTUAL_MAPPING_KEY}: __HS_MAPPING_VALUE__"),
            VIRTUAL_MAPPING_KEY,
        )
    });
    merge_resolved_contexts(
        structural,
        fallback_structural_probe_context(
            sanitized,
            insertion_byte,
            indent,
            ProbeContextKind::Mapping,
        ),
    )
}

fn resolve_full_document_sequence_item_context(
    sanitized: &str,
    insertion_start: usize,
    insertion_end: usize,
    indent: usize,
    placeholder: &str,
) -> Option<ResolvedNodeContext> {
    let insertion_byte = first_nonblank_byte(sanitized.as_bytes(), insertion_start, insertion_end)
        .unwrap_or(insertion_start);
    let structural = resolve_structural_gap_context(
        sanitized,
        insertion_byte,
        indent,
        ProbeContextKind::Sequence,
    )
    .or_else(|| {
        resolve_full_document_probe_output_context(
            sanitized,
            insertion_byte,
            indent,
            &format!("- {placeholder}"),
            placeholder,
        )
    });
    let fallback = fallback_structural_probe_context(
        sanitized,
        insertion_byte,
        indent,
        ProbeContextKind::Sequence,
    );
    merge_sequence_fragment_contexts(structural, fallback)
}

fn merge_sequence_fragment_contexts(
    structural: Option<ResolvedNodeContext>,
    fallback: Option<ResolvedNodeContext>,
) -> Option<ResolvedNodeContext> {
    match (structural, fallback) {
        (Some(structural), Some(fallback))
            if structural.entire_scalar_value
                && fallback.output_path.0.len() < structural.output_path.0.len()
                && path_has_equivalent_prefix(
                    &structural.output_path.0,
                    &fallback.output_path.0,
                ) =>
        {
            Some(fallback)
        }
        (structural, fallback) => merge_resolved_contexts(structural, fallback),
    }
}

fn resolve_full_document_probe_output_context(
    sanitized: &str,
    insertion_byte: usize,
    indent: usize,
    probe: &str,
    placeholder: &str,
) -> Option<ResolvedNodeContext> {
    let tree = parse_yaml_tree(sanitized)?;
    let mut scopes = Vec::new();
    collect_probe_scopes(
        tree.root_node(),
        sanitized,
        insertion_byte,
        &YamlPath(Vec::new()),
        &mut scopes,
    );

    for scope in scopes.into_iter().rev() {
        let relative_insertion = insertion_byte.saturating_sub(scope.span_start);
        let (dedented, dedented_insertion) = dedent_scope_source(
            &sanitized[scope.span_start..scope.span_end],
            scope.indent,
            relative_insertion,
        )?;
        let relative_indent = indent.saturating_sub(scope.indent);
        let (probe_document, probe_byte) = build_probe_document(
            &dedented,
            dedented_insertion,
            relative_indent,
            probe,
            placeholder,
        )?;
        let probe_tree = parse_yaml_tree(&probe_document)?;
        if let Some(context) = resolve_output_context(
            probe_tree.root_node(),
            &probe_document,
            probe_byte,
            placeholder,
            &scope.root_path,
        ) {
            return Some(context);
        }
    }

    None
}

fn build_probe_document(
    sanitized: &str,
    insertion_byte: usize,
    indent: usize,
    probe: &str,
    placeholder: &str,
) -> Option<(String, usize)> {
    let insertion_byte = insertion_byte.min(sanitized.len());
    let line_start = sanitized[..insertion_byte]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line_end = sanitized[insertion_byte..]
        .find('\n')
        .map_or(sanitized.len(), |index| insertion_byte + index);
    let line_after = if line_end < sanitized.len() {
        line_end + 1
    } else {
        line_end
    };
    let has_inline_prefix = !sanitized[line_start..insertion_byte].trim().is_empty();
    let prefix_end = if has_inline_prefix {
        insertion_byte
    } else {
        line_start
    };
    let suffix_start = line_after;

    let mut document = String::with_capacity(sanitized.len() + indent + probe.len() + 2);
    document.push_str(&sanitized[..prefix_end]);
    if has_inline_prefix {
        document.push('\n');
    }
    let probe_line_start = document.len();
    document.push_str(&" ".repeat(indent));
    document.push_str(probe);
    document.push('\n');
    document.push_str(&sanitized[suffix_start..]);

    let probe_offset = probe.find(placeholder)?;
    Some((document, probe_line_start + indent + probe_offset))
}

#[derive(Clone)]
struct ProbeScope {
    span_start: usize,
    span_end: usize,
    indent: usize,
    root_path: YamlPath,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProbeContextKind {
    Mapping,
    Sequence,
}

#[derive(Clone)]
struct OpenValueFrame {
    indent: usize,
    path: YamlPath,
    kind: OpenValueFrameKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpenValueFrameKind {
    MappingValue,
    SequenceItem,
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
    resolve_structural_gap_context_in_node(
        tree.root_node(),
        sanitized,
        insertion_byte,
        indent,
        &YamlPath(Vec::new()),
        kind,
    )
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
            return resolve_structural_gap_in_sequence_child(
                child, source, byte, indent, path, kind,
            );
        }
        if byte < child.start_byte() {
            return sequence_container_gap_context(node, indent, path, kind).or_else(|| {
                previous.and_then(|child| {
                    resolve_trailing_structural_gap_from_child(
                        child, source, byte, indent, path, kind,
                    )
                })
            });
        }
        previous = Some(child);
    }

    sequence_container_gap_context(node, indent, path, kind).or_else(|| {
        previous.and_then(|child| {
            resolve_trailing_structural_gap_from_child(child, source, byte, indent, path, kind)
        })
    })
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
                None
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
    let child_path = mapping_pair_open_value_path(pair, source, path)?;
    (indent > pair.start_position().column).then(|| probe_context_for_path(&child_path, kind))
}

fn mapping_pair_open_value_path(
    node: tree_sitter::Node<'_>,
    source: &str,
    path: &YamlPath,
) -> Option<YamlPath> {
    node.child_by_field_name("value")
        .is_none()
        .then(|| mapping_pair_child_path(node, source, path))
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
    }
}

fn fallback_structural_probe_context(
    sanitized: &str,
    insertion_byte: usize,
    indent: usize,
    kind: ProbeContextKind,
) -> Option<ResolvedNodeContext> {
    let target_line_start = sanitized[..insertion_byte.min(sanitized.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    if !sanitized[target_line_start..insertion_byte.min(sanitized.len())]
        .trim()
        .is_empty()
    {
        return None;
    }
    let mut frames = Vec::<OpenValueFrame>::new();
    let mut offset = 0usize;

    for line in sanitized.split_inclusive('\n') {
        let line_start = offset;
        let line_end = line_start + line.len();
        if line_start >= target_line_start {
            break;
        }
        process_structural_line(line, &mut frames);
        offset = line_end;
    }

    while frames.last().is_some_and(|frame| indent < frame.indent) {
        frames.pop();
    }
    if kind == ProbeContextKind::Mapping
        && frames.last().is_some_and(|frame| {
            frame.kind == OpenValueFrameKind::MappingValue && frame.indent == indent
        })
    {
        frames.pop();
    }
    if kind == ProbeContextKind::Sequence
        && frames.last().is_some_and(|frame| {
            frame.kind == OpenValueFrameKind::SequenceItem && frame.indent == indent
        })
    {
        frames.pop();
    }

    let parent_path = frames
        .last()
        .map(|frame| frame.path.clone())
        .unwrap_or_else(|| YamlPath(Vec::new()));
    let output_path = match kind {
        ProbeContextKind::Mapping => parent_path.clone(),
        ProbeContextKind::Sequence => append_sequence_segment(&parent_path),
    };

    Some(ResolvedNodeContext {
        current_path: parent_path.clone(),
        output_path,
        mapping_entry_path: parent_path,
        in_mapping_key: false,
        entire_scalar_value: false,
        inside_block_scalar: false,
    })
}

fn process_structural_line(line: &str, frames: &mut Vec<OpenValueFrame>) {
    let content = line.trim_end_matches(['\n', '\r']);
    if content.trim().is_empty() {
        return;
    }

    let indent = content.chars().take_while(|&ch| ch == ' ').count();
    let after = &content[indent..];
    let starts_sequence_item = after.starts_with('-');
    while let Some(frame) = frames.last() {
        if indent < frame.indent {
            frames.pop();
            continue;
        }
        if indent == frame.indent {
            if starts_sequence_item && frame.kind == OpenValueFrameKind::MappingValue {
                break;
            }
            frames.pop();
            continue;
        }
        break;
    }

    let current_path = frames
        .last()
        .map(|frame| frame.path.clone())
        .unwrap_or_else(|| YamlPath(Vec::new()));

    if let Some(rest) = after.strip_prefix('-') {
        let item_path = append_sequence_segment(&current_path);
        frames.push(OpenValueFrame {
            indent,
            path: item_path.clone(),
            kind: OpenValueFrameKind::SequenceItem,
        });
        let rest = rest.trim_start();
        if rest.is_empty() {
            return;
        }
        if let Some((key, has_nested_value)) = parse_mapping_line(rest) {
            if has_nested_value {
                frames.push(OpenValueFrame {
                    indent: indent + 2,
                    path: append_mapping_segment(&item_path, &key),
                    kind: OpenValueFrameKind::MappingValue,
                });
            }
        }
        return;
    }

    if let Some((key, has_nested_value)) = parse_mapping_line(after)
        && has_nested_value
    {
        frames.push(OpenValueFrame {
            indent,
            path: append_mapping_segment(&current_path, &key),
            kind: OpenValueFrameKind::MappingValue,
        });
    }
}

fn parse_mapping_line(text: &str) -> Option<(String, bool)> {
    let key = parse_yaml_key(text)?.into_key();
    let colon = first_mapping_colon_offset(text)?;
    let remainder = text[colon + 1..].trim();
    let opens_nested_value =
        remainder.is_empty() || matches!(remainder.chars().next(), Some('|') | Some('>'));
    Some((key, opens_nested_value))
}

fn collect_probe_scopes(
    node: tree_sitter::Node<'_>,
    source: &str,
    byte: usize,
    path: &YamlPath,
    scopes: &mut Vec<ProbeScope>,
) {
    if !contains_byte(node, byte) {
        return;
    }

    match node.kind() {
        "stream" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if !child.is_named() {
                    continue;
                }
                collect_probe_scopes(child, source, byte, path, scopes);
            }
        }
        "document" | "block_mapping" | "flow_mapping" | "block_sequence" | "flow_sequence" => {
            scopes.push(ProbeScope {
                span_start: node.start_byte(),
                span_end: node.end_byte(),
                indent: node.start_position().column,
                root_path: path.clone(),
            });
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if !child.is_named() {
                    continue;
                }
                collect_probe_scopes(child, source, byte, path, scopes);
            }
        }
        "block_mapping_pair" | "flow_pair" => {
            scopes.push(ProbeScope {
                span_start: node.start_byte(),
                span_end: node.end_byte(),
                indent: node.start_position().column,
                root_path: path.clone(),
            });

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
                collect_probe_scopes(key, source, byte, path, scopes);
            }
            if let Some(value) = value
                && contains_byte(value, byte)
            {
                collect_probe_scopes(value, source, byte, &child_path, scopes);
            }
        }
        "block_sequence_item" => {
            scopes.push(ProbeScope {
                span_start: node.start_byte(),
                span_end: node.end_byte(),
                indent: node.start_position().column,
                root_path: path.clone(),
            });

            if let Some(child) = node.named_child(0) {
                let seq_path = append_sequence_segment(path);
                collect_probe_scopes(unwrap_yaml_node(child), source, byte, &seq_path, scopes);
            }
        }
        "block_node" | "flow_node" => {
            if let Some(child) = node.named_child(0) {
                collect_probe_scopes(child, source, byte, path, scopes);
            }
        }
        _ => {}
    }
}

fn dedent_scope_source(
    snippet: &str,
    indent: usize,
    insertion_byte: usize,
) -> Option<(String, usize)> {
    if indent == 0 {
        return Some((snippet.to_string(), insertion_byte.min(snippet.len())));
    }

    let mut dedented = String::with_capacity(snippet.len());
    let mut old_line_start = 0usize;
    let mut new_insertion_byte = None;

    for line in snippet.split_inclusive('\n') {
        let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
        let removable = line_without_newline
            .chars()
            .take_while(|&ch| ch == ' ')
            .count()
            .min(indent);
        let line_start = old_line_start;
        let line_end = line_start + line.len();
        if new_insertion_byte.is_none() && (line_start..=line_end).contains(&insertion_byte) {
            new_insertion_byte = Some(
                dedented.len()
                    + insertion_byte
                        .saturating_sub(line_start)
                        .saturating_sub(removable),
            );
        }
        dedented.push_str(&line[removable..]);
        old_line_start = line_end;
    }

    Some((
        dedented,
        new_insertion_byte.unwrap_or_else(|| insertion_byte.saturating_sub(indent)),
    ))
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

fn action_structural_indent_width(sanitized: &[u8], start: usize, end: usize) -> Option<usize> {
    let text =
        std::str::from_utf8(&sanitized[start.min(sanitized.len())..end.min(sanitized.len())])
            .ok()?;
    let exprs = parse_action_expressions(text);
    fragment_indent_width_from_exprs(&exprs)
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
    let snippet = format!("{prefix}{PLACEHOLDER_PREFIX}INLINE__\n");
    let Some(tree) = parse_yaml_tree(&snippet) else {
        return false;
    };
    resolve_output_context(
        tree.root_node(),
        &snippet,
        prefix.len(),
        &format!("{PLACEHOLDER_PREFIX}INLINE__"),
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

    let Some(tree) = parse_go_template_tree(&text) else {
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

fn parse_go_template_tree(source: &str) -> Option<tree_sitter::Tree> {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    parser.parse(source, None)
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
mod tests {
    use super::placeholder_token;
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn short_placeholder_tokens_remain_distinct_for_dense_inline_actions() {
        let tokens = (0..36)
            .map(|index| placeholder_token(index, 5))
            .collect::<std::collections::BTreeSet<_>>();

        sim_assert_eq!(have: tokens.len(), want: 36);
        assert!(tokens.iter().all(|token| token.starts_with("__HS")));
    }
}
