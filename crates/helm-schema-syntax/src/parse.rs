//! The layout parser: a single pass over source lines that builds the CST
//! node forest and the per-line open-slot chain index.
//!
//! Container structure is decided purely by the visible YAML lines (indent
//! discipline); blank lines, comment lines, and lines that begin with a
//! template action are transparent to layout. This is deliberate: Helm
//! control actions routinely open a mapping entry in one branch and populate
//! it after `{{ end }}`, so control regions overlay the container structure
//! instead of bracketing it. The open/close rules below are the layout
//! semantics that helm-schema's attribution has always used (previously
//! recovered per query by an O(n²) line replay); the parser applies them
//! once and freezes the result into [`Frame`] chains.

use std::collections::HashMap;

use crate::actions::{ActionToken, TokenKind};
use crate::cst::{
    BlockScalar, CommentLine, ControlBranch, ControlKind, ControlRegion, MappingEntry, Node,
    OpaqueKind, OpaqueNode, OutputAction, PathSegment, ScalarLine, ScalarPart, ScalarParts,
    SequenceItem, Span, TemplatedDocument,
};
use crate::lines::LineIndex;
use crate::yaml_scan::{structural_mapping_colon, unquote_yaml_scalar};

/// One open-container record. Frames are arena-allocated so that per-line
/// chain snapshots stay valid after the container closes; only `marked_at`
/// is set later (once), recording when the container first saw a deeper
/// visible child — the moment it stops accepting same-indent sequence items.
#[derive(Debug)]
pub(crate) struct Frame {
    pub(crate) parent: Option<usize>,
    pub(crate) indent: usize,
    pub(crate) seg: PathSegment,
    /// The entry's inline value was empty when the scope opened.
    pub(crate) opened_empty: bool,
    pub(crate) block: bool,
    /// Line start of the first deeper visible child line, if any.
    pub(crate) marked_at: Option<usize>,
}

pub(crate) fn parse_document(source: &str, tokens: Vec<ActionToken>) -> TemplatedDocument<'_> {
    let lines = LineIndex::new(source);
    let parser = Parser {
        source,
        tokens,
        frames: Vec::new(),
        head: None,
        chains: Vec::new(),
        owners: vec![OwnerFrame::root()],
        region_modes: HashMap::new(),
        next_token: 0,
    };
    parser.run(lines)
}

enum RegionMode {
    /// Opened on a standalone action line; a branch owner is live.
    Structured,
    /// Opened inline in YAML content or inside a suppressed context; its
    /// remaining bracket tokens are consumed without structural effect.
    Consumed,
}

struct OwnerFrame {
    children: Vec<Node>,
    data: OwnerData,
}

impl OwnerFrame {
    fn root() -> Self {
        Self {
            children: Vec::new(),
            data: OwnerData::Root,
        }
    }

    fn container(data: OwnerData) -> Self {
        Self {
            children: Vec::new(),
            data,
        }
    }
}

enum OwnerData {
    Root,
    Entry(EntrySeed),
    Item(ItemSeed),
    Branch(BranchSeed),
}

struct EntrySeed {
    frame: usize,
    span: Span,
    indent: usize,
    key: ScalarParts,
    value: Option<ScalarParts>,
    block: Option<BlockSeed>,
}

struct ItemSeed {
    frame: usize,
    span: Span,
    indent: usize,
    value: Option<ScalarParts>,
    block: Option<BlockSeed>,
}

struct BlockSeed {
    header: Span,
    body: Option<Span>,
}

struct BranchSeed {
    region: usize,
    builder: RegionBuilder,
    header: Span,
}

struct RegionBuilder {
    kind: ControlKind,
    span: Span,
    branches: Vec<ControlBranch>,
    well_nested: bool,
}

struct Parser<'src> {
    source: &'src str,
    tokens: Vec<ActionToken>,
    frames: Vec<Frame>,
    head: Option<usize>,
    chains: Vec<Option<usize>>,
    owners: Vec<OwnerFrame>,
    region_modes: HashMap<usize, RegionMode>,
    next_token: usize,
}

