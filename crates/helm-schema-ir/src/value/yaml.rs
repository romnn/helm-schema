//! YAML ownership and lexical coverage sealed from the executed output graph.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use helm_schema_core::{PlainScalarPlacement, Predicate, PredicateMemo};
use helm_schema_syntax::{
    ScalarStyle, Skeleton, SkeletonSegment, Span, YamlNodeKind, YamlOwnership,
};

use super::quoted::{self, LexicalState};
use super::scalar_value::{ScalarContent, ScalarRenderPart};
use super::{
    Coverage, Datum, Exit, FileOutput, Flow, FrozenTest, FrozenValue, Leaf, ObservationId,
    OutputNode, TextBuffer, TextPart, ValueNode,
};

// Helm 4.2.3 SplitManifests uses Go's ASCII Perl whitespace class.
static SPLITTER: std::sync::LazyLock<Result<regex::Regex, regex::Error>> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"(?:^|[ \t\r\n\x0c]*\n)---[ \t\r\n\x0c]*"));

type States = BTreeMap<LexicalState, Predicate>;

#[derive(Clone, Copy)]
enum Validity {
    Valid,
    Invalid,
    Unknown,
}

#[derive(Clone, Debug)]
struct NodeRef {
    buffer: Rc<TextBuffer>,
    index: usize,
    start: usize,
    indentation: usize,
    context: Rc<Vec<ObservationId>>,
}

impl NodeRef {
    fn node(&self) -> Option<&OutputNode> {
        self.buffer.parts.get(self.index)
    }
}

#[derive(Clone)]
enum Pending {
    Node(NodeRef),
    ChoiceEnd(usize),
}

struct SelectedSpan {
    range: Span,
    condition: Predicate,
}

#[derive(Debug)]
pub(super) struct Layout {
    pub(super) condition: Predicate,
    documents: Vec<DocumentLayout>,
    terms: Vec<NodeRef>,
    placeholders: usize,
    invalid: Predicate,
    source_mapped: bool,
    empty: bool,
}

#[derive(Debug)]
struct DocumentLayout {
    ownership: YamlOwnership,
    offset: usize,
    scope: Span,
    condition: Predicate,
    boundaries: Vec<PrefixBoundary>,
}

#[derive(Debug)]
struct PrefixBoundary {
    span: Span,
    preserved: Predicate,
}

pub(crate) struct PlacedDocument<'a> {
    pub(crate) condition: Predicate,
    pub(crate) ownership: &'a YamlOwnership,
    layout: &'a Layout,
    document: &'a DocumentLayout,
}

pub(crate) struct PlacedTerm<'a> {
    pub(crate) range: Span,
    pub(crate) placeholder: bool,
    pub(crate) source_offset: Option<usize>,
    pub(crate) value: Option<&'a FrozenValue>,
    node: &'a NodeRef,
}

impl PlacedTerm<'_> {
    pub(crate) fn occurrences(&self) -> impl Iterator<Item = ObservationId> + '_ {
        self.node
            .node()
            .into_iter()
            .flat_map(|node| node.occurrences.iter().copied())
            .chain(self.context())
    }
    pub(crate) fn context(&self) -> impl Iterator<Item = ObservationId> + '_ {
        self.node.context.iter().copied()
    }
    pub(crate) fn inputs(&self) -> BTreeSet<helm_schema_core::ValuesPath> {
        match self.node.node().map(OutputNode::content) {
            Some(TextPart::Frozen(value)) => value.inputs(),
            Some(TextPart::Unknown(inputs)) => inputs.clone(),
            Some(
                TextPart::Bytes(_)
                | TextPart::Select {
                    decision: _,
                    yes: _,
                    no: _,
                },
            )
            | None => BTreeSet::new(),
        }
    }
}

impl PlacedDocument<'_> {
    pub(crate) fn block_value(&self, span: Span) -> bool {
        block_value(self.ownership, span)
    }
    pub(crate) fn terms(&self, span: Span) -> impl Iterator<Item = PlacedTerm<'_>> {
        self.ownership
            .skeleton
            .segments
            .iter()
            .filter_map(move |segment| {
                let SkeletonSegment {
                    range,
                    piece,
                    placeholder,
                } = *segment;
                if range.start >= span.end || span.start >= range.end {
                    return None;
                }
                let node = self.layout.terms.get(piece)?;
                let value = match node.node()?.content() {
                    TextPart::Frozen(value) => Some(value),
                    TextPart::Bytes(_)
                    | TextPart::Unknown(_)
                    | TextPart::Select {
                        decision: _,
                        yes: _,
                        no: _,
                    } => None,
                };
                Some(PlacedTerm {
                    range,
                    placeholder,
                    source_offset: (self.layout.source_mapped
                        && node.indentation == 0
                        && !placeholder)
                        .then(|| (range.start + self.document.offset).checked_sub(node.start))
                        .flatten(),
                    value,
                    node,
                })
            })
    }

    pub(crate) fn placement_guard(&self, span: Span) -> Predicate {
        let mut consumed = 0;
        let mut guard = Predicate::True;
        for boundary in &self.document.boundaries {
            if boundary.span == span {
                return if self.ownership.nodes.iter().any(|node| {
                    node.span == span && node.kind == YamlNodeKind::Scalar(ScalarStyle::Plain)
                }) {
                    guard
                } else {
                    boundary.preserved.clone()
                };
            }
            if boundary.span.end > span.start {
                break;
            }
            consumed = boundary.span.end;
            guard = boundary.preserved.clone();
        }
        if self.ownership.skeleton.segments.iter().any(|segment| {
            segment.placeholder
                && consumed <= segment.range.start
                && segment.range.start < span.start
        }) {
            Predicate::False
        } else {
            guard
        }
    }
}

