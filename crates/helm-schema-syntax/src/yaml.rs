//! YAML concrete syntax trees of rendered skeletons, parsed by the
//! tree-sitter YAML grammar.
//!
//! The grammar resolves plain scalars with the YAML 1.2 core schema, while
//! Helm reads YAML 1.1: `y`, `yes` and `on` are booleans and `010` is octal
//! there. The tree therefore supplies structure and scalar style only; a
//! plain scalar's type or value never comes from the grammar's scalar kinds.
//!
//! [`arm_layout`] builds one arm's skeleton (its rendered text, with a
//! placeholder for each hole whose shape is proved) and maps the skeleton's
//! YAML nodes back to the pieces that produced them. Error-free parsing is
//! necessary but not sufficient: every permitted substitution of a
//! placeholder must leave the structure in place, or the arm is uncertain.

use crate::Span;
use crate::rendered::{HoleShape, LayoutUncertainty, RenderedArm, RenderedBody, RenderedPiece};
use helm_schema_goyaml::{
    Event, EventKind, Mark, Reader, ScalarStyle as ScalarStyleEvent, Syntax, Token, TokenKind,
};

/// Upper bound on one arm's skeleton, in bytes. It is well above the largest
/// template of the chart corpus (a 1.4 MB CRD); a larger skeleton is
/// [`LayoutUncertainty::Overflow`] and is never parsed.
pub const MAX_SKELETON_BYTES: usize = 4 << 20;

thread_local! {
    /// A `Parser` is far more expensive to build than to reuse.
    static YAML_PARSER: std::cell::RefCell<YamlParser> =
        std::cell::RefCell::new(YamlParser::default());
}

struct ParsedYaml {
    text: String,
    tree: tree_sitter::Tree,
    end: tree_sitter::Point,
}

struct SharedPrefix<'a> {
    previous: &'a ParsedYaml,
    length: usize,
}

struct YamlParser {
    parser: Option<tree_sitter::Parser>,
    previous: Vec<ParsedYaml>,
    bytes: usize,
}

impl Default for YamlParser {
    fn default() -> Self {
        let language = tree_sitter::Language::new(tree_sitter_yaml::LANGUAGE);
        let mut parser = tree_sitter::Parser::new();
        Self {
            parser: parser.set_language(&language).ok().map(|()| parser),
            previous: Vec::new(),
            bytes: 0,
        }
    }
}

impl YamlParser {
    fn parse(&mut self, text: &str) -> Option<tree_sitter::Tree> {
        if let Some(previous) = self.previous.iter().find(|previous| previous.text == text) {
            return Some(previous.tree.clone());
        }
        let previous = self
            .previous
            .iter()
            .filter_map(|previous| {
                let mut length = common_prefix(text.as_bytes(), previous.text.as_bytes());
                while !text.is_char_boundary(length) {
                    length -= 1;
                }
                (length > 0).then_some(SharedPrefix { previous, length })
            })
            .max_by_key(|prefix| prefix.length);
        let end = |text: &str| tree_sitter::Point {
            row: text.matches('\n').count(),
            column: text
                .rsplit_once('\n')
                .map_or(text.len(), |(_, tail)| tail.len()),
        };
        let new_end = end(text);
        let edited = previous.map(|SharedPrefix { previous, length }| {
            let mut tree = previous.tree.clone();
            tree.edit(&tree_sitter::InputEdit {
                start_byte: length,
                old_end_byte: previous.text.len(),
                new_end_byte: text.len(),
                start_position: end(text.get(..length).unwrap_or_default()),
                old_end_position: previous.end,
                new_end_position: new_end,
            });
            tree
        });
        let tree = self.parser.as_mut()?.parse(text, edited.as_ref())?;
        if text.len() <= MAX_SKELETON_BYTES {
            while self.previous.len() >= 8
                || self.bytes.saturating_add(text.len()) > 8 * 1024 * 1024
            {
                let Some(first) = self.previous.first() else {
                    break;
                };
                self.bytes -= first.text.len();
                self.previous.remove(0);
            }
            self.bytes += text.len();
            self.previous.push(ParsedYaml {
                text: text.to_owned(),
                tree: tree.clone(),
                end: new_end,
            });
        }
        Some(tree)
    }
}

fn common_prefix(left: &[u8], right: &[u8]) -> usize {
    let (left_blocks, _) = left.as_chunks::<64>();
    let (right_blocks, _) = right.as_chunks::<64>();
    let blocks = left_blocks
        .iter()
        .zip(right_blocks)
        .take_while(|(left, right)| left == right)
        .count()
        * 64;
    blocks
        + left
            .get(blocks..)
            .unwrap_or_default()
            .iter()
            .zip(right.get(blocks..).unwrap_or_default())
            .take_while(|(left, right)| left == right)
            .count()
}

