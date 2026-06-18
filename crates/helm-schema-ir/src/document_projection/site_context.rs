use helm_schema_ast::TemplateExpr;

use crate::fragment_classification::is_fragment_exprs;
use crate::template_expr_cache::parse_expr_text;
use crate::yaml_syntax::first_mapping_colon_offset;
use crate::{ResourceRef, SourceSpan, ValueKind, YamlPath};

use super::tracker::DocumentTracker;

pub(crate) struct DocumentSiteContext {
    pub(crate) kind: ValueKind,
    pub(crate) in_mapping_key: bool,
    pub(crate) in_yaml_comment: bool,
    pub(crate) entire_scalar_value: bool,
    pub(crate) path: YamlPath,
    pub(crate) resource: Option<ResourceRef>,
    pub(crate) source_span: SourceSpan,
}

pub(crate) fn collect_document_site_context(
    source: &str,
    tracker: &DocumentTracker<'_>,
    node: tree_sitter::Node<'_>,
    exprs: &[TemplateExpr],
) -> DocumentSiteContext {
    let output_action = analyze_output_action(source, node, exprs);
    let kind = if output_action.is_fragment {
        ValueKind::Fragment
    } else {
        ValueKind::Scalar
    };

    let in_mapping_key = document_site_is_mapping_key_part(source, node);
    let mut path = if in_mapping_key {
        YamlPath(Vec::new())
    } else {
        tracker.current_path()
    };
    if !in_mapping_key {
        path = adjusted_output_path(
            tracker,
            node,
            kind,
            path,
            output_action.fragment_indent_width,
        );
    }
    if tracker.output_inside_block_scalar_at(node.start_byte()) {
        path = YamlPath(Vec::new());
    }
    let path = tracker.rebase_path(path);
    let resource = tracker.current_resource().cloned();

    DocumentSiteContext {
        kind,
        in_mapping_key,
        in_yaml_comment: document_site_is_yaml_comment_part(source, node),
        entire_scalar_value: document_site_is_entire_scalar_value(source, node),
        path,
        resource,
        source_span: SourceSpan::new(node.start_byte(), node.end_byte()),
    }
}

struct OutputActionShape {
    is_fragment: bool,
    fragment_indent_width: Option<usize>,
}

fn analyze_output_action(
    source: &str,
    node: tree_sitter::Node<'_>,
    exprs: &[TemplateExpr],
) -> OutputActionShape {
    if node.kind() == "template_action" {
        return output_action_shape_from_exprs(exprs);
    }

    if let Some(text) = enclosing_action_text(source, node) {
        return output_action_shape_from_exprs(&parse_expr_text(&text));
    }

    output_action_shape_from_exprs(exprs)
}

fn output_action_shape_from_exprs(exprs: &[TemplateExpr]) -> OutputActionShape {
    OutputActionShape {
        is_fragment: is_fragment_exprs(exprs),
        fragment_indent_width: DocumentTracker::fragment_indent_width_for_exprs(exprs),
    }
}

fn adjusted_output_path(
    tracker: &DocumentTracker<'_>,
    node: tree_sitter::Node<'_>,
    kind: ValueKind,
    mut path: YamlPath,
    fragment_indent_width: Option<usize>,
) -> YamlPath {
    let (physical_indent, _physical_col) = tracker.line_indent_and_col(node.start_byte());
    if tracker.starts_template_action_line(node.start_byte()) {
        let mut logical_indent = physical_indent;
        if let Some(virtual_indent) = fragment_indent_width {
            logical_indent = virtual_indent;
        }

        let trailing_pending_segments =
            tracker.trailing_pending_mapping_segments_at_or_above(logical_indent);
        for _ in 0..trailing_pending_segments {
            path.0.pop();
        }
    }

    if kind == ValueKind::Fragment {
        if let Some(last) = path.0.last_mut()
            && let Some(stripped) = last.strip_suffix("[*]")
        {
            *last = stripped.to_string();
        }
        if matches!(path.0.last().map(std::string::String::as_str), Some("")) {
            path.0.pop();
        }
    }
    if let Some(inline_path) = tracker.inline_mapping_value_path(node) {
        path = inline_path;
    }
    path
}

fn document_site_is_mapping_key_part(source: &str, node: tree_sitter::Node<'_>) -> bool {
    let start = node.start_byte();
    let end = node.end_byte();
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[end..]
        .find('\n')
        .map_or(source.len(), |index| end + index);
    let line = &source[line_start..line_end];
    let rel_start = start - line_start;
    let rel_end = end - line_start;
    let Some(colon_offset) = first_mapping_colon_offset(line) else {
        return false;
    };
    // A template action used before the first mapping separator contributes
    // to key construction, not to the parent value slot.
    rel_start < colon_offset && rel_end <= colon_offset
}

fn enclosing_action_text(source: &str, node: tree_sitter::Node<'_>) -> Option<String> {
    let mut current = node;
    loop {
        match current.kind() {
            "template_action" => {
                return current
                    .utf8_text(source.as_bytes())
                    .ok()
                    .map(std::string::ToString::to_string);
            }
            "if_action" | "with_action" | "range_action" => return None,
            _ => {
                current = current.parent()?;
            }
        }
    }
}

fn document_site_is_entire_scalar_value(source: &str, node: tree_sitter::Node<'_>) -> bool {
    fn normalize_value_text(value_text: &str) -> &str {
        let trimmed = value_text.trim();
        let unquoted = if trimmed.len() >= 2
            && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
                || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
        {
            &trimmed[1..trimmed.len() - 1]
        } else {
            trimmed
        };

        let Some(rest) = unquoted.strip_prefix("{{") else {
            return unquoted.trim();
        };
        let rest = rest.strip_prefix('-').unwrap_or(rest);
        let Some(rest) = rest.strip_suffix("}}") else {
            return unquoted.trim();
        };
        let rest = rest.strip_suffix('-').unwrap_or(rest);
        rest.trim()
    }

    let start = node.start_byte();
    let end = node.end_byte();
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[end..]
        .find('\n')
        .map_or(source.len(), |index| end + index);
    let line = &source[line_start..line_end];
    let rel_start = start - line_start;
    let rel_end = end - line_start;
    let node_text = &line[rel_start..rel_end];

    if let Some(colon_offset) = first_mapping_colon_offset(line) {
        if rel_start <= colon_offset {
            return false;
        }
        let value_text = line[colon_offset + 1..].trim();
        return normalize_value_text(value_text) == node_text.trim();
    }

    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix('-') {
        return normalize_value_text(rest.trim_start()) == node_text.trim();
    }

    if normalize_value_text(trimmed) == node_text.trim() {
        return true;
    }

    false
}

fn document_site_is_yaml_comment_part(source: &str, node: tree_sitter::Node<'_>) -> bool {
    let start = node.start_byte();
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    source[line_start..start].trim_start().starts_with('#')
}
