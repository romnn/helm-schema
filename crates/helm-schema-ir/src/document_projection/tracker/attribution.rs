use std::collections::HashMap;

use helm_schema_ast::{TemplateExpr, parse_action_expressions};

use crate::YamlPath;

use super::yaml_tree::{
    is_scalar_like, parse_yaml_tree, scalar_text, strip_scalar_quotes, unwrap_yaml_node,
};

const PLACEHOLDER_PREFIX: &str = "__HS";
const VIRTUAL_MAPPING_KEY: &str = "__HS_MAPPING_KEY__";
const MAX_CONTEXT_WINDOW_STARTS: usize = 64;

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
        let (action_start, action_end, placeholder) =
            self.placeholder_for_node_or_ancestor(node)?;
        resolve_virtual_indent_output_context(
            &self.sanitized,
            action_start,
            action_end,
            indent,
            &placeholder,
        )
    }

    pub(super) fn mapping_entry_context_in_span_at_indent(
        &self,
        start: usize,
        end: usize,
        indent: usize,
    ) -> Option<ResolvedNodeContext> {
        let insertion_byte =
            first_nonblank_byte(self.sanitized.as_bytes(), start, end).unwrap_or(start);
        resolve_virtual_mapping_entry_context(&self.sanitized, insertion_byte, indent)
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
}

struct ControlSpan {
    span_start: usize,
    span_end: usize,
    context_byte: usize,
}

pub(super) fn build_attribution_index(
    source: &str,
    root: tree_sitter::Node<'_>,
) -> AttributionIndex {
    let mut sanitized = source.as_bytes().to_vec();
    let mut outputs = Vec::<OutputSpan>::new();
    let mut controls = Vec::<ControlSpan>::new();
    sanitize_stream(
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
        let global_context = tree.as_ref().and_then(|tree| {
            resolve_output_context(
                tree.root_node(),
                &sanitized,
                output.node_start,
                &output.placeholder,
                &YamlPath(Vec::new()),
            )
        });
        let prefix_context =
            resolve_prefix_output_context(&sanitized, output.action_end, &output.placeholder);
        let local_context =
            resolve_local_output_context(&sanitized, output.action_start, &output.placeholder);

        let mut document_context = merge_resolved_contexts(global_context, prefix_context);
        if should_resolve_window_context(document_context.as_ref(), local_context.as_ref()) {
            let window_context = resolve_window_output_context(
                &sanitized,
                output.action_start,
                output.action_end,
                &output.placeholder,
                local_context.as_ref(),
            );
            document_context = merge_document_window_context(
                document_context,
                window_context,
                local_context.as_ref(),
            );
        }
        attribution.output_placeholders.insert(
            (output.action_start, output.action_end),
            output.placeholder.clone(),
        );
        attribution.output_placeholders.insert(
            (output.node_start, output.node_end),
            output.placeholder.clone(),
        );

        if let Some(context) = merge_resolved_contexts(document_context, local_context) {
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
            if let Some(context) =
                resolve_control_context_for_span(root, &sanitized, control.context_byte)
            {
                attribution
                    .control_nodes
                    .insert((control.span_start, control.span_end), context);
            }
        }
    }

    attribution
}

fn resolve_control_context_for_span(
    root: tree_sitter::Node<'_>,
    sanitized: &str,
    context_byte: usize,
) -> Option<ResolvedNodeContext> {
    let global_context =
        resolve_control_context(root, sanitized, context_byte, &YamlPath(Vec::new()));
    let prefix_context = resolve_prefix_control_context(sanitized, context_byte);
    let mut context = merge_resolved_contexts(global_context, prefix_context);
    if context.as_ref().is_none_or(context_needs_window_resolution) {
        context = merge_resolved_contexts(
            context,
            resolve_window_control_context(sanitized, context_byte),
        );
    }
    context
}

fn should_resolve_window_context(
    document_context: Option<&ResolvedNodeContext>,
    local_context: Option<&ResolvedNodeContext>,
) -> bool {
    let Some(local_context) = local_context else {
        return document_context.is_none();
    };
    let local_path = &local_context.output_path.0;
    let local_is_relative_sequence = local_path.first().is_some_and(|segment| segment == "[*]");

    let Some(document_context) = document_context else {
        return true;
    };
    let document_path = &document_context.output_path.0;
    document_path.is_empty()
        || document_path
            .first()
            .is_some_and(|segment| segment == "[*]")
        || local_is_relative_sequence && local_path.len() > document_path.len()
        || local_context.entire_scalar_value
            && !local_path.is_empty()
            && !path_has_equivalent_suffix(document_path, local_path)
}