#[derive(Debug)]
pub(super) struct OutputPlacement {
    graph: Rc<TextBuffer>,
    layouts: Vec<Layout>,
    unknown: Predicate,
    markers: MarkerProvenance,
}

#[derive(Debug, Default)]
struct MarkerProvenance {
    affected: BTreeSet<ObservationId>,
    incomplete: bool,
}

impl OutputPlacement {
    fn new(
        graph: Rc<TextBuffer>,
        condition: Predicate,
        memo: &PredicateMemo,
        output: FileOutput,
    ) -> Self {
        let mut build = LayoutBuilder {
            skeleton: Skeleton {
                text: String::new(),
                segments: Vec::new(),
            },
            terms: Vec::new(),
            layouts: Vec::new(),
            unknown: Vec::new(),
            remaining: 1024,
            lexical: LexicalTransfers::default(),
            selections: Vec::new(),
            root_condition: condition.clone(),
            output,
            markers: MarkerProvenance::default(),
            memo,
        };
        build.walk(
            vec![Pending::Node(NodeRef {
                buffer: Rc::clone(&graph),
                index: 0,
                start: 0,
                indentation: 0,
                context: Rc::new(Vec::new()),
            })],
            condition,
        );
        build.markers.incomplete |= !build.unknown.is_empty();
        Self {
            graph,
            layouts: build.layouts,
            unknown: memo.normalize(Predicate::Or(build.unknown)),
            markers: build.markers,
        }
    }

    fn invalid(&self, memo: &PredicateMemo) -> Predicate {
        let mut failures = Vec::new();
        let mut survivors = vec![self.unknown.clone()];
        for Layout {
            condition,
            documents: _,
            terms: _,
            placeholders: _,
            source_mapped: _,
            empty,
            invalid,
        } in &self.layouts
        {
            let remainder = if *empty {
                Validity::Valid
            } else {
                Validity::Unknown
            };
            for (guard, validity) in [
                (invalid.clone(), Validity::Invalid),
                (invalid.negated(), remainder),
            ] {
                let guard = Predicate::all(vec![condition.clone(), guard]);
                match validity {
                    Validity::Invalid => failures.push(guard),
                    Validity::Valid | Validity::Unknown => survivors.push(guard),
                }
            }
        }
        memo.normalize(Predicate::all(vec![
            Predicate::Or(failures),
            Predicate::Or(survivors).negated(),
        ]))
    }

    fn plain_placement(
        &self,
        site: &super::ExecutedSite,
        memo: &PredicateMemo,
    ) -> PlainScalarPlacement {
        if !memo.exactly_implies(&site.condition, &self.unknown.negated()) {
            return PlainScalarPlacement::Unknown;
        }
        let id = ObservationId {
            program: site.program,
            ordinal: site.ordinal,
            span: site.span,
        };
        let mut found = false;
        for layout in &self.layouts {
            if memo.exactly_implies(&site.condition, &layout.condition.negated()) {
                continue;
            }
            if layout.placeholders != 1 || !layout.block_value(id) {
                return PlainScalarPlacement::Unknown;
            }
            found = true;
        }
        if found {
            PlainScalarPlacement::BlockValue
        } else {
            PlainScalarPlacement::Unknown
        }
    }
}

impl Layout {
    fn block_value(&self, id: ObservationId) -> bool {
        for document in &self.documents {
            let document = &document.ownership;
            for segment in &document.skeleton.segments {
                if !segment.placeholder
                    || !self
                        .terms
                        .get(segment.piece)
                        .and_then(NodeRef::node)
                        .is_some_and(|node| node.occurrences.contains(&id))
                {
                    continue;
                }
                if block_value(document, segment.range) {
                    return true;
                }
            }
        }
        false
    }
}

fn block_value(document: &YamlOwnership, span: Span) -> bool {
    let Some(node) = document.nodes.iter().find(|node| {
        node.span == span && !node.key && node.kind == YamlNodeKind::Scalar(ScalarStyle::Plain)
    }) else {
        return false;
    };
    let Some(parent) = node.parent.and_then(|parent| document.nodes.get(parent)) else {
        return false;
    };
    if !matches!(parent.kind, YamlNodeKind::Pair | YamlNodeKind::Item) {
        return false;
    }
    let mut ancestor = Some(parent);
    while let Some(node) = ancestor {
        if matches!(
            node.kind,
            YamlNodeKind::FlowMapping | YamlNodeKind::FlowPair | YamlNodeKind::FlowSequence
        ) {
            return false;
        }
        ancestor = node.parent.and_then(|parent| document.nodes.get(parent));
    }
    true
}

