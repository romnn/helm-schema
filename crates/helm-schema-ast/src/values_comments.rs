use std::collections::BTreeMap;

use crate::{ParseError, first_mapping_colon_offset, parse_yaml_key};

/// Extract chart-authored descriptions from comments in a values YAML document.
///
/// The returned map is keyed by dotted `.Values` path without the leading
/// `.Values`. This is documentation metadata only: commented-out examples stay
/// comments and never become paths in this map.
pub fn extract_values_yaml_descriptions(
    src: &str,
) -> std::result::Result<BTreeMap<String, String>, ParseError> {
    let mut scanner = CommentScanner::default();
    for line in src.lines() {
        scanner.visit_line(line);
    }
    scanner.finish();
    Ok(scanner.descriptions)
}

#[derive(Default)]
struct CommentScanner {
    descriptions: BTreeMap<String, String>,
    path_stack: Vec<PathFrame>,
    pending: Vec<String>,
    pending_can_describe_previous: bool,
    previous_path: Option<Vec<String>>,
}

#[derive(Clone)]
struct PathFrame {
    indent: usize,
    path: Vec<String>,
}

impl CommentScanner {
    fn visit_line(&mut self, line: &str) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            self.flush_trailing_comments();
            self.previous_path = None;
            return;
        }

        let indent = line.len() - trimmed.len();
        if trimmed.starts_with('#') {
            self.visit_comment(trimmed);
            return;
        }

        self.visit_value_line(indent, trimmed);
    }

    fn finish(&mut self) {
        self.flush_trailing_comments();
    }

    fn visit_comment(&mut self, trimmed: &str) {
        let Some(body) = comment_body(trimmed) else {
            return;
        };
        if let Some((path, description)) = explicit_comment_description(body) {
            self.flush_trailing_comments();
            insert_description(&mut self.descriptions, &[path], description);
            return;
        }

        let starts_description = is_helm_docs_description_marker(body);
        if starts_description {
            self.flush_trailing_comments();
        }

        let Some(line) = normalize_comment_body(body) else {
            return;
        };
        if self.pending.is_empty() {
            self.pending_can_describe_previous =
                self.previous_path.is_some() && !starts_description;
        }
        self.pending.push(line);
    }

    fn visit_value_line(&mut self, indent: usize, trimmed: &str) {
        self.pop_closed_paths(indent);
        let Some(mapping) = mapping_entry(trimmed) else {
            self.pending.clear();
            self.pending_can_describe_previous = false;
            self.previous_path = None;
            return;
        };

        let mut path = self
            .path_stack
            .last()
            .map(|frame| frame.path.clone())
            .unwrap_or_default();
        if mapping.sequence_item {
            push_sequence_segment(&mut path);
        }
        path.push(mapping.key);

        self.attach_pending_to(&path);
        if let Some(comment) = mapping.inline_comment {
            insert_description(&mut self.descriptions, &path, comment);
        }

        if mapping.value_is_nested {
            self.path_stack.push(PathFrame {
                indent,
                path: path.clone(),
            });
        }
        self.previous_path = Some(path);
    }

    fn pop_closed_paths(&mut self, indent: usize) {
        while self
            .path_stack
            .last()
            .is_some_and(|frame| frame.indent >= indent)
        {
            self.path_stack.pop();
        }
    }

    fn attach_pending_to(&mut self, path: &[String]) {
        if self.pending.is_empty() {
            return;
        }
        insert_description(&mut self.descriptions, path, self.pending.join("\n"));
        self.pending.clear();
        self.pending_can_describe_previous = false;
    }

    fn flush_trailing_comments(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        if self.pending_can_describe_previous
            && let Some(path) = self.previous_path.as_ref()
        {
            insert_description(&mut self.descriptions, path, self.pending.join("\n"));
        }
        self.pending.clear();
        self.pending_can_describe_previous = false;
    }
}

struct MappingEntry {
    key: String,
    inline_comment: Option<String>,
    sequence_item: bool,
    value_is_nested: bool,
}

