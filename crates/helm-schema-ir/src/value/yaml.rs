//! YAML ownership and lexical coverage sealed from the executed output graph.

mod spelling;
pub(crate) use spelling::PayloadShape;
pub(crate) use spelling::SpellingTransfer;
use spelling::Spellings;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use ahash::AHashMap;
use helm_schema_core::{
    CollectionDomain, CollectionQuantifier, Guard, PlainScalarPlacement, Predicate, PredicateMemo,
    ValuesPath,
};
use helm_schema_syntax::{
    ScalarStyle, Skeleton, SkeletonSegment, Span, YamlNodeKind, YamlOwnership,
};

use super::quoted::{self, LexicalState, States};
use super::relationship::TextFormat;
use super::{
    Coverage, Datum, Exit, FileOutput, Flow, FrozenTest, FrozenValue, Leaf, ObservationId,
    OutputNode, TextBuffer, TextPart, ValueNode,
};

// Helm 4.2.3 SplitManifests uses Go's ASCII Perl whitespace class.
static SPLITTER: std::sync::LazyLock<Result<regex::Regex, regex::Error>> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"(?:^|[ \t\r\n\x0c]*\n)---[ \t\r\n\x0c]*"));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SerializerSpelling {
    shape: PayloadShape,
    flow_node: bool,
}

#[derive(Clone, Debug)]
struct NodeRef {
    buffer: Rc<TextBuffer>,
    index: usize,
    start: usize,
    indentation: usize,
    context: Rc<Vec<ObservationId>>,
    serializer: Option<SerializerSpelling>,
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
            serializer,
        } = self;
        let Self {
            buffer: _,
            index: _,
            start: other_start,
            indentation: other_indentation,
            context: other_context,
            serializer: other_shape,
        } = other;
        if start != other_start
            || indentation != other_indentation
            || context != other_context
            || serializer != other_shape
        {
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
            serializer: *serializer,
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
    node_index: Rc<std::cell::OnceCell<Vec<IndexedNode>>>,
    offset: usize,
    scope: Span,
    condition: Predicate,
    boundaries: Vec<PrefixBoundary>,
    selections: Vec<SelectedSpan>,
    serializations: Vec<SerializationSite>,
}

#[derive(Debug, Default)]
struct IndexedNode {
    children: Vec<usize>,
    literal_key: std::cell::OnceCell<Option<String>>,
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

    pub(crate) fn execution_guard(
        &self,
        execution: &super::DocumentExecution,
        memo: &PredicateMemo,
        remaining: &mut usize,
    ) -> Predicate {
        let unknown = || {
            Predicate::approximate_output_selection(
                format!(
                    "unobserved output {}:{} {:?}",
                    self.node.start, self.node.index, self.node.context
                ),
                self.inputs()
                    .into_iter()
                    .map(|path| path.encode())
                    .collect(),
                Predicate::False,
            )
        };
        let Some(nodes) = self.emitted_nodes() else {
            return unknown();
        };
        let mut guards = Vec::new();
        for (selected, node) in nodes {
            if node.occurrences.is_empty() {
                return unknown();
            }
            let mut observations = Vec::new();
            for id in &node.occurrences {
                for observation in execution.observations(*id) {
                    let Some(next) = remaining.checked_sub(1) else {
                        return unknown();
                    };
                    *remaining = next;
                    observations.push(match observation {
                        super::Observation::Text(site) => site.condition().clone(),
                        super::Observation::Value(site) => site.condition(),
                    });
                }
            }
            if observations.is_empty() {
                return unknown();
            }
            guards.push(Predicate::all(vec![selected, Predicate::Or(observations)]));
        }
        memo.normalize(Predicate::Or(guards))
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

impl<'a> PlacedDocument<'a> {
    fn indexed_nodes(&self) -> &[IndexedNode] {
        self.document.node_index.get_or_init(|| {
            let mut nodes = std::iter::repeat_with(IndexedNode::default)
                .take(self.ownership.nodes.len())
                .collect::<Vec<_>>();
            for (index, node) in self.ownership.nodes.iter().enumerate() {
                if let Some(parent) = node.parent.and_then(|parent| nodes.get_mut(parent)) {
                    parent.children.push(index);
                }
            }
            nodes
        })
    }

    pub(crate) fn children(&self, index: usize) -> &[usize] {
        self.indexed_nodes()
            .get(index)
            .map_or(&[], |node| node.children.as_slice())
    }

    pub(crate) fn roots(&self) -> impl Iterator<Item = usize> + '_ {
        self.ownership
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| node.parent.is_none().then_some(index))
    }

    pub(crate) fn literal_key(&self, index: usize) -> Option<&str> {
        let node = self.ownership.nodes.get(index)?;
        self.indexed_nodes()
            .get(index)?
            .literal_key
            .get_or_init(|| {
                if self.terms(node.span).any(|term| term.placeholder) {
                    return None;
                }
                let text = self
                    .ownership
                    .skeleton
                    .text
                    .get(node.span.start..node.span.end)?;
                helm_schema_syntax::parse_yaml_key(&format!("{text}:"))
            })
            .as_deref()
    }

    pub(crate) fn completion_guard(&self) -> Predicate {
        let end = self.ownership.skeleton.text.len();
        self.placement_guard(Span::new(end, end))
    }
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

    pub(crate) fn terms(&self, span: Span) -> impl Iterator<Item = PlacedTerm<'a>> {
        let segments = &self.ownership.skeleton.segments;
        let start = segments.partition_point(|segment| segment.range.end <= span.start);
        segments
            .get(start..)
            .unwrap_or_default()
            .iter()
            .take_while(move |segment| segment.range.start < span.end)
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

    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::placement_queries"
    )]
    pub(crate) fn selection_guard(&self, span: Span) -> Predicate {
        selection_guard(&self.document.selections, span)
    }

    pub(crate) fn placement_guard(&self, span: Span) -> Predicate {
        Predicate::all(vec![self.selection_guard(span), self.prefix_guard(span)])
    }

    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::placement_queries"
    )]
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
pub(crate) struct OutputPlacement {
    layouts: Vec<Layout>,
    unknown: Predicate,
    markers: MarkerProvenance,
}

#[derive(Debug, Default)]
struct MarkerProvenance {
    affected: BTreeSet<ObservationId>,
    incomplete: bool,
}

#[derive(Clone, Copy)]
struct PlacementSource<'a> {
    graph: &'a Rc<TextBuffer>,
    condition: &'a Predicate,
}

impl OutputPlacement {
    fn new(
        graph: &Rc<TextBuffer>,
        condition: &Predicate,
        memo: &PredicateMemo,
        output: FileOutput,
        syntax: &mut SyntaxContexts,
        input_paths: &super::input_paths::InputPaths,
        input_objects: &BTreeMap<ValuesPath, super::InputBacking>,
    ) -> Self {
        let mut remaining = 1024;
        Self::bounded(
            PlacementSource { graph, condition },
            memo,
            output,
            syntax,
            input_paths,
            input_objects,
            &mut remaining,
        )
    }