/// Parse `text` as a YAML stream. `None` when the grammar cannot be loaded
/// or the parse is cancelled; a malformed stream still yields a tree whose
/// error and missing nodes mark the malformed parts.
#[must_use]
#[tracing::instrument(skip_all)]
pub fn parse_yaml(text: &str) -> Option<tree_sitter::Tree> {
    YAML_PARSER.with_borrow_mut(|parser| parser.parse(text))
}

/// The text one arm renders, with provenance for every generated byte.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Skeleton {
    /// The rendered text, with a placeholder token for each hole.
    pub text: String,
    /// Which piece produced each byte range of `text`, in order.
    pub segments: Vec<SkeletonSegment>,
}

/// One piece's bytes in a [`Skeleton`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkeletonSegment {
    /// The byte range in [`Skeleton::text`].
    pub range: Span,
    /// The arm-local index of the piece in [`RenderedArm::pieces`].
    pub piece: usize,
    /// The bytes are a synthetic placeholder for a hole, never output text.
    pub placeholder: bool,
}

/// The layout of one arm.
#[derive(Clone, Debug, PartialEq)]
pub enum ArmLayout {
    /// The skeleton's YAML structure holds for every permitted hole output.
    Known(YamlOwnership),
    /// Nothing is claimed about the arm's YAML placement.
    Uncertain(LayoutUncertainty),
}

/// The YAML nodes of an arm's skeleton and the pieces each one owns.
#[derive(Clone, Debug, PartialEq)]
pub struct YamlOwnership {
    /// The skeleton the node spans index.
    pub skeleton: Skeleton,
    /// Structural nodes in document order; parents precede children.
    pub nodes: Vec<YamlNode>,
    /// The block scalars among `nodes`, with their decoded headers.
    pub blocks: Vec<BlockScalarOwnership>,
}

/// One structural YAML node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct YamlNode {
    /// The enclosing structural node.
    pub parent: Option<usize>,
    /// What the node is.
    pub kind: YamlNodeKind,
    /// The node is the key of its mapping entry.
    pub key: bool,
    /// The node's bytes in the skeleton.
    pub span: Span,
    /// The arm-local piece indices `[first, last]` whose bytes the node
    /// covers; `None` for a node of no bytes.
    pub pieces: Option<(usize, usize)>,
}

/// The structural node kinds ownership is expressed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YamlNodeKind {
    /// A YAML document.
    Document,
    /// An explicit `...` document-end token.
    DocumentEnd,
    /// A block mapping.
    Mapping,
    /// A block mapping entry.
    Pair,
    /// A block sequence.
    Sequence,
    /// A block sequence item.
    Item,
    /// A flow mapping `{…}`.
    FlowMapping,
    /// A flow mapping entry.
    FlowPair,
    /// A flow sequence `[…]`.
    FlowSequence,
    /// A flow scalar of the given style.
    Scalar(ScalarStyle),
    /// A `|` or `>` block scalar.
    BlockScalar,
    /// A `#` comment.
    Comment,
}

impl YamlNodeKind {
    fn is_collection(self) -> bool {
        matches!(
            self,
            Self::Mapping | Self::Sequence | Self::FlowMapping | Self::FlowSequence
        )
    }
}

/// The style of a flow scalar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarStyle {
    /// Unquoted.
    Plain,
    /// `'…'`.
    SingleQuoted,
    /// `"…"`.
    DoubleQuoted,
}

/// A block scalar and its header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockScalarOwnership {
    /// The block scalar's index in [`YamlOwnership::nodes`].
    pub node: usize,
    /// The decoded `|`/`>` header.
    pub header: BlockHeader,
}

/// A block scalar header, decoded as YAML defines it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockHeader {
    /// `|` keeps line breaks; `>` folds them.
    pub folded: bool,
    /// What happens to the final line breaks.
    pub chomping: Chomping,
    /// The explicit indentation indicator (`|2`), if any.
    pub indentation: Option<u8>,
}

/// Block scalar chomping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chomping {
    /// Default: keep one final line break.
    Clip,
    /// `-`: drop the final line breaks.
    Strip,
    /// `+`: keep every final line break.
    Keep,
}

/// The layout `arm` proves. A hole becomes a placeholder only when its
/// shape proves it stays inside one scalar token; any other hole makes the
/// arm uncertain.
#[must_use]
pub fn arm_layout(body: &RenderedBody<'_>, arm: &RenderedArm) -> ArmLayout {
    match layout(body, arm) {
        Ok(owned) => ArmLayout::Known(owned),
        Err(uncertainty) => ArmLayout::Uncertain(uncertainty),
    }
}

/// The skeleton of `arm`: literals verbatim and a placeholder token `h<i>`
/// for the arm's `i`-th piece when it is a hole.
///
/// # Errors
///
/// [`LayoutUncertainty::Provenance`] when a piece does not resolve in the
/// body's source, and [`LayoutUncertainty::Overflow`] beyond
/// [`MAX_SKELETON_BYTES`].
pub fn arm_skeleton(
    body: &RenderedBody<'_>,
    arm: &RenderedArm,
) -> Result<Skeleton, LayoutUncertainty> {
    build_skeleton(body, arm, false)
}

