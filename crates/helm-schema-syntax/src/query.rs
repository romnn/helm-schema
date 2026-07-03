//! Open-slot queries over a parsed [`TemplatedDocument`].
//!
//! These reproduce, exactly, the line-model query semantics that
//! helm-schema's document attribution has always used — including its
//! deliberate quirks (a `-foo` line pops with sequence rules during replay
//! but reads with mapping rules when queried; a partially-scanned line
//! participates in [`TemplatedDocument::open_slot_path_before`] with the
//! open-slot flags of its *prefix*). Byte-identical attribution across the
//! corpus is the compatibility contract; richer, non-compat queries can be
//! added beside these as later stages need them.

use crate::cst::{PathSegment, TemplatedDocument};
use crate::parse::Frame;
use crate::yaml_scan::{structural_mapping_colon, unquote_yaml_scalar};

/// The resolved output-slot context at a byte position: the open container
/// path and the shape of the slot the position sits in.
#[derive(Clone, Debug, Default)]
pub struct SlotContext {
    /// Open container path (never contains empty keys).
    pub path: Vec<PathSegment>,
    /// The action starts inside a mapping key (before the structural colon).
    pub in_mapping_key: bool,
    /// The action span covers the entire scalar value (or the entire
    /// interior of a quoted scalar value).
    pub entire_scalar_value: bool,
    /// The position lies inside a block-scalar body; output here is
    /// suppressed into the block text.
    pub inside_block_scalar: bool,
    /// The action sits behind a `#` on a comment line.
    pub on_comment_line: bool,
}

impl TemplatedDocument<'_> {
    /// The slot context at `byte`, optionally relative to the surrounding
    /// action span (needed for mapping-key and whole-scalar detection).
    #[must_use]
    pub fn slot_context_at(&self, byte: usize, action_span: Option<(usize, usize)>) -> SlotContext {
        let byte = byte.min(self.source.len());
        let line = self.lines.line_of(byte);
        let (ls, le) = self.lines.span(line);
        let text = &self.source[ls..le];
        let trimmed = text.trim_start();
        let indent = text.len() - trimmed.len();
        let on_comment_line = action_span.is_some_and(|(start, _)| {
            let start = start.clamp(ls, self.source.len());
            self.source[ls..start].trim_start().starts_with('#')
        });

        let head = self.chains[line];
        if let Some(id) = head
            && self.frames[id].block
            && indent > self.frames[id].indent
        {
            return SlotContext {
                path: self.chain_segments(Some(id)),
                inside_block_scalar: true,
                on_comment_line,
                ..SlotContext::default()
            };
        }
        // A `#` line never carries an output slot; a blank line resolves to
        // the empty default context.
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return SlotContext {
                on_comment_line,
                ..SlotContext::default()
            };
        }

        let is_sequence_item = valid_sequence_item(trimmed);
        let parent = self.parent_after_pops(head, indent, is_sequence_item, ls);
        let parent_path = self.chain_segments(parent);

        if is_sequence_item {
            let after_dash = &trimmed[1..];
            let nested = after_dash.trim_start();
            if action_span.is_none() && structural_mapping_colon(nested).is_none() {
                return SlotContext {
                    path: parent_path,
                    on_comment_line,
                    ..SlotContext::default()
                };
            }
            let mut item_path = parent_path;
            item_path.push(PathSegment::Item);
            let nested_start = ls + indent + 1 + (after_dash.len() - nested.len());
            return context_from_line_text(
                nested,
                item_path,
                action_span,
                nested_start,
                on_comment_line,
            );
        }

        context_from_line_text(
            trimmed,
            parent_path,
            action_span,
            ls + indent,
            on_comment_line,
        )
    }

    /// The path of the innermost slot open before `byte` that can accept
    /// structured output at `output_indent` (the `nindent`/`indent` width of
    /// a fragment-rendering action). `None` when nothing is open.
    #[must_use]
    pub fn open_slot_path_before(
        &self,
        byte: usize,
        output_indent: usize,
    ) -> Option<Vec<PathSegment>> {
        if output_indent == 0 {
            return Some(Vec::new());
        }
        let byte = byte.min(self.source.len());
        let line = self.lines.line_of(byte);
        let (ls, _) = self.lines.span(line);
        let mut view = ChainView {
            document: self,
            query_line_start: ls,
            cursor: self.chains[line],
            extra: Vec::new(),
            view_marked: None,
        };
        view.apply_partial_line(&self.source[ls..byte]);
        view.find_open_slot(output_indent)
    }

    /// Segments of the container chain ending at `frame`, root-first.
    fn chain_segments(&self, frame: Option<usize>) -> Vec<PathSegment> {
        let mut segments = Vec::new();
        let mut current = frame;
        while let Some(id) = current {
            segments.push(self.frames[id].seg.clone());
            current = self.frames[id].parent;
        }
        segments.reverse();
        segments
    }

    /// Walk the chain applying this line's pops; the surviving head is the
    /// line's parent container.
    fn parent_after_pops(
        &self,
        head: Option<usize>,
        indent: usize,
        is_sequence_item: bool,
        line_start: usize,
    ) -> Option<usize> {
        let mut current = head;
        while let Some(id) = current {
            let frame = &self.frames[id];
            let closed = if is_sequence_item {
                frame.indent > indent
                    || (frame.indent == indent && !frame_allows_item(frame, line_start))
            } else {
                frame.indent >= indent
            };
            if !closed {
                break;
            }
            current = frame.parent;
        }
        current
    }
}

