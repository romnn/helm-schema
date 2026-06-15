use super::shape::Shape;

#[derive(Default)]
pub(super) struct TextIngestState {
    text_spans: Vec<(usize, usize)>,
    text_span_idx: usize,
    text_pos: usize,
}

impl TextIngestState {
    pub(super) fn reset_for_tree(&mut self, tree: &tree_sitter::Tree) {
        self.text_spans = merged_text_spans(tree);
        self.text_span_idx = 0;
        self.text_pos = 0;
    }

    pub(super) fn set_position(&mut self, position: usize) {
        self.text_pos = position;
    }

    pub(super) fn ingest_text_up_to(&mut self, source: &str, shape: &mut Shape, target: usize) {
        let target = target.min(source.len());
        if target <= self.text_pos {
            return;
        }

        // Keep non-YAML gaps sanitized to whitespace so template actions do
        // not look like YAML keys to the shape tracker.
        let mut shape_buffer = String::new();

        while self.text_span_idx < self.text_spans.len() {
            let (start, end) = self.text_spans[self.text_span_idx];

            if end <= self.text_pos {
                self.text_span_idx += 1;
                continue;
            }
            if start >= target {
                if self.text_pos < target {
                    let gap = &source[self.text_pos..target];
                    let shape_gap = shape_text_for_gap(gap);
                    if !shape_gap.is_empty() {
                        shape_buffer.push_str(&shape_gap);
                    }
                    self.text_pos = target;
                }
                break;
            }

            if self.text_pos < start {
                let gap = &source[self.text_pos..start];
                let shape_gap = shape_text_for_gap(gap);
                if !shape_gap.is_empty() {
                    shape_buffer.push_str(&shape_gap);
                }
                self.text_pos = start;
            }

            let span_start = start.max(self.text_pos);
            let span_end = end.min(target);
            if span_start < span_end {
                let text = &source[span_start..span_end];
                shape_buffer.push_str(text);
                self.text_pos = span_end;
            }

            if self.text_pos >= end {
                self.text_span_idx += 1;
            }

            if self.text_pos >= target {
                break;
            }

            if self.text_span_idx < self.text_spans.len() {
                let (next_start, _) = self.text_spans[self.text_span_idx];
                if self.text_pos < next_start {
                    let gap_end = next_start.min(target);
                    if self.text_pos < gap_end {
                        let gap = &source[self.text_pos..gap_end];
                        let shape_gap = shape_text_for_gap(gap);
                        if !shape_gap.is_empty() {
                            shape_buffer.push_str(&shape_gap);
                        }
                        self.text_pos = gap_end;
                    }
                }
            } else {
                self.text_pos = target;
            }

            if shape_buffer.len() > 4096 {
                shape.ingest(&shape_buffer);
                shape_buffer.clear();
            }
        }

        if self.text_pos < target {
            self.text_pos = target;
        }

        if !shape_buffer.is_empty() {
            shape.ingest(&shape_buffer);
        }
    }
}

fn merged_text_spans(tree: &tree_sitter::Tree) -> Vec<(usize, usize)> {
    let mut spans = Vec::<(usize, usize)>::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.is_named() {
            if matches!(node.kind(), "text" | "yaml_no_injection_text") {
                let range = node.byte_range();
                spans.push((range.start, range.end));
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    stack.push(child);
                }
            }
        }
    }
    spans.sort_by_key(|(start, _)| *start);

    // Merge adjacent spans so YAML line prefixes such as `- ` stay in one
    // shape-tracker ingest call.
    let mut merged = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some((_, merged_end)) if start <= *merged_end => {
                *merged_end = (*merged_end).max(end);
            }
            _ => merged.push((start, end)),
        }
    }
    merged
}

fn shape_text_for_gap(gap: &str) -> String {
    if !(gap.contains("{{") || gap.contains("}}")) {
        gap.to_string()
    } else {
        // Preserve literal YAML bytes around inline template actions so shape
        // tracking still sees sequence markers, keys, and scalar prefixes on
        // mixed text/template lines.
        let mut sanitized = String::with_capacity(gap.len());
        let bytes = gap.as_bytes();
        let mut index = 0usize;
        let mut in_action = false;
        while index < bytes.len() {
            if !in_action && bytes.get(index..index + 2) == Some(b"{{") {
                in_action = true;
                index += 2;
                continue;
            }
            if in_action && bytes.get(index..index + 2) == Some(b"}}") {
                in_action = false;
                index += 2;
                continue;
            }

            let Some(ch) = gap[index..].chars().next() else {
                break;
            };
            if !in_action || matches!(ch, '\n' | ' ' | '\t') {
                sanitized.push(ch);
            }
            index += ch.len_utf8();
        }
        sanitized
    }
}
