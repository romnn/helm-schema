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