#[derive(Debug)]
pub(super) struct FilePlacement {
    outputs: Vec<OutputPlacement>,
    pub(super) abort: Predicate,
}

impl FilePlacement {
    pub(super) fn documents<'a>(
        &'a self,
        roots: &'a BTreeSet<u64>,
    ) -> impl Iterator<Item = PlacedDocument<'a>> {
        self.outputs
            .iter()
            .flat_map(move |output| {
                output.layouts.iter().filter(move |layout| {
                    layout.terms.iter().any(|term| {
                        term.context
                            .iter()
                            .chain(term.node().into_iter().flat_map(|node| &node.occurrences))
                            .any(|id| roots.contains(&id.program))
                    })
                })
            })
            .flat_map(|layout| {
                layout.documents.iter().map(move |document| PlacedDocument {
                    condition: document.condition.clone(),
                    ownership: &document.ownership,
                    layout,
                    document,
                })
            })
    }

    pub(super) fn inputs(&self) -> BTreeSet<helm_schema_core::ValuesPath> {
        self.outputs
            .iter()
            .flat_map(|output| output.graph.inputs())
            .collect()
    }
    #[tracing::instrument(skip_all)]
    pub(super) fn seal(exits: &[Exit], output: FileOutput) -> Self {
        let mut outputs = Vec::new();
        let mut failures = Vec::new();
        let mut survivors = Vec::new();
        for exit in exits {
            let condition = exit.state.path_predicate();
            let memo = &exit.state.heap.arena.predicate_memo;
            let graph = Rc::new(exit.state.output.clone());
            let placement = if matches!(exit.flow, Flow::Normal(_)) {
                OutputPlacement::new(graph, condition.clone(), memo, output)
            } else {
                OutputPlacement {
                    graph,
                    layouts: Vec::new(),
                    unknown: Predicate::True,
                    markers: MarkerProvenance::default(),
                }
            };
            if super::output::exit_may_continue(exit) {
                let invalid = if exit
                    .state
                    .active
                    .iter()
                    .any(|edge| edge.test.coverage == Coverage::Partial)
                {
                    Predicate::False
                } else {
                    placement.invalid(memo)
                };
                failures.push(Predicate::all(vec![condition.clone(), invalid.clone()]));
                survivors.push(Predicate::all(vec![condition, invalid.negated()]));
            } else {
                failures.push(condition);
            }
            outputs.push(placement);
        }
        Self {
            outputs,
            abort: Predicate::all(vec![
                Predicate::Or(failures),
                Predicate::Or(survivors).negated(),
            ])
            .normalize_boolean(),
        }
    }

    pub(super) fn certify_sites(&self, exits: &mut [Exit]) {
        for (exit, placement) in exits.iter_mut().zip(&self.outputs) {
            if !matches!(exit.flow, Flow::Normal(_)) {
                continue;
            }
            let state = &mut exit.state;
            for site in &mut state.text_sites {
                let id = ObservationId {
                    program: site.program,
                    ordinal: site.ordinal,
                    span: site.span,
                };
                if placement.markers.affected.contains(&id)
                    || (placement.markers.incomplete && may_be_stripped(&site.bytes))
                {
                    site.rendered_unchanged = false;
                }
            }
            for site in &mut state.sites {
                let id = ObservationId {
                    program: site.program,
                    ordinal: site.ordinal,
                    span: site.span,
                };
                if site.rendered
                    && (placement.markers.affected.contains(&id)
                        || (placement.markers.incomplete
                            && site
                                .value
                                .exact_text()
                                .is_none_or(|text| may_be_stripped(&text))))
                {
                    site.rendered_unchanged = false;
                }
                site.plain_scalar_placement = if !state.output_widened
                    && site.program == state.program
                    && site.rendered
                    && site.coverage_exact
                    && !site.condition.contains_approximation()
                {
                    placement.plain_placement(site, &state.heap.arena.predicate_memo)
                } else {
                    PlainScalarPlacement::Unknown
                };
            }
        }
    }
}

struct LayoutBuilder<'a> {
    skeleton: Skeleton,
    terms: Vec<NodeRef>,
    layouts: Vec<Layout>,
    unknown: Vec<Predicate>,
    remaining: usize,
    lexical: LexicalTransfers,
    selections: Vec<SelectedSpan>,
    root_condition: Predicate,
    output: FileOutput,
    markers: MarkerProvenance,
    memo: &'a PredicateMemo,
}