fn layout(body: &RenderedBody<'_>, arm: &RenderedArm) -> Result<YamlOwnership, LayoutUncertainty> {
    let skeleton = build_skeleton(body, arm, false)?;
    let mut may_be_empty = false;
    for piece in &arm.pieces {
        match body.pieces.get(piece.0) {
            Some(RenderedPiece::Hole {
                shape: HoleShape::Unknown,
                ..
            }) => return Err(LayoutUncertainty::RawHole),
            Some(RenderedPiece::Hole {
                shape: HoleShape::ScalarText { nonempty: false },
                ..
            }) => may_be_empty = true,
            _ => {}
        }
    }
    let owned = skeleton_ownership(skeleton)?;
    if may_be_empty {
        // An empty output is a permitted substitution too. It must keep every
        // node and its place; only a placeholder-only mapping value or block
        // item may vanish, leaving its entry with a null value.
        let empty = skeleton_ownership(build_skeleton(body, arm, true)?)
            .map_err(|_| LayoutUncertainty::Substitution)?;
        if structure(&owned) != structure(&empty) {
            return Err(LayoutUncertainty::Substitution);
        }
    }
    Ok(owned)
}

/// Concatenate the arm's pieces: literals verbatim, holes as a placeholder
/// token (or nothing, for the empty substitution).
fn build_skeleton(
    body: &RenderedBody<'_>,
    arm: &RenderedArm,
    empty: bool,
) -> Result<Skeleton, LayoutUncertainty> {
    let mut text = String::new();
    let mut segments = Vec::new();
    for (index, piece) in arm.pieces.iter().enumerate() {
        let start = text.len();
        let placeholder = match body.pieces.get(piece.0) {
            Some(RenderedPiece::Text { span, .. }) => {
                let literal = body
                    .source
                    .get(span.start..span.end)
                    .ok_or(LayoutUncertainty::Provenance)?;
                if start + literal.len() > MAX_SKELETON_BYTES {
                    return Err(LayoutUncertainty::Overflow);
                }
                text.push_str(literal);
                false
            }
            Some(RenderedPiece::Hole { .. }) => {
                if !empty {
                    let number = index.to_string();
                    if start + 1 + number.len() > MAX_SKELETON_BYTES {
                        return Err(LayoutUncertainty::Overflow);
                    }
                    text.push('h');
                    text.push_str(&number);
                }
                true
            }
            None => return Err(LayoutUncertainty::Provenance),
        };
        segments.push(SkeletonSegment {
            range: Span::new(start, text.len()),
            piece: index,
            placeholder,
        });
    }
    Ok(Skeleton { text, segments })
}

/// Parses a skeleton and maps its structural nodes to pieces.
///
/// Ownership describes these bytes only; callers must prove that substitutions preserve it.
///
/// # Errors
///
/// Returns layout uncertainty for parse errors or unsupported YAML ownership.
pub fn skeleton_ownership(skeleton: Skeleton) -> Result<YamlOwnership, LayoutUncertainty> {
    ownership(skeleton, false)
}

/// Owns only the first document decoded from one Helm `SplitManifests` chunk.
///
/// # Errors
///
/// Returns layout uncertainty when that document has parse errors or unsupported ownership.
pub fn first_document_ownership(skeleton: Skeleton) -> Result<YamlOwnership, LayoutUncertainty> {
    ownership(skeleton, true)
}

/// Reads a skeleton with Helm's pinned go-yaml reader and maps its nodes to pieces.
///
/// `first_only` reads what Helm reads of one `SplitManifests` chunk: the first document.
/// Otherwise each later document is read by a fresh reader from the previous document's end.
#[tracing::instrument(skip_all)]
fn ownership(skeleton: Skeleton, first_only: bool) -> Result<YamlOwnership, LayoutUncertainty> {
    // Reader marks count characters; ASCII text needs no table to turn them into bytes.
    let offsets: Vec<usize> = if skeleton.text.is_ascii() {
        Vec::new()
    } else {
        skeleton
            .text
            .char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(skeleton.text.len()))
            .collect()
    };
    let byte = |index: usize| {
        if offsets.is_empty() {
            index.min(skeleton.text.len())
        } else {
            offsets
                .get(index)
                .or(offsets.last())
                .copied()
                .unwrap_or_default()
        }
    };
    let mut tree = Tree::default();
    let mut base = 0;
    loop {
        let rest = skeleton.text.get(byte(base)..).unwrap_or_default();
        let read = read_first_document(rest)?;
        let at = |mark: Mark| byte(base + mark.index);
        let Some(end) = tree.document(&read, &at, &skeleton.text)? else {
            break;
        };
        if first_only || end.index == 0 {
            break;
        }
        base += end.index;
    }
    let mut nodes = Vec::with_capacity(tree.drafts.len());
    let mut blocks = Vec::new();
    tree.flatten(&skeleton, &mut nodes, &mut blocks)?;
    if may_form_merge_key(&nodes, &skeleton) {
        return Err(LayoutUncertainty::MergeKey);
    }
    Ok(YamlOwnership {
        skeleton,
        nodes,
        blocks,
    })
}

