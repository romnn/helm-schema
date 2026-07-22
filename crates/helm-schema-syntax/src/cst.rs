//! The `TemplatedDocument` CST: YAML layout nodes, template control regions,
//! output holes, and comments, all carrying byte spans into the source.

/// A half-open byte range `[start, end)` into the parsed source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// Inclusive byte offset where the range begins.
    pub start: usize,
    /// Exclusive byte offset where the range ends.
    pub end: usize,
}

impl Span {
    /// Creates a half-open byte range.
    #[must_use]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// A parsed templated-YAML source: the layout node forest with document
/// boundaries.
pub struct TemplatedDocument<'src> {
    pub(crate) source: &'src str,
    pub(crate) roots: Vec<Node>,
    pub(crate) document_spans: Vec<Span>,
}

impl<'src> TemplatedDocument<'src> {
    /// Parse a template source, running the Go-template parse internally.
    /// Returns an empty document when tree-sitter fails entirely (layout
    /// still parses; only action holes would be missing, so failure is
    /// modeled as a document with no action tokens).
    #[must_use]
    pub fn parse(source: &'src str) -> Self {
        match crate::actions::parse_go_template(source) {
            Some(tree) => Self::parse_with_root(source, tree.root_node()),
            None => crate::parse::parse_document(source, Vec::new()),
        }
    }

    /// Parse a template source reusing an existing Go-template parse of the
    /// same source (avoids a second tree-sitter pass).
    #[must_use]
    pub fn parse_with_root(source: &'src str, root: tree_sitter::Node<'_>) -> Self {
        let tokens = crate::actions::collect_action_tokens(root);
        crate::parse::parse_document(source, tokens)
    }

    /// Returns the source text parsed into this document.
    #[must_use]
    pub fn source(&self) -> &'src str {
        self.source
    }

    /// Returns the top-level CST nodes in source order.
    #[must_use]
    pub fn roots(&self) -> &[Node] {
        &self.roots
    }

    /// Top-level document spans, split at `---` separator lines (nonempty
    /// spans only, mirroring the resource-identity document splitter).
    #[must_use]
    pub fn document_spans(&self) -> &[Span] {
        &self.document_spans
    }
}

/// A CST node. Containers own the nodes that structurally nest below them;
/// control regions and action nodes are overlay nodes attached where they
/// appear, without affecting container structure.
#[derive(Debug)]
pub enum Node {
    /// A YAML mapping entry.
    Mapping(MappingEntry),
    /// A YAML sequence item.
    Sequence(SequenceItem),
    /// A structured Go-template control region.
    Control(ControlRegion),
    /// A standalone output action.
    Output(OutputAction),
    /// A YAML comment line.
    Comment(CommentLine),
    /// A scalar content line.
    Scalar(ScalarLine),
    /// Source retained without a more precise structural interpretation.
    Opaque(OpaqueNode),
}

/// A scalar run split into literal text and template-action holes.
#[derive(Debug)]
pub struct ScalarParts {
    /// Byte range covered by the complete scalar run.
    pub span: Span,
    /// Literal and templated pieces in source order.
    pub parts: Vec<ScalarPart>,
}

/// One literal or templated piece of a scalar run.
#[derive(Debug)]
pub enum ScalarPart {
    /// Literal source text.
    Text(Span),
    /// A Go-template output action embedded in the scalar.
    Hole(Span),
}

/// One `key: …` mapping entry line and, when the entry opens a scope, the
/// nodes nested below it.
#[derive(Debug)]
pub struct MappingEntry {
    /// The entry's own line content (key start through line end).
    pub span: Span,
    /// Effective indent. Entries nested inline after a sequence dash use the
    /// line model's `dash + 2` convention rather than the literal column.
    pub indent: usize,
    /// Parsed mapping key, including any embedded template holes.
    pub key: ScalarParts,
    /// Inline (non-block) value text, when present.
    pub value: Option<ScalarParts>,
    /// Block-scalar header and suppressed body, for `key: |`-style entries.
    pub block: Option<BlockScalar>,
    /// Whether the entry opened a container scope (empty, template, or
    /// block-scalar value with a plain key). Closed or invalid-key entries
    /// never adopt children.
    pub opens_scope: bool,
    /// Nodes structurally nested below the mapping entry.
    pub children: Vec<Node>,
}

