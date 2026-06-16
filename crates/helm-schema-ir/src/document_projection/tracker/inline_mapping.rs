use crate::YamlPath;
use crate::yaml_syntax::{first_mapping_colon_offset, parse_yaml_key};

use super::shape::Shape;

use super::source_position::line_indent_and_col;

pub(super) fn inline_mapping_value_path(
    source: &str,
    shape: &Shape,
    node: tree_sitter::Node<'_>,
) -> Option<YamlPath> {
    let start = node.start_byte();
    let end = node.end_byte();
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[end..]
        .find('\n')
        .map_or(source.len(), |index| end + index);
    let line = &source[line_start..line_end];
    let relative_start = start - line_start;
    let colon_offset = first_mapping_colon_offset(line)?;
    if relative_start <= colon_offset {
        return None;
    }

    let key = parse_yaml_key(line.trim_start())?.into_key();
    let mut path = shape.current_path();
    let (indent, _column) = line_indent_and_col(source, start);
    let trailing_pending_segments = shape.trailing_pending_mapping_segments_at_or_above(indent);
    for _ in 0..trailing_pending_segments {
        path.0.pop();
    }
    if path.0.last().is_none_or(|segment| segment != &key) {
        path.0.push(key);
    }
    Some(path)
}