/// Whether Helm refuses one split manifest chunk of exactly these bytes.
///
/// Helm 4.2.3 decodes each chunk `SplitManifests` yields into `SimpleHead` through
/// sigs.k8s.io/yaml and the pinned go-yaml reader, and any failure aborts the render with
/// `YAML parse error on <template>` (`pkg/release/v1/util/manifest_sorter.go:150-153`).
#[must_use]
pub fn manifest_chunk_rejected(chunk: &str) -> bool {
    helm_schema_goyaml::unmarshal_json(chunk.as_bytes(), helm_schema_goyaml::Target::Head).is_err()
}

/// The pinned manifest-head rejection verdict, or `None` when distinct YAML keys can become
/// the same JSON key and Go's map iteration order may choose a different head value.
#[must_use]
pub fn manifest_chunk_rejection(chunk: &str) -> Option<bool> {
    let Ok(value) = helm_schema_goyaml::unmarshal(chunk.as_bytes()) else {
        return Some(true);
    };
    if helm_schema_goyaml::has_json_key_collisions(&value) {
        return None;
    }
    Some(helm_schema_goyaml::to_json(&value, helm_schema_goyaml::Target::Head).is_err())
}

/// The value sigs.k8s.io/yaml decodes from exactly these bytes, or `None` when the pinned
/// reader refuses them or the value has no JSON form.
#[must_use]
pub fn decoded_yaml_value(text: &str) -> Option<serde_json::Value> {
    helm_schema_goyaml::unmarshal_json(text.as_bytes(), helm_schema_goyaml::Target::Generic).ok()
}

/// The tokens and events of the first document of `text`, as the pinned reader delivers them.
fn read_first_document(text: &str) -> Result<(Vec<Token>, Vec<Event>), LayoutUncertainty> {
    let mut reader = Reader::default();
    let mut tokens = Vec::new();
    let mut events = Vec::new();
    let mut observe = |syntax: Syntax<'_>| {
        match syntax {
            Syntax::Token(token) => tokens.push(token.clone()),
            Syntax::Event(event) => events.push(event.clone()),
        }
        Ok(())
    };
    for character in text.chars() {
        reader
            .step_syntax(character, &mut observe)
            .map_err(|_| LayoutUncertainty::Parse)?;
    }
    reader
        .finish_syntax(&mut observe)
        .map_err(|_| LayoutUncertainty::Parse)?;
    Ok((tokens, events))
}

fn push(
    drafts: &mut Vec<Draft>,
    parent: Option<usize>,
    kind: YamlNodeKind,
    key: bool,
    start: usize,
) -> usize {
    drafts.push(Draft {
        parent,
        kind,
        key,
        start,
        end: start,
    });
    drafts.len() - 1
}

/// A node before document order and piece coverage are fixed.
struct Draft {
    parent: Option<usize>,
    kind: YamlNodeKind,
    key: bool,
    start: usize,
    end: usize,
}

/// An open node whose children the reader is still delivering.
enum Frame {
    Document(usize),
    /// A mapping and its open entry, with whether the entry's value has started.
    Mapping {
        node: usize,
        flow: bool,
        entry: Option<(usize, bool)>,
    },
    /// A sequence and, in block style, its open item.
    Sequence {
        node: usize,
        flow: bool,
        item: Option<usize>,
    },
}

#[derive(Default)]
struct Tree {
    drafts: Vec<Draft>,
}

/// Where one read's tokens put collection openers, block entries and trivia.
struct TokenIndex {
    /// Collection openers per start offset, in token order: mapping and flow style.
    openers: std::collections::BTreeMap<usize, Vec<(bool, bool)>>,
    /// Block entry `-` indicators in token order.
    entries: std::vec::IntoIter<(usize, usize)>,
    /// Every token's extent.
    spans: Vec<(usize, usize)>,
}

impl TokenIndex {
    fn new(tokens: &[Token], at: &impl Fn(Mark) -> usize) -> Self {
        let mut openers: std::collections::BTreeMap<usize, Vec<(bool, bool)>> =
            std::collections::BTreeMap::new();
        let mut entries = Vec::new();
        let mut spans = Vec::with_capacity(tokens.len());
        for Token { kind, start, end } in tokens {
            let span = (at(*start), at(*end));
            spans.push(span);
            let opener = match kind {
                TokenKind::BlockMappingStart => (true, false),
                TokenKind::FlowMappingStart => (true, true),
                TokenKind::BlockSequenceStart => (false, false),
                TokenKind::FlowSequenceStart => (false, true),
                TokenKind::BlockEntry => {
                    entries.push(span);
                    continue;
                }
                TokenKind::StreamStart
                | TokenKind::StreamEnd
                | TokenKind::VersionDirective(..)
                | TokenKind::TagDirective(..)
                | TokenKind::DocumentStart
                | TokenKind::DocumentEnd
                | TokenKind::BlockEnd
                | TokenKind::FlowSequenceEnd
                | TokenKind::FlowMappingEnd
                | TokenKind::FlowEntry
                | TokenKind::Key
                | TokenKind::Value
                | TokenKind::Alias(_)
                | TokenKind::Anchor(_)
                | TokenKind::Tag(..)
                | TokenKind::Scalar { .. } => continue,
            };
            openers.entry(span.0).or_default().push(opener);
        }
        Self {
            openers,
            entries: entries.into_iter(),
            spans,
        }
    }