impl MappingEntry {
    /// The sequence items nested below this entry in document order, looking
    /// through control regions: an item closed while a branch was active is
    /// owned by that branch, while an item still open when the region ended
    /// escapes to the entry itself. Guard structure is dropped; use
    /// `children` to keep it.
    #[must_use]
    pub fn sequence_items(&self) -> Vec<&SequenceItem> {
        let mut items = Vec::new();
        collect_sequence_items(&self.children, &mut items);
        items.sort_by_key(|item| item.span.start);
        items
    }
}

fn collect_sequence_items<'nodes>(nodes: &'nodes [Node], items: &mut Vec<&'nodes SequenceItem>) {
    for node in nodes {
        match node {
            Node::Sequence(item) => items.push(item),
            Node::Control(region) => {
                for branch in &region.branches {
                    collect_sequence_items(&branch.body, items);
                }
            }
            _ => {}
        }
    }
}

/// One `- …` sequence item line and the nodes nested below it. An inline
/// `- key: …` entry appears as the first child with effective indent
/// `dash + 2`.
#[derive(Debug)]
pub struct SequenceItem {
    /// The sequence item's own source line.
    pub span: Span,
    /// The dash column.
    pub indent: usize,
    /// Inline scalar item content (`- foo`), when present.
    pub value: Option<ScalarParts>,
    /// Block-scalar header and suppressed body, for `- |` items.
    pub block: Option<BlockScalar>,
    /// Nodes structurally nested below the sequence item.
    pub children: Vec<Node>,
}

impl SequenceItem {
    /// The item's content span: the first content after the dash through the
    /// end of the deepest node nested below the item (a bare dash with no
    /// content spans its own line).
    #[must_use]
    pub fn content_span(&self) -> Span {
        let start = if let Some(value) = &self.value {
            value.span.start
        } else if let Some(block) = &self.block {
            block.header.start
        } else if let Some(first) = self.children.first() {
            first.span_start()
        } else {
            self.span.start
        };
        let end = subtree_end(
            self.span.end,
            self.value.as_ref(),
            self.block.as_ref(),
            &self.children,
        );
        Span::new(start, end.max(start))
    }
}

impl Node {
    /// The byte where this node's own content starts.
    #[must_use]
    pub fn span_start(&self) -> usize {
        match self {
            Node::Mapping(entry) => entry.span.start,
            Node::Sequence(item) => item.span.start,
            Node::Control(region) => region.span.start,
            Node::Output(action) => action.span.start,
            Node::Comment(comment) => comment.span.start,
            Node::Scalar(line) => line.span.start,
            Node::Opaque(opaque) => opaque.span.start,
        }
    }

    /// The end of the deepest content in this node's subtree: nested nodes,
    /// block-scalar bodies, and inline-value holes that run past the line
    /// end for multi-line actions.
    #[must_use]
    pub fn subtree_end(&self) -> usize {
        match self {
            Node::Mapping(entry) => subtree_end(
                entry.span.end,
                entry.value.as_ref(),
                entry.block.as_ref(),
                &entry.children,
            ),
            Node::Sequence(item) => subtree_end(
                item.span.end,
                item.value.as_ref(),
                item.block.as_ref(),
                &item.children,
            ),
            Node::Control(region) => region
                .branches
                .iter()
                .flat_map(|branch| &branch.body)
                .map(Node::subtree_end)
                .fold(region.span.end, usize::max),
            Node::Output(action) => action.span.end,
            Node::Comment(comment) => comment.span.end,
            Node::Scalar(line) => scalar_parts_end(&line.content).max(line.span.end),
            Node::Opaque(opaque) => opaque.span.end,
        }
    }
}

fn subtree_end(
    own_end: usize,
    value: Option<&ScalarParts>,
    block: Option<&BlockScalar>,
    children: &[Node],
) -> usize {
    let mut end = own_end;
    if let Some(value) = value {
        end = end.max(scalar_parts_end(value));
    }
    if let Some(block) = block {
        end = end.max(block.header.end).max(block.body.end);
    }
    for child in children {
        end = end.max(child.subtree_end());
    }
    end
}