fn context_needs_window_resolution(context: &ResolvedNodeContext) -> bool {
    let path = &context.output_path.0;
    path.is_empty()
        || path.first().is_some_and(|segment| segment == "[*]")
        || path_looks_like_scalar_header_artifact(path)
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

fn merge_document_window_context(
    document: Option<ResolvedNodeContext>,
    window: Option<ResolvedNodeContext>,
    local: Option<&ResolvedNodeContext>,
) -> Option<ResolvedNodeContext> {
    match (document, window) {
        (Some(document), Some(window))
            if local.is_some_and(|local| {
                !local.output_path.0.is_empty()
                    && !path_has_equivalent_suffix(&document.output_path.0, &local.output_path.0)
                    && path_has_equivalent_suffix(&window.output_path.0, &local.output_path.0)
            }) =>
        {
            Some(rebase_virtual_context(document, window))
        }
        (document, window) => merge_resolved_contexts(document, window),
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
            } else if path_looks_like_scalar_header_artifact(&right.0)
                && !path_looks_like_scalar_header_artifact(&left.0)
            {
                left
            } else if path_has_equivalent_suffix(&left.0, &right.0) && left.0.len() > right.0.len()
            {
                left
            } else if path_has_equivalent_suffix(&right.0, &left.0) && right.0.len() > left.0.len()
            {
                right
            } else if right.0.len() > left.0.len()
                && !path_looks_like_scalar_header_artifact(&right.0)
            {
                right
            } else {
                left
            }
        }
    }
}

fn resolve_prefix_output_context(
    sanitized: &str,
    action_end: usize,
    placeholder: &str,
) -> Option<ResolvedNodeContext> {
    let action_end = action_end.min(sanitized.len());
    let end = sanitized[action_end..]
        .find('\n')
        .map_or(sanitized.len(), |index| action_end + index + 1);
    let snippet = &sanitized[..end];
    let placeholder_byte = snippet.rfind(placeholder)?;
    let tree = parse_yaml_tree(snippet)?;
    resolve_output_context(
        tree.root_node(),
        snippet,
        placeholder_byte,
        placeholder,
        &YamlPath(Vec::new()),
    )
}

fn resolve_prefix_control_context(
    sanitized: &str,
    context_byte: usize,
) -> Option<ResolvedNodeContext> {
    let context_byte = context_byte.min(sanitized.len());
    let end = sanitized[context_byte..]
        .find('\n')
        .map_or(sanitized.len(), |index| context_byte + index + 1);
    let snippet = &sanitized[..end];
    let tree = parse_yaml_tree(snippet)?;
    resolve_control_context(
        tree.root_node(),
        snippet,
        context_byte,
        &YamlPath(Vec::new()),
    )
}

fn resolve_virtual_indent_output_context(
    sanitized: &str,
    action_start: usize,
    _action_end: usize,
    indent: usize,
    placeholder: &str,
) -> Option<ResolvedNodeContext> {
    let full_context = merge_resolved_contexts(
        resolve_virtual_mapping_entry_context(sanitized, action_start, indent),
        resolve_virtual_sequence_item_context(sanitized, action_start, indent, placeholder),
    );
    let local_context = resolve_virtual_indent_local_output_context(
        sanitized,
        action_start,
        indent,
        placeholder,
        full_context.as_ref(),
    );

    match (full_context, local_context) {
        (Some(full), Some(local)) => Some(rebase_virtual_context(full, local)),
        (Some(full), None) => Some(full),
        (None, Some(local)) => Some(local),
        (None, None) => None,
    }
}

fn resolve_virtual_mapping_entry_context(
    sanitized: &str,
    insertion_byte: usize,
    indent: usize,
) -> Option<ResolvedNodeContext> {
    resolve_virtual_probe_output_context(
        sanitized,
        insertion_byte,
        indent,
        &format!("{VIRTUAL_MAPPING_KEY}: __HS_MAPPING_VALUE__"),
        VIRTUAL_MAPPING_KEY,
    )
}

fn resolve_virtual_sequence_item_context(
    sanitized: &str,
    insertion_byte: usize,
    indent: usize,
    placeholder: &str,
) -> Option<ResolvedNodeContext> {
    resolve_virtual_probe_output_context(
        sanitized,
        insertion_byte,
        indent,
        &format!("- {placeholder}"),
        placeholder,
    )
}