fn mapping_entry(trimmed: &str) -> Option<MappingEntry> {
    let (sequence_item, text) = sequence_item_text(trimmed);
    let colon = first_mapping_colon_offset(text)?;
    if !mapping_colon_is_structural(text, colon) {
        return None;
    }

    let key = parse_yaml_key(text)?.into_key();
    if key.contains("{{") || key.contains("}}") {
        return None;
    }
    let value = text[colon + 1..].trim_start();
    Some(MappingEntry {
        key,
        inline_comment: inline_comment(value),
        sequence_item,
        value_is_nested: value.is_empty(),
    })
}

fn sequence_item_text(trimmed: &str) -> (bool, &str) {
    let Some(after_dash) = trimmed.strip_prefix('-') else {
        return (false, trimmed);
    };
    if !after_dash.is_empty() && !after_dash.starts_with(char::is_whitespace) {
        return (false, trimmed);
    }
    (true, after_dash.trim_start())
}

fn mapping_colon_is_structural(text: &str, colon: usize) -> bool {
    text[colon + 1..]
        .chars()
        .next()
        .is_none_or(char::is_whitespace)
}

fn inline_comment(value: &str) -> Option<String> {
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut previous = '\0';
    for (index, ch) in value.char_indices() {
        match ch {
            '\'' if !double_quoted => single_quoted = !single_quoted,
            '"' if !single_quoted && previous != '\\' => double_quoted = !double_quoted,
            '#' if !single_quoted && !double_quoted => {
                let before = value[..index].chars().next_back();
                if before.is_none_or(char::is_whitespace) {
                    return normalize_comment_body(value[index + 1..].trim_start());
                }
            }
            _ => {}
        }
        previous = ch;
    }
    None
}

fn comment_body(text: &str) -> Option<&str> {
    let mut line = text.trim_start();
    while let Some(rest) = line.strip_prefix('#') {
        line = rest.trim_start();
    }

    let line = line.trim_end();
    if line.len() == text.trim_start().len() {
        None
    } else {
        Some(line)
    }
}

fn normalize_comment_body(line: &str) -> Option<String> {
    let line = strip_helm_docs_description_marker(line)
        .unwrap_or(line)
        .trim_end();

    if line.is_empty() || line.trim_start().starts_with('@') || is_decorative_heading(line) {
        None
    } else {
        Some(line.to_string())
    }
}

fn explicit_comment_description(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("@param ")?.trim_start();
    let split_at = rest.find(char::is_whitespace)?;
    let path = rest.get(..split_at)?.trim();
    let description = rest.get(split_at..)?.trim();

    if path.is_empty() || description.is_empty() {
        None
    } else {
        Some((path.to_string(), description.to_string()))
    }
}

fn is_helm_docs_description_marker(line: &str) -> bool {
    line == "--" || line.starts_with("-- ") || line.starts_with("--\t")
}

fn strip_helm_docs_description_marker(line: &str) -> Option<&str> {
    if line == "--" {
        Some("")
    } else if let Some(rest) = line.strip_prefix("-- ") {
        Some(rest.trim_start())
    } else {
        line.strip_prefix("--\t").map(str::trim_start)
    }
}

fn is_decorative_heading(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("----") && trimmed.ends_with("----")
}

fn insert_description(
    descriptions: &mut BTreeMap<String, String>,
    path: &[String],
    description: String,
) {
    if path.is_empty() || description.trim().is_empty() {
        return;
    }

    descriptions
        .entry(path.join("."))
        .and_modify(|existing| {
            if !existing.is_empty() {
                existing.push('\n');
            }
            existing.push_str(&description);
        })
        .or_insert(description);
}

fn push_sequence_segment(path: &mut Vec<String>) {
    if let Some(last) = path.last_mut() {
        if !last.ends_with("[*]") {
            last.push_str("[*]");
        }
    } else {
        path.push("*".to_string());
    }
}

#[cfg(test)]
#[path = "tests/values_comments.rs"]
mod tests;