impl LayoutBuilder<'_> {
    fn walk(&mut self, mut pending: Vec<Pending>, condition: Predicate) {
        if condition == Predicate::False {
            return;
        }
        while let Some(next) = pending.pop() {
            let node = match next {
                Pending::Node(node) => node,
                Pending::ChoiceEnd(index) => {
                    if let Some(selection) = self.selections.get_mut(index) {
                        selection.range.end = self.skeleton.text.len();
                    }
                    continue;
                }
            };
            let Some(part) = node.node().map(OutputNode::content) else {
                continue;
            };
            if self.remaining == 0 || self.layouts.len() >= 32 {
                self.unknown.push(condition);
                return;
            }
            self.remaining -= 1;
            pending.push(Pending::Node(NodeRef {
                buffer: Rc::clone(&node.buffer),
                index: node.index + 1,
                start: 0,
                indentation: node.indentation,
                context: Rc::clone(&node.context),
            }));
            if let TextPart::Select { decision, yes, no } = part {
                self.choices(decision, [yes, no], &node, &pending, &condition);
                return;
            }
            if let TextPart::Frozen(value) = part
                && let Some(capture) = captured_layout(value, &mut 128)
            {
                if !self.append(&capture.prefix, false, node.clone()) {
                    self.unknown.push(condition);
                    return;
                }
                pending.push(Pending::Node(NodeRef {
                    buffer: capture.graph,
                    index: 0,
                    start: 0,
                    indentation: node.indentation.saturating_add(capture.indentation),
                    context: Rc::new(
                        node.context
                            .iter()
                            .copied()
                            .chain(
                                node.node()
                                    .into_iter()
                                    .flat_map(|node| node.occurrences.iter().copied()),
                            )
                            .collect(),
                    ),
                }));
                continue;
            }
            let text = match part {
                TextPart::Bytes(bytes) => Some(bytes.clone()),
                TextPart::Frozen(value) => value.exact_text(),
                TextPart::Unknown(_)
                | TextPart::Select {
                    decision: _,
                    yes: _,
                    no: _,
                } => None,
            };
            let bytes = text.as_deref().unwrap_or("00000000");
            if !self.append(bytes, text.is_none(), node) {
                self.unknown.push(condition);
                return;
            }
        }
        self.finish_layout(condition);
    }

    fn choices(
        &mut self,
        decision: &FrozenTest,
        branches: [&Rc<TextBuffer>; 2],
        node: &NodeRef,
        pending: &[Pending],
        condition: &Predicate,
    ) {
        let [yes, no] = branches;
        let FrozenTest {
            id,
            when_true,
            when_false,
            coverage,
            runtime: _,
        } = decision;
        let exact = *coverage == Coverage::Exact
            && !when_true.contains_approximation()
            && !when_false.contains_approximation();
        let bytes = self.skeleton.text.len();
        let segments = self.skeleton.segments.len();
        let terms = self.terms.len();
        let choices = self.selections.len();
        for (taken, branch, guard) in [(true, yes, when_true), (false, no, when_false)] {
            let guard = if exact {
                guard.clone()
            } else {
                Predicate::approximate_output_selection(
                    format!("executed output {id} {taken}"),
                    BTreeSet::new(),
                    Predicate::False,
                )
            };
            let mut continuation = pending.to_vec();
            self.selections.push(SelectedSpan {
                range: Span::new(bytes, bytes),
                condition: guard.clone(),
            });
            continuation.push(Pending::ChoiceEnd(choices));
            continuation.push(Pending::Node(NodeRef {
                buffer: Rc::clone(branch),
                index: 0,
                start: 0,
                indentation: node.indentation,
                context: Rc::clone(&node.context),
            }));
            self.walk(
                continuation,
                self.memo.normalize(Predicate::all(vec![
                    condition.clone(),
                    if exact { guard } else { Predicate::True },
                ])),
            );
            self.skeleton.text.truncate(bytes);
            self.skeleton.segments.truncate(segments);
            self.terms.truncate(terms);
            self.selections.truncate(choices);
        }
    }

    fn finish_layout(&mut self, condition: Predicate) {
        let removed = marker_ranges(&self.skeleton.text);
        self.markers.record(&self.skeleton, &self.terms, &removed);
        let owned_documents = match self.output {
            FileOutput::Text => Vec::new(),
            FileOutput::Manifest => {
                let Some(documents) = documents(&self.skeleton, &removed) else {
                    self.unknown.push(condition);
                    return;
                };
                documents
            }
        };
        let mut failures = Vec::new();
        let mut documents = Vec::new();
        for mut document in owned_documents {
            document.condition = self.memo.normalize(Predicate::all(
                std::iter::once(self.root_condition.clone())
                    .chain(
                        self.selections
                            .iter()
                            .filter(|selection| {
                                !removed.is_empty()
                                    || (selection.range.start <= document.scope.end
                                        && document.scope.start <= selection.range.end)
                            })
                            .map(|selection| selection.condition.clone()),
                    )
                    .collect(),
            ));
            let result = quoted_tokens(
                &document.ownership,
                &self.terms,
                &mut self.lexical,
                self.memo,
            );
            failures.push(result.invalid);
            document.boundaries = result.boundaries;
            documents.push(document);
        }
        self.layouts.push(Layout {
            condition,
            documents,
            terms: self.terms.clone(),
            placeholders: self
                .skeleton
                .segments
                .iter()
                .filter(|segment| segment.placeholder)
                .count(),
            invalid: self.memo.normalize(Predicate::Or(failures)),
            source_mapped: removed.is_empty(),
            empty: self.skeleton.text.trim().is_empty(),
        });
    }

    fn append(&mut self, bytes: &str, placeholder: bool, mut node: NodeRef) -> bool {
        let start = self.skeleton.text.len();
        let expanded = bytes.len().saturating_add(
            bytes
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                .saturating_mul(node.indentation),
        );
        if start.saturating_add(expanded) > 64 * 1024 {
            return false;
        }
        let text = if node.indentation == 0 {
            std::borrow::Cow::Borrowed(bytes)
        } else {
            std::borrow::Cow::Owned(
                bytes.replace('\n', &format!("\n{}", " ".repeat(node.indentation))),
            )
        };
        self.skeleton.text.push_str(&text);
        self.skeleton.segments.push(SkeletonSegment {
            range: Span::new(start, self.skeleton.text.len()),
            piece: self.terms.len(),
            placeholder,
        });
        node.start = start;
        self.terms.push(node);
        true
    }
}

