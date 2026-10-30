//! YAML ownership and lexical coverage sealed from the executed output graph.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use helm_schema_core::{
    CollectionDomain, CollectionQuantifier, Guard, PlainScalarPlacement, Predicate, PredicateMemo,
    ValuesPath,
};
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

    fn selected(&self, other: &Self, decision: &Rc<FrozenTest>) -> Option<Self> {
        let Self {
            buffer: _,
            index: _,
            start,
            indentation,
            context,
        } = self;
        let Self {
            buffer: _,
            index: _,
            start: other_start,
            indentation: other_indentation,
            context: other_context,
        } = other;
        if start != other_start || indentation != other_indentation || context != other_context {
            return None;
        }
        let yes = self.node()?;
        let no = other.node()?;
        // Keep the original frozen payloads while sharing their YAML ownership bytes.
        let selected = OutputNode {
            occurrences: yes
                .occurrences
                .iter()
                .chain(&no.occurrences)
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            part: TextPart::Select {
                decision: Rc::clone(decision),
                yes: Rc::new(TextBuffer {
                    parts: vec![yes.clone()],
                    ..TextBuffer::default()
                }),
                no: Rc::new(TextBuffer {
                    parts: vec![no.clone()],
                    ..TextBuffer::default()
                }),
            },
        };
        Some(Self {
            buffer: Rc::new(TextBuffer {
                parts: vec![selected],
                ..TextBuffer::default()
            }),
            index: 0,
            start: *start,
            indentation: *indentation,
            context: Rc::clone(context),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectedSpan {
    range: Span,
    condition: Predicate,
    local: bool,
}

#[derive(Debug)]
pub(super) struct Layout {
    pub(super) condition: Predicate,
    documents: Vec<DocumentLayout>,
    terms: Vec<NodeRef>,
    placeholders: usize,
    invalid: Predicate,
    source_mapped: bool,
}

#[derive(Clone, Debug)]
struct DocumentLayout {
    ownership: Rc<YamlOwnership>,
    offset: usize,
    scope: Span,
    condition: Predicate,
    boundaries: Vec<PrefixBoundary>,
    selections: Vec<SelectedSpan>,
}

#[derive(Clone, Debug)]
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
    node: &'a NodeRef,
}

impl<'a> PlacedTerm<'a> {
    pub(crate) fn occurrences(&self) -> impl Iterator<Item = ObservationId> + '_ {
        self.node
            .node()
            .into_iter()
            .flat_map(|node| node.occurrences.iter().copied())
            .chain(self.context())
    }
    fn emitted_nodes(&self) -> Option<Vec<(Predicate, &'a OutputNode)>> {
        let mut pending = vec![(Predicate::True, self.node.node()?)];
        let mut values = Vec::new();
        let mut remaining = 128_usize;
        while let Some((condition, node)) = pending.pop() {
            remaining = remaining.checked_sub(1)?;
            match node.content() {
                TextPart::Frozen(_) | TextPart::Bytes(_) | TextPart::Unknown(_) => {
                    values.push((condition, node));
                }
                TextPart::Select { decision, yes, no } => {
                    let FrozenTest {
                        id: _,
                        when_true,
                        when_false,
                        coverage,
                        runtime: _,
                    } = decision.as_ref();
                    if *coverage != Coverage::Exact
                        || when_true.contains_approximation()
                        || when_false.contains_approximation()
                    {
                        return None;
                    }
                    for (branch, guard) in [(no, when_false), (yes, when_true)] {
                        let [node] = branch.parts.as_slice() else {
                            return None;
                        };
                        pending
                            .push((Predicate::all(vec![condition.clone(), guard.clone()]), node));
                    }
                }
            }
        }
        Some(values)
    }

    pub(crate) fn frozen_alternatives(&self) -> Option<Vec<(Predicate, &'a FrozenValue)>> {
        self.emitted_nodes()?
            .into_iter()
            .map(|(guard, node)| match node.content() {
                TextPart::Frozen(value) => Some((guard, value)),
                TextPart::Bytes(_)
                | TextPart::Unknown(_)
                | TextPart::Select {
                    decision: _,
                    yes: _,
                    no: _,
                } => None,
            })
            .collect()
    }

    pub(crate) fn occurrence_guard(&self, id: ObservationId) -> Option<Predicate> {
        match self.node.node().map(OutputNode::content) {
            Some(TextPart::Frozen(_) | TextPart::Bytes(_) | TextPart::Unknown(_)) => {
                return Some(Predicate::True);
            }
            None => return None,
            Some(TextPart::Select {
                decision: _,
                yes: _,
                no: _,
            }) => {}
        }
        Some(Predicate::Or(
            self.emitted_nodes()?
                .into_iter()
                .filter_map(|(guard, node)| node.occurrences.contains(&id).then_some(guard))
                .collect(),
        ))
    }

    pub(crate) fn context(&self) -> impl Iterator<Item = ObservationId> + '_ {
        self.node.context.iter().copied()
    }
    pub(crate) fn inputs(&self) -> BTreeSet<helm_schema_core::ValuesPath> {
        match self.node.node().map(OutputNode::content) {
            Some(TextPart::Frozen(value)) => value.inputs(),
            Some(TextPart::Unknown(inputs)) => inputs.clone(),
            Some(TextPart::Select {
                decision: _,
                yes,
                no,
            }) => yes.inputs().into_iter().chain(no.inputs()).collect(),
            Some(TextPart::Bytes(_)) | None => BTreeSet::new(),
        }
    }
}

