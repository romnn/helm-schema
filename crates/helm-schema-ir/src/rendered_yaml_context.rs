use helm_schema_ast::{DefineIndex, Literal, TemplateExpr};

use crate::resource_locator::{AstResourceLocator, ResourceLocator};
use crate::template_expr_cache::parse_expr_text;
use crate::walker::is_fragment_expr;
use crate::yaml_shape::{
    Shape, first_mapping_colon_offset, parse_yaml_key, source_line_starts_block_scalar,
};
use crate::{ResourceRef, YamlPath};

/// Tracks source-position-dependent rendered YAML state while a template AST is
/// walked.
///
/// The symbolic walker decides what a Helm expression means. This context owns
/// the independent question of where that expression lands in rendered YAML and
/// which Kubernetes resource span contains it.
pub(crate) struct RenderedYamlContext<'a> {
    source: &'a str,
    defines: &'a DefineIndex,
    shape: Shape,
    output_inside_block_scalar: bool,
    resource_locator: Box<dyn ResourceLocator>,
    text_spans: Vec<(usize, usize)>,
    text_span_idx: usize,
    text_pos: usize,
}

impl<'a> RenderedYamlContext<'a> {
    pub(crate) fn new(source: &'a str, defines: &'a DefineIndex) -> Self {
        Self {
            source,
            defines,
            shape: Shape::default(),
            output_inside_block_scalar: false,
            resource_locator: Box::new(AstResourceLocator::default()),
            text_spans: Vec::new(),
            text_span_idx: 0,
            text_pos: 0,
        }
    }

    pub(crate) fn reset_for_tree(&mut self, tree: &tree_sitter::Tree) {
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
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in spans {
            match merged.last_mut() {
                Some((_, merged_end)) if start <= *merged_end => {
                    *merged_end = (*merged_end).max(end);
                }
                _ => merged.push((start, end)),
            }
        }

        self.text_spans = merged;
        self.text_span_idx = 0;
        self.text_pos = 0;
        self.resource_locator =
            Box::new(AstResourceLocator::from_source(self.source, self.defines));
        self.shape = Shape::default();
        self.output_inside_block_scalar = false;
    }

    pub(crate) fn enter_node(&mut self, node: tree_sitter::Node<'_>) {
        self.ingest_text_up_to(node.start_byte());
        self.resource_locator.advance_to(node.start_byte());
        self.sync_action_for_node(node);
    }

    pub(crate) fn current_path(&self) -> YamlPath {
        self.shape.current_path()
    }

    pub(crate) fn current_resource(&self) -> Option<&ResourceRef> {
        self.resource_locator.current_resource()
    }

    pub(crate) fn rebase_path(&self, path: YamlPath) -> YamlPath {
        self.resource_locator.rebase_path(path)
    }

    pub(crate) fn trailing_pending_mapping_segments_at_or_above(&self, indent: usize) -> usize {
        self.shape
            .trailing_pending_mapping_segments_at_or_above(indent)
    }

    pub(crate) fn output_inside_block_scalar_at(&self, byte_pos: usize) -> bool {
        let (indent, _) = self.line_indent_and_col(byte_pos);
        self.output_inside_block_scalar
            || self.source_position_is_inside_block_scalar(byte_pos, indent)
    }

    pub(crate) fn inline_mapping_value_path(
        &self,
        node: tree_sitter::Node<'_>,
    ) -> Option<YamlPath> {
        let start = node.start_byte();
        let end = node.end_byte();
        let line_start = self.source[..start].rfind('\n').map_or(0, |idx| idx + 1);
        let line_end = self.source[end..]
            .find('\n')
            .map_or(self.source.len(), |idx| end + idx);
        let line = &self.source[line_start..line_end];
        let rel_start = start - line_start;
        let colon_offset = first_mapping_colon_offset(line)?;
        if rel_start <= colon_offset {
            return None;
        }
        let trimmed = line.trim_start();
        let key = parse_yaml_key(trimmed)?.into_key();
        let mut path = self.shape.current_path();
        if path.0.last().is_none_or(|segment| segment != &key) {
            path.0.push(key);
        }
        Some(path)
    }

    pub(crate) fn ingest_text_up_to(&mut self, target: usize) {
        let target = target.min(self.source.len());
        if target <= self.text_pos {
            return;
        }

        // Keep non-YAML gaps sanitized to whitespace so template actions do
        // not look like YAML keys to the shape tracker.
        let mut shape_buf = String::new();

        while self.text_span_idx < self.text_spans.len() {
            let (start, end) = self.text_spans[self.text_span_idx];

            if end <= self.text_pos {
                self.text_span_idx += 1;
                continue;
            }
            if start >= target {
                if self.text_pos < target {
                    let gap = &self.source[self.text_pos..target];
                    let shape_gap = shape_text_for_gap(gap);
                    if !shape_gap.is_empty() {
                        shape_buf.push_str(&shape_gap);
                    }
                    self.text_pos = target;
                }
                break;
            }

            if self.text_pos < start {
                let gap = &self.source[self.text_pos..start];
                let shape_gap = shape_text_for_gap(gap);
                if !shape_gap.is_empty() {
                    shape_buf.push_str(&shape_gap);
                }
                self.text_pos = start;
            }

            let span_start = start.max(self.text_pos);
            let span_end = end.min(target);
            if span_start < span_end {
                let text = &self.source[span_start..span_end];
                shape_buf.push_str(text);
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
                        let gap = &self.source[self.text_pos..gap_end];
                        let shape_gap = shape_text_for_gap(gap);
                        if !shape_gap.is_empty() {
                            shape_buf.push_str(&shape_gap);
                        }
                        self.text_pos = gap_end;
                    }
                }
            } else {
                self.text_pos = target;
            }