struct CapturedLayout {
    graph: Rc<TextBuffer>,
    prefix: String,
    indentation: usize,
}

fn captured_layout(value: &FrozenValue, remaining: &mut usize) -> Option<CapturedLayout> {
    *remaining = remaining.checked_sub(1)?;
    let text = match value.value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Text(text),
            origin: _,
        }) => text,
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Scalar {
                    go_type: _,
                    content: _,
                }
                | Datum::Map { view: _, target: _ }
                | Datum::Sequence(_)
                | Datum::Strings(_)
                | Datum::Slot {
                    backing: _,
                    index: _,
                }
                | Datum::Unknown(_),
            origin: _,
        })
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => return None,
    };
    match text.0.as_ref() {
        super::text::TextTerm::Capture {
            graph,
            strip_marker: false,
        } => Some(CapturedLayout {
            graph: Rc::clone(graph),
            prefix: String::new(),
            indentation: 0,
        }),
        super::text::TextTerm::Indent {
            value,
            width,
            newline,
        } => {
            let mut capture = captured_layout(value, remaining)?;
            capture.prefix = super::text::indent(&capture.prefix, *width, *newline);
            capture.indentation = capture.indentation.checked_add(*width)?;
            (capture.prefix.len() <= 64 * 1024 && capture.indentation <= 64 * 1024)
                .then_some(capture)
        }
        super::text::TextTerm::Capture {
            graph: _,
            strip_marker: true,
        }
        | super::text::TextTerm::Quote { value: _ }
        | super::text::TextTerm::Yaml {
            value: _,
            pretty: _,
        }
        | super::text::TextTerm::Encoded {
            value: _,
            alphabet: _,
        } => None,
    }
}

fn documents(skeleton: &Skeleton, removed: &[Span]) -> Option<Vec<DocumentLayout>> {
    let skeleton = stripped_skeleton(skeleton, removed);
    let Ok(splitter) = &*SPLITTER else {
        return None;
    };
    let mut ranges = Vec::new();
    let mut start = 0;
    let trimmed = skeleton.text.trim_start_matches(char::is_whitespace);
    let trim = skeleton.text.len() - trimmed.len();
    for separator in splitter.find_iter(trimmed) {
        ranges.push(Span::new(start, trim + separator.start()));
        start = trim + separator.end();
    }
    ranges.push(Span::new(start, skeleton.text.len()));
    let mut documents = Vec::new();
    for scope @ Span { start, end } in ranges {
        let text = skeleton
            .text
            .get(start..end)?
            .trim_start_matches(char::is_whitespace);
        let start = end - text.len();
        if text.is_empty() {
            continue;
        }
        let document = Skeleton {
            text: text.to_owned(),
            segments: skeleton
                .segments
                .iter()
                .filter_map(|segment| {
                    let SkeletonSegment {
                        range,
                        piece,
                        placeholder,
                    } = *segment;
                    let from = range.start.max(start);
                    let to = range.end.min(end);
                    (from < to).then(|| SkeletonSegment {
                        range: Span::new(from - start, to - start),
                        piece,
                        placeholder,
                    })
                })
                .collect(),
        };
        let Ok(ownership) = helm_schema_syntax::first_document_ownership(document) else {
            continue;
        };
        documents.push(DocumentLayout {
            ownership,
            offset: start,
            scope,
            condition: Predicate::True,
            boundaries: Vec::new(),
        });
    }
    Some(documents)
}

const REMOVED_MARKER: &str = "<no value>";

fn marker_ranges(text: &str) -> Vec<Span> {
    text.match_indices(REMOVED_MARKER)
        .map(|(start, marker)| Span::new(start, start + marker.len()))
        .collect()
}

impl MarkerProvenance {
    fn record(&mut self, skeleton: &Skeleton, terms: &[NodeRef], removed: &[Span]) {
        let mut affected = BTreeSet::new();
        let mut mark = |span: Span| {
            let start = skeleton
                .segments
                .partition_point(|segment| segment.range.end <= span.start);
            affected.extend(
                skeleton
                    .segments
                    .get(start..)
                    .into_iter()
                    .flatten()
                    .take_while(|segment| segment.range.start < span.end)
                    .map(|segment| segment.piece),
            );
        };
        for span in removed {
            mark(*span);
        }
        for segment in skeleton
            .segments
            .iter()
            .filter(|segment| segment.placeholder)
        {
            mark(segment.range);
            let before = &skeleton.text[..segment.range.start];
            let after = &skeleton.text[segment.range.end..];
            for length in 1..REMOVED_MARKER.len() {
                if before.ends_with(&REMOVED_MARKER[..length]) {
                    mark(Span::new(segment.range.start - length, segment.range.start));
                }
                if after.starts_with(&REMOVED_MARKER[REMOVED_MARKER.len() - length..]) {
                    mark(Span::new(segment.range.end, segment.range.end + length));
                }
            }
        }
        for index in affected {
            let Some(term) = terms.get(index) else {
                self.incomplete = true;
                continue;
            };
            let ids = term
                .node()
                .into_iter()
                .flat_map(|node| node.occurrences.iter())
                .chain(term.context.iter())
                .copied()
                .collect::<Vec<_>>();
            self.incomplete |= ids.is_empty();
            self.affected.extend(ids);
        }
    }
}