impl PlacedDocument<'_> {
    pub(crate) fn block_value(&self, span: Span) -> bool {
        block_value(self.ownership, span)
    }
    pub(crate) fn flow_context(&self, span: Span) -> bool {
        self.ownership
            .nodes
            .iter()
            .filter(|node| node.span == span)
            .any(|node| {
                let mut parent = node.parent;
                while let Some(node) = parent.and_then(|parent| self.ownership.nodes.get(parent)) {
                    if matches!(
                        node.kind,
                        YamlNodeKind::FlowMapping | YamlNodeKind::FlowSequence
                    ) {
                        return true;
                    }
                    parent = node.parent;
                }
                false
            })
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
                Some(PlacedTerm {
                    range,
                    placeholder,
                    source_offset: (self.layout.source_mapped
                        && node.indentation == 0
                        && !placeholder)
                        .then(|| (range.start + self.document.offset).checked_sub(node.start))
                        .flatten(),
                    node,
                })
            })
    }

    pub(crate) fn selection_guard(&self, span: Span) -> Predicate {
        selection_guard(&self.document.selections, span)
    }

    pub(crate) fn placement_guard(&self, span: Span) -> Predicate {
        Predicate::all(vec![self.selection_guard(span), self.prefix_guard(span)])
    }

    fn prefix_guard(&self, span: Span) -> Predicate {
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
        graph: &Rc<TextBuffer>,
        condition: &Predicate,
        memo: &PredicateMemo,
        output: FileOutput,
        syntax: &mut SyntaxContexts,
        input_paths: &BTreeMap<super::ObjectId, ValuesPath>,
        input_objects: &BTreeMap<ValuesPath, super::InputBacking>,
    ) -> Self {
        let mut build = LayoutBuilder {
            layouts: Vec::new(),
            unknown: Vec::new(),
            remaining: 1024,
            lexical: LexicalTransfers::new(input_paths),
            input_objects,
            root_condition: condition.clone(),
            output,
            markers: MarkerProvenance::default(),
            memo,
            syntax,
        };
        let LayoutContexts { known, unknown } =
            build.walk(graph, 0, &Rc::new(Vec::new()), condition);
        // Graph-local unknown coverage gains the file's path condition only at sealing.
        let unknown = memo.normalize(Predicate::all(vec![condition.clone(), unknown]));
        if unknown != Predicate::False {
            build.unknown.push(unknown);
        }
        for mut context in known {
            context.condition =
                memo.normalize(Predicate::all(vec![condition.clone(), context.condition]));
            if context.condition != Predicate::False {
                build.finish_layout(context);
            }
        }
        build.markers.incomplete |= !build.unknown.is_empty();
        Self {
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
            invalid,
        } in &self.layouts
        {
            failures.push(Predicate::all(vec![condition.clone(), invalid.clone()]));
            survivors.push(Predicate::all(vec![condition.clone(), invalid.negated()]));
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
        let condition = site.condition();
        if !memo.exactly_implies(&condition, &self.unknown.negated()) {
            return PlainScalarPlacement::Unknown;
        }
        let id = ObservationId {
            program: site.program,
            ordinal: site.ordinal,
            span: site.span,
        };
        let mut found = false;
        for layout in &self.layouts {
            if memo.exactly_implies(&condition, &layout.condition.negated()) {
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
    matches!(
        block_parent(document, node),
        Some(YamlNodeKind::Pair | YamlNodeKind::Item)
    )
}

fn block_parent(
    document: &YamlOwnership,
    node: &helm_schema_syntax::YamlNode,
) -> Option<YamlNodeKind> {
    let parent = node.parent.and_then(|parent| document.nodes.get(parent))?;
    let mut ancestor = Some(parent);
    while let Some(node) = ancestor {
        if matches!(
            node.kind,
            YamlNodeKind::FlowMapping | YamlNodeKind::FlowPair | YamlNodeKind::FlowSequence
        ) {
            return None;
        }
        ancestor = node.parent.and_then(|parent| document.nodes.get(parent));
    }
    Some(parent.kind)
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

    #[tracing::instrument(skip_all)]
    pub(super) fn seal(exits: &[Exit], output: FileOutput) -> Self {
        let mut syntax = SyntaxContexts::default();
        let mut outputs = Vec::new();
        let mut failures = Vec::new();
        let mut survivors = Vec::new();
        for exit in exits {
            let condition = exit.state.path_predicate();
            let memo = &exit.state.heap.arena.predicate_memo;
            let placement = if matches!(exit.flow, Flow::Normal(_)) {
                OutputPlacement::new(
                    &Rc::new(exit.state.output.clone()),
                    &condition,
                    memo,
                    output,
                    &mut syntax,
                    &exit.state.heap.arena.input_paths.borrow(),
                    &exit.state.heap.arena.input_objects.borrow(),
                )
            } else {
                OutputPlacement {
                    layouts: Vec::new(),
                    unknown: Predicate::True,
                    markers: MarkerProvenance::default(),
                }
            };
            if super::output::exit_may_continue(exit) {
                let invalid = placement.invalid(memo);
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
                    && site.coverage_exact()
                    && !site.condition().contains_approximation()
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
    layouts: Vec<Layout>,
    unknown: Vec<Predicate>,
    remaining: usize,
    lexical: LexicalTransfers<'a>,
    input_objects: &'a BTreeMap<ValuesPath, super::InputBacking>,
    root_condition: Predicate,
    output: FileOutput,
    markers: MarkerProvenance,
    memo: &'a PredicateMemo,
    syntax: &'a mut SyntaxContexts,
}

#[derive(Clone)]
struct LayoutContext {
    skeleton: Skeleton,
    terms: Vec<NodeRef>,
    selections: Vec<SelectedSpan>,
    condition: Predicate,
}

struct LayoutContexts {
    known: Vec<LayoutContext>,
    unknown: Predicate,
}

impl LayoutBuilder<'_> {
    #[tracing::instrument(skip_all, name = "walk_layouts")]
    fn walk(
        &mut self,
        graph: &Rc<TextBuffer>,
        indentation: usize,
        context: &Rc<Vec<ObservationId>>,
        ambient: &Predicate,
    ) -> LayoutContexts {
        if !self.feasible(ambient, &Predicate::True) {
            return LayoutContexts {
                known: Vec::new(),
                unknown: Predicate::False,
            };
        }
        let mut contexts = LayoutContexts {
            known: vec![LayoutContext::empty()],
            unknown: Predicate::False,
        };
        for (index, output) in graph.parts.iter().enumerate() {
            if contexts.known.is_empty() {
                break;
            }
            if self.remaining == 0 {
                contexts.unknown = self.memo.normalize(Predicate::Or(
                    std::iter::once(contexts.unknown)
                        .chain(contexts.known.drain(..).map(|layout| layout.condition))
                        .collect(),
                ));
                break;
            }
            self.remaining -= 1;
            let node = NodeRef {
                buffer: Rc::clone(graph),
                index,
                start: 0,
                indentation,
                context: Rc::clone(context),
            };
            match output.content() {
                TextPart::Select { decision, yes, no } => {
                    let active = self.active(&contexts, ambient);
                    let mut delta = self.choices(decision, [yes, no], &node, &active);
                    self.local_choices(&contexts, &mut delta);
                    contexts = self.then(contexts, delta, ambient);
                }
                TextPart::Frozen(value) => {
                    let payload = self.payload(value, &contexts);
                    if !payload.known.is_empty()
                        && payload.unknown != Predicate::False
                        && let Some(capture) = captured_layout(value, &mut 128)
                    {
                        let active = self.active(&contexts, ambient);
                        let captured = self.capture(&capture, &node, &active);
                        contexts = self.then(contexts, captured, ambient);
                    } else if !payload.known.is_empty() {
                        let mut spellings = BTreeMap::<String, Vec<Predicate>>::new();
                        for arm in payload.known {
                            spellings
                                .entry(arm.spelling)
                                .or_default()
                                .push(arm.condition);
                        }
                        let mut delta = LayoutContexts {
                            known: Vec::new(),
                            unknown: Predicate::False,
                        };
                        for (spelling, guards) in spellings {
                            let mut layout = LayoutContext::empty();
                            layout.condition = self.memo.normalize(Predicate::Or(guards));
                            if layout.append_selected(&spelling, node.clone()) {
                                delta.known.push(layout);
                            } else {
                                delta.unknown = Predicate::True;
                            }
                        }
                        if payload.unknown != Predicate::False {
                            let mut layout = LayoutContext::empty();
                            layout.condition = payload.unknown;
                            if layout.append_selected("00000000", node.clone()) {
                                delta.known.push(layout);
                            } else {
                                delta.unknown = Predicate::True;
                            }
                        }
                        contexts = self.then(contexts, delta, ambient);
                    } else if let Some(capture) = captured_layout(value, &mut 128) {
                        let active = self.active(&contexts, ambient);
                        let captured = self.capture(&capture, &node, &active);
                        contexts = self.then(contexts, captured, ambient);
                    } else {
                        let text = value.exact_text();
                        self.append(
                            &mut contexts,
                            text.as_deref().unwrap_or("00000000"),
                            text.is_none(),
                            &node,
                        );
                    }
                }
                TextPart::Bytes(bytes) => self.append(&mut contexts, bytes, false, &node),
                TextPart::Unknown(_) => self.append(&mut contexts, "00000000", true, &node),
            }
        }
        contexts
    }

    fn capture(
        &mut self,
        capture: &CapturedLayout,
        node: &NodeRef,
        ambient: &Predicate,
    ) -> LayoutContexts {
        let mut prefix = LayoutContext::empty();
        if !prefix.append(&capture.prefix, false, node.clone()) {
            return LayoutContexts {
                known: Vec::new(),
                unknown: Predicate::True,
            };
        }
        let occurrences = node
            .node()
            .into_iter()
            .flat_map(|output| &output.occurrences);
        let nested = self.walk(
            &capture.graph,
            node.indentation.saturating_add(capture.indentation),
            &Rc::new(node.context.iter().chain(occurrences).copied().collect()),
            ambient,
        );
        self.then(
            LayoutContexts {
                known: vec![prefix],
                unknown: Predicate::False,
            },
            nested,
            ambient,
        )
    }

    fn payload<'v>(
        &mut self,
        value: &'v FrozenValue,
        contexts: &LayoutContexts,
    ) -> super::output::PayloadView<'v> {
        use super::output::{PayloadQuery, PayloadShape};
        let mut query =
            PayloadQuery::with_limit(self.input_objects, PayloadShape::Mapping, self.remaining);
        let mapping = query.value(value);
        self.remaining = self.remaining.saturating_sub(query.work());
        if self.remaining == 0 {
            return super::output::PayloadView {
                known: Vec::new(),
                unknown: Predicate::True,
            };
        }
        if mapping.known.is_empty() {
            return mapping;
        }
        let scalar = mapping.shaped(PayloadShape::Scalar, &mut self.remaining);
        if self.remaining == 0 {
            return super::output::PayloadView {
                known: Vec::new(),
                unknown: Predicate::True,
            };
        }
        if scalar
            .known
            .iter()
            .all(|arm| arm.spelling.trim() == "00000000")
            && contexts.known.iter().all(|prefix| {
                scalar
                    .known
                    .iter()
                    .all(|arm| scalar_payload_position(&prefix.skeleton.text, &arm.spelling))
            })
        {
            return scalar;
        }
        if self.remaining == 0 {
            return mapping;
        }
        let sequence = mapping.shaped(PayloadShape::Sequence, &mut self.remaining);
        if self.remaining == 0 {
            return super::output::PayloadView {
                known: Vec::new(),
                unknown: Predicate::True,
            };
        }
        if !sequence.known.is_empty()
            && sequence.known.len() == mapping.known.len()
            && contexts.known.iter().all(|prefix| {
                mapping
                    .known
                    .iter()
                    .zip(&sequence.known)
                    .all(|(mapping, sequence)| {
                        sequence_payload_position(
                            &prefix.skeleton.text,
                            &mapping.spelling,
                            &sequence.spelling,
                        )
                    })
            })
        {
            return sequence;
        }
        mapping
    }

    fn feasible(&self, ambient: &Predicate, condition: &Predicate) -> bool {
        self.memo
            .normalize(Predicate::all(vec![ambient.clone(), condition.clone()]))
            != Predicate::False
    }

    fn active(&self, contexts: &LayoutContexts, ambient: &Predicate) -> Predicate {
        self.memo.normalize(Predicate::all(vec![
            ambient.clone(),
            Predicate::Or(
                contexts
                    .known
                    .iter()
                    .map(|layout| layout.condition.clone())
                    .collect(),
            ),
        ]))
    }

    fn append(
        &self,
        contexts: &mut LayoutContexts,
        bytes: &str,
        placeholder: bool,
        node: &NodeRef,
    ) {
        let mut lost = Vec::new();
        contexts.known.retain_mut(|layout| {
            if layout.append(bytes, placeholder, node.clone()) {
                true
            } else {
                lost.push(layout.condition.clone());
                false
            }
        });
        if !lost.is_empty() {
            contexts.unknown = self.memo.normalize(Predicate::Or(
                std::iter::once(contexts.unknown.clone())
                    .chain(lost)
                    .collect(),
            ));
        }
    }

    fn choices(
        &mut self,
        decision: &Rc<FrozenTest>,
        branches: [&Rc<TextBuffer>; 2],
        node: &NodeRef,
        ambient: &Predicate,
    ) -> LayoutContexts {
        let [yes, no] = branches;
        let FrozenTest {
            id: _,
            when_true,
            when_false,
            coverage,
            runtime: _,
        } = decision.as_ref();
        let exact = *coverage == Coverage::Exact
            && !when_true.contains_approximation()
            && !when_false.contains_approximation();
        // Reconverge each branch before appending the shared continuation.
        let mut branch = |graph, guard: &Predicate| {
            let active = self.memo.normalize(Predicate::all(vec![
                ambient.clone(),
                if exact {
                    guard.clone()
                } else {
                    Predicate::True
                },
            ]));
            self.walk(graph, node.indentation, &node.context, &active)
        };
        let LayoutContexts {
            known: yes,
            unknown: yes_unknown,
        } = branch(yes, when_true);
        let LayoutContexts {
            known: mut no,
            unknown: no_unknown,
        } = branch(no, when_false);
        let mut result = LayoutContexts {
            known: Vec::new(),
            unknown: self.memo.normalize(Predicate::Or(vec![
                Predicate::all(vec![
                    yes_unknown,
                    if exact {
                        when_true.clone()
                    } else {
                        Predicate::True
                    },
                ]),
                Predicate::all(vec![
                    no_unknown,
                    if exact {
                        when_false.clone()
                    } else {
                        Predicate::True
                    },
                ]),
            ])),
        };
        for candidate in yes {
            let merged = exact
                .then(|| {
                    no.iter().enumerate().find_map(|(index, other)| {
                        self.zip(decision, &candidate, other)
                            .map(|merged| (index, merged))
                    })
                })
                .flatten();
            if let Some((index, merged)) = merged {
                no.remove(index);
                if self.feasible(ambient, &merged.condition) {
                    result.known.push(merged);
                }
            } else {
                self.selected(&mut result, candidate, decision, true, exact, ambient);
            }
        }
        for candidate in no {
            self.selected(&mut result, candidate, decision, false, exact, ambient);
        }
        result
    }

    fn local_choices(&mut self, prefixes: &LayoutContexts, delta: &mut LayoutContexts) {
        if delta.unknown != Predicate::False || delta.known.len() != 2 {
            return;
        }
        if prefixes.known.iter().any(|prefix| {
            !prefix.skeleton.text.trim().is_empty()
                && helm_schema_syntax::first_document_ownership(prefix.skeleton.clone()).is_err()
        }) {
            return;
        }
        let mut shape = None;
        for branch in &delta.known {
            if branch.skeleton.text.trim().is_empty() {
                continue;
            }
            let Some(found) = block_branch(&branch.skeleton.text) else {
                return;
            };
            if shape.is_some_and(|previous| previous != found) {
                return;
            }
            shape = Some(found);
        }
        let Some((kind, indentation)) = shape else {
            return;
        };
        let exclusive = self.memo.normalize(Predicate::all(
            delta
                .known
                .iter()
                .map(|branch| branch.condition.clone())
                .collect(),
        )) == Predicate::False;
        let literal_sequence = kind == YamlNodeKind::Sequence
            && delta.known.iter().all(|branch| {
                branch
                    .skeleton
                    .segments
                    .iter()
                    .all(|segment| !segment.placeholder)
            })
            && !prefixes.known.is_empty()
            && prefixes
                .known
                .iter()
                .all(|prefix| sequence_tail(&prefix.skeleton, indentation));
        // Literal sequence continuations cannot replace their shared prefix. Their members retain
        // their selection guards, including unknown selections that prohibit sink typing.
        if (indentation == 0 || !exclusive) && !literal_sequence {
            return;
        }
        let mut merged = LayoutContext::empty();
        let mut covered = Vec::new();
        for mut branch in delta.known.clone() {
            covered.push(branch.condition.clone());
            for selection in &mut branch.selections {
                selection.local = true;
            }
            branch.selections.push(SelectedSpan {
                range: Span::new(0, branch.skeleton.text.len()),
                condition: branch.condition.clone(),
                local: true,
            });
            if !merged.extend(branch) {
                return;
            }
        }
        merged.condition = self.memo.normalize(Predicate::Or(covered));
        delta.known = vec![merged];
    }

    fn selected(
        &self,
        result: &mut LayoutContexts,
        layout: LayoutContext,
        decision: &FrozenTest,
        taken: bool,
        exact: bool,
        ambient: &Predicate,
    ) {
        let FrozenTest {
            id,
            when_true,
            when_false,
            coverage: _,
            runtime: _,
        } = decision;
        let guard = if taken { when_true } else { when_false };
        let LayoutContext {
            skeleton,
            terms,
            mut selections,
            condition,
        } = layout;
        let condition = self.memo.normalize(Predicate::all(vec![
            condition,
            if exact {
                guard.clone()
            } else {
                Predicate::True
            },
        ]));
        if !self.feasible(ambient, &condition) {
            return;
        }
        if result.known.len() >= 32 {
            result.unknown = self
                .memo
                .normalize(Predicate::Or(vec![result.unknown.clone(), condition]));
            return;
        }
        selections.push(SelectedSpan {
            range: Span::new(0, skeleton.text.len()),
            local: false,
            condition: if exact {
                guard.clone()
            } else {
                Predicate::approximate_output_selection(
                    format!("executed output {id} {taken}"),
                    BTreeSet::new(),
                    Predicate::False,
                )
            },
        });
        result.known.push(LayoutContext {
            skeleton,
            terms,
            selections,
            condition,
        });
    }

    #[tracing::instrument(skip_all)]
    fn zip(
        &self,
        decision: &Rc<FrozenTest>,
        yes: &LayoutContext,
        no: &LayoutContext,
    ) -> Option<LayoutContext> {
        let LayoutContext {
            skeleton: yes_skeleton,
            terms: yes_terms,
            selections: yes_selections,
            condition: yes_condition,
        } = yes;
        let LayoutContext {
            skeleton: no_skeleton,
            terms: no_terms,
            selections: no_selections,
            condition: no_condition,
        } = no;
        if yes_skeleton != no_skeleton
            || yes_condition != no_condition
            || yes_selections != no_selections
            || yes_terms.len() != no_terms.len()
        {
            return None;
        }
        let terms = yes_terms
            .iter()
            .zip(no_terms)
            .map(|(yes, no)| yes.selected(no, decision))
            .collect::<Option<Vec<_>>>()?;
        let FrozenTest {
            id: _,
            when_true,
            when_false,
            coverage: _,
            runtime: _,
        } = decision.as_ref();
        let covered = self
            .memo
            .normalize(Predicate::Or(vec![when_true.clone(), when_false.clone()]));
        let mut selections = yes_selections.clone();
        if covered != Predicate::True {
            selections.push(SelectedSpan {
                range: Span::new(0, yes_skeleton.text.len()),
                local: false,
                condition: covered.clone(),
            });
        }
        Some(LayoutContext {
            skeleton: yes_skeleton.clone(),
            terms,
            selections,
            condition: self
                .memo
                .normalize(Predicate::all(vec![yes_condition.clone(), covered])),
        })
    }

    #[tracing::instrument(skip_all, fields(prefixes = prefix.known.len(), suffixes = suffix.known.len()))]
    fn then(
        &self,
        prefix: LayoutContexts,
        suffix: LayoutContexts,
        ambient: &Predicate,
    ) -> LayoutContexts {
        let LayoutContexts {
            known: prefixes,
            unknown: prefix_unknown,
        } = prefix;
        let LayoutContexts {
            known: suffixes,
            unknown: suffix_unknown,
        } = suffix;
        let prefix_condition = Predicate::Or(
            prefixes
                .iter()
                .map(|layout| layout.condition.clone())
                .collect(),
        );
        let suffix_unknown = Predicate::all(vec![prefix_condition, suffix_unknown]);
        let mut unknown = vec![prefix_unknown, suffix_unknown];
        let mut result = LayoutContexts {
            known: Vec::new(),
            unknown: Predicate::False,
        };
        let mut append = |prefix: Cow<'_, LayoutContext>,
                          suffix: Cow<'_, LayoutContext>,
                          result: &mut LayoutContexts| {
            let condition = self.memo.normalize(Predicate::all(vec![
                prefix.condition.clone(),
                suffix.condition.clone(),
            ]));
            if !self.feasible(ambient, &condition) {
                return;
            }
            if result.known.len() >= 32 {
                unknown.push(condition);
                return;
            }
            let mut prefix = prefix.into_owned();
            if !prefix.extend(suffix.into_owned()) {
                unknown.push(condition);
                return;
            }
            prefix.condition = condition;
            result.known.push(prefix);
        };
        // Copy only feasible combinations admitted below the structural-context bound.
        match suffixes.len() {
            1 => {
                if let Some(suffix) = suffixes.into_iter().next() {
                    let mut prefixes = prefixes.into_iter();
                    if let Some(last) = prefixes.next_back() {
                        for prefix in prefixes {
                            append(Cow::Owned(prefix), Cow::Borrowed(&suffix), &mut result);
                        }
                        append(Cow::Owned(last), Cow::Owned(suffix), &mut result);
                    }
                }
            }
            _ => {
                for prefix in prefixes {
                    let mut suffixes = suffixes.iter();
                    if let Some(last) = suffixes.next_back() {
                        for suffix in suffixes {
                            append(Cow::Borrowed(&prefix), Cow::Borrowed(suffix), &mut result);
                        }
                        append(Cow::Owned(prefix), Cow::Borrowed(last), &mut result);
                    }
                }
            }
        }
        result.unknown = self.memo.normalize(Predicate::Or(unknown));
        result
    }

    #[tracing::instrument(skip_all)]
    fn recover_multiline(
        &mut self,
        skeleton: &mut Skeleton,
        terms: &[NodeRef],
        removed: &mut Vec<Span>,
    ) -> Vec<DocumentLayout> {
        let mut text = String::new();
        let mut start = 0;
        for segment in &mut skeleton.segments {
            if !segment.placeholder
                && skeleton.text[segment.range.start..segment.range.end].contains('\n')
                && terms
                    .get(segment.piece)
                    .and_then(NodeRef::node)
                    .is_some_and(|node| matches!(node.content(), TextPart::Frozen(_)))
            {
                segment.placeholder = true;
                text.push_str(&skeleton.text[start..segment.range.start]);
                text.extend(std::iter::repeat_n(
                    '0',
                    segment.range.end - segment.range.start,
                ));
                start = segment.range.end;
            }
        }
        if start != 0 {
            text.push_str(&skeleton.text[start..]);
            skeleton.text = text;
            *removed = marker_ranges(&skeleton.text);
            return self.syntax.documents(skeleton, removed).unwrap_or_default();
        }
        Vec::new()
    }

    fn finish_layout(&mut self, context: LayoutContext) {
        if !context.local_selections_complete() {
            self.unknown.push(context.condition);
            return;
        }
        let LayoutContext {
            mut skeleton,
            terms,
            selections,
            condition,
        } = context;
        let empty = skeleton.text.trim().is_empty();
        let mut removed = marker_ranges(&skeleton.text);
        self.markers.record(&skeleton, &terms, &removed);
        let mut owned_documents = match self.output {
            FileOutput::Text => Vec::new(),
            FileOutput::Manifest => {
                let Some(documents) = self.syntax.documents(&skeleton, &removed) else {
                    self.unknown.push(condition);
                    return;
                };
                documents
            }
        };
        let recovered = owned_documents.is_empty();
        // Recover multiline frozen text for unindented quoted continuations Helm accepts.
        // Its hole retains the complete lexical transfer, including every structural byte.
        if matches!(self.output, FileOutput::Manifest) && !empty && owned_documents.is_empty() {
            owned_documents = self.recover_multiline(&mut skeleton, &terms, &mut removed);
        }
        let mut failures = Vec::new();
        let mut documents = Vec::new();
        for mut document in owned_documents {
            document.condition = self.memo.normalize(Predicate::all(
                std::iter::once(self.root_condition.clone())
                    .chain(
                        selections
                            .iter()
                            .filter(|selection| {
                                !selection.local
                                    && (!removed.is_empty()
                                        || (selection.range.start <= document.scope.end
                                            && document.scope.start <= selection.range.end))
                            })
                            .map(|selection| selection.condition.clone()),
                    )
                    .collect(),
            ));
            if removed.is_empty() {
                document.selections = selections
                    .iter()
                    .filter(|selection| selection.local)
                    .filter_map(|selection| {
                        let start = selection.range.start.max(document.offset);
                        let end = selection.range.end.min(document.scope.end);
                        (start < end).then(|| SelectedSpan {
                            range: Span::new(start - document.offset, end - document.offset),
                            condition: selection.condition.clone(),
                            local: true,
                        })
                    })
                    .collect();
            } else if selections.iter().any(|selection| selection.local) {
                self.unknown.push(condition.clone());
                return;
            }
            let result = quoted_tokens(
                &document.ownership,
                &terms,
                &mut self.lexical,
                self.input_objects,
                &document.selections,
                self.memo,
            );
            failures.push(result.invalid);
            failures.push(serialized_failures(
                &document.ownership,
                &terms,
                &result.boundaries,
                &document.selections,
                self.input_objects,
                self.memo,
                &mut self.lexical.remaining,
            ));
            document.boundaries = result.boundaries;
            // Recovery proves only irreversible lexical failure, not resource placement.
            if !recovered {
                documents.push(document);
            }
        }
        self.layouts.push(Layout {
            condition,
            documents,
            placeholders: skeleton
                .segments
                .iter()
                .filter(|segment| segment.placeholder)
                .count(),
            invalid: self.memo.normalize(Predicate::Or(failures)),
            source_mapped: removed.is_empty(),
            terms,
        });
    }
}

impl LayoutContext {
    fn local_selections_complete(&self) -> bool {
        !self.selections.iter().any(|selection| {
            selection.local
                && self
                    .skeleton
                    .text
                    .get(selection.range.end..)
                    .is_some_and(|suffix| {
                        !suffix.is_empty()
                            && !suffix.starts_with('\n')
                            && !self
                                .skeleton
                                .text
                                .get(..selection.range.end)
                                .is_some_and(|prefix| prefix.ends_with('\n'))
                    })
        })
    }

    fn empty() -> Self {
        Self {
            skeleton: Skeleton {
                text: String::new(),
                segments: Vec::new(),
            },
            terms: Vec::new(),
            selections: Vec::new(),
            condition: Predicate::True,
        }
    }

    fn extend(&mut self, suffix: Self) -> bool {
        let Self {
            mut skeleton,
            terms: mut suffix_terms,
            mut selections,
            condition: _,
        } = suffix;
        let bytes = self.skeleton.text.len();
        if bytes.saturating_add(skeleton.text.len()) > 64 * 1024 {
            return false;
        }
        let terms = self.terms.len();
        self.skeleton.text.push_str(&skeleton.text);
        for SkeletonSegment {
            range,
            piece,
            placeholder: _,
        } in &mut skeleton.segments
        {
            *range = Span::new(bytes + range.start, bytes + range.end);
            *piece += terms;
        }
        self.skeleton.segments.append(&mut skeleton.segments);
        for NodeRef {
            start,
            buffer: _,
            index: _,
            indentation: _,
            context: _,
        } in &mut suffix_terms
        {
            *start += bytes;
        }
        self.terms.append(&mut suffix_terms);
        for SelectedSpan {
            range,
            condition: _,
            local: _,
        } in &mut selections
        {
            *range = Span::new(bytes + range.start, bytes + range.end);
        }
        self.selections.append(&mut selections);
        true
    }

    fn append_selected(&mut self, bytes: &str, node: NodeRef) -> bool {
        if !self.append(bytes, true, node) {
            return false;
        }
        self.selections.push(SelectedSpan {
            range: Span::new(0, self.skeleton.text.len()),
            local: false,
            condition: self.condition.clone(),
        });
        true
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
            Cow::Borrowed(bytes)
        } else {
            Cow::Owned(bytes.replace('\n', &format!("\n{}", " ".repeat(node.indentation))))
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
        super::text::TextTerm::Resolved {
            term: _,
            literal: _,
        }
        | super::text::TextTerm::Capture {
            graph: _,
            strip_marker: true,
        }
        | super::text::TextTerm::Tpl {
            source: _,
            result: _,
        }
        | super::text::TextTerm::Json {
            value: _,
            pretty: _,
            escape_html: _,
            literal: _,
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

#[derive(Default)]
struct SyntaxContexts {
    entries: Vec<SyntaxContext>,
}

struct SyntaxContext {
    skeleton: Skeleton,
    documents: Option<Vec<DocumentLayout>>,
}

impl SyntaxContexts {
    #[tracing::instrument(skip_all, name = "syntax_context", fields(cache_hit = tracing::field::Empty))]
    fn documents(&mut self, skeleton: &Skeleton, removed: &[Span]) -> Option<Vec<DocumentLayout>> {
        let skeleton = stripped_skeleton(skeleton, removed);
        for entry in &self.entries {
            let SyntaxContext {
                skeleton: existing,
                documents,
            } = entry;
            if *existing == skeleton {
                tracing::Span::current().record("cache_hit", true);
                return documents.clone();
            }
        }
        tracing::Span::current().record("cache_hit", false);
        let documents = documents(&skeleton);
        // Cache only syntax; execution guards and lexical boundaries remain with each layout.
        if self.entries.len() < 32 {
            self.entries.push(SyntaxContext {
                skeleton,
                documents: documents.clone(),
            });
        }
        documents
    }
}

#[tracing::instrument(skip_all)]
fn documents(skeleton: &Skeleton) -> Option<Vec<DocumentLayout>> {
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
            ownership: Rc::new(ownership),
            offset: start,
            scope,
            condition: Predicate::True,
            boundaries: Vec::new(),
            selections: Vec::new(),
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
                stringified,
                lexical_escapes,
            } if lexical_escapes.is_empty() => {
                if *stringified {
                    quoted::formatted_input(states, path, memo)
                } else {
                    quoted::input(states, path, memo)
                }
            }
            ScalarRenderPart::Identity {
                path: _,
                stringified: _,
                lexical_escapes: _,
            } => states
                .into_iter()
                .filter(|(state, _)| state.is_absorbing())
                .collect(),
        };
    }
    states
}

struct LexicalTransfers<'a> {
    values: BTreeMap<(*const super::HeapVersion, *const ValueNode, LexicalState), States>,
    buffers: BTreeMap<(*const TextBuffer, LexicalState), States>,
    remaining: usize,
    input_paths: &'a BTreeMap<super::ObjectId, ValuesPath>,
    map_depth: usize,
}

impl<'a> LexicalTransfers<'a> {
    fn new(input_paths: &'a BTreeMap<super::ObjectId, ValuesPath>) -> Self {
        Self {
            values: BTreeMap::new(),
            buffers: BTreeMap::new(),
            remaining: 4096,
            input_paths,
            map_depth: 0,
        }
    }

    fn value(&mut self, value: &FrozenValue, state: LexicalState, memo: &PredicateMemo) -> States {
        let FrozenValue {
            heap,
            value: super::Value { node },
        } = value;
        let key = (Rc::as_ptr(heap), Rc::as_ptr(node), state);
        if let Some(result) = self.values.get(&key) {
            return result.clone();
        }
        if self.remaining == 0 && !state.is_absorbing() {
            return States::new();
        }
        self.remaining = self.remaining.saturating_sub(1);
        let initial = BTreeMap::from([(state, Predicate::True)]);
        let result = if state.is_absorbing() {
            initial
        } else if let Some(path) = value.identity() {
            quoted::formatted_input(initial, path, memo)
        } else if let Some(text) = value.exact_text() {
            quoted::literal(initial, &text, memo)
        } else {
            match value.value.node.as_ref() {
                ValueNode::Leaf(Leaf {
                    datum: Datum::Text(text),
                    origin: _,
                }) => self.text(text, state, memo),
                ValueNode::Select { decision, yes, no } => {
                    let branches = [yes, no].map(|branch| {
                        self.value(
                            &FrozenValue {
                                heap: Rc::clone(&value.heap),
                                value: branch.clone(),
                            },
                            state,
                            memo,
                        )
                    });
                    Self::choice(decision, branches, memo)
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
                        Datum::Map {
                            view: _,
                            target: Some(target),
                        },
                    origin: _,
                }) => self.map(value, target, state, memo),
                ValueNode::Leaf(Leaf {
                    datum:
                        Datum::Map {
                            view: _,
                            target: None,
                        },
                    origin: _,
                }) => quoted::literal(initial, "map[]", memo),
                ValueNode::Leaf(Leaf {
                    datum:
                        Datum::Scalar {
                            go_type: _,
                            content: _,
                        }
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

    fn map(
        &mut self,
        value: &FrozenValue,
        target: &super::ObjectId,
        state: LexicalState,
        memo: &PredicateMemo,
    ) -> States {
        if self.map_depth == 32 {
            return States::new();
        }
        let Some(super::ObjectVersion {
            revision: _,
            multiplicity,
            data,
        }) = value.heap.object(target)
        else {
            return States::new();
        };
        match multiplicity {
            super::Multiplicity::Singleton => {}
            super::Multiplicity::Summary => return States::new(),
        }
        let (fields, remainder) = match data {
            super::ObjectData::Map { fields, remainder } => (fields, remainder),
            super::ObjectData::Strings(_)
            | super::ObjectData::Opaque { reachable: _ }
            | super::ObjectData::Widened { inputs: _ } => return States::new(),
        };
        let Some(guard) = self.map_guard(target, fields, remainder) else {
            return States::new();
        };
        self.map_depth += 1;
        let mut states = quoted::literal(BTreeMap::from([(state, guard)]), "map[", memo);
        let mut emitted = false;
        for (key, field) in fields {
            let child = match field {
                super::Field::Present(child) => child,
                super::Field::Absent => continue,
                super::Field::Select {
                    decision: _,
                    yes: _,
                    no: _,
                }
                | super::Field::Unknown => {
                    states.clear();
                    break;
                }
            };
            states = quoted::literal(
                states,
                &format!("{}{key}:", if emitted { " " } else { "" }),
                memo,
            );
            let child = FrozenValue {
                heap: Rc::clone(&value.heap),
                value: child.clone(),
            };
            let mut next = States::new();
            for (state, prefix) in states {
                for (end, condition) in self.value(&child, state, memo) {
                    quoted::merge(
                        &mut next,
                        end,
                        Predicate::all(vec![prefix.clone(), condition]),
                        memo,
                    );
                }
            }
            states = next;
            emitted = true;
        }
        let result = quoted::literal(states, "]", memo);
        self.map_depth -= 1;
        result
    }

    fn map_guard(
        &self,
        target: &super::ObjectId,
        fields: &BTreeMap<String, super::Field>,
        remainder: &super::Remainder,
    ) -> Option<Predicate> {
        match remainder {
            super::Remainder::Closed => Some(Predicate::True),
            super::Remainder::Open | super::Remainder::Unknown { inputs: _ } => None,
            super::Remainder::Input => {
                let path = self.input_paths.get(target)?;
                let keys = fields
                    .keys()
                    .map(|key| helm_schema_core::escape_regex_literal(key))
                    .collect::<Vec<_>>()
                    .join("|");
                // Only overwritten or removed input members may remain in this projection.
                Some(Predicate::all(vec![
                    Predicate::from(Guard::TypeIs {
                        path: path.clone(),
                        schema_type: "object".to_owned(),
                    }),
                    Predicate::from(Guard::CollectionPattern {
                        path: path.clone(),
                        domain: CollectionDomain::ObjectKeys,
                        quantifier: CollectionQuantifier::All,
                        pattern: if fields.is_empty() {
                            r"\b\B".to_owned()
                        } else {
                            format!("^(?:{keys})$")
                        },
                    }),
                ]))
            }
        }
    }

    fn text(
        &mut self,
        text: &super::text::FrozenText,
        state: LexicalState,
        memo: &PredicateMemo,
    ) -> States {
        let initial = BTreeMap::from([(state, Predicate::True)]);
        match text.0.as_ref() {
            super::text::TextTerm::Resolved { term: _, literal } => {
                quoted::literal(initial, &literal.to_string(), memo)
            }
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
                use super::kinds::GoType;
                // Sprig skips nil interfaces; typed nil maps and slices still print.
                let nil = match value.go_type() {
                    GoType::Nil => Some(Predicate::True),
                    GoType::Unknown => value.identity().map(super::kinds::input_nil_predicate),
                    GoType::Bool
                    | GoType::String
                    | GoType::Int
                    | GoType::Int64
                    | GoType::Uint64
                    | GoType::Float64
                    | GoType::Map
                    | GoType::Slice => Some(Predicate::False),
                };
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
            super::text::TextTerm::Tpl { source: _, result } => self.value(result, state, memo),
            super::text::TextTerm::Yaml { value, pretty: _ }
            | super::text::TextTerm::Json {
                value,
                pretty: _,
                escape_html: _,
                literal: _,
            } => quoted::serialized_input(initial, value, memo),
        }
    }

    fn choice(decision: &FrozenTest, branches: [States; 2], memo: &PredicateMemo) -> States {
        let FrozenTest {
            id: _,
            when_true,
            when_false,
            coverage,
            runtime: _,
        } = decision;
        let [yes, no] = branches;
        let mut result = States::new();
        match coverage {
            Coverage::Exact
                if !when_true.contains_approximation() && !when_false.contains_approximation() =>
            {
                for (branch, guard) in [(yes, when_true), (no, when_false)] {
                    for (end, condition) in branch {
                        quoted::merge(
                            &mut result,
                            end,
                            Predicate::all(vec![guard.clone(), condition]),
                            memo,
                        );
                    }
                }
            }
            Coverage::Exact | Coverage::Partial => {
                // A shared transition is proved independently of the unknown branch outcome.
                for (end, condition) in yes {
                    if let Some(other) = no.get(&end) {
                        quoted::merge(
                            &mut result,
                            end,
                            Predicate::all(vec![condition, other.clone()]),
                            memo,
                        );
                    }
                }
            }
        }
        result
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
        self.range(
            owned,
            Span::new(node.span.start + skip, node.span.end),
            terms,
            BTreeMap::from([(state, Predicate::True)]),
            memo,
        )
    }

    fn flow_tail(
        &mut self,
        owned: &YamlOwnership,
        node: &helm_schema_syntax::YamlNode,
        terms: &[NodeRef],
        states: &States,
        memo: &PredicateMemo,
    ) -> Predicate {
        let mut parent = node.parent;
        while let Some(ancestor) = parent.and_then(|parent| owned.nodes.get(parent)) {
            if matches!(
                ancestor.kind,
                YamlNodeKind::FlowSequence | YamlNodeKind::FlowMapping
            ) {
                // Only irreversible failure survives this provisional container boundary.
                let mut states = self.range(
                    owned,
                    Span::new(node.span.end, ancestor.span.end),
                    terms,
                    states
                        .iter()
                        .filter(|(state, _)| state.is_unterminated())
                        .map(|(state, guard)| (*state, guard.clone()))
                        .collect(),
                    memo,
                );
                return invalid_states(owned, node, &mut states);
            }
            parent = ancestor.parent;
        }
        Predicate::False
    }

    fn range(
        &mut self,
        owned: &YamlOwnership,
        range: Span,
        terms: &[NodeRef],
        mut states: States,
        memo: &PredicateMemo,
    ) -> States {
        for segment in &owned.skeleton.segments {
            let start = segment.range.start.max(range.start);
            let end = segment.range.end.min(range.end);
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
                states.retain(|state, _| state.is_absorbing());
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
        if state.is_absorbing() {
            return BTreeMap::from([(state, Predicate::True)]);
        }
        match part {
            TextPart::Frozen(value) => self.value(value, state, memo),
            TextPart::Bytes(text) => {
                quoted::literal(BTreeMap::from([(state, Predicate::True)]), text, memo)
            }
            TextPart::Unknown(_) => States::new(),
            TextPart::Select { decision, yes, no } => {
                let branches = [yes, no].map(|branch| self.buffer(branch, state, memo));
                Self::choice(decision, branches, memo)
            }
        }
    }
}

struct QuotedPrefix {
    invalid: Predicate,
    boundaries: Vec<PrefixBoundary>,
}

impl QuotedPrefix {
    fn unknown() -> Self {
        Self {
            invalid: Predicate::False,
            boundaries: Vec::new(),
        }
    }
}

#[tracing::instrument(skip_all)]
fn quoted_tokens(
    owned: &YamlOwnership,
    terms: &[NodeRef],
    lexical: &mut LexicalTransfers,
    inputs: &BTreeMap<ValuesPath, super::InputBacking>,
    selections: &[SelectedSpan],
    memo: &PredicateMemo,
) -> QuotedPrefix {
    if owned.skeleton.text.contains('<') {
        return QuotedPrefix::unknown();
    }
    let mut prefix = Predicate::True;
    let mut failures = Vec::new();
    let mut consumed = 0;
    let mut boundaries = Vec::new();
    let mut payloads = super::output::PayloadQuery::new(inputs);
    for node in &owned.nodes {
        if node.span.end <= consumed {
            continue;
        }
        if let Some((span, preserved)) =
            serialized_boundary(owned, node, terms, &mut payloads, &mut lexical.remaining).or_else(
                || serialized_node_boundary(owned, node, terms, inputs, &mut lexical.remaining),
            )
        {
            let active = selection_guard(selections, span);
            prefix = memo.normalize(Predicate::all(vec![
                prefix,
                Predicate::Or(vec![active.negated(), preserved]),
            ]));
            boundaries.push(PrefixBoundary {
                span,
                preserved: prefix.clone(),
            });
            consumed = span.end;
            continue;
        }
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
        let active = selection_guard(selections, node.span);
        let mut states = lexical.token(owned, node, terms, memo);
        failures.push(Predicate::all(vec![
            prefix.clone(),
            active.clone(),
            lexical.flow_tail(owned, node, terms, &states, memo),
        ]));
        for failure in token_failures(owned, node, &mut states) {
            failures.push(Predicate::all(vec![
                prefix.clone(),
                active.clone(),
                failure,
            ]));
        }
        let mut preserved = vec![states.remove(&closed).unwrap_or(Predicate::False)];
        if style == ScalarStyle::Plain {
            for end in [LexicalState::Closed, LexicalState::PlainStart] {
                preserved.push(states.remove(&end).unwrap_or(Predicate::False));
            }
        }
        prefix = memo.normalize(Predicate::all(vec![
            prefix,
            Predicate::Or(vec![active.negated(), Predicate::Or(preserved)]),
        ]));
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

fn token_failures(
    owned: &YamlOwnership,
    node: &helm_schema_syntax::YamlNode,
    states: &mut States,
) -> Vec<Predicate> {
    let mut failures = vec![invalid_states(owned, node, states)];
    if ends_document(owned, node.span) {
        failures.push(Predicate::Or(
            states
                .iter()
                .filter(|(state, _)| state.is_unterminated())
                .map(|(_, condition)| condition.clone())
                .collect(),
        ));
    }
    failures
}

// A block serializer owns a complete node; its synthetic key and value are not scalar emissions.
fn serialized_node_boundary(
    owned: &YamlOwnership,
    node: &helm_schema_syntax::YamlNode,
    terms: &[NodeRef],
    inputs: &BTreeMap<ValuesPath, super::InputBacking>,
    remaining: &mut usize,
) -> Option<(Span, Predicate)> {
    let shape = match node.kind {
        YamlNodeKind::Scalar(ScalarStyle::Plain) if !node.key => {
            super::output::PayloadShape::Scalar
        }
        YamlNodeKind::Item => super::output::PayloadShape::Sequence,
        YamlNodeKind::FlowMapping => super::output::PayloadShape::Mapping,
        _ => return None,
    };
    let segment = owned.skeleton.segments.iter().find(|segment| {
        segment.placeholder
            && segment.range.start <= node.span.start
            && node.span.end <= segment.range.end
    })?;
    let text = owned
        .skeleton
        .text
        .get(segment.range.start..segment.range.end)?;
    if text.trim()
        != owned
            .skeleton
            .text
            .get(node.span.start..node.span.end)?
            .trim()
    {
        return None;
    }
    let term = PlacedTerm {
        range: segment.range,
        placeholder: true,
        source_offset: None,
        node: terms.get(segment.piece)?,
    };
    let mut query = super::output::PayloadQuery::with_shape(inputs, shape);
    let mut preserved = Vec::new();
    for (guard, value) in term.frozen_alternatives()? {
        for payload in query.bounded_value(value, remaining).known {
            if payload.spelling.trim() != text.trim() {
                continue;
            }
            if !payload.omitted_members.is_empty()
                && block_parent(owned, node).is_none()
                && !payload.flow_node
            {
                continue;
            }
            let kind = if shape == super::output::PayloadShape::Sequence {
                Predicate::all(vec![
                    Predicate::from(Guard::TypeIs {
                        path: payload.path.clone(),
                        schema_type: "array".to_owned(),
                    }),
                    Predicate::truthy_path(payload.path.encode()),
                ])
            } else if shape == super::output::PayloadShape::Scalar {
                if block_parent(owned, node).is_some() {
                    payload.scalar_condition()
                } else {
                    Predicate::all(vec![payload.scalar_condition(), payload.flow_condition()])
                }
            } else if payload.flow_node {
                payload.unchanged_condition()
            } else {
                Predicate::all(vec![
                    payload.unchanged_condition(),
                    payload.flow_condition(),
                ])
            };
            let unchanged = payload.unchanged_condition();
            let mut condition = vec![guard.clone(), payload.condition, kind];
            if payload.kind == crate::ValueKind::TemplatedYamlSerialized {
                condition.push(unchanged);
            }
            preserved.push(Predicate::all(condition));
        }
    }
    (!preserved.is_empty()).then(|| {
        (
            Span::new(node.span.start, segment.range.end),
            Predicate::Or(preserved),
        )
    })
}

fn serialized_boundary<'a>(
    owned: &YamlOwnership,
    node: &helm_schema_syntax::YamlNode,
    terms: &'a [NodeRef],
    query: &mut super::output::PayloadQuery<'a, '_>,
    remaining: &mut usize,
) -> Option<(Span, Predicate)> {
    if node.kind != YamlNodeKind::Pair {
        return None;
    }
    let segment = owned.skeleton.segments.iter().find(|segment| {
        segment.placeholder
            && segment.range.start <= node.span.start
            && node.span.end <= segment.range.end
    })?;
    let text = owned
        .skeleton
        .text
        .get(segment.range.start..segment.range.end)?;
    let pair = owned.skeleton.text.get(node.span.start..node.span.end)?;
    if text.trim() != pair.trim() {
        return None;
    }
    let mapping = owned.nodes.get(node.parent?)?;
    if mapping.kind != YamlNodeKind::Mapping {
        return None;
    }
    let whole = owned
        .nodes
        .iter()
        .filter(|child| child.parent == node.parent)
        .count()
        == 1;
    let block_operand = whole
        && mapping
            .parent
            .and_then(|parent| owned.nodes.get(parent))
            .is_some_and(|parent| matches!(parent.kind, YamlNodeKind::Pair | YamlNodeKind::Item));
    let term = PlacedTerm {
        range: segment.range,
        placeholder: true,
        source_offset: None,
        node: terms.get(segment.piece)?,
    };
    let mut preserved = Vec::new();
    for (guard, value) in term.frozen_alternatives()? {
        for payload in query.bounded_value(value, remaining).known {
            if block_operand && payload.kind == crate::ValueKind::YamlSerialized {
                preserved.push(Predicate::all(vec![guard.clone(), payload.condition]));
                continue;
            }
            let unchanged = payload.unchanged_condition();
            let mut parts = vec![guard.clone(), payload.condition, unchanged];
            if !whole {
                parts.push(Predicate::from(Guard::TypeIs {
                    path: payload.path.clone(),
                    schema_type: "object".to_owned(),
                }));
                parts.push(Predicate::from(Guard::MinMembers {
                    path: payload.path.clone(),
                    bound: 1,
                }));
            }
            preserved.push(Predicate::all(parts));
        }
    }
    Some((
        Span::new(node.span.start, segment.range.end),
        Predicate::Or(preserved),
    ))
}

fn selection_guard(selections: &[SelectedSpan], span: Span) -> Predicate {
    Predicate::all(
        selections
            .iter()
            .filter(|selection| {
                selection.range.start <= span.start && span.end <= selection.range.end
            })
            .map(|selection| selection.condition.clone())
            .collect(),
    )
}

#[tracing::instrument(skip_all, level = "debug", name = "sequence_continuation")]
fn sequence_tail(skeleton: &Skeleton, indentation: usize) -> bool {
    let Some(documents) = documents(skeleton) else {
        return false;
    };
    let Some(document) = documents.last() else {
        return false;
    };
    let ownership = &document.ownership;
    let mut current = ownership
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                YamlNodeKind::Scalar(_) | YamlNodeKind::BlockScalar
            )
        })
        .max_by_key(|node| node.span.end);
    while let Some(node) = current {
        if node.kind == YamlNodeKind::Sequence
            && node
                .parent
                .and_then(|parent| ownership.nodes.get(parent))
                .is_some_and(|parent| parent.kind == YamlNodeKind::Pair)
            && let Some(before) = ownership.skeleton.text.get(..node.span.start)
        {
            let line = before.rsplit('\n').next().unwrap_or_default();
            if line.len() == indentation && line.bytes().all(|byte| byte == b' ') {
                return true;
            }
        }
        current = node.parent.and_then(|parent| ownership.nodes.get(parent));
    }
    false
}

fn block_branch(text: &str) -> Option<(YamlNodeKind, usize)> {
    if !text.starts_with('\n') {
        return None;
    }
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let indentation = lines.first()?.len() - lines.first()?.trim_start_matches(' ').len();
    if lines
        .iter()
        .any(|line| line.len() - line.trim_start_matches(' ').len() < indentation)
    {
        return None;
    }
    let text = lines
        .iter()
        .map(|line| line.get(indentation..))
        .collect::<Option<Vec<_>>>()?
        .join("\n");
    let documents = documents(&Skeleton {
        text,
        segments: Vec::new(),
    })?;
    let [document] = documents.as_slice() else {
        return None;
    };
    if document.offset != 0
        || document
            .ownership
            .nodes
            .iter()
            .any(|node| node.kind == YamlNodeKind::DocumentEnd)
    {
        return None;
    }
    let owned = &document.ownership;
    let root = owned.nodes.iter().find(|node| {
        node.parent
            .and_then(|parent| owned.nodes.get(parent))
            .is_some_and(|parent| parent.kind == YamlNodeKind::Document)
    })?;
    matches!(root.kind, YamlNodeKind::Mapping | YamlNodeKind::Sequence)
        .then_some((root.kind, indentation))
}

fn scalar_payload_position(prefix: &str, spelling: &str) -> bool {
    let Some(first) = spelling.find('0') else {
        return false;
    };
    let start = prefix.len() + first;
    let text = format!("{prefix}{spelling}");
    let Some(mut tree) = helm_schema_syntax::parse_yaml(&text) else {
        return false;
    };
    if tree.root_node().has_error() {
        let mut pending = vec![tree.root_node()];
        let mut delimiters = Vec::new();
        let mut remaining = 256_usize;
        while let Some(node) = pending.pop() {
            let Some(next) = remaining.checked_sub(1) else {
                return false;
            };
            remaining = next;
            match node.kind() {
                "[" => delimiters.push(']'),
                "{" => delimiters.push('}'),
                "]" | "}" => {
                    if delimiters.pop().is_none_or(|expected| {
                        node.kind() != if expected == ']' { "]" } else { "}" }
                    }) {
                        return false;
                    }
                }
                "plain_scalar" | "double_quote_scalar" | "single_quote_scalar" | "block_scalar" => {
                }
                _ => {
                    let mut cursor = node.walk();
                    let children = node.children(&mut cursor).collect::<Vec<_>>();
                    pending.extend(children.into_iter().rev());
                }
            }
        }
        drop(pending);
        if !delimiters.is_empty() && delimiters.len() <= 8 {
            let completed = format!("{text}{}", delimiters.into_iter().rev().collect::<String>());
            if let Some(completed) = helm_schema_syntax::parse_yaml(&completed) {
                tree = completed;
            }
        }
    }
    let Some(mut node) = tree
        .root_node()
        .named_descendant_for_byte_range(start, start + 8)
    else {
        return false;
    };
    let mut atomic = false;
    loop {
        match node.kind() {
            "plain_scalar" => atomic = node.start_byte() == start && node.end_byte() == start + 8,
            "single_quote_scalar" | "double_quote_scalar" | "block_scalar" => return false,
            "flow_sequence" | "flow_mapping" if atomic => return true,
            "flow_pair" if atomic => {
                return node
                    .child_by_field_name("value")
                    .is_some_and(|value| value.start_byte() <= start && start < value.end_byte());
            }
            "block_mapping_pair" if atomic => {
                return node.child_by_field_name("value").is_some_and(|value| {
                    value.start_byte() <= start
                        && start < value.end_byte()
                        && node.start_position().row == value.start_position().row
                });
            }
            _ => {}
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        node = parent;
    }
}

fn sequence_payload_position(prefix: &str, mapping: &str, sequence: &str) -> bool {
    let mapping = Skeleton {
        text: format!("{prefix}{mapping}"),
        segments: Vec::new(),
    };
    if helm_schema_syntax::first_document_ownership(mapping).is_ok() {
        return false;
    }
    let skeleton = Skeleton {
        text: format!("{prefix}{sequence}"),
        segments: Vec::new(),
    };
    let Ok(owned) = helm_schema_syntax::first_document_ownership(skeleton) else {
        return false;
    };
    owned.nodes.iter().any(|node| {
        node.kind == YamlNodeKind::Item
            && prefix.len() <= node.span.start
            && owned
                .skeleton
                .text
                .get(node.span.start..node.span.end)
                .is_some_and(|text| text.trim() == sequence.trim())
            && owned.nodes.iter().any(|peer| {
                peer.kind == YamlNodeKind::Item
                    && peer.parent == node.parent
                    && peer.span.end <= prefix.len()
            })
    })
}

fn ends_document(owned: &YamlOwnership, span: Span) -> bool {
    let document_end = owned
        .nodes
        .iter()
        .find(|end| end.kind == YamlNodeKind::DocumentEnd && span.end <= end.span.start)
        .map_or(owned.skeleton.text.len(), |end| end.span.start);
    owned
        .skeleton
        .text
        .get(span.end..document_end)
        .is_some_and(|tail| tail.trim().is_empty())
}

fn invalid_states(
    owned: &YamlOwnership,
    node: &helm_schema_syntax::YamlNode,
    states: &mut States,
) -> Predicate {
    let mut failures = vec![
        states
            .remove(&LexicalState::Invalid)
            .unwrap_or(Predicate::False),
    ];
    if !node.key && block_parent(owned, node) == Some(YamlNodeKind::Pair) {
        for state in [LexicalState::BlockInvalid, LexicalState::ClosedCollection] {
            failures.push(states.remove(&state).unwrap_or(Predicate::False));
        }
    }
    // Helm may stop decoding after a root flow collection, ignoring a later token.
    // An enclosing block container still needs completion on this line.
    let mut parent = node.parent;
    while let Some(ancestor) = parent.and_then(|parent| owned.nodes.get(parent)) {
        if matches!(
            ancestor.kind,
            YamlNodeKind::Mapping | YamlNodeKind::Sequence
        ) {
            failures.push(
                states
                    .remove(&LexicalState::CollectionInvalid)
                    .unwrap_or(Predicate::False),
            );
            break;
        }
        parent = ancestor.parent;
    }
    Predicate::Or(failures)
}

#[cfg(test)]
#[path = "../tests/runtime_output_layout.rs"]
mod tests;

fn serialized_prefix(
    owned: &YamlOwnership,
    boundaries: &[PrefixBoundary],
    start: usize,
) -> Option<Predicate> {
    if owned.skeleton.segments.iter().any(|other| {
        other.placeholder
            && other.range.end <= start
            && !boundaries
                .iter()
                .any(|boundary| other.range.end <= boundary.span.end && boundary.span.end <= start)
    }) {
        return None;
    }
    Some(
        boundaries
            .iter()
            .take_while(|boundary| boundary.span.end <= start)
            .last()
            .map_or(Predicate::True, |boundary| boundary.preserved.clone()),
    )
}

fn serialized_failures(
    owned: &YamlOwnership,
    terms: &[NodeRef],
    boundaries: &[PrefixBoundary],
    selections: &[SelectedSpan],
    inputs: &BTreeMap<ValuesPath, super::InputBacking>,
    memo: &PredicateMemo,
    remaining: &mut usize,
) -> Predicate {
    let mut failures = vec![serialized_atomic_failures(
        owned, terms, boundaries, selections, inputs, memo, remaining,
    )];
    let mut query = super::output::PayloadQuery::new(inputs);
    for segment in owned
        .skeleton
        .segments
        .iter()
        .filter(|segment| segment.placeholder)
    {
        let Some(text) = owned
            .skeleton
            .text
            .get(segment.range.start..segment.range.end)
        else {
            continue;
        };
        let Some(pair) = owned.nodes.iter().find(|node| {
            node.kind == YamlNodeKind::Pair
                && segment.range.start <= node.span.start
                && node.span.end <= segment.range.end
                && owned
                    .skeleton
                    .text
                    .get(node.span.start..node.span.end)
                    .is_some_and(|pair| pair.trim() == text.trim())
        }) else {
            continue;
        };
        let Some(mapping_index) = pair.parent else {
            continue;
        };
        let Some(mapping) = owned.nodes.get(mapping_index) else {
            continue;
        };
        if mapping.kind != YamlNodeKind::Mapping {
            continue;
        }
        if owned.nodes.iter().any(|node| {
            node.parent == pair.parent
                && node.kind == YamlNodeKind::Pair
                && node.span.start >= pair.span.end
        }) {
            continue;
        }
        let members = owned
            .nodes
            .iter()
            .filter(|node| node.parent == Some(mapping_index) && node.kind == YamlNodeKind::Pair)
            .count()
            > 1;
        let path = mapping_path(owned, mapping_index);
        let metadata = path.as_deref().is_some_and(|path| matches!(path, [metadata, field] if metadata == "metadata" && field == "annotations"));
        let root = mapping
            .parent
            .and_then(|parent| owned.nodes.get(parent))
            .is_some_and(|parent| parent.kind == YamlNodeKind::Document);
        if !members && !metadata && !root {
            continue;
        }
        let Some(term) = terms.get(segment.piece) else {
            continue;
        };
        let placed = PlacedTerm {
            range: segment.range,
            placeholder: true,
            source_offset: None,
            node: term,
        };
        let Some(alternatives) = placed.frozen_alternatives() else {
            continue;
        };
        let Some(prefix) = serialized_prefix(owned, boundaries, segment.range.start) else {
            continue;
        };
        for (guard, value) in alternatives {
            for payload in query.bounded_value(value, remaining).known {
                if !payload.omitted_members.is_empty() {
                    continue;
                }
                failures.push(Predicate::all(vec![
                    prefix.clone(),
                    selection_guard(selections, segment.range),
                    guard.clone(),
                    serialized_failure(&payload, metadata, members),
                ]));
            }
        }
    }
    memo.normalize(Predicate::Or(failures))
}

#[derive(Clone, Copy)]
enum SerializedDecoding {
    Annotations,
    String,
    Document,
}

struct SerializedPosition {
    decoding: Option<SerializedDecoding>,
    flow: Option<YamlNodeKind>,
    inline: bool,
    sequence: bool,
}

impl SerializedPosition {
    fn new(owned: &YamlOwnership, index: usize, node: &helm_schema_syntax::YamlNode) -> Self {
        let decoding = match mapping_path(owned, index).as_deref() {
            Some([metadata, field]) if metadata == "metadata" && field == "annotations" => {
                Some(SerializedDecoding::Annotations)
            }
            Some([field]) if matches!(field.as_str(), "apiVersion" | "kind") => {
                Some(SerializedDecoding::String)
            }
            Some([metadata, field]) if metadata == "metadata" && field == "name" => {
                Some(SerializedDecoding::String)
            }
            _ if node
                .parent
                .and_then(|parent| owned.nodes.get(parent))
                .is_some_and(|parent| parent.kind == YamlNodeKind::Document) =>
            {
                Some(SerializedDecoding::Document)
            }
            _ => None,
        };
        let mut flow = None;
        let mut parent = node.parent;
        while let Some(ancestor) = parent.and_then(|parent| owned.nodes.get(parent)) {
            if matches!(
                ancestor.kind,
                YamlNodeKind::FlowMapping | YamlNodeKind::FlowSequence
            ) {
                flow = Some(ancestor.kind);
                break;
            }
            parent = ancestor.parent;
        }
        let inline = node
            .parent
            .and_then(|parent| owned.nodes.get(parent))
            .is_some_and(|parent| {
                parent.kind == YamlNodeKind::Pair
                    && owned
                        .skeleton
                        .text
                        .get(parent.span.start..node.span.start)
                        .is_some_and(|text| !text.contains('\n'))
            });
        Self {
            decoding,
            flow,
            inline,
            sequence: node.kind == YamlNodeKind::Item,
        }
    }

    fn invalid(&self, path: &ValuesPath) -> Predicate {
        if let Some(decoding) = self.decoding {
            let accepted = match decoding {
                SerializedDecoding::Annotations => Predicate::from(Guard::MatchesValue {
                    path: path.clone(),
                    pattern: helm_schema_core::ValuePattern::StringMap,
                }),
                SerializedDecoding::String => Predicate::from(Guard::TypeIs {
                    path: path.clone(),
                    schema_type: "string".to_owned(),
                }),
                SerializedDecoding::Document => Predicate::from(Guard::TypeIs {
                    path: path.clone(),
                    schema_type: "object".to_owned(),
                }),
            };
            return Predicate::Or(vec![
                Predicate::from(Guard::Absent { path: path.clone() }),
                Predicate::from(Guard::TypeIs {
                    path: path.clone(),
                    schema_type: "null".to_owned(),
                }),
                accepted,
            ])
            .negated();
        }
        let nonempty = |schema_type: &str, bound| {
            Predicate::all(vec![
                Predicate::from(Guard::TypeIs {
                    path: path.clone(),
                    schema_type: schema_type.to_owned(),
                }),
                if schema_type == "array" {
                    Predicate::truthy_path(path.encode())
                } else {
                    Predicate::from(Guard::MinMembers {
                        path: path.clone(),
                        bound,
                    })
                },
            ])
        };
        if self.sequence {
            return nonempty("array", 1).negated();
        }
        let mut invalid = vec![
            nonempty("array", 1),
            nonempty(
                "object",
                if self.flow == Some(YamlNodeKind::FlowSequence) {
                    2
                } else {
                    1
                },
            ),
        ];
        if self.flow.is_some() {
            invalid.push(Predicate::from(Guard::MatchesPattern {
                path: path.clone(),
                pattern: "\n".to_owned(),
                templated: false,
            }));
        }
        Predicate::Or(invalid)
    }
}

fn serialized_atomic_failures(
    owned: &YamlOwnership,
    terms: &[NodeRef],
    boundaries: &[PrefixBoundary],
    selections: &[SelectedSpan],
    inputs: &BTreeMap<ValuesPath, super::InputBacking>,
    memo: &PredicateMemo,
    remaining: &mut usize,
) -> Predicate {
    let mut failures = Vec::new();
    for segment in owned
        .skeleton
        .segments
        .iter()
        .filter(|segment| segment.placeholder)
    {
        let Some((node_index, node)) = owned.nodes.iter().enumerate().find(|(_, node)| {
            !node.key
                && matches!(
                    node.kind,
                    YamlNodeKind::Scalar(ScalarStyle::Plain)
                        | YamlNodeKind::Item
                        | YamlNodeKind::FlowMapping
                )
                && segment.range.start <= node.span.start
                && node.span.end <= segment.range.end
                && owned
                    .skeleton
                    .text
                    .get(segment.range.start..segment.range.end)
                    .zip(owned.skeleton.text.get(node.span.start..node.span.end))
                    .is_some_and(|(text, node)| text.trim() == node.trim())
        }) else {
            continue;
        };
        let position = SerializedPosition::new(owned, node_index, node);
        if !position.inline
            && position.flow.is_none()
            && !position.sequence
            && position.decoding.is_none()
        {
            continue;
        }
        let Some(reference) = terms.get(segment.piece) else {
            continue;
        };
        let term = PlacedTerm {
            range: segment.range,
            placeholder: true,
            source_offset: None,
            node: reference,
        };
        let Some(alternatives) = term.frozen_alternatives() else {
            continue;
        };
        let Some(prefix) = serialized_prefix(owned, boundaries, segment.range.start) else {
            continue;
        };
        let shape = if position.sequence {
            super::output::PayloadShape::Sequence
        } else if node.kind == YamlNodeKind::FlowMapping {
            super::output::PayloadShape::Mapping
        } else {
            super::output::PayloadShape::Scalar
        };
        let mut query = super::output::PayloadQuery::with_shape(inputs, shape);
        let Some(text) = owned
            .skeleton
            .text
            .get(segment.range.start..segment.range.end)
        else {
            continue;
        };
        for (guard, value) in alternatives {
            for payload in query.bounded_value(value, remaining).known {
                if !payload.omitted_members.is_empty()
                    || (node.kind == YamlNodeKind::FlowMapping
                        && position.decoding.is_none()
                        && payload.flow_node)
                {
                    continue;
                }
                if payload.spelling.trim() != text.trim() {
                    continue;
                }
                let invalid = position.invalid(payload.path);
                let coverage = if position.flow.is_some() && node.kind != YamlNodeKind::FlowMapping
                {
                    payload.flow_condition()
                } else {
                    payload.unchanged_condition()
                };
                failures.push(Predicate::all(vec![
                    prefix.clone(),
                    selection_guard(selections, segment.range),
                    guard.clone(),
                    payload.condition,
                    coverage,
                    invalid,
                ]));
            }
        }
    }
    memo.normalize(Predicate::Or(failures))
}

fn serialized_failure(
    payload: &super::output::SerializedPayload<'_>,
    metadata: bool,
    members: bool,
) -> Predicate {
    let accepted = if metadata {
        Predicate::from(Guard::MatchesValue {
            path: payload.path.clone(),
            pattern: helm_schema_core::ValuePattern::StringMap,
        })
    } else {
        Predicate::from(Guard::TypeIs {
            path: payload.path.clone(),
            schema_type: "object".to_owned(),
        })
    };
    let accepted = if members {
        Predicate::all(vec![
            accepted,
            Predicate::from(Guard::MinMembers {
                path: payload.path.clone(),
                bound: 1,
            }),
        ])
    } else if !metadata {
        Predicate::Or(vec![
            accepted,
            Predicate::from(Guard::TypeIs {
                path: payload.path.clone(),
                schema_type: "null".to_owned(),
            }),
            Predicate::from(Guard::Absent {
                path: payload.path.clone(),
            }),
        ])
    } else {
        accepted
    };
    let accepted = if members {
        accepted
    } else {
        Predicate::Or(vec![
            accepted,
            Predicate::from(Guard::Absent {
                path: payload.path.clone(),
            }),
        ])
    };
    Predicate::all(vec![
        payload.condition.clone(),
        payload.unchanged_condition(),
        accepted.negated(),
    ])
}

fn mapping_path(owned: &YamlOwnership, mut index: usize) -> Option<Vec<String>> {
    let mut path = Vec::new();
    let mut remaining = 128_usize;
    while let Some(node) = owned.nodes.get(index) {
        remaining = remaining.checked_sub(1)?;
        if matches!(node.kind, YamlNodeKind::Pair | YamlNodeKind::FlowPair) {
            let key = owned
                .nodes
                .iter()
                .find(|key| key.parent == Some(index) && key.key)?;
            if owned.skeleton.segments.iter().any(|segment| {
                segment.placeholder
                    && segment.range.start < key.span.end
                    && key.span.start < segment.range.end
            }) {
                return None;
            }
            let text = owned.skeleton.text.get(key.span.start..key.span.end)?;
            path.push(helm_schema_syntax::parse_yaml_key(&format!("{text}:"))?);
        }
        if matches!(node.kind, YamlNodeKind::Item) {
            path.push("[*]".to_owned());
        }
        let Some(parent) = node.parent else {
            break;
        };
        index = parent;
    }
    path.reverse();
    Some(path)
}