    /// Consumes the opener of a mapping or sequence starting at `start`; `Some(true)` when it
    /// is a flow bracket.
    fn flow(&mut self, start: usize, mapping: bool) -> Option<bool> {
        let openers = self.openers.get_mut(&start)?;
        let index = openers.iter().position(|(kind, _)| *kind == mapping)?;
        Some(openers.remove(index).1)
    }
}

/// Anchors, tags and aliases are interpretations the layout does not resolve.
fn untagged(anchor: &str, tag: &str) -> Result<(), LayoutUncertainty> {
    if anchor.is_empty() && tag.is_empty() {
        Ok(())
    } else {
        Err(LayoutUncertainty::TagOrAlias)
    }
}

impl Tree {
    /// Adds the first document of one read; returns its end mark, or `None` for a stream
    /// without a document.
    fn document(
        &mut self,
        (tokens, events): &(Vec<Token>, Vec<Event>),
        at: &impl Fn(Mark) -> usize,
        text: &str,
    ) -> Result<Option<Mark>, LayoutUncertainty> {
        let mut index = TokenIndex::new(tokens, at);
        let mut stack: Vec<Frame> = Vec::new();
        let mut document = None;
        for Event { kind, start, end } in events {
            let (start, end_offset) = (at(*start), at(*end));
            match kind {
                EventKind::StreamStart | EventKind::StreamEnd => {}
                EventKind::DocumentStart { implicit: _ } => {
                    let node = self.push(None, YamlNodeKind::Document, false, start);
                    document = Some(node);
                    stack.push(Frame::Document(node));
                }
                EventKind::DocumentEnd { implicit } => {
                    let Some(node) = document else {
                        return Err(LayoutUncertainty::Parse);
                    };
                    if !implicit {
                        let marker = self.push(Some(node), YamlNodeKind::DocumentEnd, false, start);
                        self.extend(marker, end_offset);
                    }
                    self.extend(node, end_offset);
                    self.comments(node, &index.spans, text);
                    return Ok(Some(*end));
                }
                EventKind::Alias(_) => return Err(LayoutUncertainty::TagOrAlias),
                EventKind::Scalar {
                    anchor,
                    tag,
                    value: _,
                    style,
                    implicit: _,
                } => {
                    untagged(anchor, tag)?;
                    self.scalar(&mut stack, &mut index, *style, (start, end_offset), text)?;
                }
                EventKind::MappingStart {
                    anchor,
                    tag,
                    implicit: _,
                } => {
                    untagged(anchor, tag)?;
                    self.open_collection(&mut stack, &mut index, true, start)?;
                }
                EventKind::SequenceStart {
                    anchor,
                    tag,
                    implicit: _,
                } => {
                    untagged(anchor, tag)?;
                    self.open_collection(&mut stack, &mut index, false, start)?;
                }
                EventKind::MappingEnd | EventKind::SequenceEnd => {
                    self.close_collection(&mut stack, end_offset, text)?;
                }
            }
        }
        Ok(None)
    }

    /// Adds a scalar of `style` spanning `(start, end)` in the slot it fills.
    fn scalar(
        &mut self,
        stack: &mut [Frame],
        index: &mut TokenIndex,
        style: ScalarStyleEvent,
        (start, end): (usize, usize),
        text: &str,
    ) -> Result<(), LayoutUncertainty> {
        let (parent, key) = self.open_slot(stack, &mut index.entries, start)?;
        // An empty node is implicit null: no bytes and no node, only an extent.
        if start < end {
            let kind = match style {
                ScalarStyleEvent::Plain => YamlNodeKind::Scalar(ScalarStyle::Plain),
                ScalarStyleEvent::SingleQuoted => YamlNodeKind::Scalar(ScalarStyle::SingleQuoted),
                ScalarStyleEvent::DoubleQuoted => YamlNodeKind::Scalar(ScalarStyle::DoubleQuoted),
                ScalarStyleEvent::Literal | ScalarStyleEvent::Folded => YamlNodeKind::BlockScalar,
            };
            let node = self.push(Some(parent), kind, key, start);
            // A block scalar's token runs through its final line breaks; the node spans its
            // header and content lines only.
            let end = if kind == YamlNodeKind::BlockScalar {
                start
                    + text
                        .get(start..end)
                        .unwrap_or_default()
                        .trim_end_matches([' ', '\t', '\r', '\n'])
                        .len()
            } else {
                end
            };
            self.extend(node, end);
        }
        self.close_slot(stack, end);
        Ok(())
    }