fn may_be_stripped(text: &str) -> bool {
    text.contains(REMOVED_MARKER)
        || (!text.is_empty() && REMOVED_MARKER.contains(text))
        || (1..REMOVED_MARKER.len()).any(|length| {
            text.ends_with(&REMOVED_MARKER[..length])
                || text.starts_with(&REMOVED_MARKER[REMOVED_MARKER.len() - length..])
        })
}

fn stripped_skeleton(skeleton: &Skeleton, removed: &[Span]) -> Skeleton {
    let translate = |position: usize| {
        position
            - removed
                .iter()
                .map(|range| position.min(range.end).saturating_sub(range.start))
                .sum::<usize>()
    };
    let mut text = String::with_capacity(skeleton.text.len());
    let mut start = 0;
    for range in removed {
        text.push_str(&skeleton.text[start..range.start]);
        start = range.end;
    }
    text.push_str(&skeleton.text[start..]);
    Skeleton {
        text,
        segments: skeleton
            .segments
            .iter()
            .filter_map(|segment| {
                let SkeletonSegment {
                    range,
                    piece,
                    placeholder,
                } = *segment;
                let start = translate(range.start);
                let end = translate(range.end);
                (start < end).then_some(SkeletonSegment {
                    range: Span::new(start, end),
                    piece,
                    placeholder,
                })
            })
            .collect(),
    }
}

fn rendered_parts(parts: &[ScalarRenderPart], mut states: States, memo: &PredicateMemo) -> States {
    for part in parts {
        states = match part {
            ScalarRenderPart::Text(text) => quoted::literal(states, text, memo),
            ScalarRenderPart::Identity {
                path,
                stringified: _,
                lexical_escapes,
            } if lexical_escapes.is_empty() => quoted::input(states, path, memo),
            ScalarRenderPart::Identity {
                path: _,
                stringified: _,
                lexical_escapes: _,
            } => states
                .into_iter()
                .filter(|(state, _)| *state == LexicalState::Invalid)
                .collect(),
        };
    }
    states
}

struct LexicalTransfers {
    values: BTreeMap<(*const super::HeapVersion, *const ValueNode, LexicalState), States>,
    buffers: BTreeMap<(*const TextBuffer, LexicalState), States>,
    remaining: usize,
}

impl Default for LexicalTransfers {
    fn default() -> Self {
        Self {
            values: BTreeMap::new(),
            buffers: BTreeMap::new(),
            remaining: 4096,
        }
    }
}

impl LexicalTransfers {
    fn value(&mut self, value: &FrozenValue, state: LexicalState, memo: &PredicateMemo) -> States {
        let FrozenValue {
            heap,
            value: super::Value { node },
        } = value;
        let key = (Rc::as_ptr(heap), Rc::as_ptr(node), state);
        if let Some(result) = self.values.get(&key) {
            return result.clone();
        }
        if self.remaining == 0 && state != LexicalState::Invalid {
            return States::new();
        }
        self.remaining = self.remaining.saturating_sub(1);
        let initial = BTreeMap::from([(state, Predicate::True)]);
        let result = if state == LexicalState::Invalid {
            initial
        } else if let Some(path) = value.identity() {
            quoted::input(initial, path, memo)
        } else if let Some(text) = value.exact_text() {
            quoted::literal(initial, &text, memo)
        } else {
            match value.value.node.as_ref() {
                ValueNode::Leaf(Leaf {
                    datum: Datum::Text(text),
                    origin: _,
                }) => self.text(text, state, memo),
                ValueNode::Select { decision, yes, no } => {
                    let FrozenTest {
                        id: _,
                        when_true,
                        when_false,
                        coverage,
                        runtime: _,
                    } = decision.as_ref();
                    let mut result = States::new();
                    if *coverage == Coverage::Exact
                        && !when_true.contains_approximation()
                        && !when_false.contains_approximation()
                    {
                        for (branch, guard) in [(yes, when_true), (no, when_false)] {
                            for (end, condition) in self.value(
                                &FrozenValue {
                                    heap: Rc::clone(&value.heap),
                                    value: branch.clone(),
                                },
                                state,
                                memo,
                            ) {
                                quoted::merge(
                                    &mut result,
                                    end,
                                    Predicate::all(vec![guard.clone(), condition]),
                                    memo,
                                );
                            }
                        }
                    }
                    result
                }
                ValueNode::Leaf(Leaf {
                    datum:
                        Datum::Scalar {
                            content: Some(ScalarContent::Rendered(parts)),
                            go_type: _,
                        },
                    origin: _,
                }) => rendered_parts(parts, initial, memo),
                ValueNode::Leaf(Leaf {
                    datum:
                        Datum::Scalar {
                            go_type: _,
                            content: _,
                        }
                        | Datum::Map { view: _, target: _ }
                        | Datum::Sequence(_)
                        | Datum::Strings(_)
                        | Datum::Slot {
                            backing: _,
                            index: _,
                        }
                        | Datum::Unknown(_),
                    origin: _,
                }) => States::new(),
            }
        };
        self.values.insert(key, result.clone());
        result
    }

