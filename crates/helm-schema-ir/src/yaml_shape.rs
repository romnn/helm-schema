use crate::{YamlPath, fragment_classification::is_fragment_expr};

/// Tracks the rendered YAML location while the symbolic walker moves across
/// mixed YAML/template source.
#[derive(Clone, Debug)]
pub(crate) struct Shape {
    stack: Vec<StackEntry>,
    at_line_start: bool,
    clear_pending_on_newline_at_indent: Option<usize>,
    prefix: Vec<String>,
    stack_floor: usize,
    block_scalar_parent_indent: Option<usize>,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            at_line_start: true,
            clear_pending_on_newline_at_indent: None,
            prefix: Vec::new(),
            stack_floor: 0,
            block_scalar_parent_indent: None,
        }
    }
}

#[derive(Clone, Debug)]
struct StackEntry {
    indent: usize,
    container: Container,
    pending_key: Option<String>,
}

impl StackEntry {
    fn mapping(indent: usize, pending_key: Option<String>) -> Self {
        Self {
            indent,
            container: Container::Mapping,
            pending_key,
        }
    }

    fn sequence(indent: usize) -> Self {
        Self {
            indent,
            container: Container::Sequence,
            pending_key: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Container {
    Mapping,
    Sequence,
}

pub(crate) struct ParsedYamlKey {
    key: String,
    scalar_value_present: bool,
    starts_block_scalar: bool,
}

impl ParsedYamlKey {
    pub(crate) fn into_key(self) -> String {
        self.key
    }
}

pub(crate) fn parse_yaml_key(after: &str) -> Option<ParsedYamlKey> {
    fn finalize_yaml_key(key: String, rest: &str) -> Option<ParsedYamlKey> {
        if key.is_empty() {
            return None;
        }
        let rest = rest.trim_start();
        let rest = rest.strip_prefix(':').unwrap_or(rest);
        let rest = rest.trim_start();
        let starts_block_scalar = rest.starts_with('|') || rest.starts_with('>');
        let is_template_fragment = rest.starts_with("{{") && is_fragment_expr(rest);
        let is_block = rest.is_empty() || starts_block_scalar || is_template_fragment;
        Some(ParsedYamlKey {
            key,
            scalar_value_present: !is_block,
            starts_block_scalar,
        })
    }

    let after = after.trim_end();
    if after.starts_with("{{") {
        return None;
    }

    if let Some(quote) = after.chars().next().filter(|c| matches!(c, '"' | '\'')) {
        let quoted = &after[quote.len_utf8()..];
        let mut chars = quoted.char_indices().peekable();
        let mut key = String::new();
        while let Some((idx, ch)) = chars.next() {
            match (quote, ch) {
                ('"', '\\') => {
                    let (_, escaped) = chars.next()?;
                    key.push(escaped);
                }
                ('\'', '\'') if chars.peek().is_some_and(|(_, next)| *next == '\'') => {
                    let _ = chars.next();
                    key.push('\'');
                }
                (_, ch) if ch == quote => {
                    let rest = &quoted[(idx + ch.len_utf8())..];
                    return finalize_yaml_key(key, rest);
                }
                _ => key.push(ch),
            }
        }
        return None;
    }

    let mut chars = after.chars();
    let mut key = String::new();
    while let Some(ch) = chars.next() {
        if ch == ':' {
            return finalize_yaml_key(key.trim().to_string(), chars.as_str());
        }
        if ch.is_whitespace() {
            return None;
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/') {
            key.push(ch);
            continue;
        }
        return None;
    }
    None
}

pub(crate) fn source_line_starts_block_scalar(after: &str) -> bool {
    let after = after.trim_start();
    let after = after.strip_prefix("- ").unwrap_or(after);
    parse_yaml_key(after).is_some_and(|key| key.starts_block_scalar)
}

pub(crate) fn first_mapping_colon_offset(line: &str) -> Option<usize> {
    // Find the first YAML mapping separator on the physical line while skipping
    // quoted scalars and Helm actions. This distinguishes template output that
    // contributes to a mapping key from output that contributes to the value.
    let bytes = line.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        match bytes[idx] {
            b'{' if bytes.get(idx + 1) == Some(&b'{') => {
                idx += 2;
                while idx + 1 < bytes.len() {
                    if bytes[idx] == b'}' && bytes[idx + 1] == b'}' {
                        idx += 2;
                        break;
                    }
                    idx += 1;
                }
            }
            b'"' => {
                idx += 1;
                while idx < bytes.len() {
                    match bytes[idx] {
                        b'\\' => idx += 2,
                        b'"' => {
                            idx += 1;
                            break;
                        }
                        _ => idx += 1,
                    }
                }
            }
            b'\'' => {
                idx += 1;
                while idx < bytes.len() {
                    if bytes[idx] == b'\'' {
                        if bytes.get(idx + 1) == Some(&b'\'') {
                            idx += 2;
                            continue;
                        }
                        idx += 1;
                        break;
                    }
                    idx += 1;
                }
            }
            b':' => return Some(idx),
            _ => idx += 1,
        }
    }
    None
}

impl Shape {
    pub(crate) fn current_path(&self) -> YamlPath {
        let mut out: Vec<String> = self.prefix.clone();
        for entry in &self.stack {
            match (entry.container, entry.pending_key.as_ref()) {
                (Container::Mapping, Some(key)) => {
                    if !key.is_empty() {
                        out.push(key.clone());
                    }
                }
                (Container::Mapping, None) => {}
                (Container::Sequence, _) => {
                    if let Some(last) = out.last_mut() {
                        if !last.ends_with("[*]") {
                            last.push_str("[*]");
                        }
                    } else {
                        out.push("[*]".to_string());
                    }
                }
            }
        }
        YamlPath(out)
    }

    pub(crate) fn sync_action_position(
        &mut self,
        indent: usize,
        col: usize,
        allow_clear_pending: bool,
    ) {
        let effective = std::cmp::max(indent, col);
        if self.is_inside_block_scalar_line(indent) {
            return;
        }
        self.block_scalar_parent_indent = None;

        while let Some(entry) = self.stack.last() {
            if entry.indent > effective {
                if self.stack.len() <= self.stack_floor {
                    break;
                }
                self.stack.pop();
            } else {
                break;
            }
        }

        if allow_clear_pending && col > indent && self.clear_pending_on_newline_at_indent.is_none()
        {
            let mut candidate: Option<usize> = None;
            for entry in self.stack.iter().rev() {
                if entry.container != Container::Mapping {
                    continue;
                }
                if entry.pending_key.is_none() {
                    continue;
                }
                if entry.indent < indent {
                    break;
                }
                if entry.indent <= col {
                    candidate = Some(entry.indent);
                    break;
                }
            }
            if let Some(indent) = candidate {
                self.clear_pending_on_newline_at_indent = Some(indent);
            }
        }
    }

    pub(crate) fn is_inside_block_scalar_line(&self, indent: usize) -> bool {
        self.block_scalar_parent_indent
            .is_some_and(|parent_indent| indent > parent_indent)
    }

    pub(crate) fn trailing_pending_mapping_segments_at_or_above(
        &self,
        logical_indent: usize,
    ) -> usize {
        self.stack
            .iter()
            .rev()
            .take_while(|entry| {
                entry.indent >= logical_indent
                    && entry.container == Container::Mapping
                    && entry.pending_key.is_some()
            })
            .count()
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn ingest(&mut self, text: &str) {
        fn clear_pending_at_indent(stack: &mut [StackEntry], indent: usize) {
            for entry in stack.iter_mut().rev() {
                if entry.indent == indent {
                    if entry.container == Container::Mapping {
                        entry.pending_key = None;
                    }
                    break;
                }
            }
        }

        for raw in text.split_inclusive('\n') {
            let is_newline_terminated = raw.ends_with('\n');
            if !self.at_line_start {
                if is_newline_terminated
                    && let Some(indent) = self.clear_pending_on_newline_at_indent.take()
                {
                    clear_pending_at_indent(&mut self.stack, indent);
                }
                self.at_line_start = is_newline_terminated;
                continue;
            }

            let line = raw.trim_end_matches('\n');
            let indent = line.chars().take_while(|&ch| ch == ' ').count();
            let after = &line[indent..];
            let trimmed = after.trim_end();

            if trimmed.is_empty() {
                self.at_line_start = is_newline_terminated;
                continue;
            }

            if self.is_inside_block_scalar_line(indent) {
                self.at_line_start = is_newline_terminated;
                continue;
            }
            self.block_scalar_parent_indent = None;

            if trimmed == "---" || trimmed == "..." {
                self.stack.truncate(self.stack_floor);
                self.at_line_start = is_newline_terminated;
                continue;
            }

            if trimmed.trim_start().starts_with("{{") {
                self.at_line_start = is_newline_terminated;
                continue;
            }

            while let Some(entry) = self.stack.last() {
                if entry.indent > indent {
                    if self.stack.len() <= self.stack_floor {
                        break;
                    }
                    self.stack.pop();
                } else {
                    break;
                }
            }

            if after.starts_with("- ") {
                match self.stack.last_mut() {
                    Some(entry)
                        if entry.indent == indent && entry.container == Container::Sequence =>
                    {
                        entry.pending_key = None;
                    }
                    Some(entry)
                        if entry.indent == indent && entry.container == Container::Mapping =>
                    {
                        self.stack.push(StackEntry::sequence(indent));
                    }
                    Some(entry) if entry.indent < indent => {
                        self.stack.push(StackEntry::sequence(indent));
                    }
                    None => {
                        self.stack.push(StackEntry::sequence(indent));
                    }
                    _ => {}
                }

                if let Some(colon) = after.find(':') {
                    let key = after["- ".len()..colon].trim().to_string();
                    let child_indent = indent + 2;
                    self.stack
                        .push(StackEntry::mapping(child_indent, Some(key)));

                    let rest = after[colon + 1..].trim_start();
                    let starts_block_scalar = rest.starts_with('|') || rest.starts_with('>');
                    if starts_block_scalar {
                        self.block_scalar_parent_indent = Some(child_indent);
                    }
                    if !rest.is_empty() && !starts_block_scalar {
                        if is_newline_terminated {
                            clear_pending_at_indent(&mut self.stack, child_indent);
                        } else {
                            self.clear_pending_on_newline_at_indent = Some(child_indent);
                        }
                    }
                }

                self.at_line_start = is_newline_terminated;
                continue;
            }

            if let Some(parsed_key) = parse_yaml_key(after) {
                // If we were in a sequence at this indent and now see a mapping
                // key, treat it as ending the sequence and starting a sibling
                // mapping entry. Helm whitespace trimming can make sibling keys
                // appear at the same indentation level as list items.
                if let Some(entry) = self.stack.last()
                    && entry.indent == indent
                    && entry.container == Container::Sequence
                {
                    self.stack.pop();
                }
                match self.stack.last_mut() {
                    Some(entry)
                        if entry.indent == indent && entry.container == Container::Mapping =>
                    {
                        entry.pending_key = Some(parsed_key.key);
                    }
                    Some(entry) if entry.indent < indent => {
                        self.stack
                            .push(StackEntry::mapping(indent, Some(parsed_key.key)));
                    }
                    None => {
                        self.stack
                            .push(StackEntry::mapping(indent, Some(parsed_key.key)));
                    }
                    _ => {}
                }

                if parsed_key.starts_block_scalar {
                    self.block_scalar_parent_indent = Some(indent);
                }

                if parsed_key.scalar_value_present {
                    if is_newline_terminated {
                        clear_pending_at_indent(&mut self.stack, indent);
                    } else {
                        self.clear_pending_on_newline_at_indent = Some(indent);
                    }
                }

                self.at_line_start = is_newline_terminated;
                continue;
            }

            if self.stack.is_empty() {
                self.stack.push(StackEntry::mapping(indent, None));
            }

            self.at_line_start = is_newline_terminated;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Shape, first_mapping_colon_offset, parse_yaml_key};

    #[test]
    fn parse_yaml_key_handles_plain_and_quoted_keys() {
        assert_eq!(
            parse_yaml_key("metadata.name: value").map(super::ParsedYamlKey::into_key),
            Some("metadata.name".to_string())
        );
        assert_eq!(
            parse_yaml_key(r#""app.kubernetes.io/name": value"#)
                .map(super::ParsedYamlKey::into_key),
            Some("app.kubernetes.io/name".to_string())
        );
        assert_eq!(
            parse_yaml_key("'it''s': value").map(super::ParsedYamlKey::into_key),
            Some("it's".to_string())
        );
    }

    #[test]
    fn first_mapping_colon_skips_templates_and_quoted_scalars() {
        let line = r#"{{ printf "not:a:key" }}: value"#;
        assert_eq!(first_mapping_colon_offset(line), Some(24));

        let line = r#""not:a:key": value"#;
        assert_eq!(first_mapping_colon_offset(line), Some(11));
    }

    #[test]
    fn shape_tracks_mapping_and_sequence_paths() {
        let mut shape = Shape::default();
        shape.ingest(
            r#"
metadata:
  labels:
    app: demo
spec:
  containers:
    - name: app
      image: example
"#,
        );

        assert_eq!(shape.current_path().0, vec!["spec", "containers[*]"]);
    }
}