impl<'src> Parser<'src> {
    fn run(mut self, lines: LineIndex) -> TemplatedDocument<'src> {
        for line in 0..lines.count() {
            self.chains.push(self.head);
            let (ls, le) = lines.span(line);
            let followed_by_newline = line + 1 < lines.count();
            self.process_line(ls, le, followed_by_newline);
        }
        self.close_all();
        let roots = self
            .owners
            .pop()
            .map(|owner| owner.children)
            .unwrap_or_default();
        TemplatedDocument {
            source: self.source,
            roots,
            document_spans: document_spans(self.source),
            lines,
            frames: self.frames,
            chains: self.chains,
        }
    }

    fn process_line(&mut self, ls: usize, le: usize, followed_by_newline: bool) {
        let raw = &self.source[ls..le];
        // Mirror `str::lines()`: the replay side never sees a trailing `\r`
        // that precedes a newline (a final newline-less line keeps it).
        let replay = if followed_by_newline {
            raw.strip_suffix('\r').unwrap_or(raw)
        } else {
            raw
        };
        let trimmed = replay.trim_start();
        let indent = replay.len() - trimmed.len();
        if trimmed.is_empty() {
            return;
        }
        // Body of an open block scalar: any line deeper than the block
        // header is suppressed content, regardless of its own shape.
        if let Some(head) = self.head
            && self.frames[head].block
            && indent > self.frames[head].indent
        {
            self.extend_block_body(ls, le);
            self.consume_line_tokens(le, false);
            return;
        }
        if trimmed.starts_with('#') {
            let content = self.parts_for_span(ls + indent, le);
            self.consume_line_tokens(le, false);
            self.attach(Node::Comment(CommentLine {
                span: Span::new(ls + indent, le),
                content,
            }));
            return;
        }
        if trimmed.starts_with("{{") {
            self.process_action_line(ls, le);
            return;
        }
        self.process_content_line(ls, le, trimmed, indent);
    }

    /// A visible YAML content line: apply the pop/mark/push layout rules.
    fn process_content_line(&mut self, ls: usize, le: usize, trimmed: &str, indent: usize) {
        if let Some(after_dash) = trimmed.strip_prefix('-') {
            self.seq_pop(indent);
            self.consume_line_tokens(le, true);
            if !after_dash.is_empty() && !after_dash.starts_with(char::is_whitespace) {
                // `-foo` / `---`: a plain scalar; only its pops count.
                let content = self.parts_for_span(ls + indent, le);
                self.attach(Node::Scalar(ScalarLine {
                    span: Span::new(ls + indent, le),
                    indent,
                    content,
                }));
                return;
            }
            self.mark(indent, ls);
            self.sequence_item_line(ls, le, after_dash, indent);
            return;
        }
        self.pop(indent);
        self.consume_line_tokens(le, true);
        self.mark(indent, ls);
        self.entry_line(trimmed, indent, ls + indent, le);
    }

    fn sequence_item_line(&mut self, ls: usize, le: usize, after_dash: &str, indent: usize) {
        let nested = after_dash.trim_start();
        let nested_start = ls + indent + 1 + (after_dash.len() - nested.len());
        let item_block = nested.starts_with('|') || nested.starts_with('>');
        let frame = self.push_frame(indent, PathSegment::Item, false, item_block);
        let mut seed = ItemSeed {
            frame,
            span: Span::new(ls + indent, le),
            indent,
            value: None,
            block: item_block.then(|| BlockSeed {
                header: Span::new(nested_start, nested_start + nested.len()),
                body: None,
            }),
        };
        if !nested.is_empty() && !item_block {
            if structural_mapping_colon(nested).is_some() {
                self.owners
                    .push(OwnerFrame::container(OwnerData::Item(seed)));
                self.entry_line(nested, indent + 2, nested_start, le);
                return;
            }
            seed.value = Some(self.parts_for_span(nested_start, nested_start + nested.len()));
        }
        self.owners
            .push(OwnerFrame::container(OwnerData::Item(seed)));
    }

    /// A (potential) `key: …` line at `eff_indent`. Pops and marks have
    /// already been applied by the caller.
    fn entry_line(&mut self, text: &str, eff_indent: usize, text_start: usize, le: usize) {
        let Some(colon) = structural_mapping_colon(text) else {
            let content = self.parts_for_span(text_start, text_start + text.len());
            self.attach(Node::Scalar(ScalarLine {
                span: Span::new(text_start, le),
                indent: eff_indent,
                content,
            }));
            return;
        };
        let value_text = &text[colon + 1..];
        let value = value_text.trim();
        let block = value.starts_with('|') || value.starts_with('>');
        let template_value = value.contains("{{");
        let key_trimmed = text[..colon].trim_end();
        let key_text = unquote_yaml_scalar(key_trimmed);
        let key_invalid = key_text.is_empty() || key_text.contains("{{") || key_text.contains("}}");
        let key = self.parts_for_span(text_start, text_start + key_trimmed.len());
        let leading = value_text.len() - value_text.trim_start().len();
        let value_start = text_start + colon + 1 + leading;
        let value_parts = (!value.is_empty() && !block)
            .then(|| self.parts_for_span(value_start, value_start + value.len()));

        let closed = !value.is_empty() && !block && !template_value;
        if closed || key_invalid {
            self.attach(Node::Mapping(MappingEntry {
                span: Span::new(text_start, le),
                indent: eff_indent,
                key,
                value: value_parts,
                block: None,
                opens_scope: false,
                children: Vec::new(),
            }));
            return;
        }
        let frame = self.push_frame(
            eff_indent,
            PathSegment::Key(key_text.to_string()),
            value.is_empty(),
            block,
        );
        self.owners
            .push(OwnerFrame::container(OwnerData::Entry(EntrySeed {
                frame,
                span: Span::new(text_start, le),
                indent: eff_indent,
                key,
                value: value_parts,
                block: block.then(|| BlockSeed {
                    header: Span::new(value_start, value_start + value.len()),
                    body: None,
                }),
            })));
    }

    /// A line whose first content is a template action: transparent to
    /// layout; its tokens carry the structure (control regions, outputs).
    fn process_action_line(&mut self, ls: usize, le: usize) {
        let mut pos = ls;
        while self.next_token < self.tokens.len() && self.tokens[self.next_token].span.start < le {
            let token = self.tokens[self.next_token];
            self.next_token += 1;
            self.attach_gap_text(pos, token.span.start.min(le));
            pos = pos.max(token.span.end);
            self.handle_token_structural(token);
        }
        self.attach_gap_text(pos, le);
    }

    fn attach_gap_text(&mut self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        let text = &self.source[start..end];
        if text.trim().is_empty() {
            return;
        }
        let lead = text.len() - text.trim_start().len();
        let content_start = start + lead;
        let content_end = content_start + text.trim().len();
        self.attach(Node::Opaque(OpaqueNode {
            span: Span::new(content_start, content_end),
            kind: OpaqueKind::ActionLineText,
        }));
    }

    fn handle_token_structural(&mut self, token: ActionToken) {
        match token.kind {
            TokenKind::Output { expr_span } => self.attach(Node::Output(OutputAction {
                span: token.span,
                expr_span,
            })),
            TokenKind::Assign => self.attach_opaque(token.span, OpaqueKind::Assignment),
            TokenKind::TemplateComment => {
                self.attach_opaque(token.span, OpaqueKind::TemplateComment);
            }
            TokenKind::ControlAtom => self.attach_opaque(token.span, OpaqueKind::ControlAtom),
            TokenKind::Error => self.attach_opaque(token.span, OpaqueKind::ParseError),
            TokenKind::RegionOpen {
                region,
                kind,
                region_end,
            } => {
                self.region_modes.insert(region, RegionMode::Structured);
                self.owners.push(OwnerFrame {
                    children: Vec::new(),
                    data: OwnerData::Branch(BranchSeed {
                        region,
                        builder: RegionBuilder {
                            kind,
                            span: Span::new(token.span.start, region_end),
                            branches: Vec::new(),
                            well_nested: true,
                        },
                        header: token.span,
                    }),
                });
            }
            TokenKind::RegionBranch { region } => self.rotate_branch(region, token.span, true),
            TokenKind::RegionEnd { region } => self.end_region(region, true),
        }
    }

    fn rotate_branch(&mut self, region: usize, header: Span, clean_boundary: bool) {
        if let Some((mut seed, _)) = self.close_branch(region, clean_boundary) {
            seed.header = header;
            self.owners.push(OwnerFrame {
                children: Vec::new(),
                data: OwnerData::Branch(seed),
            });
        }
    }

    fn end_region(&mut self, region: usize, clean_boundary: bool) {
        if let Some((seed, index)) = self.close_branch(region, clean_boundary) {
            self.region_modes.remove(&region);
            // Attach below where the branch sat, not to a container that
            // escaped it: the region is a sibling of the escaped container.
            self.attach_at(
                index.saturating_sub(1),
                Node::Control(ControlRegion {
                    kind: seed.builder.kind,
                    span: seed.builder.span,
                    branches: seed.builder.branches,
                    well_nested: seed.builder.well_nested,
                }),
            );
        }
    }

    /// Close the live branch owner of `region`, folding its children into
    /// the region builder; returns the seed and the stack index the branch
    /// occupied. Containers still open above the branch escape it: they stay
    /// open (layout is decided by lines alone) and the region is flagged as
    /// not well-nested.
    fn close_branch(&mut self, region: usize, clean_boundary: bool) -> Option<(BranchSeed, usize)> {
        if !matches!(self.region_modes.get(&region), Some(RegionMode::Structured)) {
            return None;
        }
        let index = self.owners.iter().rposition(
            |owner| matches!(&owner.data, OwnerData::Branch(seed) if seed.region == region),
        )?;
        let escaped = self.owners.len() - 1 - index;
        let owner = self.owners.remove(index);
        let OwnerData::Branch(mut seed) = owner.data else {
            return None;
        };
        if escaped > 0 || !clean_boundary {
            seed.builder.well_nested = false;
        }
        seed.builder.branches.push(ControlBranch {
            header: seed.header,
            body: owner.children,
        });
        Some((seed, index))
    }

    /// Consume the tokens of a line that is not a standalone action line.
    /// Output/comment tokens become holes in the surrounding text (no
    /// nodes); a region opening here cannot bracket layout, so it is
    /// consumed — and, on a visible content line, degraded to an opaque node
    /// covering the whole region ("never guess" — the raw span is
    /// preserved). A structured region's `else`/`end` landing here still
    /// rotates/closes the region, flagged as an unclean boundary.
    fn consume_line_tokens(&mut self, le: usize, opaque_inline_regions: bool) {
        while self.next_token < self.tokens.len() && self.tokens[self.next_token].span.start < le {
            let token = self.tokens[self.next_token];
            self.next_token += 1;
            match token.kind {
                TokenKind::RegionOpen {
                    region, region_end, ..
                } => {
                    self.region_modes.insert(region, RegionMode::Consumed);
                    if opaque_inline_regions {
                        self.attach(Node::Opaque(OpaqueNode {
                            span: Span::new(token.span.start, region_end),
                            kind: OpaqueKind::InlineRegion,
                        }));
                    }
                }
                TokenKind::RegionBranch { region } => self.rotate_branch(region, token.span, false),
                TokenKind::RegionEnd { region } => self.end_region(region, false),
                _ => {}
            }
        }
    }

    fn extend_block_body(&mut self, ls: usize, le: usize) {
        for owner in self.owners.iter_mut().rev() {
            let block = match &mut owner.data {
                OwnerData::Entry(seed) => seed.block.as_mut(),
                OwnerData::Item(seed) => seed.block.as_mut(),
                OwnerData::Root | OwnerData::Branch(_) => continue,
            };
            if let Some(block) = block {
                let start = block.body.map_or(ls, |span| span.start);
                block.body = Some(Span::new(start, le));
            }
            return;
        }
    }

    fn push_frame(
        &mut self,
        indent: usize,
        seg: PathSegment,
        opened_empty: bool,
        block: bool,
    ) -> usize {
        let id = self.frames.len();
        self.frames.push(Frame {
            parent: self.head,
            indent,
            seg,
            opened_empty,
            block,
            marked_at: None,
        });
        self.head = Some(id);
        id
    }

    /// Content-line pops: close containers at `indent` or deeper.
    fn pop(&mut self, indent: usize) {
        while let Some(head) = self.head {
            if self.frames[head].indent >= indent {
                self.close_container();
            } else {
                break;
            }
        }
    }

    /// Sequence-item pops: a container at the item's own indent survives
    /// while it still accepts same-indent items (empty value, unmarked).
    fn seq_pop(&mut self, indent: usize) {
        while let Some(head) = self.head {
            let frame = &self.frames[head];
            let allow = frame.opened_empty && frame.marked_at.is_none();
            if frame.indent > indent || (frame.indent == indent && !allow) {
                self.close_container();
            } else {
                break;
            }
        }
    }

    /// Record that the nearest shallower container saw a visible child line.
    fn mark(&mut self, indent: usize, ls: usize) {
        let mut current = self.head;
        while let Some(id) = current {
            if self.frames[id].indent < indent {
                if self.frames[id].marked_at.is_none() {
                    self.frames[id].marked_at = Some(ls);
                }
                return;
            }
            current = self.frames[id].parent;
        }
    }

    /// Close the innermost open container. Branch owners above it stay live
    /// (regions overlay containers); the closed node attaches to the owner
    /// directly beneath it.
    fn close_container(&mut self) {
        let Some(index) = self
            .owners
            .iter()
            .rposition(|owner| matches!(owner.data, OwnerData::Entry(_) | OwnerData::Item(_)))
        else {
            self.head = None;
            return;
        };
        let owner = self.owners.remove(index);
        let children = owner.children;
        let (node, frame) = match owner.data {
            OwnerData::Entry(seed) => (
                Node::Mapping(MappingEntry {
                    span: seed.span,
                    indent: seed.indent,
                    key: seed.key,
                    value: seed.value,
                    block: seed.block.map(|block| self.finish_block(block)),
                    opens_scope: true,
                    children,
                }),
                seed.frame,
            ),
            OwnerData::Item(seed) => (
                Node::Sequence(SequenceItem {
                    span: seed.span,
                    indent: seed.indent,
                    value: seed.value,
                    block: seed.block.map(|block| self.finish_block(block)),
                    children,
                }),
                seed.frame,
            ),
            OwnerData::Root | OwnerData::Branch(_) => {
                // Unreachable by construction: `index` matched a container.
                self.head = None;
                return;
            }
        };
        self.head = self.frames[frame].parent;
        self.attach_at(index.saturating_sub(1), node);
    }

    fn finish_block(&self, seed: BlockSeed) -> BlockScalar {
        let body = seed
            .body
            .unwrap_or(Span::new(seed.header.end, seed.header.end));
        let holes = self
            .tokens
            .iter()
            .filter(|token| token.span.start >= body.start && token.span.start < body.end)
            .map(|token| token.span)
            .collect();
        BlockScalar {
            header: seed.header,
            body,
            holes,
        }
    }

    fn close_all(&mut self) {
        while self.owners.len() > 1 {
            let top = self.owners.len() - 1;
            match &self.owners[top].data {
                OwnerData::Entry(_) | OwnerData::Item(_) => self.close_container(),
                OwnerData::Branch(seed) => {
                    let region = seed.region;
                    self.end_region(region, false);
                }
                OwnerData::Root => break,
            }
        }
    }

    fn attach(&mut self, node: Node) {
        let index = self.owners.len() - 1;
        self.attach_at(index, node);
    }

    fn attach_at(&mut self, index: usize, node: Node) {
        if let Some(owner) = self.owners.get_mut(index) {
            owner.children.push(node);
        }
    }

    fn attach_opaque(&mut self, span: Span, kind: OpaqueKind) {
        self.attach(Node::Opaque(OpaqueNode { span, kind }));
    }

    /// Split `[start, end)` into literal text runs and action holes. Holes
    /// keep their full token span, which may extend past `end` for
    /// multi-line actions.
    fn parts_for_span(&self, start: usize, end: usize) -> ScalarParts {
        let mut parts = Vec::new();
        let mut pos = start;
        let first = self.tokens.partition_point(|token| token.span.end <= start);
        for token in &self.tokens[first..] {
            if token.span.start >= end {
                break;
            }
            if token.span.start > pos {
                parts.push(ScalarPart::Text(Span::new(pos, token.span.start)));
            }
            parts.push(ScalarPart::Hole(token.span));
            pos = pos.max(token.span.end);
        }
        if pos < end {
            parts.push(ScalarPart::Text(Span::new(pos, end)));
        }
        ScalarParts {
            span: Span::new(start, end),
            parts,
        }
    }
}

/// Top-level document spans, split at lines whose trimmed text is exactly
/// `---`. Mirrors the resource-identity splitter: only nonempty spans.
fn document_spans(source: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    let mut byte = 0usize;
    for line in source.split_inclusive('\n') {
        if line.trim() == "---" {
            if start < byte {
                spans.push(Span::new(start, byte));
            }
            start = byte + line.len();
        }
        byte += line.len();
    }
    if start < source.len() {
        spans.push(Span::new(start, source.len()));
    }
    spans
}