/// Whether the container still accepts same-indent sequence items as seen
/// from `at` (opened with an empty value and not yet marked by a deeper
/// visible child line before `at`).
fn frame_allows_item(frame: &Frame, at: usize) -> bool {
    frame.opened_empty && frame.marked_at.is_none_or(|marked| marked >= at)
}

fn context_from_line_text(
    text: &str,
    parent_path: Vec<PathSegment>,
    action_span: Option<(usize, usize)>,
    text_start: usize,
    on_comment_line: bool,
) -> SlotContext {
    let Some(colon) = structural_mapping_colon(text) else {
        return scalar_line_context(text, parent_path, action_span, text_start, on_comment_line);
    };

    if action_span.is_some_and(|(start, _)| start >= text_start && start <= text_start + colon) {
        return SlotContext {
            path: parent_path,
            in_mapping_key: true,
            on_comment_line,
            ..SlotContext::default()
        };
    }

    let key_text = unquote_yaml_scalar(text[..colon].trim());
    let mut key_path = parent_path;
    if !key_text.contains("{{") && !key_text.contains("}}") && !key_text.is_empty() {
        key_path.push(PathSegment::Key(key_text.to_string()));
    }
    scalar_line_context(
        &text[colon + 1..],
        key_path,
        action_span,
        text_start + colon + 1,
        on_comment_line,
    )
}

fn scalar_line_context(
    text: &str,
    path: Vec<PathSegment>,
    action_span: Option<(usize, usize)>,
    text_start: usize,
    on_comment_line: bool,
) -> SlotContext {
    let value = text.trim();
    let value_start = text_start + text.len().saturating_sub(text.trim_start().len());
    SlotContext {
        path,
        entire_scalar_value: action_span
            .is_some_and(|span| span_is_entire_scalar(value, value_start, span)),
        on_comment_line,
        ..SlotContext::default()
    }
}

fn span_is_entire_scalar(text: &str, text_start: usize, (start, end): (usize, usize)) -> bool {
    let trimmed_end = text_start + text.len();
    if start == text_start && end == trimmed_end {
        return true;
    }
    if unquote_yaml_scalar(text).len() != text.len() {
        return start == text_start + 1 && end == trimmed_end - 1;
    }
    false
}

fn valid_sequence_item(trimmed: &str) -> bool {
    let Some(after_dash) = trimmed.strip_prefix('-') else {
        return false;
    };
    after_dash.is_empty() || after_dash.starts_with(char::is_whitespace)
}

/// A read-only stack view for [`TemplatedDocument::open_slot_path_before`]:
/// the frozen chain before the query line, plus the effect of the query
/// line's prefix (pops walk the cursor down; pushes go to `extra`; a mark
/// from the prefix is recorded as a view-local override).
struct ChainView<'doc, 'src> {
    document: &'doc TemplatedDocument<'src>,
    query_line_start: usize,
    cursor: Option<usize>,
    extra: Vec<ViewFrame>,
    view_marked: Option<usize>,
}

struct ViewFrame {
    indent: usize,
    seg: PathSegment,
    allow_same_indent_output: bool,
}