fn resolve_virtual_probe_output_context(
    sanitized: &str,
    insertion_byte: usize,
    indent: usize,
    probe: &str,
    placeholder: &str,
) -> Option<ResolvedNodeContext> {
    let insertion_byte = insertion_byte.min(sanitized.len());
    let line_start = sanitized[..insertion_byte]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let has_inline_prefix = !sanitized[line_start..insertion_byte].trim().is_empty();
    let prefix_end = if has_inline_prefix {
        insertion_byte
    } else {
        line_start
    };
    let mut snippet = String::with_capacity(prefix_end + indent + probe.len() + 2);
    snippet.push_str(&sanitized[..prefix_end]);
    if has_inline_prefix {
        snippet.push('\n');
    }
    snippet.push_str(&" ".repeat(indent));
    snippet.push_str(probe);
    snippet.push('\n');
    let placeholder_byte = snippet.rfind(placeholder)?;
    let tree = parse_yaml_tree(&snippet)?;
    resolve_output_context(
        tree.root_node(),
        &snippet,
        placeholder_byte,
        placeholder,
        &YamlPath(Vec::new()),
    )
}

fn resolve_virtual_indent_local_output_context(
    sanitized: &str,
    action_start: usize,
    indent: usize,
    placeholder: &str,
    base_context: Option<&ResolvedNodeContext>,
) -> Option<ResolvedNodeContext> {
    merge_local_contexts_for_base(
        resolve_virtual_indent_local_probe_context(
            sanitized,
            action_start,
            indent,
            &format!("{VIRTUAL_MAPPING_KEY}: __HS_MAPPING_VALUE__"),
            VIRTUAL_MAPPING_KEY,
            base_context,
        ),
        resolve_virtual_indent_local_probe_context(
            sanitized,
            action_start,
            indent,
            &format!("- {placeholder}"),
            placeholder,
            base_context,
        ),
        base_context,
    )
}

fn resolve_virtual_indent_local_probe_context(
    sanitized: &str,
    insertion_byte: usize,
    indent: usize,
    probe: &str,
    placeholder: &str,
    base_context: Option<&ResolvedNodeContext>,
) -> Option<ResolvedNodeContext> {
    let insertion_byte = insertion_byte.min(sanitized.len());
    let line_start = sanitized[..insertion_byte]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let has_inline_prefix = !sanitized[line_start..insertion_byte].trim().is_empty();
    let prefix_end = if has_inline_prefix {
        insertion_byte
    } else {
        line_start
    };
    let mut starts = sanitized[..line_start]
        .match_indices('\n')
        .map(|(index, _)| index + 1)
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();

    let mut best = None;
    for start in starts.into_iter().rev().take(MAX_CONTEXT_WINDOW_STARTS) {
        let mut snippet = String::with_capacity(prefix_end - start + indent + probe.len() + 2);
        snippet.push_str(&sanitized[start..prefix_end]);
        if has_inline_prefix {
            snippet.push('\n');
        }
        snippet.push_str(&" ".repeat(indent));
        snippet.push_str(probe);
        snippet.push('\n');
        let Some(placeholder_byte) = snippet.rfind(placeholder) else {
            continue;
        };
        let context = parse_yaml_tree(&snippet)
            .and_then(|tree| {
                resolve_output_context(
                    tree.root_node(),
                    &snippet,
                    placeholder_byte,
                    placeholder,
                    &YamlPath(Vec::new()),
                )
            })
            .or_else(|| {
                dedent_yaml_window(&snippet, placeholder_byte).and_then(
                    |(dedented, dedented_placeholder_byte)| {
                        parse_yaml_tree(&dedented).and_then(|tree| {
                            resolve_output_context(
                                tree.root_node(),
                                &dedented,
                                dedented_placeholder_byte,
                                placeholder,
                                &YamlPath(Vec::new()),
                            )
                        })
                    },
                )
            });
        let Some(context) = context else {
            continue;
        };
        if context_specificity_score(&context) == 0 {
            continue;
        }
        best = Some(match best {
            Some(best) => prefer_local_context_for_base(best, context, base_context),
            None => context,
        });
    }
    best
}