    /// Opens a mapping or sequence starting at `start` in the slot it fills.
    fn open_collection(
        &mut self,
        stack: &mut Vec<Frame>,
        index: &mut TokenIndex,
        mapping: bool,
        start: usize,
    ) -> Result<(), LayoutUncertainty> {
        let in_flow = stack.iter().any(|frame| match frame {
            Frame::Mapping { flow, .. } | Frame::Sequence { flow, .. } => *flow,
            Frame::Document(_) => false,
        });
        // Only a flow sequence's single-pair entry opens a mapping without a token.
        let flow = index.flow(start, mapping).unwrap_or(in_flow);
        let (parent, key) = self.open_slot(stack, &mut index.entries, start)?;
        let kind = match (mapping, flow) {
            (true, false) => YamlNodeKind::Mapping,
            (true, true) => YamlNodeKind::FlowMapping,
            (false, false) => YamlNodeKind::Sequence,
            (false, true) => YamlNodeKind::FlowSequence,
        };
        let node = self.push(Some(parent), kind, key, start);
        stack.push(if mapping {
            Frame::Mapping {
                node,
                flow,
                entry: None,
            }
        } else {
            Frame::Sequence {
                node,
                flow,
                item: None,
            }
        });
        Ok(())
    }

    /// Closes the innermost collection at its end event `end`.
    fn close_collection(
        &mut self,
        stack: &mut Vec<Frame>,
        end: usize,
        text: &str,
    ) -> Result<(), LayoutUncertainty> {
        let (node, flow) = match stack.pop() {
            Some(Frame::Mapping { node, flow, .. } | Frame::Sequence { node, flow, .. }) => {
                (node, flow)
            }
            Some(Frame::Document(_)) | None => return Err(LayoutUncertainty::Parse),
        };
        // A flow collection ends at its bracket. A block one ends at its last child, unless
        // the stream end closes it: then its `BLOCK-END` token sits at the end of input
        // (`scannerc.go:1029`) and the trailing trivia is its.
        if flow || end == text.len() {
            self.extend(node, end);
        }
        let end = self.drafts.get(node).map_or(end, |draft| draft.end);
        self.close_slot(stack, end);
        Ok(())
    }

    fn push(
        &mut self,
        parent: Option<usize>,
        kind: YamlNodeKind,
        key: bool,
        start: usize,
    ) -> usize {
        push(&mut self.drafts, parent, kind, key, start)
    }

    /// Extends a node and its ancestors to `end`.
    fn extend(&mut self, mut node: usize, end: usize) {
        while let Some(draft) = self.drafts.get_mut(node) {
            draft.end = draft.end.max(end);
            let Some(parent) = draft.parent else {
                return;
            };
            node = parent;
        }
    }

    /// The parent and key role of a node starting at `start`, opening its mapping entry
    /// or block sequence item.
    fn open_slot(
        &mut self,
        stack: &mut [Frame],
        entries: &mut impl Iterator<Item = (usize, usize)>,
        start: usize,
    ) -> Result<(usize, bool), LayoutUncertainty> {
        match stack.last_mut() {
            None => Err(LayoutUncertainty::Parse),
            Some(
                Frame::Document(node)
                | Frame::Sequence {
                    node, flow: true, ..
                },
            ) => Ok((*node, false)),
            Some(Frame::Mapping {
                entry: Some((pair, value)),
                ..
            }) => {
                *value = true;
                Ok((*pair, false))
            }
            Some(Frame::Mapping {
                node,
                flow,
                entry: entry @ None,
            }) => {
                let kind = if *flow {
                    YamlNodeKind::FlowPair
                } else {
                    YamlNodeKind::Pair
                };
                let pair = push(&mut self.drafts, Some(*node), kind, false, start);
                *entry = Some((pair, false));
                Ok((pair, true))
            }
            Some(Frame::Sequence {
                node,
                flow: false,
                item,
            }) => {
                // Each block item owns the next `-` indicator, in token order.
                let (dash, dash_end) = entries.next().ok_or(LayoutUncertainty::Parse)?;
                let index = push(
                    &mut self.drafts,
                    Some(*node),
                    YamlNodeKind::Item,
                    false,
                    dash,
                );
                *item = Some(index);
                self.extend(index, dash_end);
                Ok((index, false))
            }
        }
    }