    pub(crate) fn placed_documents(&self) -> impl Iterator<Item = PlacedDocument<'_>> {
        self.layouts.iter().flat_map(|layout| {
            layout.documents.iter().map(move |document| PlacedDocument {
                condition: document.condition.clone(),
                ownership: &document.ownership,
                layout,
                document,
            })
        })
    }

    fn bounded(
        source: PlacementSource<'_>,
        memo: &PredicateMemo,
        output: FileOutput,
        syntax: &mut SyntaxContexts,
        input_paths: &super::input_paths::InputPaths,
        input_objects: &BTreeMap<ValuesPath, super::InputBacking>,
        remaining: &mut usize,
    ) -> Self {
        let PlacementSource { graph, condition } = source;
        let limit = (*remaining).min(1024);
        let mut build = LayoutBuilder {
            layouts: Vec::new(),
            unknown: Vec::new(),
            remaining: limit,
            lexical: LexicalTransfers::new(input_paths, input_objects),
            input_objects,
            root_condition: condition.clone(),
            output,
            markers: MarkerProvenance::default(),
            memo,
            syntax,
        };
        build.lexical.remaining = limit;
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
        let cost = limit
            .saturating_sub(build.remaining)
            .saturating_add(limit.saturating_sub(build.lexical.remaining));
        *remaining = remaining.saturating_sub(cost);
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

    fn opaque_block_text(&self, site: &super::ExecutedSite, memo: &PredicateMemo) -> bool {
        let condition = site.condition();
        if !memo.exactly_implies(&condition, &self.unknown.negated()) {
            return false;
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
            for document in &layout.documents {
                let owned = &document.ownership;
                for segment in &owned.skeleton.segments {
                    if !segment.placeholder
                        || !layout
                            .terms
                            .get(segment.piece)
                            .and_then(NodeRef::node)
                            .is_some_and(|node| node.occurrences.contains(&id))
                    {
                        continue;
                    }
                    if !owned.nodes.iter().any(|node| {
                        node.kind == YamlNodeKind::BlockScalar
                            && node.span.start <= segment.range.start
                            && segment.range.end <= node.span.end
                    }) {
                        return false;
                    }
                    found = true;
                }
            }
        }
        found
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
        let abort = Predicate::all(vec![
            Predicate::Or(failures),
            Predicate::Or(survivors).negated(),
        ]);
        let abort = match exits.first() {
            Some(exit) => exit.state.heap.arena.predicate_memo.normalize(abort),
            None => abort.normalize_boolean(),
        };
        Self { outputs, abort }
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
            let opaque_sites = state
                .sites
                .iter()
                .filter(|site| {
                    !state.output_widened
                        && site.rendered
                        && site.rendered_unchanged
                        && site.coverage_exact()
                        && placement.opaque_block_text(site, &state.heap.arena.predicate_memo)
                })
                .cloned()
                .collect::<Vec<_>>();
            for site in opaque_sites {
                state.record_opaque_site(&site);
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
struct FrozenSpelling {
    text: Option<String>,
    independent: bool,
}

#[derive(Clone)]
struct LayoutContext {
    skeleton: Skeleton,
    terms: Vec<NodeRef>,
    selections: Vec<SelectedSpan>,
    condition: Predicate,
}

#[derive(Clone)]
struct LayoutContexts {
    known: Vec<LayoutContext>,
    unknown: Predicate,
}

#[derive(PartialEq, Eq)]
struct LayoutVariant {
    condition: Predicate,
    spelling: String,
    segments: Vec<SkeletonSegment>,
    serializers: Vec<Option<SerializerSpelling>>,
}

struct CompletedLayout {
    context: LayoutContext,
    documents: Option<Vec<DocumentLayout>>,
    removed: Vec<Span>,
}

struct CompletionCandidate {
    piece: usize,
    scalar_only: bool,
}

fn completion_candidates(
    context: &LayoutContext,
    documents: &[DocumentLayout],
) -> Vec<CompletionCandidate> {
    let flow_pieces = documents
        .iter()
        .flat_map(|document| serialization_sites(&document.ownership, &[], &context.terms))
        .filter(|site| site.position.flow.is_some())
        .map(|site| site.piece)
        .collect::<BTreeSet<_>>();
    let mut candidates = Vec::new();
    for (piece, reference) in context
        .terms
        .iter()
        .enumerate()
        .filter(|(_, term)| term.serializer.is_some())
    {
        let Some(segment) = context
            .skeleton
            .segments
            .iter()
            .find(|segment| segment.placeholder && segment.piece == piece)
        else {
            continue;
        };
        let owner = documents.iter().find(|document| {
            document.scope.start <= segment.range.start && segment.range.end <= document.scope.end
        });
        let flow_scalar = flow_pieces.contains(&piece)
            && reference.serializer.is_some_and(|spelling| {
                spelling.shape != PayloadShape::Scalar && !spelling.flow_node
            });
        if owner.is_none()
            && reference
                .serializer
                .is_some_and(|spelling| spelling.shape != PayloadShape::Mapping)
        {
            continue;
        }
        if owner.is_none() || flow_scalar {
            candidates.push(CompletionCandidate {
                piece,
                scalar_only: flow_scalar,
            });
        }
    }
    candidates
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
                serializer: None,
            };
            match output.content() {
                TextPart::Select { decision, yes, no } => {
                    let active = self.active(&contexts, ambient);
                    let mut delta = self.choices(decision, [yes, no], &node, &active);
                    self.local_choices(&contexts, &mut delta);
                    contexts = self.then(contexts, delta, ambient);
                }
                TextPart::Frozen(value) => {
                    let spelling = self.syntax.frozen_spelling(value);
                    if let Some(text) = &spelling.text
                        && spelling.independent
                    {
                        self.append(&mut contexts, text, false, &node);
                        continue;
                    }
                    let payload = self.payload(value);
                    if !payload.known.is_empty()
                        && payload.unknown != Predicate::False
                        && let Some(capture) = captured_layout(value, &mut 128)
                    {
                        let active = self.active(&contexts, ambient);
                        let captured = self.capture(&capture, &node, &active);
                        contexts = self.then(contexts, captured, ambient);
                    } else if !payload.known.is_empty() {
                        let delta = self.payload_layouts(payload, &node);
                        contexts = self.then(contexts, delta, ambient);
                    } else if let Some(capture) = captured_layout(value, &mut 128) {
                        let active = self.active(&contexts, ambient);
                        let captured = self.capture(&capture, &node, &active);
                        contexts = self.then(contexts, captured, ambient);
                    } else {
                        let text = spelling.text;
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

    #[tracing::instrument(skip_all)]
    fn payload_layouts(&self, payload: Spellings, node: &NodeRef) -> LayoutContexts {
        let mut spellings = BTreeMap::<(String, bool), Vec<Predicate>>::new();
        for arm in payload.known {
            spellings
                .entry((arm.bytes, arm.flow_node))
                .or_default()
                .push(arm.condition);
        }
        let mut delta = LayoutContexts {
            known: Vec::new(),
            unknown: Predicate::False,
        };
        for ((spelling, flow_node), guards) in spellings {
            let mut layout = LayoutContext::empty();
            layout.condition = self.memo.normalize(Predicate::Or(guards));
            let mut node = node.clone();
            node.serializer = Some(SerializerSpelling {
                shape: PayloadShape::Mapping,
                flow_node,
            });
            if layout.append_selected(&spelling, node) {
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
        delta
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

    fn payload(&mut self, value: &FrozenValue) -> Spellings {
        self.syntax
            .payload(value, PayloadShape::Mapping, &mut self.remaining)
    }

    fn feasible(&self, ambient: &Predicate, condition: &Predicate) -> bool {
        self.memo
            .may_be_true(Predicate::all(vec![ambient.clone(), condition.clone()]))
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
        if Rc::ptr_eq(yes, no) {
            return self.walk(yes, node.indentation, &node.context, ambient);
        }
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
        if delta.unknown != Predicate::False || delta.known.len() < 2 {
            return;
        }
        let mut contribution = None;
        for branch in &delta.known {
            if branch.skeleton.text.trim().is_empty() {
                continue;
            }
            let Some(found) = self.syntax.block_contribution(&branch.skeleton.text) else {
                return;
            };
            if contribution.is_some_and(|previous| previous != found) {
                return;
            }
            contribution = Some(found);
        }
        let Some(contribution) = contribution else {
            return;
        };
        if !prefixes.known.iter().all(|prefix| {
            self.syntax
                .contribution_host(&prefix.skeleton, contribution)
                .is_some()
        }) {
            return;
        }
        let exclusive = delta.known.iter().enumerate().all(|(index, branch)| {
            delta.known.iter().skip(index + 1).all(|other| {
                self.memo.normalize(Predicate::all(vec![
                    branch.condition.clone(),
                    other.condition.clone(),
                ])) == Predicate::False
            })
        });
        if !exclusive
            && delta.known.iter().any(|branch| {
                branch
                    .skeleton
                    .segments
                    .iter()
                    .any(|segment| segment.placeholder)
            })
        {
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

    #[tracing::instrument(skip_all)]
    fn complete_layout(&mut self, initial: LayoutContext) -> Vec<CompletedLayout> {
        if initial.terms.iter().all(|term| term.serializer.is_none()) {
            if self.remaining == 0 {
                self.unknown.push(initial.condition);
                return Vec::new();
            }
            self.remaining -= 1;
            let removed = marker_ranges(&initial.skeleton.text);
            let documents = self.syntax.documents(&initial.skeleton, &removed);
            return vec![CompletedLayout {
                context: initial,
                documents,
                removed,
            }];
        }
        let mut pending = vec![initial];
        let mut seen = Vec::new();
        let mut complete = Vec::new();
        let mut unresolved = Vec::new();
        while let Some(context) = pending.pop() {
            if self.remaining == 0 || seen.len() >= 32 {
                unresolved.push(context.condition);
                unresolved.extend(pending.into_iter().map(|context| context.condition));
                break;
            }
            self.remaining -= 1;
            let variant = LayoutVariant {
                condition: context.condition.clone(),
                spelling: context.skeleton.text.clone(),
                segments: context.skeleton.segments.clone(),
                serializers: context.terms.iter().map(|term| term.serializer).collect(),
            };
            if seen.contains(&variant) {
                continue;
            }
            seen.push(variant);
            let removed = marker_ranges(&context.skeleton.text);
            let documents = self.syntax.documents(&context.skeleton, &removed);
            let owned = documents.as_deref().unwrap_or_default();
            let candidates = completion_candidates(&context, owned);
            if candidates.is_empty() {
                let unowned = context.skeleton.segments.iter().any(|segment| {
                    segment.placeholder
                        && context
                            .terms
                            .get(segment.piece)
                            .is_some_and(|term| term.serializer.is_some())
                        && !owned.iter().any(|document| {
                            document.scope.start <= segment.range.start
                                && segment.range.end <= document.scope.end
                        })
                });
                if unowned {
                    unresolved.push(context.condition);
                } else {
                    complete.push(CompletedLayout {
                        context,
                        documents,
                        removed,
                    });
                }
                continue;
            }
            let mut changed = false;
            for candidate in candidates.into_iter().rev() {
                let mut repairs = self.repair_spelling(&context, &candidate);
                changed |= !repairs.is_empty();
                for repaired in repairs.drain(..) {
                    if pending.len() + complete.len() < 32 {
                        pending.push(repaired);
                    } else {
                        self.unknown.push(repaired.condition);
                    }
                }
            }
            if !changed {
                unresolved.push(context.condition);
            }
        }
        let covered = self.memo.normalize(Predicate::Or(
            complete
                .iter()
                .map(|completed| completed.context.condition.clone())
                .collect(),
        ));
        for condition in unresolved {
            let unknown = self
                .memo
                .normalize(Predicate::all(vec![condition, covered.negated()]));
            if unknown != Predicate::False {
                self.unknown.push(unknown);
            }
        }
        complete
    }

    #[tracing::instrument(skip_all)]
    fn repair_spelling(
        &mut self,
        context: &LayoutContext,
        candidate: &CompletionCandidate,
    ) -> Vec<LayoutContext> {
        let Some(reference) = context.terms.get(candidate.piece) else {
            return Vec::new();
        };
        let Some(segment) = context
            .skeleton
            .segments
            .iter()
            .find(|segment| segment.placeholder && segment.piece == candidate.piece)
        else {
            return Vec::new();
        };
        let term = PlacedTerm {
            range: segment.range,
            placeholder: true,
            source_offset: None,
            node: reference,
        };
        let Some(alternatives) = term.frozen_alternatives() else {
            return Vec::new();
        };
        let mut repairs = Vec::new();
        for shape in [
            PayloadShape::Mapping,
            PayloadShape::Scalar,
            PayloadShape::Sequence,
        ] {
            if reference
                .serializer
                .is_some_and(|spelling| spelling.shape == shape)
                || candidate.scalar_only && shape != PayloadShape::Scalar
            {
                continue;
            }
            let mut query = SpellingTransfer::new(shape, self.remaining);
            for (guard, value) in &alternatives {
                for payload in query.value(value).known {
                    if candidate.scalar_only && payload.flow_node {
                        continue;
                    }
                    let condition = self.memo.normalize(Predicate::all(vec![
                        context.condition.clone(),
                        guard.clone(),
                        payload.condition,
                    ]));
                    if condition == Predicate::False {
                        continue;
                    }
                    let mut repaired = context.clone();
                    if repaired.replace_spelling(
                        candidate.piece,
                        &payload.bytes,
                        SerializerSpelling {
                            shape,
                            flow_node: payload.flow_node,
                        },
                    ) {
                        repaired.condition = condition;
                        repairs.push(repaired);
                    }
                }
            }
            self.remaining = self.remaining.saturating_sub(query.work());
        }
        repairs
    }

    #[tracing::instrument(skip_all, fields(manifest = matches!(self.output, FileOutput::Manifest)))]
    fn finish_layout(&mut self, context: LayoutContext) {
        if matches!(self.output, FileOutput::Text) {
            let removed = marker_ranges(&context.skeleton.text);
            self.seal_layout(CompletedLayout {
                context,
                documents: Some(Vec::new()),
                removed,
            });
            return;
        }
        for context in self.complete_layout(context) {
            self.seal_layout(context);
        }
    }

    fn seal_layout(&mut self, completed: CompletedLayout) {
        let CompletedLayout {
            context,
            documents,
            mut removed,
        } = completed;
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
        self.markers.record(&skeleton, &terms, &removed);
        let mut owned_documents = match self.output {
            FileOutput::Text => Vec::new(),
            FileOutput::Manifest => {
                let Some(documents) = documents else {
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
            document.serializations =
                serialization_sites(&document.ownership, &document.selections, &terms);
            let result = quoted_tokens(
                &document.ownership,
                &document.serializations,
                &terms,
                &mut self.lexical,
                self.input_objects,
                &document.selections,
                self.memo,
            );
            failures.push(result.invalid);

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
    fn replace_spelling(
        &mut self,
        piece: usize,
        spelling: &str,
        class: SerializerSpelling,
    ) -> bool {
        let Some(range) = self
            .skeleton
            .segments
            .iter()
            .find(|segment| segment.piece == piece && segment.placeholder)
            .map(|segment| segment.range)
        else {
            return false;
        };
        let Ok(added) = isize::try_from(spelling.len()) else {
            return false;
        };
        let Ok(removed) = isize::try_from(range.end.saturating_sub(range.start)) else {
            return false;
        };
        let Some(size) = self.skeleton.text.len().checked_add_signed(added - removed) else {
            return false;
        };
        if size > 64 * 1024 || self.skeleton.text.get(range.start..range.end) == Some(spelling) {
            return false;
        }
        let translate = |position: usize| {
            if position <= range.start {
                Some(position)
            } else if position >= range.end {
                position.checked_add_signed(added - removed)
            } else {
                None
            }
        };
        let mut selections = self.selections.clone();
        for selection in &mut selections {
            let (Some(start), Some(end)) = (
                translate(selection.range.start),
                translate(selection.range.end),
            ) else {
                return false;
            };
            selection.range = Span::new(start, end);
        }
        let mut segments = self.skeleton.segments.clone();
        for segment in &mut segments {
            if segment.piece == piece {
                segment.range = Span::new(range.start, range.start + spelling.len());
            } else {
                let (Some(start), Some(end)) =
                    (translate(segment.range.start), translate(segment.range.end))
                else {
                    return false;
                };
                segment.range = Span::new(start, end);
            }
        }
        let mut terms = self.terms.clone();
        for (index, term) in terms.iter_mut().enumerate() {
            if index == piece {
                term.serializer = Some(class);
            } else if term.start >= range.end {
                let Some(start) = translate(term.start) else {
                    return false;
                };
                term.start = start;
            }
        }
        self.skeleton
            .text
            .replace_range(range.start..range.end, spelling);
        self.skeleton.segments = segments;
        self.terms = terms;
        self.selections = selections;
        true
    }

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
            serializer: _,
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
    frozen: BTreeMap<(*const super::HeapVersion, *const ValueNode), FrozenSpelling>,
    frozen_bytes: usize,
    spellings:
        BTreeMap<(*const super::HeapVersion, *const ValueNode, PayloadShape), CachedSpelling>,
    spelling_bytes: usize,
    entries: ahash::AHashMap<Skeleton, SyntaxContext>,
    bytes: usize,
    contributions: BTreeMap<String, Option<BlockContribution>>,
    contribution_bytes: usize,
}

struct CachedSpelling {
    result: Spellings,
    work: usize,
}

struct SyntaxContext {
    documents: Option<Vec<DocumentLayout>>,
    hosts: Vec<ContributionHost>,
}

struct ContributionHost {
    contribution: BlockContribution,
    destination: Option<ContributionDestination>,
}

impl SyntaxContexts {
    fn frozen_spelling(&mut self, value: &FrozenValue) -> FrozenSpelling {
        let key = (Rc::as_ptr(&value.heap), Rc::as_ptr(&value.value.node));
        if let Some(spelling) = self.frozen.get(&key) {
            return spelling.clone();
        }
        let text = value.exact_text();
        let dependent = match value.value.node.as_ref() {
            ValueNode::Leaf(Leaf { datum: _, origin }) => {
                origin.identity.is_some() || !origin.influences.is_empty()
            }
            ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => false,
        };
        let spelling = FrozenSpelling {
            independent: text.is_some() && !dependent && value.inputs().is_empty(),
            text,
        };
        let cost = spelling.text.as_ref().map_or(0, String::len);
        if self.frozen.len() < 1024 && self.frozen_bytes.saturating_add(cost) <= 1024 * 1024 {
            self.frozen_bytes += cost;
            self.frozen.insert(key, spelling.clone());
        }
        spelling
    }

    fn payload(
        &mut self,
        value: &FrozenValue,
        shape: PayloadShape,
        remaining: &mut usize,
    ) -> Spellings {
        let key = (
            Rc::as_ptr(&value.heap),
            Rc::as_ptr(&value.value.node),
            shape,
        );
        if let Some(cached) = self.spellings.get(&key)
            && *remaining > cached.work
        {
            *remaining -= cached.work;
            return cached.result.clone();
        }
        let mut query = SpellingTransfer::new(shape, *remaining);
        let result = query.value(value);
        let work = query.work();
        let cost = result
            .known
            .iter()
            .map(|arm| arm.bytes.len())
            .sum::<usize>();
        if work < *remaining
            && self.spellings.len() < 1024
            && self.spelling_bytes.saturating_add(cost) <= 1024 * 1024
        {
            self.spelling_bytes += cost;
            self.spellings.insert(
                key,
                CachedSpelling {
                    result: result.clone(),
                    work,
                },
            );
        }
        *remaining = remaining.saturating_sub(work);
        result
    }

    fn block_contribution(&mut self, text: &str) -> Option<BlockContribution> {
        if let Some(cached) = self.contributions.get(text) {
            return *cached;
        }
        let result = block_contribution(text);
        if self.contributions.len() < 256
            && self.contribution_bytes.saturating_add(text.len()) <= 256 * 1024
        {
            self.contribution_bytes += text.len();
            self.contributions.insert(text.to_owned(), result);
        }
        result
    }

    fn contribution_host(
        &mut self,
        skeleton: &Skeleton,
        contribution: BlockContribution,
    ) -> Option<ContributionDestination> {
        if skeleton.text.trim().is_empty() {
            return (contribution.indentation == 0).then_some(ContributionDestination::Node);
        }
        if let Some(entry) = self.entries.get(skeleton)
            && let Some(host) = entry
                .hosts
                .iter()
                .find(|host| host.contribution == contribution)
        {
            return host.destination;
        }
        let destination = self
            .documents(skeleton, &[])
            .and_then(|documents| contribution_host(&documents.last()?.ownership, contribution));
        if let Some(entry) = self.entries.get_mut(skeleton)
            && entry.hosts.len() < 32
        {
            entry.hosts.push(ContributionHost {
                contribution,
                destination,
            });
        }
        destination
    }

    #[tracing::instrument(skip_all, name = "syntax_context", fields(cache_hit = tracing::field::Empty))]
    fn documents(&mut self, skeleton: &Skeleton, removed: &[Span]) -> Option<Vec<DocumentLayout>> {
        let skeleton = if removed.is_empty() {
            Cow::Borrowed(skeleton)
        } else {
            Cow::Owned(stripped_skeleton(skeleton, removed))
        };
        if let Some(entry) = self.entries.get(skeleton.as_ref()) {
            tracing::Span::current().record("cache_hit", true);
            return entry.documents.clone();
        }
        tracing::Span::current().record("cache_hit", false);
        let documents = documents(&skeleton);
        let cost = skeleton
            .text
            .len()
            .saturating_mul(3)
            .saturating_add(
                skeleton
                    .segments
                    .len()
                    .saturating_mul(std::mem::size_of::<SkeletonSegment>()),
            )
            .saturating_add(
                documents
                    .as_ref()
                    .into_iter()
                    .flatten()
                    .map(|document| {
                        document.ownership.nodes.len().saturating_mul(
                            std::mem::size_of::<helm_schema_syntax::YamlNode>()
                                + std::mem::size_of::<IndexedNode>(),
                        )
                    })
                    .sum::<usize>(),
            );
        // Cache only syntax; execution guards and lexical boundaries remain with each layout.
        if self.entries.len() < 2048 && self.bytes.saturating_add(cost) <= 16 * 1024 * 1024 {
            self.bytes += cost;
            self.entries.insert(
                skeleton.into_owned(),
                SyntaxContext {
                    documents: documents.clone(),
                    hosts: Vec::new(),
                },
            );
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
            node_index: Rc::default(),
            offset: start,
            scope,
            condition: Predicate::True,
            boundaries: Vec::new(),
            selections: Vec::new(),
            serializations: Vec::new(),
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

#[derive(Clone, PartialEq, Eq, Hash)]
struct BufferContext {
    graph: *const TextBuffer,
    state: LexicalState,
    transforms: Vec<super::relationship::TextTransform>,
}

struct LexicalTransfers<'a> {
    /// Address-keyed, so each entry holds its key alive: a key never names a reused address
    /// (layouts build and drop temporary choice buffers while this cache lives).
    values: AHashMap<
        (*const super::HeapVersion, *const ValueNode, LexicalState),
        (FrozenValue, States),
    >,
    buffers: AHashMap<BufferContext, (Rc<TextBuffer>, States)>,
    remaining: usize,
    input_paths: &'a super::input_paths::InputPaths,
    input_objects: &'a BTreeMap<ValuesPath, super::InputBacking>,
    map_depth: usize,
}

impl<'a> LexicalTransfers<'a> {
    fn new(
        input_paths: &'a super::input_paths::InputPaths,
        input_objects: &'a BTreeMap<ValuesPath, super::InputBacking>,
    ) -> Self {
        Self {
            values: AHashMap::new(),
            buffers: AHashMap::new(),
            remaining: 4096,
            input_paths,
            input_objects,
            map_depth: 0,
        }
    }

    fn value(&mut self, value: &FrozenValue, state: LexicalState, memo: &PredicateMemo) -> States {
        let FrozenValue {
            heap,
            value: super::Value { node },
        } = value;
        let key = (Rc::as_ptr(heap), Rc::as_ptr(node), state);
        if let Some((_, result)) = self.values.get(&key) {
            return result.clone();
        }
        if self.remaining == 0 && !state.is_absorbing() {
            return States::new();
        }
        self.remaining = self.remaining.saturating_sub(1);
        let initial = BTreeMap::from([(state, Predicate::True)]);
        let result = if state.is_absorbing() {
            initial
        } else if let Some(helm_schema_core::GuardValue::String(text)) = value.literal() {
            quoted::literal(initial, text, memo)
        } else if let Some(derivation) = value.value.operation_preimage() {
            self.derived(derivation, state, memo)
        } else if let Some(path) = value.identity() {
            if value.go_type() == super::kinds::GoType::String {
                quoted::input(initial, path, memo)
            } else {
                quoted::formatted_input(initial, path, value.go_type(), memo)
            }
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
                            &FrozenValue::at(Rc::clone(&value.heap), branch.clone()),
                            state,
                            memo,
                        )
                    });
                    Self::choice(decision, branches, memo)
                }
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
        self.values.insert(key, (value.clone(), result.clone()));
        result
    }

    fn derived(
        &mut self,
        derivation: &super::relationship::Derivation,
        state: LexicalState,
        memo: &PredicateMemo,
    ) -> States {
        match &derivation.operation {
            super::relationship::ValueOperation::Text(transform) => self.transformed(
                &derivation.source,
                state,
                std::slice::from_ref(transform),
                memo,
            ),
            super::relationship::ValueOperation::Format(TextFormat::Value | TextFormat::String) => {
                self.stringified(&derivation.source, state, memo)
            }
            super::relationship::ValueOperation::Decode(_)
            | super::relationship::ValueOperation::Capability {
                guard: _,
                decision: _,
            }
            | super::relationship::ValueOperation::Trim
            | super::relationship::ValueOperation::Numeric(_)
            | super::relationship::ValueOperation::Descriptor { kind_only: _ }
            | super::relationship::ValueOperation::Member(_)
            | super::relationship::ValueOperation::Copy
            | super::relationship::ValueOperation::Omit(_)
            | super::relationship::ValueOperation::Pick(_)
            | super::relationship::ValueOperation::Merge { overwrite: _ } => States::new(),
        }
    }

    fn stringified(
        &mut self,
        source: &FrozenValue,
        state: LexicalState,
        memo: &PredicateMemo,
    ) -> States {
        if let ValueNode::Select { decision, yes, no } = source.value.node.as_ref() {
            if self.remaining == 0 {
                return States::new();
            }
            self.remaining -= 1;
            let branches = [yes, no].map(|branch| {
                self.stringified(
                    &FrozenValue::at(Rc::clone(&source.heap), branch.clone()),
                    state,
                    memo,
                )
            });
            return Self::choice(decision, branches, memo);
        }
        let path = source.identity();
        if let Some(path) = path
            && matches!(
                source.go_type(),
                super::kinds::GoType::Map
                    | super::kinds::GoType::Slice
                    | super::kinds::GoType::Unknown
            )
        {
            let mut query = super::relationship::RelationshipQuery::new(
                self.input_objects,
                self.remaining.min(1024),
            );
            let unchanged = query.original_unchanged(path, &source.heap);
            self.remaining = self.remaining.saturating_sub(query.work());
            if !unchanged {
                return States::new();
            }
        }
        let mut states = self.value(source, state, memo);
        // Sprig prints nil as <nil>, outside the raw-emission alphabet.
        if let Some(path) = path {
            let nil = super::kinds::input_nil_predicate(path);
            for condition in states.values_mut() {
                *condition = memo.normalize(Predicate::all(vec![
                    condition.clone(),
                    nil.clone().negated(),
                ]));
            }
        }
        states
    }

    fn transformed(
        &mut self,
        value: &FrozenValue,
        state: LexicalState,
        transforms: &[super::relationship::TextTransform],
        memo: &PredicateMemo,
    ) -> States {
        if transforms.len() > 32 || self.remaining == 0 {
            return States::new();
        }
        self.remaining -= 1;
        let prefix = super::text_actions::prefix(value, transforms, &mut self.remaining);
        let proved = quoted::literal(BTreeMap::from([(state, Predicate::True)]), &prefix, memo);
        if proved.keys().any(|state| state.is_absorbing()) {
            return proved;
        }
        if let Some(text) = value.exact_text() {
            let transformed = quoted::mapped_text(&text, transforms);
            return transformed.map_or_else(States::new, |text| {
                quoted::literal(BTreeMap::from([(state, Predicate::True)]), &text, memo)
            });
        }
        if let Some(path) = value.identity() {
            return quoted::transformed_input(state, path, transforms, memo);
        }
        if let Some(derivation) = value.value.operation_preimage() {
            if let super::relationship::ValueOperation::Text(transform) = &derivation.operation {
                let mut composed = vec![transform.clone()];
                composed.extend_from_slice(transforms);
                return self.transformed(&derivation.source, state, &composed, memo);
            }
            return States::new();
        }
        match value.value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => {
                let branches = [yes, no].map(|branch| {
                    self.transformed(
                        &FrozenValue::at(Rc::clone(&value.heap), branch.clone()),
                        state,
                        transforms,
                        memo,
                    )
                });
                Self::choice(decision, branches, memo)
            }
            ValueNode::Leaf(Leaf {
                datum: Datum::Text(text),
                origin: _,
            }) => match text.term() {
                super::text::TextTerm::Capture {
                    graph,
                    strip_marker: false,
                } => self.transfer_buffer(graph, state, transforms, memo),
                _ => States::new(),
            },
            ValueNode::Leaf(_) => States::new(),
        }
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
        let storage = value.heap.object(target);
        let Some(super::ObjectVersion {
            revision: _,
            multiplicity,
            data,
        }) = storage.as_deref()
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
        for (key, field) in fields.iter() {
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
            let child = FrozenValue::at(Rc::clone(&value.heap), child.clone());
            states = quoted::compose(states, |start| self.value(&child, start, memo), memo);
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
                // Sprig skips nil interfaces; typed nil maps and slices still print.
                let nil = super::text::quote_nil_condition(value);
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
                let states = quoted::literal(initial, &prefix, memo);
                quoted::compose(states, |start| self.value(value, start, memo), memo)
            }
            super::text::TextTerm::Tpl { source, result } => {
                let mut outcomes = self.value(result, state, memo);
                let action_free = super::text_actions::action_free(
                    source,
                    self.input_objects,
                    memo,
                    &mut self.remaining,
                );
                if action_free != Predicate::False {
                    for (end, condition) in self.value(source, state, memo) {
                        quoted::merge(
                            &mut outcomes,
                            end,
                            Predicate::all(vec![action_free.clone(), condition]),
                            memo,
                        );
                    }
                }
                outcomes
            }
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
            Coverage::Exact | Coverage::CertifiedPartial
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
            Coverage::Exact | Coverage::Partial | Coverage::CertifiedPartial => {
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
        quoted::compose(
            states,
            |start| self.part(node.content(), start, &[], memo),
            memo,
        )
    }

    fn buffer(
        &mut self,
        buffer: &Rc<TextBuffer>,
        state: LexicalState,
        memo: &PredicateMemo,
    ) -> States {
        self.transfer_buffer(buffer, state, &[], memo)
    }

    fn transfer_buffer(
        &mut self,
        buffer: &Rc<TextBuffer>,
        state: LexicalState,
        transforms: &[super::relationship::TextTransform],
        memo: &PredicateMemo,
    ) -> States {
        let key = BufferContext {
            graph: Rc::as_ptr(buffer),
            state,
            transforms: transforms.to_vec(),
        };
        if let Some((_, result)) = self.buffers.get(&key) {
            return result.clone();
        }
        let mut states = BTreeMap::from([(state, Predicate::True)]);
        for node in &buffer.parts {
            if self.remaining == 0 {
                states.retain(|state, _| state.is_absorbing());
                break;
            }
            self.remaining -= 1;
            states = quoted::compose(
                states,
                |start| self.part(node.content(), start, transforms, memo),
                memo,
            );
        }
        if self.remaining > 0 {
            self.buffers
                .insert(key, (Rc::clone(buffer), states.clone()));
        }
        states
    }

    fn part(
        &mut self,
        part: &TextPart,
        state: LexicalState,
        transforms: &[super::relationship::TextTransform],
        memo: &PredicateMemo,
    ) -> States {
        if state.is_absorbing() {
            return BTreeMap::from([(state, Predicate::True)]);
        }
        match part {
            TextPart::Frozen(value) => {
                if transforms.is_empty() {
                    self.value(value, state, memo)
                } else {
                    self.transformed(value, state, transforms, memo)
                }
            }
            TextPart::Bytes(text) => {
                if transforms.is_empty() {
                    return quoted::literal(BTreeMap::from([(state, Predicate::True)]), text, memo);
                }
                quoted::mapped_text(text, transforms).map_or_else(States::new, |text| {
                    quoted::literal(BTreeMap::from([(state, Predicate::True)]), &text, memo)
                })
            }
            TextPart::Unknown(_) => States::new(),
            TextPart::Select { decision, yes, no } => {
                let branches =
                    [yes, no].map(|branch| self.transfer_buffer(branch, state, transforms, memo));
                Self::choice(decision, branches, memo)
            }
        }
    }
}

struct QuotedPrefix {
    invalid: Predicate,
    boundaries: Vec<PrefixBoundary>,
}

#[tracing::instrument(skip_all)]
fn quoted_tokens(
    owned: &YamlOwnership,
    serializations: &[SerializationSite],
    terms: &[NodeRef],
    lexical: &mut LexicalTransfers,
    inputs: &BTreeMap<ValuesPath, super::InputBacking>,
    selections: &[SelectedSpan],
    memo: &PredicateMemo,
) -> QuotedPrefix {
    let mut prefix = Predicate::True;
    let mut failures = Vec::new();
    let mut consumed = 0;
    let mut boundaries = Vec::new();
    for (index, node) in owned.nodes.iter().enumerate() {
        if node.span.end <= consumed {
            continue;
        }
        if let Some(transfer) = serializations
            .iter()
            .find(|site| site.node == index)
            .and_then(|site| {
                serialization_transfer(owned, site, terms, inputs, memo, &mut lexical.remaining)
            })
        {
            let active = selection_guard(selections, transfer.span);
            failures.push(Predicate::all(vec![
                prefix.clone(),
                active.clone(),
                transfer.invalid,
            ]));
            prefix = memo.normalize(Predicate::all(vec![
                prefix,
                Predicate::Or(vec![active.negated(), transfer.preserved]),
            ]));
            boundaries.push(PrefixBoundary {
                span: transfer.span,
                preserved: prefix.clone(),
            });
            consumed = transfer.span.end;
            continue;
        }
        if node.kind == YamlNodeKind::BlockScalar {
            let guard = block_scalar_boundary(owned, node, terms, memo, &mut lexical.remaining);
            let active = selection_guard(selections, node.span);
            prefix = memo.normalize(Predicate::all(vec![
                prefix,
                Predicate::Or(vec![active.negated(), guard]),
            ]));
            boundaries.push(PrefixBoundary {
                span: node.span,
                preserved: prefix.clone(),
            });
            consumed = node.span.end;
            continue;
        }
        let Some(style) = token_style(node.kind) else {
            continue;
        };
        if !symbolic_token(owned, node, style) {
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
        let preserved = token_preservation(&mut states, style, closed);
        prefix = memo.normalize(Predicate::all(vec![
            prefix,
            Predicate::Or(vec![active.negated(), preserved]),
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

fn token_style(kind: YamlNodeKind) -> Option<ScalarStyle> {
    match kind {
        YamlNodeKind::Scalar(style) => Some(style),
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
        | YamlNodeKind::Comment => None,
    }
}

fn symbolic_token(
    owned: &YamlOwnership,
    node: &helm_schema_syntax::YamlNode,
    style: ScalarStyle,
) -> bool {
    style != ScalarStyle::Plain
        || owned.skeleton.segments.iter().any(|segment| {
            segment.placeholder
                && segment.range.start < node.span.end
                && node.span.start < segment.range.end
        })
}

fn token_preservation(
    states: &mut BTreeMap<LexicalState, Predicate>,
    style: ScalarStyle,
    closed: LexicalState,
) -> Predicate {
    let mut preserved = vec![states.remove(&closed).unwrap_or(Predicate::False)];
    if style == ScalarStyle::Plain {
        for end in [
            LexicalState::Closed,
            LexicalState::PlainStart,
            LexicalState::PlainColon,
        ] {
            preserved.push(states.remove(&end).unwrap_or(Predicate::False));
        }
    }
    Predicate::Or(preserved)
}

fn block_scalar_boundary(
    owned: &YamlOwnership,
    node: &helm_schema_syntax::YamlNode,
    terms: &[NodeRef],
    memo: &PredicateMemo,
    remaining: &mut usize,
) -> Predicate {
    let Some(text) = owned.skeleton.text.get(node.span.start..node.span.end) else {
        return Predicate::False;
    };
    let Some(body) = text.split_once('\n').map(|(_, body)| body) else {
        return Predicate::True;
    };
    let Some(line) = body.lines().find(|line| !line.trim().is_empty()) else {
        return Predicate::True;
    };
    let indentation = line.len() - line.trim_start_matches(' ').len();
    let mut guards = Vec::new();
    for segment in owned.skeleton.segments.iter().filter(|segment| {
        segment.placeholder
            && segment.range.start < node.span.end
            && node.span.start < segment.range.end
    }) {
        let Some(reference) = terms.get(segment.piece) else {
            return Predicate::False;
        };
        if reference.indentation >= indentation {
            continue;
        }
        let term = PlacedTerm {
            range: segment.range,
            placeholder: true,
            source_offset: None,
            node: reference,
        };
        let Some(alternatives) = term.frozen_alternatives() else {
            return Predicate::False;
        };
        let mut coverage = Vec::new();
        for (guard, value) in alternatives {
            coverage.push(Predicate::all(vec![
                guard,
                indented_value(value, indentation - reference.indentation, memo, remaining),
            ]));
        }
        guards.push(Predicate::Or(coverage));
    }
    memo.normalize(Predicate::all(guards))
}

fn indented_value(
    value: &FrozenValue,
    required: usize,
    memo: &PredicateMemo,
    remaining: &mut usize,
) -> Predicate {
    let Some(next) = remaining.checked_sub(1) else {
        return Predicate::False;
    };
    *remaining = next;
    if required == 0 {
        return Predicate::True;
    }
    if let Some(helm_schema_core::GuardValue::String(text)) = value.value.constant() {
        return if text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .all(|line| line.bytes().take_while(|byte| *byte == b' ').count() >= required)
        {
            Predicate::True
        } else {
            Predicate::False
        };
    }
    match value.value.node.as_ref() {
        ValueNode::Select { decision, yes, no } => {
            let branches = [yes, no].map(|branch| {
                indented_value(
                    &FrozenValue::at(Rc::clone(&value.heap), branch.clone()),
                    required,
                    memo,
                    remaining,
                )
            });
            let [yes, no] = branches;
            if decision.coverage == Coverage::Exact
                && !decision.when_true.contains_approximation()
                && !decision.when_false.contains_approximation()
            {
                memo.normalize(Predicate::Or(vec![
                    Predicate::all(vec![decision.when_true.clone(), yes]),
                    Predicate::all(vec![decision.when_false.clone(), no]),
                ]))
            } else {
                memo.normalize(Predicate::all(vec![yes, no]))
            }
        }
        ValueNode::Leaf(Leaf {
            datum: Datum::Text(text),
            origin: _,
        }) => indented_text(&text.0, required, memo, remaining),
        ValueNode::Leaf(_) => Predicate::False,
    }
}

fn indented_text(
    term: &super::text::TextTerm,
    required: usize,
    memo: &PredicateMemo,
    remaining: &mut usize,
) -> Predicate {
    use super::text::TextTerm;
    let Some(next) = remaining.checked_sub(1) else {
        return Predicate::False;
    };
    *remaining = next;
    match term {
        TextTerm::Resolved { term, literal: _ } => indented_text(term, required, memo, remaining),
        TextTerm::Indent {
            value,
            width,
            newline: _,
        } => indented_value(value, required.saturating_sub(*width), memo, remaining),
        TextTerm::Tpl { source: _, result } => indented_value(result, required, memo, remaining),
        TextTerm::Capture {
            graph,
            strip_marker: _,
        } => {
            let mut guards = Vec::new();
            for node in &graph.parts {
                match node.content() {
                    TextPart::Frozen(value) => {
                        guards.push(indented_value(value, required, memo, remaining));
                    }
                    TextPart::Bytes(text) if text.chars().all(char::is_whitespace) => {}
                    _ => return Predicate::False,
                }
            }
            if guards.is_empty() {
                Predicate::False
            } else {
                memo.normalize(Predicate::all(guards))
            }
        }
        TextTerm::Yaml {
            value: _,
            pretty: _,
        }
        | TextTerm::Json {
            value: _,
            pretty: _,
            escape_html: _,
            literal: _,
        }
        | TextTerm::Quote { value: _ }
        | TextTerm::Encoded {
            value: _,
            alphabet: _,
        } => Predicate::False,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlockContribution {
    kind: YamlNodeKind,
    indentation: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContributionDestination {
    Node,
    Members,
    Items,
}

#[tracing::instrument(skip_all)]
fn contribution_host(
    owned: &YamlOwnership,
    contribution: BlockContribution,
) -> Option<ContributionDestination> {
    let mut current = owned
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            matches!(
                node.kind,
                YamlNodeKind::Scalar(_) | YamlNodeKind::BlockScalar
            )
        })
        .max_by_key(|(_, node)| node.span.end)
        .map(|(index, _)| index);
    while let Some(index) = current {
        let node = owned.nodes.get(index)?;
        let column = owned
            .skeleton
            .text
            .get(..node.span.start)
            .map(|text| text.rsplit('\n').next().unwrap_or_default().len());
        if node.kind == contribution.kind && column == Some(contribution.indentation) {
            return Some(if node.kind == YamlNodeKind::Sequence {
                ContributionDestination::Items
            } else {
                ContributionDestination::Members
            });
        }
        if node.kind == YamlNodeKind::Pair
            && owned
                .nodes
                .iter()
                .filter(|child| child.parent == Some(index))
                .all(|child| child.key)
            && column.is_some_and(|column| {
                contribution.indentation > column
                    || contribution.kind == YamlNodeKind::Sequence
                        && contribution.indentation == column
            })
        {
            return Some(ContributionDestination::Node);
        }
        current = node.parent;
    }
    None
}

#[tracing::instrument(skip_all)]
fn block_contribution(text: &str) -> Option<BlockContribution> {
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
    matches!(root.kind, YamlNodeKind::Mapping | YamlNodeKind::Sequence).then_some(
        BlockContribution {
            kind: root.kind,
            indentation,
        },
    )
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
    if !node.key
        && matches!(
            block_parent(owned, node),
            Some(YamlNodeKind::Pair | YamlNodeKind::Item)
        )
    {
        failures.push(
            states
                .remove(&LexicalState::DirectiveInvalid)
                .unwrap_or(Predicate::False),
        );
    }
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

#[derive(Clone, Copy, Debug)]
enum SerializedDecoding {
    Annotations,
    String,
    Document,
}

#[derive(Clone, Copy, Debug)]
struct SerializedPosition {
    decoding: Option<SerializedDecoding>,
    flow: Option<YamlNodeKind>,
    inline: bool,
    sequence: bool,
}

impl SerializedPosition {
    fn new(owned: &YamlOwnership, index: usize, node: &helm_schema_syntax::YamlNode) -> Self {
        let decoding = match mapping_path(owned, index).as_deref() {
            Some([metadata, field])
                if metadata == "metadata" && matches!(field.as_str(), "annotations" | "labels") =>
            {
                Some(SerializedDecoding::Annotations)
            }
            Some([field]) if matches!(field.as_str(), "apiVersion" | "kind") => {
                Some(SerializedDecoding::String)
            }
            Some([metadata, field]) if metadata == "metadata" && field == "name" => {
                Some(SerializedDecoding::String)
            }
            _ if owned
                .nodes
                .get(index)
                .and_then(|target| target.parent)
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

    fn invalid(self, path: &ValuesPath) -> Predicate {
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

struct SerializationTransfer {
    span: Span,
    preserved: Predicate,
    invalid: Predicate,
}

#[derive(Clone, Debug)]
struct SerializationSite {
    node: usize,
    range: Span,
    piece: usize,
    shape: PayloadShape,
    position: SerializedPosition,
    peers: Predicate,
    member_splice: bool,
    block_node: bool,
}

fn serialization_sites(
    owned: &YamlOwnership,
    selections: &[SelectedSpan],
    terms: &[NodeRef],
) -> Vec<SerializationSite> {
    use PayloadShape;
    let mut result = Vec::new();
    for segment in owned.skeleton.segments.iter().filter(|segment| {
        segment.placeholder
            && terms
                .get(segment.piece)
                .is_some_and(|term| term.serializer.is_some())
    }) {
        let start = owned
            .nodes
            .partition_point(|node| node.span.start < segment.range.start);
        for (index, node) in owned
            .nodes
            .iter()
            .enumerate()
            .skip(start)
            .take_while(|(_, node)| node.span.start < segment.range.end)
        {
            let shape = match node.kind {
                YamlNodeKind::Mapping | YamlNodeKind::Pair | YamlNodeKind::FlowMapping => {
                    PayloadShape::Mapping
                }
                YamlNodeKind::Item => PayloadShape::Sequence,
                YamlNodeKind::Scalar(ScalarStyle::Plain) if !node.key => PayloadShape::Scalar,
                _ => continue,
            };
            if node.span.end > segment.range.end
                || owned
                    .skeleton
                    .text
                    .get(segment.range.start..segment.range.end)
                    .zip(owned.skeleton.text.get(node.span.start..node.span.end))
                    .is_none_or(|(term, node)| term.trim() != node.trim())
            {
                continue;
            }
            let target = if node.kind == YamlNodeKind::Pair {
                node.parent.unwrap_or(index)
            } else {
                index
            };
            let position = SerializedPosition::new(owned, target, node);
            let member_splice = matches!(node.kind, YamlNodeKind::Pair | YamlNodeKind::Item);
            let peers = if member_splice {
                coexecuting_structure(owned, index, selections)
            } else {
                Predicate::False
            };
            let block_node = block_parent(owned, node).is_some();
            result.push(SerializationSite {
                node: index,
                range: segment.range,
                piece: segment.piece,
                shape,
                position,
                peers,
                member_splice,
                block_node,
            });
            break;
        }
    }
    result
}

pub(crate) struct PlacedSerialization<'a> {
    pub(crate) term: PlacedTerm<'a>,
    pub(crate) shape: PayloadShape,
    pub(crate) peers: &'a Predicate,
    pub(crate) sequence: bool,
}

impl<'a> PlacedDocument<'a> {
    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::placement_queries"
    )]
    pub(crate) fn serialization(&self, span: Span) -> Option<PlacedSerialization<'a>> {
        let site = self.document.serializations.iter().find(|site| {
            self.ownership
                .nodes
                .get(site.node)
                .is_some_and(|node| node.span == span)
        })?;
        Some(PlacedSerialization {
            term: PlacedTerm {
                range: site.range,
                placeholder: true,
                source_offset: None,
                node: self.layout.terms.get(site.piece)?,
            },
            shape: site.shape,
            peers: &site.peers,
            sequence: site.position.sequence,
        })
    }
}

fn coexecuting_structure(
    owned: &YamlOwnership,
    index: usize,
    selections: &[SelectedSpan],
) -> Predicate {
    let Some(node) = owned.nodes.get(index) else {
        return Predicate::False;
    };
    Predicate::Or(
        owned
            .nodes
            .iter()
            .enumerate()
            .filter(|(other, sibling)| *other != index && sibling.parent == node.parent)
            .filter(|(other, sibling)| {
                let structural = if sibling.kind == YamlNodeKind::Pair {
                    owned
                        .nodes
                        .iter()
                        .find(|key| key.parent == Some(*other) && key.key)
                        .map(|key| key.span)
                } else if sibling.kind == YamlNodeKind::Item {
                    Some(Span::new(
                        sibling.span.start,
                        sibling.span.start.saturating_add(1),
                    ))
                } else {
                    None
                };
                structural.is_some_and(|span| {
                    owned
                        .skeleton
                        .text
                        .get(span.start..span.end)
                        .is_some_and(|text| !text.contains('<'))
                        && !owned.skeleton.segments.iter().any(|segment| {
                            segment.placeholder
                                && segment.range.start < span.end
                                && span.start < segment.range.end
                        })
                })
            })
            .map(|(index, sibling)| {
                let span = if sibling.kind == YamlNodeKind::Pair {
                    owned
                        .nodes
                        .iter()
                        .find(|key| key.parent == Some(index) && key.key)
                        .map_or(sibling.span, |key| key.span)
                } else {
                    Span::new(sibling.span.start, sibling.span.start.saturating_add(1))
                };
                selection_guard(selections, span)
            })
            .collect(),
    )
}

#[tracing::instrument(skip_all)]
fn serialization_transfer(
    owned: &YamlOwnership,
    site: &SerializationSite,
    terms: &[NodeRef],
    inputs: &BTreeMap<ValuesPath, super::InputBacking>,
    memo: &PredicateMemo,
    remaining: &mut usize,
) -> Option<SerializationTransfer> {
    let node = owned.nodes.get(site.node)?;
    let term = PlacedTerm {
        range: site.range,
        placeholder: true,
        source_offset: None,
        node: terms.get(site.piece)?,
    };
    let alternatives = term.frozen_alternatives()?;
    let mut query = super::relationship::RelationshipQuery::new(inputs, *remaining);
    let mut preserved = Vec::new();
    let mut invalid = Vec::new();
    for (guard, value) in alternatives {
        for payload in query
            .value(value, &[])
            .known
            .into_iter()
            .filter_map(super::relationship::Relationship::destination)
        {
            if payload.capture.is_some()
                || !payload.output_path.is_empty()
                || !matches!(
                    payload.kind,
                    crate::ValueKind::YamlSerialized | crate::ValueKind::TemplatedYamlSerialized
                )
            {
                continue;
            }
            let coverage = Predicate::all(vec![guard.clone(), payload.condition.clone()]);
            let unchanged = payload.unchanged_condition();
            let boundary = site.preservation(&payload);
            preserved.push(Predicate::all(vec![coverage.clone(), boundary]));
            if !payload.omitted_members.is_empty() {
                continue;
            }
            let bad_node = if site.position.decoding.is_some()
                || !payload.flow_node && (site.position.inline || site.position.flow.is_some())
            {
                site.position.invalid(&payload.source)
            } else {
                Predicate::False
            };
            invalid.push(Predicate::all(vec![
                coverage.clone(),
                unchanged.clone(),
                bad_node,
            ]));
            if site.member_splice {
                invalid.push(Predicate::all(vec![
                    coverage,
                    unchanged,
                    site.peers.clone(),
                    if payload.flow_node {
                        Predicate::True
                    } else {
                        site.structural(&payload).negated()
                    },
                ]));
            }
        }
    }
    *remaining = remaining.saturating_sub(query.work());
    if preserved.is_empty() {
        return None;
    }
    Some(SerializationTransfer {
        span: Span::new(node.span.start, site.range.end),
        preserved: memo.normalize(Predicate::Or(preserved)),
        invalid: memo.normalize(Predicate::Or(invalid)),
    })
}

impl SerializationSite {
    fn structural(&self, payload: &super::use_policy::RelationshipUse<'_>) -> Predicate {
        let typed = |schema_type: &str| {
            Predicate::from(Guard::TypeIs {
                path: payload.source.as_ref().clone(),
                schema_type: schema_type.to_owned(),
            })
        };
        Predicate::all(vec![
            typed(if self.position.sequence {
                "array"
            } else {
                "object"
            }),
            if self.position.sequence {
                Predicate::truthy_path(payload.source.encode())
            } else {
                Predicate::from(Guard::MinMembers {
                    path: payload.source.as_ref().clone(),
                    bound: 1,
                })
            },
        ])
    }

    fn preservation(&self, payload: &super::use_policy::RelationshipUse<'_>) -> Predicate {
        let unchanged = payload.unchanged_condition();
        let structural = self.structural(payload);
        let whole = if payload.flow_node || self.block_node && !self.position.inline {
            if payload.kind == crate::ValueKind::TemplatedYamlSerialized {
                unchanged.clone()
            } else {
                Predicate::True
            }
        } else {
            Predicate::all(vec![payload.scalar_condition(), payload.flow_condition()])
        };
        if self.member_splice {
            Predicate::Or(vec![
                Predicate::all(vec![self.peers.negated(), whole]),
                Predicate::all(vec![
                    self.peers.clone(),
                    structural.clone(),
                    unchanged.clone(),
                ]),
            ])
        } else {
            whole
        }
    }
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

impl super::DocumentExecution {
    #[tracing::instrument(skip_all)]
    pub(crate) fn place_capture(
        &self,
        graph: &Rc<TextBuffer>,
        condition: &Predicate,
        remaining: &mut usize,
    ) -> Option<OutputPlacement> {
        let exit = self.exits.first()?;
        if *remaining == 0 {
            return None;
        }
        let placement = OutputPlacement::bounded(
            PlacementSource { graph, condition },
            &exit.state.heap.arena.predicate_memo,
            FileOutput::Manifest,
            &mut SyntaxContexts::default(),
            &exit.state.heap.arena.input_paths.borrow(),
            &exit.state.heap.arena.input_objects.borrow(),
            remaining,
        );
        (*remaining != 0).then_some(placement)
    }
}