impl ChainView<'_, '_> {
    /// Replay the query line's prefix with the layout push rules (the
    /// partial text is what `source[..byte]` ends with).
    fn apply_partial_line(&mut self, partial: &str) {
        let trimmed = partial.trim_start();
        let indent = partial.len() - trimmed.len();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("{{") {
            return;
        }
        if let Some(id) = self.cursor {
            let frame = &self.document.frames[id];
            if frame.block && indent > frame.indent {
                return;
            }
        }
        if let Some(after_dash) = trimmed.strip_prefix('-') {
            self.pop_for_item(indent);
            if !after_dash.is_empty() && !after_dash.starts_with(char::is_whitespace) {
                return;
            }
            self.mark(indent);
            let nested = after_dash.trim_start();
            let block = nested.starts_with('|') || nested.starts_with('>');
            self.extra.push(ViewFrame {
                indent,
                seg: PathSegment::Item,
                allow_same_indent_output: false,
            });
            if !nested.is_empty() && !block {
                self.push_mapping(nested, indent + 2);
            }
            return;
        }
        self.pop_for_entry(indent);
        self.mark(indent);
        self.push_mapping(trimmed, indent);
    }

    fn push_mapping(&mut self, text: &str, indent: usize) {
        let Some(colon) = structural_mapping_colon(text) else {
            return;
        };
        let value = text[colon + 1..].trim();
        let block = value.starts_with('|') || value.starts_with('>');
        let template_value = value.contains("{{");
        if !value.is_empty() && !block && !template_value {
            return;
        }
        let key = unquote_yaml_scalar(text[..colon].trim());
        if key.is_empty() || key.contains("{{") || key.contains("}}") {
            return;
        }
        self.extra.push(ViewFrame {
            indent,
            seg: PathSegment::Key(key.to_string()),
            allow_same_indent_output: value.is_empty(),
        });
    }

    fn pop_for_entry(&mut self, indent: usize) {
        while let Some(id) = self.cursor {
            if self.document.frames[id].indent >= indent {
                self.cursor = self.document.frames[id].parent;
            } else {
                break;
            }
        }
    }

    fn pop_for_item(&mut self, indent: usize) {
        while let Some(id) = self.cursor {
            let frame = &self.document.frames[id];
            if frame.indent > indent
                || (frame.indent == indent && !frame_allows_item(frame, self.query_line_start))
            {
                self.cursor = frame.parent;
            } else {
                break;
            }
        }
    }

    fn mark(&mut self, indent: usize) {
        let mut current = self.cursor;
        while let Some(id) = current {
            if self.document.frames[id].indent < indent {
                self.view_marked = Some(id);
                return;
            }
            current = self.document.frames[id].parent;
        }
    }

    fn chain_allows_output(&self, id: usize) -> bool {
        if self.view_marked == Some(id) {
            return false;
        }
        frame_allows_item(&self.document.frames[id], self.query_line_start)
    }

    /// The find of `structural_path_before`: from the top of the stack, the
    /// first slot shallower than `output_indent` (or at it, while it still
    /// accepts same-indent output); otherwise the top slot; `None` on an
    /// empty stack.
    fn find_open_slot(self, output_indent: usize) -> Option<Vec<PathSegment>> {
        for (position, frame) in self.extra.iter().enumerate().rev() {
            if frame.indent < output_indent
                || (frame.indent == output_indent && frame.allow_same_indent_output)
            {
                return Some(self.segments_through_extra(position));
            }
        }
        let mut current = self.cursor;
        while let Some(id) = current {
            let frame = &self.document.frames[id];
            if frame.indent < output_indent
                || (frame.indent == output_indent && self.chain_allows_output(id))
            {
                return Some(self.document.chain_segments(Some(id)));
            }
            current = frame.parent;
        }
        // Fallback: the top of the stack, whatever its indent.
        if !self.extra.is_empty() {
            return Some(self.segments_through_extra(self.extra.len() - 1));
        }
        self.cursor.map(|id| self.document.chain_segments(Some(id)))
    }

    fn segments_through_extra(&self, position: usize) -> Vec<PathSegment> {
        let mut segments = self.document.chain_segments(self.cursor);
        for frame in &self.extra[..=position] {
            segments.push(frame.seg.clone());
        }
        segments
    }
}