    /// Completes the slot a node ending at `end` filled: an entry after its value, an item
    /// after its node.
    fn close_slot(&mut self, stack: &mut [Frame], end: usize) {
        let open = match stack.last_mut() {
            None | Some(Frame::Document(_)) => None,
            Some(Frame::Mapping { entry, .. }) => match *entry {
                Some((pair, true)) => {
                    *entry = None;
                    Some(pair)
                }
                Some((pair, false)) => Some(pair),
                None => None,
            },
            Some(Frame::Sequence { item, .. }) => item.take(),
        };
        if let Some(open) = open {
            self.extend(open, end);
        }
    }

    /// Adds a node for each comment between the document's tokens.
    ///
    /// Outside its tokens a document holds only blanks, line breaks and comments
    /// (`scannerc.go:1443`), so a `#` there starts a comment that runs to the line break.
    fn comments(&mut self, document: usize, spans: &[(usize, usize)], text: &str) {
        let Some(Draft { start, end, .. }) = self.drafts.get(document) else {
            return;
        };
        let (start, end) = (*start, *end);
        let mut spans = spans.to_vec();
        spans.sort_unstable();
        let mut gaps = Vec::new();
        let mut cursor = start;
        for (token_start, token_end) in spans {
            if token_start > cursor {
                gaps.push((cursor, token_start.min(end)));
            }
            cursor = cursor.max(token_end);
        }
        if cursor < end {
            gaps.push((cursor, end));
        }
        let mut comments = Vec::new();
        for (gap_start, gap_end) in gaps {
            let gap = text.get(gap_start..gap_end).unwrap_or_default();
            let mut offset = 0;
            while let Some(found) = gap.get(offset..).and_then(|rest| rest.find('#')) {
                let comment_start = offset + found;
                let length = gap
                    .get(comment_start..)
                    .unwrap_or_default()
                    .find(['\r', '\n', '\u{85}', '\u{2028}', '\u{2029}'])
                    .unwrap_or(gap.len() - comment_start);
                comments.push((
                    gap_start + comment_start,
                    gap_start + comment_start + length,
                ));
                offset = comment_start + length;
            }
        }
        // Containers were created at their starts, parents first, so one sweep keeps the
        // chain of those containing the current comment.
        let containers: Vec<usize> = (document + 1..self.drafts.len())
            .filter(|index| {
                self.drafts.get(*index).is_some_and(|draft| {
                    !matches!(
                        draft.kind,
                        YamlNodeKind::Scalar(_)
                            | YamlNodeKind::BlockScalar
                            | YamlNodeKind::Comment
                            | YamlNodeKind::DocumentEnd
                            | YamlNodeKind::Document
                    )
                })
            })
            .collect();
        let mut next = containers.iter().peekable();
        let mut open: Vec<(usize, usize)> = Vec::new();
        for (from, to) in comments {
            while let Some(&&index) = next.peek() {
                let Some(draft) = self.drafts.get(index) else {
                    break;
                };
                if draft.start > from {
                    break;
                }
                open.retain(|(_, end)| *end > draft.start);
                open.push((index, draft.end));
                next.next();
            }
            open.retain(|(_, end)| *end > from);
            let parent = open.last().map_or(document, |(index, _)| *index);
            let node = self.push(Some(parent), YamlNodeKind::Comment, false, from);
            if let Some(draft) = self.drafts.get_mut(node) {
                draft.end = to;
            }
        }
    }

    /// Emits nodes in document order, parents before children.
    fn flatten(
        &self,
        skeleton: &Skeleton,
        nodes: &mut Vec<YamlNode>,
        blocks: &mut Vec<BlockScalarOwnership>,
    ) -> Result<(), LayoutUncertainty> {
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); self.drafts.len()];
        let mut roots = Vec::new();
        for (index, draft) in self.drafts.iter().enumerate() {
            match draft.parent.and_then(|parent| children.get_mut(parent)) {
                Some(siblings) => siblings.push(index),
                None => roots.push(index),
            }
        }
        for siblings in &mut children {
            siblings.sort_by_key(|&index| self.drafts.get(index).map(|draft| draft.start));
        }
        let mut stack: Vec<(usize, Option<usize>)> =
            roots.into_iter().rev().map(|root| (root, None)).collect();
        while let Some((index, parent)) = stack.pop() {
            let Some(Draft {
                parent: _,
                kind,
                key,
                start,
                end,
            }) = self.drafts.get(index)
            else {
                continue;
            };
            let span = Span::new(*start, *end);
            let position = nodes.len();
            nodes.push(YamlNode {
                parent,
                kind: *kind,
                key: *key,
                span,
                pieces: covered_pieces(skeleton, span),
            });
            if *kind == YamlNodeKind::BlockScalar {
                let text = skeleton.text.get(span.start..span.end).unwrap_or_default();
                blocks.push(BlockScalarOwnership {
                    node: position,
                    header: block_header(text).ok_or(LayoutUncertainty::Parse)?,
                });
            }
            if let Some(siblings) = children.get(index) {
                stack.extend(siblings.iter().rev().map(|child| (*child, Some(position))));
            }
        }
        Ok(())
    }
}

