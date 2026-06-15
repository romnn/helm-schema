use crate::yaml_shape::source_line_starts_block_scalar;

pub(super) fn line_indent_and_col(source: &str, byte_pos: usize) -> (usize, usize) {
    let bytes = source.as_bytes();
    let line_start = line_start_before(bytes, byte_pos);

    let column = byte_pos.saturating_sub(line_start);
    let mut indent = 0usize;
    while line_start + indent < bytes.len() {
        if bytes[line_start + indent] == b' ' {
            indent += 1;
        } else {
            break;
        }
    }
    (indent, column)
}

pub(super) fn starts_template_action_line(source: &str, byte_pos: usize) -> bool {
    let bytes = source.as_bytes();
    let line_start = line_start_before(bytes, byte_pos);
    let prefix = &source[line_start..byte_pos.min(source.len())];
    prefix.trim_start().starts_with("{{")
}

pub(super) fn source_position_is_inside_block_scalar(
    source: &str,
    byte_pos: usize,
    indent: usize,
) -> bool {
    let bytes = source.as_bytes();
    let mut line_start = line_start_before(bytes, byte_pos);

    while line_start > 0 {
        let previous_line_end = line_start.saturating_sub(1);
        let previous_line_start = line_start_before(bytes, previous_line_end);

        let line = &source[previous_line_start..previous_line_end];
        let previous_indent = line.chars().take_while(|&ch| ch == ' ').count();
        let after_indent = &line[previous_indent..];
        let trimmed = after_indent.trim();

        if trimmed.is_empty() {
            line_start = previous_line_start;
            continue;
        }

        if previous_indent >= indent || trimmed.starts_with("{{") {
            line_start = previous_line_start;
            continue;
        }

        return source_line_starts_block_scalar(after_indent);
    }

    false
}

fn line_start_before(bytes: &[u8], byte_pos: usize) -> usize {
    let mut line_start = byte_pos.min(bytes.len());
    while line_start > 0 {
        if bytes[line_start - 1] == b'\n' {
            break;
        }
        line_start -= 1;
    }
    line_start
}