            if shape_buf.len() > 4096 {
                self.shape.ingest(&shape_buf);
                shape_buf.clear();
            }
        }

        if self.text_pos < target {
            self.text_pos = target;
        }

        if !shape_buf.is_empty() {
            self.shape.ingest(&shape_buf);
        }
    }

    pub(crate) fn line_indent_and_col(&self, byte_pos: usize) -> (usize, usize) {
        let bytes = self.source.as_bytes();
        let mut line_start = byte_pos.min(bytes.len());
        while line_start > 0 {
            if bytes[line_start - 1] == b'\n' {
                break;
            }
            line_start -= 1;
        }

        let col = byte_pos.saturating_sub(line_start);
        let mut indent = 0usize;
        while line_start + indent < bytes.len() {
            if bytes[line_start + indent] == b' ' {
                indent += 1;
            } else {
                break;
            }
        }
        (indent, col)
    }

    pub(crate) fn starts_template_action_line(&self, byte_pos: usize) -> bool {
        let bytes = self.source.as_bytes();
        let mut line_start = byte_pos.min(bytes.len());
        while line_start > 0 {
            if bytes[line_start - 1] == b'\n' {
                break;
            }
            line_start -= 1;
        }

        let prefix = &self.source[line_start..byte_pos.min(self.source.len())];
        let trimmed = prefix.trim_start();
        trimmed.starts_with("{{")
    }

    pub(crate) fn fragment_indent_width(text: &str) -> Option<usize> {
        fn call_indent_width(expr: &TemplateExpr) -> Option<usize> {
            match expr {
                TemplateExpr::Call { function, args }
                    if matches!(function.as_str(), "indent" | "nindent") =>
                {
                    match args.first() {
                        Some(TemplateExpr::Literal(Literal::Int(n))) => usize::try_from(*n).ok(),
                        Some(TemplateExpr::Parenthesized(inner)) => call_indent_width(inner),
                        _ => None,
                    }
                }
                TemplateExpr::Parenthesized(inner) => call_indent_width(inner),
                TemplateExpr::Pipeline(stages) => stages.iter().rev().find_map(call_indent_width),
                _ => None,
            }
        }

        parse_expr_text(text)
            .iter()
            .rev()
            .find_map(call_indent_width)
    }

    fn source_position_is_inside_block_scalar(&self, byte_pos: usize, indent: usize) -> bool {
        let bytes = self.source.as_bytes();
        let mut line_start = byte_pos.min(bytes.len());
        while line_start > 0 {
            if bytes[line_start - 1] == b'\n' {
                break;
            }
            line_start -= 1;
        }

        while line_start > 0 {
            let previous_line_end = line_start.saturating_sub(1);
            let mut previous_line_start = previous_line_end;
            while previous_line_start > 0 {
                if bytes[previous_line_start - 1] == b'\n' {
                    break;
                }
                previous_line_start -= 1;
            }

            let line = &self.source[previous_line_start..previous_line_end];
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

    fn sync_action_for_node(&mut self, node: tree_sitter::Node<'_>) {
        if matches!(node.kind(), "text" | "yaml_no_injection_text") {
            return;
        }

        // Control actions do not emit YAML structure, so they must not mutate
        // the rendered shape stack.
        if !matches!(node.kind(), "template_action" | "variable") {
            return;
        }

        let mut pos = node.start_byte().min(self.source.len());
        let end = node.end_byte().min(self.source.len());
        while pos < end {
            match self.source.as_bytes()[pos] {
                b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
                _ => break,
            }
        }

        if pos > node.start_byte() {
            let leading = &self.source[node.start_byte()..pos];
            let mut sanitized = String::with_capacity(leading.len());
            for ch in leading.chars() {
                if ch == '\n' || ch == ' ' || ch == '\t' {
                    sanitized.push(ch);
                }
            }
            if !sanitized.is_empty() {
                self.shape.ingest(&sanitized);
                self.text_pos = pos;
            }
        }

        let (physical_indent, physical_col) = self.line_indent_and_col(pos);
        let shape_inside_block_scalar = self.shape.is_inside_block_scalar_line(physical_indent);
        let source_inside_block_scalar =
            self.source_position_is_inside_block_scalar(pos, physical_indent);
        self.output_inside_block_scalar = shape_inside_block_scalar || source_inside_block_scalar;

        let allow_clear_pending = if node.kind() == "template_action" {
            if let Ok(text) = node.utf8_text(self.source.as_bytes()) {
                !is_fragment_expr(text)
            } else {
                true
            }
        } else {
            false
        };

        let (indent, col) = if node.kind() == "template_action" && !allow_clear_pending {
            if let Ok(text) = node.utf8_text(self.source.as_bytes())
                && let Some(virtual_indent) = Self::fragment_indent_width(text)
                && virtual_indent > physical_indent
            {
                (virtual_indent, virtual_indent)
            } else {
                (physical_indent, physical_col)
            }
        } else {
            (physical_indent, physical_col)
        };

        self.shape
            .sync_action_position(indent, col, allow_clear_pending);
    }
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
        let mut idx = 0usize;
        let mut in_action = false;
        while idx < bytes.len() {
            if !in_action && bytes.get(idx..idx + 2) == Some(b"{{") {
                in_action = true;
                idx += 2;
                continue;
            }
            if in_action && bytes.get(idx..idx + 2) == Some(b"}}") {
                in_action = false;
                idx += 2;
                continue;
            }

            let Some(ch) = gap[idx..].chars().next() else {
                break;
            };
            if !in_action || matches!(ch, '\n' | ' ' | '\t') {
                sanitized.push(ch);
            }
            idx += ch.len_utf8();
        }
        sanitized
    }
}