/// The first and last arm-local pieces whose bytes intersect `span`.
fn covered_pieces(skeleton: &Skeleton, span: Span) -> Option<(usize, usize)> {
    let first = skeleton
        .segments
        .partition_point(|segment| segment.range.end <= span.start);
    let end = skeleton
        .segments
        .partition_point(|segment| segment.range.start < span.end);
    let covered = skeleton.segments.get(first..end)?;
    Some((covered.first()?.piece, covered.last()?.piece))
}

/// Decode a block scalar header: `|` or `>`, then at most one chomping
/// indicator and one indentation digit in either order, as YAML 1.2 §8.1.1
/// defines it. The header ends at the first blank, comment or line break.
fn block_header(text: &str) -> Option<BlockHeader> {
    let mut chars = text.chars();
    let folded = match chars.next()? {
        '|' => false,
        '>' => true,
        _ => return None,
    };
    let mut chomping = None;
    let mut indentation = None;
    for c in chars {
        match c {
            '-' | '+' if chomping.is_none() => {
                chomping = Some(if c == '-' {
                    Chomping::Strip
                } else {
                    Chomping::Keep
                });
            }
            '1'..='9' if indentation.is_none() => indentation = c.to_digit(10)?.try_into().ok(),
            ' ' | '\t' | '\r' | '\n' | '#' => break,
            _ => return None,
        }
    }
    Some(BlockHeader {
        folded,
        chomping: chomping.unwrap_or(Chomping::Clip),
        indentation,
    })
}

/// Whether a plain mapping key is, or a hole output can make it, the merge
/// key `<<` over a collection. Helm then merges the collection's entries
/// into the enclosing mapping; over a scalar or null it refuses the
/// document, so such a key cannot change a rendered layout.
fn may_form_merge_key(nodes: &[YamlNode], skeleton: &Skeleton) -> bool {
    nodes.iter().any(|key| {
        key.key
            && key.kind == YamlNodeKind::Scalar(ScalarStyle::Plain)
            && can_spell_merge_key(skeleton, key.span)
            && nodes
                .iter()
                .any(|value| value.parent == key.parent && !value.key && value.kind.is_collection())
    })
}

/// Whether the key at `span` is `<<`, or can become it when its holes
/// render: its literal bytes are at most two `<` and nothing else.
fn can_spell_merge_key(skeleton: &Skeleton, span: Span) -> bool {
    let first = skeleton
        .segments
        .partition_point(|segment| segment.range.end <= span.start);
    let segments = skeleton
        .segments
        .get(first..)
        .unwrap_or_default()
        .iter()
        .take_while(|segment| segment.range.start <= span.end);
    let mut literal = 0_usize;
    for segment in segments.clone() {
        let start = segment.range.start.max(span.start);
        let end = segment.range.end.min(span.end);
        if start < end && !segment.placeholder {
            let text = skeleton.text.get(start..end).unwrap_or_default();
            if text.bytes().any(|byte| byte != b'<') {
                return false;
            }
            literal += text.len();
        }
    }
    literal <= 2
        && (literal == 2
            || skeleton
                .segments
                .get(
                    skeleton
                        .segments
                        .partition_point(|segment| segment.range.start < span.start)..,
                )
                .unwrap_or_default()
                .iter()
                .take_while(|segment| segment.range.start < span.end)
                .any(|segment| segment.placeholder))
}

/// The structure a substitution must preserve: every node's kind, key role
/// and parent, except a placeholder-only scalar that is a mapping value or
/// a block sequence item (an empty output leaves its entry with a null
/// value). A flow sequence element or a key made of placeholder bytes only
/// is kept, so its disappearance changes the structure.
fn structure(owned: &YamlOwnership) -> Vec<(YamlNodeKind, bool, Option<usize>)> {
    let skeleton = &owned.skeleton;
    let may_vanish = |node: &YamlNode| {
        let parent = node
            .parent
            .and_then(|parent| owned.nodes.get(parent))
            .map(|parent| parent.kind);
        matches!(node.kind, YamlNodeKind::Scalar(_))
            && !node.key
            && matches!(
                parent,
                Some(YamlNodeKind::Pair | YamlNodeKind::FlowPair | YamlNodeKind::Item)
            )
            && skeleton.segments.iter().all(|segment| {
                segment.placeholder
                    || segment.range.end <= node.span.start
                    || node.span.end <= segment.range.start
            })
    };
    // Kept nodes renumbered, so a vanished leaf shifts no parent reference.
    let mut kept_index = Vec::with_capacity(owned.nodes.len());
    let mut kept = Vec::new();
    for node in &owned.nodes {
        if may_vanish(node) {
            kept_index.push(None);
            continue;
        }
        kept_index.push(Some(kept.len()));
        let parent = node
            .parent
            .and_then(|parent| kept_index.get(parent).copied().flatten());
        kept.push((node.kind, node.key, parent));
    }
    kept
}

#[cfg(test)]
#[path = "tests/yaml.rs"]
mod tests;