    fn text(
        &mut self,
        text: &super::text::FrozenText,
        state: LexicalState,
        memo: &PredicateMemo,
    ) -> States {
        let initial = BTreeMap::from([(state, Predicate::True)]);
        match text.0.as_ref() {
            super::text::TextTerm::Capture {
                graph,
                strip_marker: _,
            } => {
                // Retained paths contain no marker start before an irreparable invalid prefix.
                self.buffer(graph, state, memo)
            }
            super::text::TextTerm::Encoded { value: _, alphabet } => match alphabet {
                crate::function_semantics::OutputAlphabet::Hexadecimal
                | crate::function_semantics::OutputAlphabet::Decimal
                | crate::function_semantics::OutputAlphabet::Base64 => {
                    if matches!(state, LexicalState::Double | LexicalState::Single) {
                        initial
                    } else {
                        States::new()
                    }
                }
            },
            super::text::TextTerm::Quote { value } => {
                let nil = super::kinds::nilness(&value.value)
                    .map(|nil| {
                        if nil {
                            Predicate::True
                        } else {
                            Predicate::False
                        }
                    })
                    .or_else(|| value.identity().map(super::kinds::input_nil_predicate));
                let mut result = States::new();
                if let Some(nil) = nil {
                    quoted::merge(&mut result, state, nil.clone(), memo);
                    if state == LexicalState::PlainStart {
                        quoted::merge(&mut result, LexicalState::Closed, nil.negated(), memo);
                    }
                }
                result
            }
            super::text::TextTerm::Indent {
                value,
                width,
                newline,
            } => {
                let prefix = super::text::indent("", *width, *newline);
                let mut result = States::new();
                for (state, condition) in quoted::literal(initial, &prefix, memo) {
                    for (end, guard) in self.value(value, state, memo) {
                        quoted::merge(
                            &mut result,
                            end,
                            Predicate::all(vec![condition.clone(), guard]),
                            memo,
                        );
                    }
                }
                result
            }
            super::text::TextTerm::Yaml {
                value: _,
                pretty: _,
            } => States::new(),
        }
    }

    fn token(
        &mut self,
        owned: &YamlOwnership,
        node: &helm_schema_syntax::YamlNode,
        terms: &[NodeRef],
        memo: &PredicateMemo,
    ) -> States {
        let style = match node.kind {
            YamlNodeKind::Scalar(style) => style,
            YamlNodeKind::Document
            | YamlNodeKind::DocumentEnd
            | YamlNodeKind::Mapping
            | YamlNodeKind::Pair
            | YamlNodeKind::Sequence
            | YamlNodeKind::Item
            | YamlNodeKind::FlowMapping
            | YamlNodeKind::FlowPair
            | YamlNodeKind::FlowSequence
            | YamlNodeKind::BlockScalar
            | YamlNodeKind::Comment => return States::new(),
        };
        let (state, skip) = match style {
            ScalarStyle::DoubleQuoted => (LexicalState::Double, 1),
            ScalarStyle::SingleQuoted => (LexicalState::Single, 1),
            ScalarStyle::Plain => (LexicalState::PlainStart, 0),
        };
        let mut states = BTreeMap::from([(state, Predicate::True)]);
        for segment in &owned.skeleton.segments {
            let start = segment.range.start.max(node.span.start + skip);
            let end = segment.range.end.min(node.span.end);
            if start >= end {
                continue;
            }
            states = if segment.placeholder {
                terms
                    .get(segment.piece)
                    .map_or_else(States::new, |term| self.apply(term, states.clone(), memo))
            } else {
                owned
                    .skeleton
                    .text
                    .get(start..end)
                    .map_or_else(States::new, |text| {
                        quoted::literal(states.clone(), text, memo)
                    })
            };
        }
        states
    }

    fn apply(&mut self, term: &NodeRef, states: States, memo: &PredicateMemo) -> States {
        let Some(node) = term.node() else {
            return States::new();
        };
        let mut result = States::new();
        for (state, condition) in states {
            let ends = self.part(node.content(), state, memo);
            for (end, guard) in ends {
                quoted::merge(
                    &mut result,
                    end,
                    Predicate::all(vec![condition.clone(), guard]),
                    memo,
                );
            }
        }
        result
    }