fn scalar_parts_end(parts: &ScalarParts) -> usize {
    let mut end = parts.span.end;
    for part in &parts.parts {
        let (ScalarPart::Text(span) | ScalarPart::Hole(span)) = part;
        end = end.max(span.end);
    }
    end
}

/// A literal block scalar (`|` / `>` families): its header token and the
/// suppressed body span, with any template actions inside the body kept as
/// suppressed holes.
#[derive(Debug)]
pub struct BlockScalar {
    /// Block-scalar header token, including chomping and indentation markers.
    pub header: Span,
    /// Full body lines; empty (`start == end`) when the block has no body.
    pub body: Span,
    /// Template actions suppressed from normal YAML layout parsing.
    pub holes: Vec<Span>,
}

/// Go-template action that owns a structured body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    /// Conditional `if` action.
    If,
    /// Context-selecting `with` action.
    With,
    /// Iterating `range` action.
    Range,
    /// Named-template `define` action.
    Define,
    /// Overridable named-template `block` action.
    Block,
}

/// A template control region (`{{ if }}…{{ end }}` and friends) with its
/// branch bodies. Container structure is decided by the visible YAML lines
/// alone, so a region's branches hold exactly the nodes that opened and
/// closed while the branch was active.
#[derive(Debug)]
pub struct ControlRegion {
    /// Kind of action that opened the region.
    pub kind: ControlKind,
    /// Full source range from opener through closing action.
    pub span: Span,
    /// Conditional or fallback branches in source order.
    pub branches: Vec<ControlBranch>,
    /// `false` when the region provably violates the well-nested assumption:
    /// a container opened inside a branch was still open when the branch
    /// ended (its children escape the region), or a branch boundary sat
    /// mid-line inside YAML content. Downstream must treat such regions
    /// conservatively.
    pub well_nested: bool,
}

/// One branch of a control region: its header action (`{{ if … }}`,
/// `{{ else }}`, …) and the nodes emitted while the branch was active.
#[derive(Debug)]
pub struct ControlBranch {
    /// Template action that opens this branch.
    pub header: Span,
    /// CST nodes emitted while this branch is active.
    pub body: Vec<Node>,
}

/// A standalone-line output action (`{{ include "x" . }}` on its own line).
/// Inline actions inside scalars are represented as [`ScalarPart::Hole`]s
/// instead.
#[derive(Debug)]
pub struct OutputAction {
    /// The full action span including delimiters.
    pub span: Span,
    /// The span of the expression inside the delimiters.
    pub expr_span: Span,
}

/// A YAML `#` comment line (which may itself contain template actions).
#[derive(Debug)]
pub struct CommentLine {
    /// Full source range of the comment line.
    pub span: Span,
    /// Comment content split around any template holes.
    pub content: ScalarParts,
}

/// A plain scalar content line that is not a mapping entry or sequence item:
/// flow-collection continuations, `---` markers, malformed keys, and other
/// text the layout keeps only for its popping effect.
#[derive(Debug)]
pub struct ScalarLine {
    /// Full source range of the scalar line.
    pub span: Span,
    /// YAML indentation column.
    pub indent: usize,
    /// Scalar content split around any template holes.
    pub content: ScalarParts,
}

/// A span kept without further interpretation. Opaque nodes never guess:
/// they preserve the raw span so downstream can attribute conservatively.
#[derive(Debug)]
pub struct OpaqueNode {
    /// Source range retained for attribution.
    pub span: Span,
    /// Reason the source remains opaque.
    pub kind: OpaqueKind,
}

/// Classification of source retained without deeper CST structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpaqueKind {
    /// A `{{/* … */}}` template comment.
    TemplateComment,
    /// A `{{ $x := … }}` / `{{ $x = … }}` assignment action.
    Assignment,
    /// A `{{ break }}` atom.
    Break,
    /// A `{{ continue }}` atom.
    Continue,
    /// A control region that opened mid-line inside YAML content; the whole
    /// region is preserved as one raw span.
    InlineRegion,
    /// Literal YAML text sharing a line with a standalone action.
    ActionLineText,
    /// Unparsable template content (tree-sitter `ERROR` output).
    ParseError,
}