fn merge_local_contexts_for_base(
    left: Option<ResolvedNodeContext>,
    right: Option<ResolvedNodeContext>,
    base_context: Option<&ResolvedNodeContext>,
) -> Option<ResolvedNodeContext> {
    match (left, right) {
        (Some(left), Some(right)) => Some(prefer_local_context_for_base(left, right, base_context)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn prefer_local_context_for_base(
    left: ResolvedNodeContext,
    right: ResolvedNodeContext,
    base_context: Option<&ResolvedNodeContext>,
) -> ResolvedNodeContext {
    let mut preferred = if local_context_score_for_base(&right, base_context)
        > local_context_score_for_base(&left, base_context)
    {
        right.clone()
    } else {
        left.clone()
    };
    preferred.in_mapping_key |= left.in_mapping_key || right.in_mapping_key;
    preferred.entire_scalar_value |= left.entire_scalar_value || right.entire_scalar_value;
    preferred.inside_block_scalar |= left.inside_block_scalar || right.inside_block_scalar;
    preferred
}

fn local_context_score_for_base(
    context: &ResolvedNodeContext,
    base_context: Option<&ResolvedNodeContext>,
) -> usize {
    let base_path = base_context
        .filter(|base| !context_needs_window_resolution(base))
        .map(|base| &base.output_path.0);
    let path = &context.output_path.0;
    if path_looks_like_scalar_header_artifact(path) || path_is_relative_sequence(path) {
        return 0;
    }

    let Some(base_path) = base_path.filter(|path| !path.is_empty()) else {
        return path.len();
    };
    if path_has_equivalent_prefix(path, base_path) {
        return path.len().saturating_sub(base_path.len());
    }
    if path_has_equivalent_suffix(base_path, path) || path_has_equivalent_prefix(base_path, path) {
        return 0;
    }
    path.len()
}

fn rebase_virtual_context(
    mut full: ResolvedNodeContext,
    local: ResolvedNodeContext,
) -> ResolvedNodeContext {
    if local.output_path.0.is_empty() || context_needs_window_resolution(&local) {
        return full;
    }
    if full.output_path.0.is_empty() || context_needs_window_resolution(&full) {
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

fn resolve_window_output_context(
    sanitized: &str,
    action_start: usize,
    action_end: usize,
    placeholder: &str,
    local_context: Option<&ResolvedNodeContext>,
) -> Option<ResolvedNodeContext> {
    let action_start = action_start.min(sanitized.len());
    let action_end = action_end.min(sanitized.len());
    let line_end = sanitized[action_end..]
        .find('\n')
        .map_or(sanitized.len(), |index| action_end + index + 1);
    let mut starts = sanitized[..action_start]
        .match_indices('\n')
        .map(|(index, _)| index + 1)
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();

    let mut best = None;
    for start in starts.into_iter().rev().take(MAX_CONTEXT_WINDOW_STARTS) {
        let snippet = &sanitized[start..line_end];
        let Some(placeholder_byte) = snippet.rfind(placeholder) else {
            continue;
        };
        let Some(context) =
            resolve_output_context_in_window(snippet, placeholder_byte, placeholder)
        else {
            continue;
        };
        best = Some(match best {
            Some(best) => prefer_more_specific_context_for_local(best, context, local_context),
            None => context,
        });
    }
    best
}

fn resolve_output_context_in_window(
    snippet: &str,
    placeholder_byte: usize,
    placeholder: &str,
) -> Option<ResolvedNodeContext> {
    let direct = parse_yaml_tree(snippet).and_then(|tree| {
        resolve_output_context(
            tree.root_node(),
            snippet,
            placeholder_byte,
            placeholder,
            &YamlPath(Vec::new()),
        )
    });
    let dedented = dedent_yaml_window(snippet, placeholder_byte).and_then(
        |(dedented, dedented_placeholder_byte)| {
            parse_yaml_tree(&dedented).and_then(|tree| {
                resolve_output_context(
                    tree.root_node(),
                    &dedented,
                    dedented_placeholder_byte,
                    placeholder,
                    &YamlPath(Vec::new()),
                )
            })
        },
    );
    merge_resolved_contexts(direct, dedented)
}

fn dedent_yaml_window(snippet: &str, placeholder_byte: usize) -> Option<(String, usize)> {
    let common_indent = snippet
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| line.bytes().position(|byte| byte != b' '))
        .min()?;
    if common_indent == 0 {
        return None;
    }

    let mut dedented = String::with_capacity(snippet.len());
    let mut old_line_start = 0usize;
    let mut new_placeholder_byte = None;

    for line in snippet.split_inclusive('\n') {
        let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
        let line_indent = line_without_newline
            .bytes()
            .position(|byte| byte != b' ')
            .unwrap_or(line_without_newline.len());
        let drop = common_indent.min(line_indent);
        let line_start = old_line_start;
        let line_end = line_start + line.len();

        if (line_start..line_end).contains(&placeholder_byte) {
            new_placeholder_byte = Some(dedented.len() + placeholder_byte - line_start - drop);
        }

        dedented.push_str(&line[drop..]);
        old_line_start = line_end;
    }

    Some((dedented, new_placeholder_byte?))
}

fn resolve_window_control_context(
    sanitized: &str,
    context_byte: usize,
) -> Option<ResolvedNodeContext> {
    let context_byte = context_byte.min(sanitized.len());
    let line_end = sanitized[context_byte..]
        .find('\n')
        .map_or(sanitized.len(), |index| context_byte + index + 1);
    let mut starts = sanitized[..context_byte]
        .match_indices('\n')
        .map(|(index, _)| index + 1)
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    starts.sort_unstable();
    starts.dedup();

    let mut best = None;
    for start in starts.into_iter().rev().take(MAX_CONTEXT_WINDOW_STARTS) {
        let snippet = &sanitized[start..line_end];
        let Some(tree) = parse_yaml_tree(snippet) else {
            continue;
        };
        let Some(context) = resolve_control_context(
            tree.root_node(),
            snippet,
            context_byte.saturating_sub(start),
            &YamlPath(Vec::new()),
        ) else {
            continue;
        };
        best = Some(match best {
            Some(best) => prefer_more_specific_context(best, context),
            None => context,
        });
    }
    best
}

fn prefer_more_specific_context(
    left: ResolvedNodeContext,
    right: ResolvedNodeContext,
) -> ResolvedNodeContext {
    let mut preferred = if context_specificity_score(&right) > context_specificity_score(&left) {
        right.clone()
    } else {
        left.clone()
    };
    preferred.in_mapping_key |= left.in_mapping_key || right.in_mapping_key;
    preferred.entire_scalar_value |= left.entire_scalar_value || right.entire_scalar_value;
    preferred.inside_block_scalar |= left.inside_block_scalar || right.inside_block_scalar;
    preferred
}

fn prefer_more_specific_context_for_local(
    left: ResolvedNodeContext,
    right: ResolvedNodeContext,
    local: Option<&ResolvedNodeContext>,
) -> ResolvedNodeContext {
    let Some(local) = local.filter(|context| !context.output_path.0.is_empty()) else {
        return prefer_more_specific_context(left, right);
    };

    let left_has_local_suffix =
        path_has_equivalent_suffix(&left.output_path.0, &local.output_path.0);
    let right_has_local_suffix =
        path_has_equivalent_suffix(&right.output_path.0, &local.output_path.0);

    match (left_has_local_suffix, right_has_local_suffix) {
        (true, false) => left,
        (false, true) => right,
        _ => prefer_more_specific_context(left, right),
    }
}

fn context_specificity_score(context: &ResolvedNodeContext) -> usize {
    if path_looks_like_scalar_header_artifact(&context.output_path.0) {
        0
    } else if path_is_relative_sequence(&context.output_path.0) {
        0
    } else {
        context.output_path.0.len()
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
    Some(context)
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
    if action_uses_structural_indent_filter(sanitized, start, end) {
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
            sanitize_control_node(node, sanitized, outputs, controls);
            index += 1;
            continue;
        }

        if node.is_named() && is_output_root_kind(node.kind()) {
            let (action_start, action_end) = enclosing_template_action_span(node);
            let token = placeholder_token(outputs.len(), action_end.saturating_sub(action_start));
            sanitize_output_action(sanitized, action_start, action_end, &token);
            outputs.push(OutputSpan {
                node_start: node.start_byte(),
                node_end: node.end_byte(),
                action_start,
                action_end,
                placeholder: token,
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
                    let token = placeholder_token(outputs.len(), end.saturating_sub(start));
                    sanitize_output_action(sanitized, start, end, &token);
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
        } else if matches!(node.kind(), "text" | "yaml_no_injection_text") {
            sanitize_embedded_template_actions(node, sanitized);
        }

        index += 1;
    }
}

fn sanitize_control_node(
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
    sanitize_stream(&kept_children, sanitized, outputs, controls);

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
