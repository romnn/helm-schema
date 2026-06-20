use super::state::DocumentState;

#[derive(Default)]
pub(super) struct TemplateTextCursor {
    text_spans: Vec<(usize, usize)>,
    text_span_idx: usize,
    text_pos: usize,
}

impl TemplateTextCursor {
    pub(super) fn reset_for_tree(&mut self, tree: &tree_sitter::Tree) {
        self.text_spans = merged_text_spans(tree);
        self.text_span_idx = 0;
        self.text_pos = 0;
    }

    pub(super) fn set_position(&mut self, position: usize) {
        self.text_pos = position;
    }

    pub(super) fn ingest_text_up_to(
        &mut self,
        source: &str,
        state: &mut DocumentState,
        target: usize,
    ) {
        let target = target.min(source.len());
        if target <= self.text_pos {
            return;
        }

        let mut state_buffer = String::new();

        while self.text_span_idx < self.text_spans.len() {
            let (start, end) = self.text_spans[self.text_span_idx];

            if end <= self.text_pos {
                self.text_span_idx += 1;
                continue;
            }
            if start >= target {
                if self.text_pos < target {
                    let gap = &source[self.text_pos..target];
                    let state_gap = text_for_template_gap(gap);
                    if !state_gap.is_empty() {
                        state_buffer.push_str(&state_gap);
                    }
                    self.text_pos = target;
                }
                break;
            }

            if self.text_pos < start {
                let gap = &source[self.text_pos..start];
                let state_gap = text_for_template_gap(gap);
                if !state_gap.is_empty() {
                    state_buffer.push_str(&state_gap);
                }
                self.text_pos = start;
            }

            let span_start = start.max(self.text_pos);
            let span_end = end.min(target);
            if span_start < span_end {
                state_buffer.push_str(&source[span_start..span_end]);
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
                        let state_gap = text_for_template_gap(gap);
                        if !state_gap.is_empty() {
                            state_buffer.push_str(&state_gap);
                        }
                        self.text_pos = gap_end;
                    }
                }
            } else {
                self.text_pos = target;
            }

            if state_buffer.len() > 4096 {
                state.ingest(&state_buffer);
                state_buffer.clear();
            }
        }

        if self.text_pos < target {
            self.text_pos = target;
        }

        if !state_buffer.is_empty() {
            state.ingest(&state_buffer);
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

fn text_for_template_gap(gap: &str) -> String {
    if !(gap.contains("{{") || gap.contains("}}")) {
        return gap.to_string();
    }

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

#[cfg(test)]
mod tests {
    use super::{DocumentState, TemplateTextCursor};
    use crate::document_projection::tracker::source_position::line_indent_and_col;

    fn parse_template(source: &str) -> tree_sitter::Tree {
        let language =
            tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language)
            .expect("go-template grammar should load");
        parser.parse(source, None).expect("template should parse")
    }

    #[test]
    fn cursor_preserves_sequence_block_scalar_state_before_embedded_actions() {
        let source = r#"args:
  - -ec
  - |
    chown -R {{ .Values.user }}:{{ .Values.group }}
    {{- if .Values.dir }}
    mkdir -p {{ .Values.dir }}
    {{- end }}
next: {{ .Values.next }}
"#;
        let tree = parse_template(source);
        let mut cursor = TemplateTextCursor::default();
        let mut state = DocumentState::default();
        cursor.reset_for_tree(&tree);

        let first_script_expr = source.find(".Values.user").expect("script expression");
        let first_action = source[..first_script_expr]
            .rfind("{{")
            .expect("script action start");
        cursor.ingest_text_up_to(source, &mut state, first_action);
        let (action_indent, action_col) = line_indent_and_col(source, first_action);
        state.sync_action_position(action_indent, action_col, true);
        cursor.ingest_text_up_to(source, &mut state, first_script_expr);
        let (script_indent, _) = line_indent_and_col(source, first_script_expr);
        assert!(state.is_inside_block_scalar_line(script_indent));

        let nested_control_expr = source
            .find(".Values.dir")
            .expect("nested control expression");
        let nested_control_action = source[..nested_control_expr]
            .rfind("{{")
            .expect("nested control action start");
        cursor.ingest_text_up_to(source, &mut state, nested_control_action);
        let (control_action_indent, control_action_col) =
            line_indent_and_col(source, nested_control_action);
        state.sync_action_position(control_action_indent, control_action_col, true);
        cursor.ingest_text_up_to(source, &mut state, nested_control_expr);
        let (control_indent, _) = line_indent_and_col(source, nested_control_expr);
        assert!(state.is_inside_block_scalar_line(control_indent));

        let next_expr = source.find(".Values.next").expect("next expression");
        cursor.ingest_text_up_to(source, &mut state, next_expr);
        let (next_indent, _) = line_indent_and_col(source, next_expr);
        assert!(!state.is_inside_block_scalar_line(next_indent));
        assert_eq!(state.current_path().0, vec!["next"]);
    }
}
