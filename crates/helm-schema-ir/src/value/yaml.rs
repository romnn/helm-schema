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

#[derive(Clone, PartialEq, Eq)]
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
        condition: &Predicate,
        memo: &PredicateMemo,
        output: FileOutput,
    ) -> Self {
        let mut build = LayoutBuilder {
            layouts: Vec::new(),
            unknown: Vec::new(),
            remaining: 1024,
            lexical: LexicalTransfers::default(),
            root_condition: condition.clone(),
            output,
            markers: MarkerProvenance::default(),
            memo,
        };
        let LayoutContexts { known, unknown } =
            build.walk(&graph, 0, &Rc::new(Vec::new()), condition);
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
        let mut outputs = Vec::new();
        let mut failures = Vec::new();
        let mut survivors = Vec::new();
        for exit in exits {
            let condition = exit.state.path_predicate();
            let memo = &exit.state.heap.arena.predicate_memo;
            let graph = Rc::new(exit.state.output.clone());
            let placement = if matches!(exit.flow, Flow::Normal(_)) {
                OutputPlacement::new(graph, &condition, memo, output)
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
    layouts: Vec<Layout>,
    unknown: Vec<Predicate>,
    remaining: usize,
    lexical: LexicalTransfers,
    root_condition: Predicate,
    output: FileOutput,
    markers: MarkerProvenance,
    memo: &'a PredicateMemo,
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
                    let delta = self.choices(decision, [yes, no], &node, &active);
                    contexts = self.then(contexts, delta, ambient);
                }
                TextPart::Frozen(value) => {
                    if let Some(capture) = captured_layout(value, &mut 128) {
                        self.append(&mut contexts, &capture.prefix, false, &node);
                        if !contexts.known.is_empty() {
                            let active = self.active(&contexts, ambient);
                            let nested = self.walk(
                                &capture.graph,
                                indentation.saturating_add(capture.indentation),
                                &Rc::new(
                                    context.iter().chain(&output.occurrences).copied().collect(),
                                ),
                                &active,
                            );
                            contexts = self.then(contexts, nested, ambient);
                        }
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
        let mut result = LayoutContexts {
            known: Vec::new(),
            unknown: self
                .memo
                .normalize(Predicate::Or(vec![prefix_unknown, suffix_unknown])),
        };
        let append =
            |mut prefix: LayoutContext, suffix: &LayoutContext, result: &mut LayoutContexts| {
                let condition = self.memo.normalize(Predicate::all(vec![
                    prefix.condition.clone(),
                    suffix.condition.clone(),
                ]));
                if !self.feasible(ambient, &condition) {
                    return;
                }
                if result.known.len() >= 32 || !prefix.extend(suffix) {
                    result.unknown = self
                        .memo
                        .normalize(Predicate::Or(vec![result.unknown.clone(), condition]));
                    return;
                }
                prefix.condition = condition;
                result.known.push(prefix);
            };
        // A single suffix appends in place; distinct layouts duplicate the accumulated prefix.
        match suffixes.as_slice() {
            [suffix] => {
                for prefix in prefixes {
                    append(prefix, suffix, &mut result);
                }
            }
            suffixes => {
                for prefix in prefixes {
                    for suffix in suffixes {
                        append(prefix.clone(), suffix, &mut result);
                    }
                }
            }
        }
        result
    }

    fn finish_layout(&mut self, context: LayoutContext) {
        let LayoutContext {
            skeleton,
            terms,
            selections,
            condition,
        } = context;
        let removed = marker_ranges(&skeleton.text);
        self.markers.record(&skeleton, &terms, &removed);
        let owned_documents = match self.output {
            FileOutput::Text => Vec::new(),
            FileOutput::Manifest => {
                let Some(documents) = documents(&skeleton, &removed) else {
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
                        selections
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
            let result = quoted_tokens(&document.ownership, &terms, &mut self.lexical, self.memo);
            failures.push(result.invalid);
            document.boundaries = result.boundaries;
            documents.push(document);
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
            empty: skeleton.text.trim().is_empty(),
            terms,
        });
    }
}

impl LayoutContext {
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

    fn extend(&mut self, suffix: &Self) -> bool {
        let Self {
            skeleton,
            terms: suffix_terms,
            selections,
            condition: _,
        } = suffix;
        let bytes = self.skeleton.text.len();
        if bytes.saturating_add(skeleton.text.len()) > 64 * 1024 {
            return false;
        }
        let terms = self.terms.len();
        self.skeleton.text.push_str(&skeleton.text);
        for segment in &skeleton.segments {
            let SkeletonSegment {
                range,
                piece,
                placeholder,
            } = *segment;
            self.skeleton.segments.push(SkeletonSegment {
                range: Span::new(bytes + range.start, bytes + range.end),
                piece: terms + piece,
                placeholder,
            });
        }
        for NodeRef {
            buffer,
            index,
            start,
            indentation,
            context,
        } in suffix_terms
        {
            self.terms.push(NodeRef {
                buffer: Rc::clone(buffer),
                index: *index,
                start: bytes + start,
                indentation: *indentation,
                context: Rc::clone(context),
            });
        }
        for SelectedSpan { range, condition } in selections {
            self.selections.push(SelectedSpan {
                range: Span::new(bytes + range.start, bytes + range.end),
                condition: condition.clone(),
            });
        }
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
                .filter(|(state, _)| state.is_absorbing())
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
        if self.remaining == 0 && !state.is_absorbing() {
            return States::new();
        }
        self.remaining = self.remaining.saturating_sub(1);
        let initial = BTreeMap::from([(state, Predicate::True)]);
        let result = if state.is_absorbing() {
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
            lexical.flow_tail(owned, node, terms, &states, memo),
        ]));
        failures.push(Predicate::all(vec![
            prefix.clone(),
            invalid_states(owned, node, &mut states),
        ]));
        if ends_document(owned, node.span) {
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