    fn buffer(&mut self, buffer: &TextBuffer, state: LexicalState, memo: &PredicateMemo) -> States {
        let key = (std::ptr::from_ref(buffer), state);
        if let Some(result) = self.buffers.get(&key) {
            return result.clone();
        }
        let mut states = BTreeMap::from([(state, Predicate::True)]);
        for node in &buffer.parts {
            if self.remaining == 0 {
                states.retain(|state, _| *state == LexicalState::Invalid);
                break;
            }
            self.remaining -= 1;
            let mut next = States::new();
            for (state, condition) in states {
                for (end, guard) in self.part(node.content(), state, memo) {
                    quoted::merge(
                        &mut next,
                        end,
                        Predicate::all(vec![condition.clone(), guard]),
                        memo,
                    );
                }
            }
            states = next;
        }
        self.buffers.insert(key, states.clone());
        states
    }

    fn part(&mut self, part: &TextPart, state: LexicalState, memo: &PredicateMemo) -> States {
        if state == LexicalState::Invalid {
            return BTreeMap::from([(state, Predicate::True)]);
        }
        match part {
            TextPart::Frozen(value) => self.value(value, state, memo),
            TextPart::Bytes(text) => {
                quoted::literal(BTreeMap::from([(state, Predicate::True)]), text, memo)
            }
            TextPart::Unknown(_) => States::new(),
            TextPart::Select { decision, yes, no } => {
                let FrozenTest {
                    id: _,
                    when_true,
                    when_false,
                    coverage,
                    runtime: _,
                } = decision.as_ref();
                let mut result = States::new();
                if *coverage == Coverage::Exact
                    && !when_true.contains_approximation()
                    && !when_false.contains_approximation()
                {
                    for (branch, guard) in [(yes, when_true), (no, when_false)] {
                        for (end, condition) in self.buffer(branch, state, memo) {
                            quoted::merge(
                                &mut result,
                                end,
                                Predicate::all(vec![guard.clone(), condition]),
                                memo,
                            );
                        }
                    }
                }
                result
            }
        }
    }
}

struct QuotedPrefix {
    invalid: Predicate,
    boundaries: Vec<PrefixBoundary>,
}

fn quoted_tokens(
    owned: &YamlOwnership,
    terms: &[NodeRef],
    lexical: &mut LexicalTransfers,
    memo: &PredicateMemo,
) -> QuotedPrefix {
    if owned.skeleton.text.contains('<') {
        return QuotedPrefix {
            invalid: Predicate::False,
            boundaries: Vec::new(),
        };
    }
    let mut prefix = Predicate::True;
    let mut failures = Vec::new();
    let mut consumed = 0;
    let mut boundaries = Vec::new();
    for node in &owned.nodes {
        let style = match node.kind {
            YamlNodeKind::Scalar(style) => style,
            YamlNodeKind::Document
            | YamlNodeKind::DocumentEnd
            | YamlNodeKind::Mapping
            | YamlNodeKind::Pair
            | YamlNodeKind::Sequence
            | YamlNodeKind::Item
            | YamlNodeKind::FlowMapping
            | YamlNodeKind::FlowPair
            | YamlNodeKind::FlowSequence
            | YamlNodeKind::BlockScalar
            | YamlNodeKind::Comment => continue,
        };
        if style == ScalarStyle::Plain
            && !owned.skeleton.segments.iter().any(|segment| {
                segment.placeholder
                    && segment.range.start < node.span.end
                    && node.span.start < segment.range.end
            })
        {
            continue;
        }
        if owned.skeleton.segments.iter().any(|segment| {
            segment.placeholder
                && consumed <= segment.range.start
                && segment.range.start < node.span.start
        }) {
            break;
        }
        let closed = match style {
            ScalarStyle::DoubleQuoted => LexicalState::Closed,
            ScalarStyle::SingleQuoted => LexicalState::SingleEnd,
            ScalarStyle::Plain => LexicalState::Plain,
        };
        let mut states = lexical.token(owned, node, terms, memo);
        failures.push(Predicate::all(vec![
            prefix.clone(),
            states
                .remove(&LexicalState::Invalid)
                .unwrap_or(Predicate::False),
        ]));
        let document_end = owned
            .nodes
            .iter()
            .find(|end| end.kind == YamlNodeKind::DocumentEnd && node.span.end <= end.span.start)
            .map_or(owned.skeleton.text.len(), |end| end.span.start);
        if owned
            .skeleton
            .text
            .get(node.span.end..document_end)
            .is_some_and(|tail| tail.trim().is_empty())
        {
            failures.push(Predicate::all(vec![
                prefix.clone(),
                Predicate::Or(
                    states
                        .iter()
                        .filter(|(state, _)| state.is_unterminated())
                        .map(|(_, condition)| condition.clone())
                        .collect(),
                ),
            ]));
        }
        let mut preserved = vec![states.remove(&closed).unwrap_or(Predicate::False)];
        if style == ScalarStyle::Plain {
            for end in [LexicalState::Closed, LexicalState::PlainStart] {
                preserved.push(states.remove(&end).unwrap_or(Predicate::False));
            }
        }
        prefix = memo.normalize(Predicate::all(vec![prefix, Predicate::Or(preserved)]));
        boundaries.push(PrefixBoundary {
            span: node.span,
            preserved: prefix.clone(),
        });
        consumed = node.span.end;
    }
    QuotedPrefix {
        invalid: memo.normalize(Predicate::Or(failures)),
        boundaries,
    }
}

#[cfg(test)]
#[path = "../tests/runtime_output_layout.rs"]
mod tests;
