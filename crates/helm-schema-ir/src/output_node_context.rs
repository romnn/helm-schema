use crate::rendered_yaml_context::RenderedYamlContext;
use crate::walker::is_fragment_expr;
use crate::yaml_shape::first_mapping_colon_offset;
use crate::{ResourceRef, ValueKind, YamlPath};

pub(crate) struct OutputNodeContext {
    pub(crate) kind: ValueKind,
    pub(crate) in_mapping_key: bool,
    pub(crate) entire_scalar_value: bool,
    pub(crate) path: YamlPath,
    pub(crate) resource: Option<ResourceRef>,
}

pub(crate) fn output_node_context(
    source: &str,
    rendered_yaml: &RenderedYamlContext<'_>,
    node: tree_sitter::Node<'_>,
    text: &str,
) -> OutputNodeContext {
    let enclosing_action_text = enclosing_action_text(source, node);
    let kind = if enclosing_action_text
        .as_deref()
        .is_some_and(is_fragment_expr)
        || is_fragment_expr(text)
    {
        ValueKind::Fragment
    } else {
        ValueKind::Scalar
    };

    let in_mapping_key = output_node_is_mapping_key_part(source, node);
    let mut path = if in_mapping_key {
        YamlPath(Vec::new())
    } else {
        rendered_yaml.current_path()
    };
    if !in_mapping_key {
        path = adjusted_output_path(rendered_yaml, node, text, kind, path);
    }
    if rendered_yaml.output_inside_block_scalar_at(node.start_byte()) {
        path = YamlPath(Vec::new());
    }
    let path = rendered_yaml.rebase_path(path);
    let resource = rendered_yaml.current_resource().cloned();

    OutputNodeContext {
        kind,
        in_mapping_key,
        entire_scalar_value: output_node_is_entire_scalar_value(source, node),
        path,
        resource,
    }
}

fn adjusted_output_path(
    rendered_yaml: &RenderedYamlContext<'_>,
    node: tree_sitter::Node<'_>,
    text: &str,
    kind: ValueKind,
    mut path: YamlPath,
) -> YamlPath {
    let (physical_indent, _physical_col) = rendered_yaml.line_indent_and_col(node.start_byte());
    if rendered_yaml.starts_template_action_line(node.start_byte()) {
        let mut logical_indent = physical_indent;
        if let Some(virtual_indent) = RenderedYamlContext::fragment_indent_width(text) {
            logical_indent = virtual_indent;
        }

        let trailing_pending_segments =
            rendered_yaml.trailing_pending_mapping_segments_at_or_above(logical_indent);
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
    if let Some(inline_path) = rendered_yaml.inline_mapping_value_path(node) {
        path = inline_path;
    }
    path
}

fn output_node_is_mapping_key_part(source: &str, node: tree_sitter::Node<'_>) -> bool {
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

fn output_node_is_entire_scalar_value(source: &str, node: tree_sitter::Node<'_>) -> bool {
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
