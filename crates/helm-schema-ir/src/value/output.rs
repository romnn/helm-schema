use super::scalar_value::ScalarContent;
use super::{Datum, Exit, Flow, FrozenValue, Leaf, ObjectData, Value, ValueNode};
use super::{
    DocumentExecution, ExecutedSite, ExecutedText, ObservationId, OutputNode, TextBuffer, TextPart,
};
use helm_schema_core::{GuardValue, Predicate};
use std::collections::{BTreeMap, BTreeSet};

impl TextBuffer {
    fn immutable_text(parts: Vec<OutputNode>) -> Self {
        Self {
            parts,
            counts: BranchCounts::empty(),
            arena: None,
        }
    }

    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::output_work"
    )]
    fn push(&mut self, part: OutputNode) {
        self.parts.push(part);
        self.counts.increment();
        if self.counts.is_empty() || self.counts.definitely_below(129) {
            return;
        }
        if self.counts.definitely_at_least(129) {
            self.parts = vec![TextPart::Unknown(self.inputs()).into()];
            self.counts = BranchCounts::constant_on(1, self.counts.domain.clone());
            return;
        }
        if self.counts.is_exhausted() {
            self.parts = vec![TextPart::Unknown(self.inputs()).into()];
            self.counts = BranchCounts::constant_on(1, self.counts.domain.clone());
            return;
        }
        if let Some(arena) = &self.arena {
            let Ok(overflow) = self.counts.overflow(arena) else {
                self.parts = vec![TextPart::Unknown(self.inputs()).into()];
                self.counts = BranchCounts::constant_on(1, self.counts.domain.clone());
                return;
            };
            if overflow.is_impossible() {
                return;
            }
            tracing::debug!(reason = ?super::BoundReason::OutputTerms, "widen output");
            let unknown = Self::immutable_text(vec![TextPart::Unknown(self.inputs()).into()]);
            if overflow == self.counts.domain {
                self.parts = unknown.parts;
                self.counts = BranchCounts::constant_on(1, self.counts.domain.clone());
                return;
            }
            let allowed = arena.complement_runtime_guard(&overflow);
            let decision = arena.budget_test(overflow.clone());
            let exact = std::rc::Rc::new(Self::immutable_text(self.parts.clone()));
            let exact = {
                let _span = tracing::debug_span!(
                    target: "helm_schema_ir::value::output_work",
                    "retain_output"
                )
                .entered();
                TextRetention::default().retain(&exact, &allowed)
            };
            self.parts = vec![
                TextPart::Select {
                    decision,
                    yes: std::rc::Rc::new(unknown),
                    no: exact,
                }
                .into(),
            ];
            self.counts.reset_overflow(overflow);
        } else {
            // Variable counts are produced only by joins/restriction, which attach the arena.
            self.parts = vec![TextPart::Unknown(self.inputs()).into()];
            self.counts = BranchCounts::constant_on(1, self.counts.domain.clone());
        }
    }

    pub(super) fn restrict_budget(
        &mut self,
        active: &super::guard::RuntimeGuard,
        arena: &std::rc::Rc<super::Arena>,
    ) {
        self.counts = self.counts.restrict(active, arena);
        self.arena = Some(std::rc::Rc::clone(arena));
    }

    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::output_work"
    )]
    pub(super) fn inputs(&self) -> BTreeSet<helm_schema_core::ValuesPath> {
        let mut inputs = BTreeSet::new();
        let mut pending = vec![self];
        let mut seen = BTreeSet::new();
        while let Some(buffer) = pending.pop() {
            if !seen.insert(std::ptr::from_ref(buffer)) {
                continue;
            }
            for part in &buffer.parts {
                match part.content() {
                    TextPart::Bytes(_) => {}
                    TextPart::Frozen(value) => inputs.extend(value.inputs()),
                    TextPart::Unknown(paths) => inputs.extend(paths.iter().cloned()),
                    TextPart::Select {
                        decision: _,
                        yes,
                        no,
                    } => pending.extend([yes.as_ref(), no.as_ref()]),
                }
            }
        }
        inputs
    }

    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::output_work"
    )]
    pub(super) fn is_widened_under(
        &self,
        active: &super::guard::RuntimeGuard,
        arena: &super::Arena,
    ) -> bool {
        TextQuery::default().is_widened_under(self, active, arena)
    }

    pub(super) fn join(
        decision: std::rc::Rc<super::FrozenTest>,
        yes: &Self,
        no: &Self,
        arena: &std::rc::Rc<super::Arena>,
    ) -> Self {
        let prefix = yes
            .parts
            .iter()
            .zip(&no.parts)
            .take_while(|(yes, no)| yes.same_part(no))
            .count();
        let mut output = Self {
            parts: yes.parts.iter().take(prefix).cloned().collect(),
            counts: BranchCounts::selected(&decision, &yes.counts, &no.counts, arena),
            arena: Some(std::rc::Rc::clone(arena)),
        };
        let yes = Self::immutable_text(yes.parts.iter().skip(prefix).cloned().collect());
        let no = Self::immutable_text(no.parts.iter().skip(prefix).cloned().collect());
        if !yes.parts.is_empty() || !no.parts.is_empty() {
            // The selector stores alternatives; each execution pays only for its selected source terms.
            output.parts.push(
                TextPart::Select {
                    decision,
                    yes: std::rc::Rc::new(yes),
                    no: std::rc::Rc::new(no),
                }
                .into(),
            );
        }
        output
    }

    pub(super) fn bytes(&mut self, bytes: &str) {
        if !bytes.is_empty() {
            self.push(TextPart::Bytes(bytes.to_owned()).into());
        }
    }

    pub(super) fn emit(&mut self, value: FrozenValue) {
        self.push(TextPart::Frozen(value).into());
    }

    pub(crate) fn exact_text(&self) -> Option<String> {
        TextQuery::default().exact_text(self)
    }

    pub(super) fn stripped_value(&self) -> Value {
        super::text::capture(self, true)
    }
}

// Overflow makes the complementary output unreachable; release its snapshots before storing it.
#[derive(Default)]
struct TextRetention {
    buffers: BTreeMap<(*const TextBuffer, u64), std::rc::Rc<TextBuffer>>,
}

impl TextRetention {
    fn retain(
        &mut self,
        buffer: &std::rc::Rc<TextBuffer>,
        allowed: &super::guard::RuntimeGuard,
    ) -> std::rc::Rc<TextBuffer> {
        let key = (std::rc::Rc::as_ptr(buffer), allowed.id());
        if let Some(retained) = self.buffers.get(&key) {
            return std::rc::Rc::clone(retained);
        }
        let mut parts = Vec::new();
        let mut changed = allowed.is_impossible() && !buffer.parts.is_empty();
        if !allowed.is_impossible() {
            for part in &buffer.parts {
                match part.content() {
                    TextPart::Bytes(_) | TextPart::Frozen(_) | TextPart::Unknown(_) => {
                        parts.push(part.clone());
                    }
                    TextPart::Select { decision, yes, no } => {
                        if let Some(taken) = allowed.implies_test(decision) {
                            changed = true;
                            let selected = self.retain(if taken { yes } else { no }, allowed);
                            parts.extend(selected.parts.iter().cloned());
                        } else {
                            // Keep one admissibility mask across shared children.
                            // Only globally forced arms are pruned.
                            let retained_yes = self.retain(yes, allowed);
                            let retained_no = self.retain(no, allowed);
                            changed |= !std::rc::Rc::ptr_eq(yes, &retained_yes)
                                || !std::rc::Rc::ptr_eq(no, &retained_no);
                            parts.push(
                                TextPart::Select {
                                    decision: std::rc::Rc::clone(decision),
                                    yes: retained_yes,
                                    no: retained_no,
                                }
                                .into(),
                            );
                        }
                    }
                }
            }
        }
        let retained = if changed {
            std::rc::Rc::new(TextBuffer::immutable_text(parts))
        } else {
            std::rc::Rc::clone(buffer)
        };
        self.buffers.insert(key, std::rc::Rc::clone(&retained));
        retained
    }
}

// Each query visits the shared output graph, rather than re-expanding its execution paths.
#[derive(Default)]
pub(super) struct TextQuery {
    uncertainty: BTreeMap<*const TextBuffer, bool>,
    widening_domains: BTreeMap<*const TextBuffer, Option<super::guard::RuntimeGuard>>,
    widened: BTreeMap<(*const TextBuffer, u64), bool>,
    text: BTreeMap<*const TextBuffer, Option<String>>,
}

impl TextQuery {
    fn widening_domain(
        &mut self,
        buffer: &TextBuffer,
        arena: &super::Arena,
    ) -> Option<super::guard::RuntimeGuard> {
        let key = std::ptr::from_ref(buffer);
        if let Some(domain) = self.widening_domains.get(&key) {
            return domain.clone();
        }
        let mut domain = Some(super::guard::RuntimeGuard::False);
        for part in &buffer.parts {
            let widened = match part.content() {
                TextPart::Bytes(_) | TextPart::Frozen(_) => Some(super::guard::RuntimeGuard::False),
                TextPart::Unknown(_) => Some(super::guard::RuntimeGuard::True),
                TextPart::Select { decision, yes, no } => {
                    if decision.coverage == super::Coverage::Partial
                        && decision
                            .runtime
                            .as_ref()
                            .is_none_or(|runtime| !runtime.is_input_exact())
                    {
                        None
                    } else {
                        self.widening_domain(yes, arena)
                            .zip(self.widening_domain(no, arena))
                            .map(|(yes, no)| {
                                let mut result = super::guard::RuntimeGuard::False;
                                for (branch, taken) in [(yes, true), (no, false)] {
                                    let selected = arena.runtime_path(&[super::DecisionEdge {
                                        test: std::rc::Rc::clone(decision),
                                        taken,
                                    }]);
                                    result = arena.union_runtime_guards(
                                        &result,
                                        &arena.intersect_runtime_guards(&branch, &selected),
                                    );
                                }
                                result
                            })
                    }
                }
            };
            domain = domain
                .zip(widened)
                .map(|(domain, widened)| arena.union_runtime_guards(&domain, &widened));
            if domain.is_none() {
                break;
            }
        }
        self.widening_domains.insert(key, domain.clone());
        domain
    }

    fn can_be_widened(&mut self, buffer: &TextBuffer) -> bool {
        let key = std::ptr::from_ref(buffer);
        if let Some(result) = self.uncertainty.get(&key) {
            return *result;
        }
        let result = buffer.parts.iter().any(|part| match part.content() {
            TextPart::Bytes(_) | TextPart::Frozen(_) => false,
            TextPart::Unknown(_) => true,
            TextPart::Select { decision, yes, no } => {
                decision.coverage == super::Coverage::Partial
                    || self.can_be_widened(yes)
                    || self.can_be_widened(no)
            }
        });
        self.uncertainty.insert(key, result);
        result
    }

    fn is_widened_under(
        &mut self,
        buffer: &TextBuffer,
        active: &super::guard::RuntimeGuard,
        arena: &super::Arena,
    ) -> bool {
        if active.is_impossible() || !self.can_be_widened(buffer) {
            return false;
        }
        // Derive an exact domain once per shared buffer before restricting it to this execution.
        // Carrying each path into the graph would enumerate independent output choices.
        if let Some(domain) = self.widening_domain(buffer, arena) {
            return !arena
                .intersect_runtime_guards(active, &domain)
                .is_impossible();
        }
        let key = (std::ptr::from_ref(buffer), active.id());
        if let Some(result) = self.widened.get(&key) {
            return *result;
        }
        let result = buffer.parts.iter().any(|part| match part.content() {
            TextPart::Bytes(_) | TextPart::Frozen(_) => false,
            TextPart::Unknown(_) => true,
            TextPart::Select { decision, yes, no } => {
                if let Some(taken) = active.implies_test(decision) {
                    self.is_widened_under(if taken { yes } else { no }, active, arena)
                } else {
                    decision.coverage == super::Coverage::Partial
                        || [(yes, true), (no, false)]
                            .into_iter()
                            .any(|(branch, taken)| {
                                let selected = arena.runtime_path(&[super::DecisionEdge {
                                    test: std::rc::Rc::clone(decision),
                                    taken,
                                }]);
                                self.is_widened_under(
                                    branch,
                                    &arena.intersect_runtime_guards(active, &selected),
                                    arena,
                                )
                            })
                }
            }
        });
        self.widened.insert(key, result);
        result
    }

    fn exact_text(&mut self, buffer: &TextBuffer) -> Option<String> {
        let key = std::ptr::from_ref(buffer);
        if let Some(result) = self.text.get(&key) {
            return result.clone();
        }
        let mut output = String::new();
        let mut exact = true;
        for part in &buffer.parts {
            let text = match part.content() {
                TextPart::Bytes(bytes) => Some(bytes.clone()),
                TextPart::Frozen(value) => value.exact_text(),
                TextPart::Unknown(_) => None,
                TextPart::Select {
                    decision: _,
                    yes,
                    no,
                } => self
                    .exact_text(yes)
                    .zip(self.exact_text(no))
                    .and_then(|(yes, no)| (yes == no).then_some(yes)),
            };
            if let Some(text) = text {
                output.push_str(&text);
            } else {
                exact = false;
                break;
            }
        }
        let result = exact.then_some(output);
        self.text.insert(key, result.clone());
        result
    }
}

impl OutputNode {
    fn same_part(&self, other: &Self) -> bool {
        let Self { occurrences, part } = self;
        let Self {
            occurrences: other_occurrences,
            part: other_part,
        } = other;
        occurrences == other_occurrences && part.same_part(other_part)
    }
}

impl TextPart {
    fn same_part(&self, other: &Self) -> bool {
        match self {
            Self::Bytes(bytes) => matches!(other, Self::Bytes(other) if bytes == other),
            Self::Frozen(value) => {
                matches!(other, Self::Frozen(other) if same_frozen_value(value, other))
            }
            Self::Unknown(inputs) => matches!(other, Self::Unknown(other) if inputs == other),
            Self::Select { decision, yes, no } => matches!(
                other,
                Self::Select { decision: other, yes: other_yes, no: other_no }
                    if decision.id == other.id && std::rc::Rc::ptr_eq(yes, other_yes) && std::rc::Rc::ptr_eq(no, other_no)
            ),
        }
    }
}

fn same_frozen_value(left: &FrozenValue, right: &FrozenValue) -> bool {
    let FrozenValue {
        heap: left_heap,
        value: left_value,
    } = left;
    let FrozenValue {
        heap: right_heap,
        value: right_value,
    } = right;
    std::rc::Rc::ptr_eq(left_heap, right_heap) && left_value == right_value
}

impl ExecutedSite {
    pub(crate) fn plain_scalar_placement(&self) -> helm_schema_core::PlainScalarPlacement {
        self.plain_scalar_placement
    }

    fn same_site(&self, other: &Self) -> bool {
        let Self {
            program,
            coverage_exact: _,
            rendered_unchanged,
            ordinal,
            span,
            source_path,
            condition,
            value,
            raw_format_inputs,
            plain_scalar_placement,
            rendered,
        } = self;
        let Self {
            program: other_program,
            coverage_exact: _,
            rendered_unchanged: other_unchanged,
            ordinal: other_ordinal,
            span: other_span,
            source_path: other_source,
            condition: other_condition,
            value: other_value,
            raw_format_inputs: other_raw_format_inputs,
            plain_scalar_placement: other_plain_scalar_placement,
            rendered: other_rendered,
        } = other;
        program == other_program
            && rendered_unchanged == other_unchanged
            && ordinal == other_ordinal
            && span == other_span
            && source_path == other_source
            && raw_format_inputs == other_raw_format_inputs
            && plain_scalar_placement == other_plain_scalar_placement
            && same_frozen_value(value, other_value)
            && rendered == other_rendered
            && condition == other_condition
            && self.coverage_exact() == other.coverage_exact()
    }

    pub(crate) fn coverage_exact(&self) -> bool {
        self.coverage_exact && self.condition.exact()
    }

    pub(crate) fn span(&self) -> helm_schema_syntax::Span {
        self.span
    }
    pub(crate) fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }
    pub(crate) fn condition(&self) -> Predicate {
        self.condition.predicate()
    }
    pub(crate) fn rendered(&self) -> bool {
        self.rendered
    }
    pub(crate) fn value(&self) -> &FrozenValue {
        &self.value
    }
    pub(crate) fn projected_scalar(&self) -> Option<super::ScalarValueDispatch> {
        if !self.coverage_exact() {
            return None;
        }
        let mut arms = Vec::new();
        let mut remaining = super::MAX_SCALAR_DISPATCH_STATES;
        let complete = collect_scalar_projection(
            &self.value.value,
            Predicate::True,
            &mut arms,
            &mut remaining,
        );
        (!arms.is_empty()).then(|| super::ScalarValueDispatch::bind_content(arms, complete))
    }

    pub(crate) fn rendered_scalar(&self) -> Option<super::ScalarValueDispatch> {
        if !self.rendered {
            return None;
        }
        let projected = self.projected_scalar()?;
        if self.rendered_unchanged {
            return Some(projected);
        }
        let mut arms = Vec::new();
        for (condition, content) in projected.content()?.arms() {
            if let ScalarContent::Identity(path) = content {
                arms.push((
                    Predicate::all(vec![condition.clone(), marker_preserving_scalar(path)])
                        .normalize_boolean(),
                    content.clone(),
                ));
            }
        }
        (!arms.is_empty()).then(|| super::ScalarValueDispatch::bind_content(arms, false))
    }

    pub(crate) fn formatted_containers(&self) -> Vec<super::RenderedContainerUse> {
        let mut uses = Vec::new();
        if !self.rendered || !self.coverage_exact() {
            return uses;
        }
        for super::RawFormatInput { condition, path } in &self.raw_format_inputs {
            for (schema_type, kind) in [
                ("object", crate::ValueKind::GoFormattedMap),
                ("array", crate::ValueKind::GoFormattedSequence),
            ] {
                uses.push(super::RenderedContainerUse {
                    condition: Predicate::all(vec![
                        condition.clone(),
                        Predicate::from(helm_schema_core::Guard::TypeIs {
                            path: path.clone(),
                            schema_type: schema_type.to_owned(),
                        }),
                    ])
                    .normalize_boolean(),
                    path: path.clone(),
                    kind,
                });
            }
        }
        uses
    }
}

fn marker_preserving_scalar(path: &helm_schema_core::ValuesPath) -> Predicate {
    use helm_schema_core::Guard;
    let type_is = |schema_type: &str| {
        Predicate::from(Guard::TypeIs {
            path: path.clone(),
            schema_type: schema_type.to_owned(),
        })
    };
    Predicate::Or(vec![
        type_is("boolean"),
        type_is("number"),
        Predicate::all(vec![
            type_is("string"),
            Predicate::from(Guard::NotMatchesPattern {
                path: path.clone(),
                pattern: marker_overlap_pattern().to_owned(),
            }),
        ]),
    ])
    .normalize_boolean()
}

fn marker_overlap_pattern() -> &'static str {
    static PATTERN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| {
        const MARKER: &str = "<no value>";
        let mut overlaps = BTreeSet::from(["^$".to_owned(), MARKER.to_owned()]);
        for boundary in 1..MARKER.len() {
            if let Some(suffix) = MARKER.get(boundary..) {
                overlaps.insert(format!("^{suffix}"));
            }
            if let Some(prefix) = MARKER.get(..boundary) {
                overlaps.insert(format!("{prefix}$"));
            }
        }
        // An emission can also lie entirely inside a marker spanning both neighbors.
        for start in 0..MARKER.len() {
            for end in start + 1..=MARKER.len() {
                if let Some(middle) = MARKER.get(start..end) {
                    overlaps.insert(format!("^{middle}$"));
                }
            }
        }
        overlaps.into_iter().collect::<Vec<_>>().join("|")
    })
}

fn collect_scalar_projection(
    value: &Value,
    condition: Predicate,
    arms: &mut Vec<(Predicate, ScalarContent)>,
    remaining: &mut usize,
) -> bool {
    if condition == Predicate::False {
        return true;
    }
    match value.node.as_ref() {
        ValueNode::Select { decision, yes, no } => {
            if decision.coverage != super::Coverage::Exact
                || decision.when_true.contains_approximation()
                || decision.when_false.contains_approximation()
            {
                return false;
            }
            let yes_complete = collect_scalar_projection(
                yes,
                Predicate::all(vec![condition.clone(), decision.when_true.clone()])
                    .normalize_boolean(),
                arms,
                remaining,
            );
            let no_complete = collect_scalar_projection(
                no,
                Predicate::all(vec![condition, decision.when_false.clone()]).normalize_boolean(),
                arms,
                remaining,
            );
            yes_complete && no_complete
        }
        ValueNode::Leaf(Leaf { datum, origin }) => {
            if *remaining == 0 {
                return false;
            }
            *remaining -= 1;
            let content = match datum {
                Datum::Text(_) => None,
                Datum::Scalar {
                    go_type: _,
                    content: Some(content),
                } => Some(content.clone()),
                Datum::Scalar {
                    go_type: _,
                    content: None,
                }
                | Datum::Map { view: _, target: _ }
                | Datum::Sequence(_)
                | Datum::Strings(_)
                | Datum::Slot {
                    backing: _,
                    index: _,
                }
                | Datum::Unknown(_) => origin.identity.clone().map(ScalarContent::Identity),
            };
            if let Some(content) = content {
                arms.push((condition, content));
                true
            } else {
                false
            }
        }
    }
}

#[derive(Debug)]
struct CountThresholdResult {
    threshold: usize,
    guard: super::guard::RuntimeGuard,
}

#[derive(Debug)]
struct CountExpression {
    // A select retains only its latest exact threshold, until its count graph dies.
    // Increment nodes forward queries without keeping another strong guard owner.
    threshold: std::cell::RefCell<Option<CountThresholdResult>>,
    minimum: usize,
    maximum: usize,
    kind: CountExpressionKind,
}

#[derive(Clone, Debug)]
enum CountExpressionKind {
    Constant(usize),
    Increment(std::rc::Rc<CountExpression>),
    Select {
        condition: super::guard::RuntimeGuard,
        yes: std::rc::Rc<CountExpression>,
        no: std::rc::Rc<CountExpression>,
    },
}

impl CountExpression {
    fn constant(count: usize) -> std::rc::Rc<Self> {
        let count = count.min(129);
        std::rc::Rc::new(Self {
            minimum: count,
            maximum: count,
            kind: CountExpressionKind::Constant(count),
            threshold: std::cell::RefCell::default(),
        })
    }

    fn increment(source: &std::rc::Rc<Self>) -> std::rc::Rc<Self> {
        if source.minimum == source.maximum {
            return Self::constant(source.minimum.saturating_add(1));
        }
        std::rc::Rc::new(Self {
            minimum: source.minimum.saturating_add(1).min(129),
            maximum: source.maximum.saturating_add(1).min(129),
            kind: CountExpressionKind::Increment(std::rc::Rc::clone(source)),
            threshold: std::cell::RefCell::default(),
        })
    }

    fn selected(
        condition: super::guard::RuntimeGuard,
        yes: &std::rc::Rc<Self>,
        no: &std::rc::Rc<Self>,
    ) -> std::rc::Rc<Self> {
        if condition.is_impossible() {
            return std::rc::Rc::clone(no);
        }
        if condition == super::guard::RuntimeGuard::True || std::rc::Rc::ptr_eq(yes, no) {
            return std::rc::Rc::clone(yes);
        }
        if yes.minimum == yes.maximum && no.minimum == no.maximum && yes.minimum == no.minimum {
            return Self::constant(yes.minimum);
        }
        std::rc::Rc::new(Self {
            minimum: yes.minimum.min(no.minimum),
            maximum: yes.maximum.max(no.maximum),
            threshold: std::cell::RefCell::default(),
            kind: CountExpressionKind::Select {
                condition,
                yes: std::rc::Rc::clone(yes),
                no: std::rc::Rc::clone(no),
            },
        })
    }
}

struct CountThreshold {
    results: BTreeMap<(*const CountExpression, usize), super::guard::RuntimeGuard>,
    remaining: usize,
    guards: super::guard::GuardWorkBudget,
}

impl Default for CountThreshold {
    fn default() -> Self {
        Self::new(4096, 32768)
    }
}

impl CountThreshold {
    fn new(nodes: usize, guards: usize) -> Self {
        Self {
            results: BTreeMap::new(),
            remaining: nodes,
            guards: super::guard::GuardWorkBudget::new(guards),
        }
    }

    fn select(
        &mut self,
        condition: &super::guard::RuntimeGuard,
        yes: &super::guard::RuntimeGuard,
        no: &super::guard::RuntimeGuard,
        arena: &super::Arena,
    ) -> Result<super::guard::RuntimeGuard, super::BoundReason> {
        arena.select_runtime_guard_bounded(condition, yes, no, &mut self.guards)
    }

    fn below(
        &mut self,
        expression: &std::rc::Rc<CountExpression>,
        threshold: usize,
        arena: &super::Arena,
    ) -> Result<super::guard::RuntimeGuard, super::BoundReason> {
        if expression.minimum >= threshold {
            return Ok(super::guard::RuntimeGuard::False);
        }
        if expression.maximum < threshold {
            return Ok(super::guard::RuntimeGuard::True);
        }
        let key = (std::rc::Rc::as_ptr(expression), threshold);
        if let Some(result) = self.results.get(&key) {
            return Ok(result.clone());
        }
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or(super::BoundReason::BudgetNodes)?;
        let cached = expression
            .threshold
            .borrow()
            .as_ref()
            .filter(|result| result.threshold == threshold)
            .map(|result| result.guard.clone());
        if let Some(result) = cached {
            self.results.insert(key, result.clone());
            return Ok(result);
        }
        let result = match &expression.kind {
            CountExpressionKind::Constant(count) => {
                if *count < threshold {
                    super::guard::RuntimeGuard::True
                } else {
                    super::guard::RuntimeGuard::False
                }
            }
            CountExpressionKind::Increment(source) => {
                self.below(source, threshold.saturating_sub(1), arena)?
            }
            CountExpressionKind::Select { condition, yes, no } => {
                let yes = self.below(yes, threshold, arena)?;
                let no = self.below(no, threshold, arena)?;
                self.select(condition, &yes, &no, arena)?
            }
        };
        if matches!(
            &expression.kind,
            CountExpressionKind::Select {
                condition: _,
                yes: _,
                no: _
            }
        ) {
            *expression.threshold.borrow_mut() = Some(CountThresholdResult {
                threshold,
                guard: result.clone(),
            });
        }
        self.results.insert(key, result.clone());
        Ok(result)
    }
}

#[derive(Clone, Debug)]
enum CountState {
    Empty,
    Exact(std::rc::Rc<CountExpression>),
    Exhausted,
}

// Only mutable execution roots own counts. Bounds are conservative on their live domain.
#[derive(Clone, Debug)]
pub(super) struct BranchCounts {
    state: std::cell::RefCell<CountState>,
    domain: super::guard::RuntimeGuard,
}

impl Default for BranchCounts {
    fn default() -> Self {
        Self::constant_on(0, super::guard::RuntimeGuard::True)
    }
}

impl BranchCounts {
    fn empty() -> Self {
        Self::with_state(CountState::Empty, super::guard::RuntimeGuard::False)
    }
    fn with_state(state: CountState, domain: super::guard::RuntimeGuard) -> Self {
        Self {
            state: std::cell::RefCell::new(state),
            domain,
        }
    }
    fn constant_on(count: usize, domain: super::guard::RuntimeGuard) -> Self {
        if domain.is_impossible() {
            return Self::empty();
        }
        Self::with_state(CountState::Exact(CountExpression::constant(count)), domain)
    }
    fn exhaust(&self) {
        if !self.is_exhausted() {
            tracing::debug!(reason = ?super::BoundReason::BudgetNodes, "exhaust budget computation");
        }
        *self.state.borrow_mut() = CountState::Exhausted;
    }
    fn is_empty(&self) -> bool {
        !self.is_exhausted()
            && (self.domain.is_impossible() || matches!(*self.state.borrow(), CountState::Empty))
    }
    fn is_exhausted(&self) -> bool {
        matches!(*self.state.borrow(), CountState::Exhausted)
    }
    fn definitely_below(&self, threshold: usize) -> bool {
        match &*self.state.borrow() {
            CountState::Empty => true,
            CountState::Exact(value) => value.maximum < threshold,
            CountState::Exhausted => false,
        }
    }
    fn definitely_at_least(&self, threshold: usize) -> bool {
        match &*self.state.borrow() {
            CountState::Exact(value) => value.minimum >= threshold,
            CountState::Empty | CountState::Exhausted => false,
        }
    }

    fn selected(
        decision: &std::rc::Rc<super::FrozenTest>,
        yes: &Self,
        no: &Self,
        arena: &super::Arena,
    ) -> Self {
        let condition = arena.runtime_path(&[super::DecisionEdge {
            test: std::rc::Rc::clone(decision),
            taken: true,
        }]);
        let domain = CountThreshold::default().select(&condition, &yes.domain, &no.domain, arena);
        let Ok(domain) = domain else {
            return Self::with_state(CountState::Exhausted, super::guard::RuntimeGuard::True);
        };
        let state = match (&*yes.state.borrow(), &*no.state.borrow()) {
            (
                CountState::Exhausted,
                CountState::Empty | CountState::Exact(_) | CountState::Exhausted,
            )
            | (CountState::Empty | CountState::Exact(_), CountState::Exhausted) => {
                CountState::Exhausted
            }
            (CountState::Exact(yes), CountState::Exact(no)) => {
                CountState::Exact(CountExpression::selected(condition, yes, no))
            }
            (CountState::Exact(value), CountState::Empty)
            | (CountState::Empty, CountState::Exact(value)) => {
                CountState::Exact(std::rc::Rc::clone(value))
            }
            (CountState::Empty, CountState::Empty) => CountState::Empty,
        };
        Self::with_state(state, domain)
    }

    fn increment(&mut self) {
        let state = self.state.get_mut();
        if let CountState::Exact(source) = state {
            *source = CountExpression::increment(source);
        }
    }

    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::budget_work"
    )]
    fn overflow(
        &self,
        arena: &super::Arena,
    ) -> Result<super::guard::RuntimeGuard, super::BoundReason> {
        if self.is_exhausted() {
            return Err(super::BoundReason::BudgetNodes);
        }
        if self.is_empty() || self.definitely_below(129) {
            return Ok(super::guard::RuntimeGuard::False);
        }
        if self.definitely_at_least(129) {
            return Ok(self.domain.clone());
        }
        let mut query = CountThreshold::default();
        // D ∧ ¬(D ∧ L) = D ∧ ¬L: mask the raw threshold once, after complementing it.
        let result = match &*self.state.borrow() {
            CountState::Exact(expression) => {
                query.below(expression, 129, arena).and_then(|below| {
                    query.select(
                        &below,
                        &super::guard::RuntimeGuard::False,
                        &self.domain,
                        arena,
                    )
                })
            }
            CountState::Empty => Ok(super::guard::RuntimeGuard::False),
            CountState::Exhausted => Err(super::BoundReason::BudgetNodes),
        };
        if result.is_err() {
            self.exhaust();
        }
        result
    }

    fn reset_overflow(&mut self, overflow: super::guard::RuntimeGuard) {
        let CountState::Exact(source) = self.state.get_mut() else {
            return;
        };
        if overflow.is_impossible() {
            return;
        }
        if overflow == self.domain {
            *source = CountExpression::constant(1);
            return;
        }
        // On this live domain only count 129 overflowed; the complementary arm is at most 128.
        *source = std::rc::Rc::new(CountExpression {
            minimum: source.minimum.min(1),
            maximum: 128,
            threshold: std::cell::RefCell::default(),
            kind: CountExpressionKind::Select {
                condition: overflow,
                yes: CountExpression::constant(1),
                no: std::rc::Rc::clone(source),
            },
        });
    }

    pub(super) fn overflow_under(
        &self,
        active: &super::guard::RuntimeGuard,
        arena: &super::Arena,
    ) -> bool {
        let result = self.overflow(arena).and_then(|overflow| {
            CountThreshold::default().select(
                active,
                &overflow,
                &super::guard::RuntimeGuard::False,
                arena,
            )
        });
        if let Ok(overflow) = result {
            !overflow.is_impossible()
        } else {
            self.exhaust();
            true
        }
    }

    fn restrict(&self, active: &super::guard::RuntimeGuard, arena: &super::Arena) -> Self {
        if self.is_exhausted() {
            return self.clone();
        }
        let domain = CountThreshold::default().select(
            active,
            &self.domain,
            &super::guard::RuntimeGuard::False,
            arena,
        );
        match domain {
            Ok(domain) if domain.is_impossible() => Self::empty(),
            Ok(domain) => Self::with_state(self.state.borrow().clone(), domain),
            Err(_) => Self::with_state(CountState::Exhausted, self.domain.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Admission {
    All,
    Restricted(super::guard::RuntimeGuard),
}

#[derive(Clone, Debug)]
pub(super) struct RuntimeCondition {
    arena: std::rc::Rc<super::Arena>,
    // Retain the admitted joint so its existing projection entries remain reachable.
    admitted: super::guard::RuntimeGuard,
    ambient: std::rc::Rc<Vec<super::DecisionEdge>>,
    admission: Admission,
}

impl PartialEq for RuntimeCondition {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            arena,
            admitted,
            ambient,
            admission,
        } = self;
        let Self {
            arena: other_arena,
            admitted: other_admitted,
            ambient: other_ambient,
            admission: other_admission,
        } = other;
        (std::rc::Rc::ptr_eq(arena, other_arena)
            && admitted == other_admitted
            && ambient == other_ambient
            && admission == other_admission)
            || self.predicate() == other.predicate()
    }
}
impl Eq for RuntimeCondition {}

impl RuntimeCondition {
    fn restricted_test(&self) -> Option<std::rc::Rc<super::FrozenTest>> {
        match &self.admission {
            Admission::All => None,
            Admission::Restricted(_) => Some(self.arena.budget_test(self.admitted.clone())),
        }
    }

    fn exact(&self) -> bool {
        self.restricted_test()
            .is_none_or(|test| test.coverage == super::Coverage::Exact)
    }

    #[tracing::instrument(skip_all, level = "debug", target = "helm_schema_ir::value::proof_materialization", fields(admitted = self.admitted.id(), restricted = matches!(self.admission, Admission::Restricted(_))))]
    fn predicate(&self) -> Predicate {
        if self.admitted.is_impossible() {
            return Predicate::False;
        }
        // Preserve each frozen test's partial-coverage envelope at the observation boundary.
        let domain = Predicate::all(
            self.ambient
                .iter()
                .map(|edge| edge.test.possible_predicate(edge.taken))
                .collect(),
        );
        match self.restricted_test() {
            None => domain,
            Some(admitted) => self.arena.predicate_memo.normalize(Predicate::all(vec![
                domain,
                admitted.possible_predicate(true),
            ])),
        }
    }
}

pub(super) struct BudgetSlot {
    pub(super) condition: RuntimeCondition,
    pub(super) ordinal: usize,
}

impl super::State {
    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::output_work"
    )]
    fn reserve_count(
        &self,
        count: &BranchCounts,
        ordinal: usize,
    ) -> Result<(Vec<BudgetSlot>, bool), super::BoundReason> {
        if count.is_empty() {
            return Ok((Vec::new(), false));
        }
        let ambient_slot = || {
            vec![BudgetSlot {
                condition: RuntimeCondition {
                    arena: std::rc::Rc::clone(&self.heap.arena),
                    admitted: count.domain.clone(),
                    ambient: std::rc::Rc::clone(&self.active),
                    admission: Admission::All,
                },
                ordinal,
            }]
        };
        // Storage ordinals preserve event order; only admission consumes the branch-local budget.
        if count.definitely_below(128) {
            return Ok((ambient_slot(), false));
        }
        let mut query = CountThreshold::default();
        let expression = match &*count.state.borrow() {
            CountState::Exact(expression) => std::rc::Rc::clone(expression),
            CountState::Empty => return Ok((Vec::new(), false)),
            CountState::Exhausted => return Err(super::BoundReason::BudgetNodes),
        };
        let admission = query
            .below(&expression, 128, &self.heap.arena)
            .inspect_err(|_| count.exhaust())?;
        let admitted = query
            .select(
                &admission,
                &count.domain,
                &super::guard::RuntimeGuard::False,
                &self.heap.arena,
            )
            .inspect_err(|_| count.exhaust())?;
        let rejected = admitted != count.domain;
        if admitted.is_impossible() {
            return Ok((Vec::new(), rejected));
        }
        // The active domain already carries this guard when every live branch has room.
        if !rejected {
            return Ok((ambient_slot(), false));
        }
        Ok((
            vec![BudgetSlot {
                condition: RuntimeCondition {
                    arena: std::rc::Rc::clone(&self.heap.arena),
                    admitted,
                    ambient: std::rc::Rc::clone(&self.active),
                    admission: Admission::Restricted(admission),
                },
                ordinal,
            }],
            rejected,
        ))
    }
    fn reserve_observation(&mut self) -> Vec<BudgetSlot> {
        self.observation_count = self.observation_count.restrict(
            &self.heap.arena.runtime_path(&self.active),
            &self.heap.arena,
        );
        let Ok((slots, rejected)) = self.reserve_count(
            &self.observation_count,
            self.sites.len() + self.text_sites.len(),
        ) else {
            self.observation_count.exhaust();
            self.output_widened = true;
            return Vec::new();
        };
        self.output_widened |= rejected;
        self.observation_count.increment();
        slots
    }
    pub(super) fn invocations_complete(&self) -> bool {
        self.invocation_records_complete
            && !self.invocation_count.overflow_under(
                &self.heap.arena.runtime_path(&self.active),
                &self.heap.arena,
            )
    }

    pub(super) fn reserve_invocation(&mut self) -> Vec<BudgetSlot> {
        self.invocation_count = self.invocation_count.restrict(
            &self.heap.arena.runtime_path(&self.active),
            &self.heap.arena,
        );
        let Ok((slots, _)) = self.reserve_count(&self.invocation_count, self.invocations.len())
        else {
            self.invocation_count.exhaust();
            self.invocation_records_complete = false;
            return Vec::new();
        };
        self.invocation_count.increment();
        slots
    }
    pub(super) fn merge_observations(
        &mut self,
        decision: &std::rc::Rc<super::FrozenTest>,
        yes: &Self,
        no: &Self,
    ) {
        self.output = TextBuffer::join(
            std::rc::Rc::clone(decision),
            &yes.output,
            &no.output,
            &self.heap.arena,
        );
        self.output_widened = yes.output_widened || no.output_widened;
        self.observation_count = BranchCounts::selected(
            decision,
            &yes.observation_count,
            &no.observation_count,
            &self.heap.arena,
        );
        self.invocation_count = BranchCounts::selected(
            decision,
            &yes.invocation_count,
            &no.invocation_count,
            &self.heap.arena,
        );
        self.sites = yes.sites.union_by(&no.sites, ExecutedSite::same_site);
        self.text_sites = yes.text_sites.union_by(&no.text_sites, PartialEq::eq);
        self.invocations = yes.invocations.union_by(&no.invocations, PartialEq::eq);
        self.invocation_records_complete =
            yes.invocation_records_complete && no.invocation_records_complete;
    }

    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::output_work"
    )]
    pub(super) fn emit_bytes(
        &mut self,
        span: helm_schema_syntax::Span,
        source_path: Option<&str>,
        bytes: &str,
    ) {
        if bytes.is_empty() {
            return;
        }
        let slots = self.reserve_observation();
        self.output.push(OutputNode {
            occurrences: slots
                .iter()
                .map(|slot| ObservationId {
                    program: self.program,
                    ordinal: slot.ordinal,
                    span,
                })
                .collect(),
            part: TextPart::Bytes(bytes.to_owned()),
        });
        if self.output.is_widened_under(
            &self.heap.arena.runtime_path(&self.active),
            &self.heap.arena,
        ) {
            self.output_widened = true;
        }
        for slot in slots {
            self.text_sites.push(ExecutedText {
                program: self.program,
                rendered_unchanged: true,
                coverage_exact: self
                    .active
                    .iter()
                    .all(|edge| edge.test.coverage == super::Coverage::Exact),
                ordinal: slot.ordinal,
                bytes: bytes.to_owned(),
                span,
                source_path: source_path.map(str::to_owned),
                condition: slot.condition,
            });
        }
    }
    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::output_work"
    )]
    pub(super) fn observe(
        &mut self,
        span: helm_schema_syntax::Span,
        source_path: Option<&str>,
        value: &Value,
        rendered: bool,
    ) {
        if rendered {
            self.record_container_read(value);
        }
        let value = self.heap.freeze(&self.consumed(value));
        let raw_format_inputs = raw_format_inputs(&value.value, self);
        let slots = self.reserve_observation();
        if rendered {
            self.output.push(OutputNode {
                occurrences: slots
                    .iter()
                    .map(|slot| ObservationId {
                        program: self.program,
                        ordinal: slot.ordinal,
                        span,
                    })
                    .collect(),
                part: TextPart::Frozen(value.clone()),
            });
        }
        if self.output.is_widened_under(
            &self.heap.arena.runtime_path(&self.active),
            &self.heap.arena,
        ) {
            self.output_widened = true;
        }
        for slot in slots {
            self.sites.push(ExecutedSite {
                program: self.program,
                rendered_unchanged: true,
                coverage_exact: self
                    .active
                    .iter()
                    .all(|edge| edge.test.coverage == super::Coverage::Exact),
                ordinal: slot.ordinal,
                span,
                source_path: source_path.map(str::to_owned),
                condition: slot.condition,
                value: value.clone(),
                raw_format_inputs: raw_format_inputs.clone(),
                plain_scalar_placement: helm_schema_core::PlainScalarPlacement::Unknown,
                rendered,
            });
        }
    }
}
fn capture_map_argument(
    read: &super::RuntimeRead,
    allows_nil: bool,
    facts: &mut crate::observed_facts::ObservedFacts,
    failures: &mut Vec<Predicate>,
) -> bool {
    let condition = read.condition.clone();
    let invalid = Predicate::from(helm_schema_core::Guard::TypeIs {
        path: read.path.clone(),
        schema_type: "object".to_owned(),
    })
    .negated();
    failures.push(Predicate::all(vec![
        condition.clone(),
        invalid,
        if allows_nil {
            super::kinds::input_nil_predicate(&read.path).negated()
        } else {
            Predicate::True
        },
    ]));
    let mut ranged = crate::range_modes::RangeModes::default();
    if read
        .path
        .segments()
        .any(helm_schema_core::Segment::is_each_member)
    {
        if allows_nil || read.path.item_parent().is_some() {
            return false;
        }
        let collection = helm_schema_core::ValuesPath::from_segments(
            read.path
                .segments()
                .take_while(|segment| !segment.is_each_member())
                .cloned(),
        );
        ranged.mark_member_identity(&collection);
        ranged.mark_destructured(&collection);
        let mut parent = read.path.parent();
        for _ in 0..super::MAX_SCALAR_DISPATCH_STATES {
            let Some(path) = parent.filter(|path| {
                path.segments()
                    .any(helm_schema_core::Segment::is_each_member)
            }) else {
                break;
            };
            facts.captures.insert(crate::observed_facts::FailCapture {
                conjunction: helm_schema_core::Conjunction::new([condition.clone()]),
                ranged: ranged.clone(),
                kind: crate::observed_facts::CaptureKind::ValueType {
                    path: path.clone(),
                    schema_type: "object".to_owned(),
                    null_aborts: true,
                },
            });
            facts.captures.insert(crate::observed_facts::FailCapture {
                conjunction: helm_schema_core::Conjunction::new([condition.clone()]),
                ranged: ranged.clone(),
                kind: crate::observed_facts::CaptureKind::AbsenceAborts { path: path.clone() },
            });
            parent = path.parent();
        }
    }
    let type_condition = super::query::without_ancestor_nil(&condition, &read.path)
        .unwrap_or_else(|| condition.clone());
    facts.captures.insert(crate::observed_facts::FailCapture {
        conjunction: helm_schema_core::Conjunction::new([type_condition]),
        ranged: ranged.clone(),
        kind: crate::observed_facts::CaptureKind::ValueType {
            path: read.path.clone(),
            schema_type: "object".to_owned(),
            null_aborts: !allows_nil,
        },
    });
    if !allows_nil {
        facts.captures.insert(crate::observed_facts::FailCapture {
            conjunction: helm_schema_core::Conjunction::new([condition]),
            ranged,
            kind: crate::observed_facts::CaptureKind::AbsenceAborts {
                path: read.path.clone(),
            },
        });
    }
    true
}

fn capture_member_host(
    read: &super::RuntimeRead,
    allows_nil: bool,
    facts: &mut crate::observed_facts::ObservedFacts,
) {
    let mut ranged = crate::range_modes::RangeModes::default();
    if let Some(parent) = read.path.item_parent() {
        ranged.mark_member_identity(&parent);
        ranged.mark_destructured(&parent);
    }
    let invalid_host = Predicate::from(helm_schema_core::Guard::TypeIs {
        path: read.path.clone(),
        schema_type: "object".to_owned(),
    })
    .negated();
    facts.captures.insert(crate::observed_facts::FailCapture {
        conjunction: helm_schema_core::Conjunction::new([read.condition.clone(), invalid_host]),
        ranged,
        kind: crate::observed_facts::CaptureKind::MemberAccess {
            handled_kinds: if allows_nil {
                BTreeSet::from(["null".to_owned()])
            } else {
                BTreeSet::new()
            },
        },
    });
}

impl DocumentExecution {
    pub(super) fn new(mut exits: Vec<Exit>, output: super::FileOutput) -> Self {
        let site_index = super::site_index::SiteIndex::new(&exits);
        let placement = super::yaml::FilePlacement::seal(&exits, output);
        placement.certify_sites(&mut exits);
        let exits = super::session::finish_file(exits);
        let root_programs = exits.iter().map(|exit| exit.state.program).collect();
        let mut execution = Self {
            exits,
            root_programs,
            site_index,
            observed_facts: crate::observed_facts::ObservedFacts::default(),
            placement,
        };
        let mut observed_type_failures = Vec::new();
        let mut map_arguments: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for read in execution
            .exits
            .iter()
            .flat_map(|exit| exit.state.reads.iter())
        {
            let allows_nil = match read.coverage {
                super::ReadCoverage::MemberHost { allows_nil } => allows_nil,
                super::ReadCoverage::MapArgument { allows_nil } => {
                    map_arguments
                        .entry((read.path.clone(), allows_nil))
                        .or_default()
                        .push(read.condition.clone());
                    continue;
                }
                super::ReadCoverage::ExactPath | super::ReadCoverage::UnknownRegion => continue,
            };
            capture_member_host(read, allows_nil, &mut execution.observed_facts);
        }
        for ((path, allows_nil), conditions) in map_arguments {
            let read = super::RuntimeRead {
                path,
                condition: Predicate::Or(conditions).normalize_boolean(),
                coverage: super::ReadCoverage::MapArgument { allows_nil },
            };
            if !capture_map_argument(
                &read,
                allows_nil,
                &mut execution.observed_facts,
                &mut observed_type_failures,
            ) {
                capture_member_host(&read, allows_nil, &mut execution.observed_facts);
            }
        }
        let aborted = execution.abort_predicate();
        let aborted = execution.exits.first().map_or(aborted.clone(), |exit| {
            exit.state
                .heap
                .arena
                .predicate_memo
                .normalize(Predicate::all(vec![
                    aborted,
                    Predicate::Or(observed_type_failures).negated(),
                ]))
        });
        if aborted != Predicate::False {
            execution
                .observed_facts
                .captures
                .insert(crate::observed_facts::FailCapture {
                    conjunction: helm_schema_core::Conjunction::new([aborted]),
                    ranged: crate::range_modes::RangeModes::default(),
                    kind: crate::observed_facts::CaptureKind::Fail,
                });
        }
        execution
    }
    pub(crate) fn unknown_read_regions(
        &self,
    ) -> impl Iterator<Item = &helm_schema_core::ValuesPath> {
        self.exits
            .iter()
            .flat_map(|exit| &exit.state.reads)
            .filter(|read| read.coverage == super::ReadCoverage::UnknownRegion)
            .map(|read| &read.path)
    }
    pub(crate) fn reads(&self) -> Vec<crate::fragment_eval::ValueRead> {
        self.exits
            .iter()
            .flat_map(|exit| exit.state.reads.iter())
            .filter_map(|read| {
                let kind = match read.coverage {
                    super::ReadCoverage::ExactPath => crate::ValueKind::WidenedDependency,
                    super::ReadCoverage::UnknownRegion => crate::ValueKind::WidenedRegion,
                    super::ReadCoverage::MemberHost { .. }
                    | super::ReadCoverage::MapArgument { .. } => return None,
                };
                Some(crate::fragment_eval::ValueRead {
                    values_path: read.path.clone(),
                    kind,
                    condition: helm_schema_core::GuardDnf::from_conjunction([read
                        .condition
                        .clone()]),
                    resource: None,
                    provenance: Vec::new(),
                    dependency: true,
                })
            })
            .collect()
    }

    pub(crate) fn projection_reads(&self) -> Vec<crate::fragment_eval::ValueRead> {
        let mut reads = self.reads();
        for site in self.sites().filter(|site| site.rendered) {
            for values_path in site.value.inputs() {
                if values_path.segments().next().is_none() {
                    continue;
                }
                reads.push(crate::fragment_eval::ValueRead {
                    values_path,
                    kind: crate::ValueKind::Serialized,
                    condition: helm_schema_core::GuardDnf::from_conjunction([site.condition()]),
                    resource: None,
                    provenance: Vec::new(),
                    dependency: true,
                });
            }
        }
        reads.sort();
        reads.dedup();
        reads
    }
    pub(crate) fn observed_facts(&self) -> &crate::observed_facts::ObservedFacts {
        &self.observed_facts
    }
    pub(crate) fn observations(
        &self,
        id: super::ObservationId,
    ) -> impl Iterator<Item = super::site_index::Observation<'_>> {
        self.site_index.observations(&self.exits, id)
    }

    pub(crate) fn is_root_observation(&self, id: super::ObservationId) -> bool {
        self.root_programs.contains(&id.program)
    }

    pub(crate) fn placed_documents(&self) -> impl Iterator<Item = super::yaml::PlacedDocument<'_>> {
        self.placement.documents(&self.root_programs)
    }

    pub(crate) fn sites(&self) -> impl Iterator<Item = &ExecutedSite> {
        self.exits
            .iter()
            .filter(|exit| matches!(exit.flow, Flow::Normal(_)))
            .flat_map(|exit| exit.state.sites.iter())
            .filter(|site| self.root_programs.contains(&site.program))
    }
    pub(crate) fn abort_predicate(&self) -> Predicate {
        self.placement.abort.clone()
    }
}
pub(super) fn exit_may_continue(exit: &Exit) -> bool {
    flow_may_continue(&exit.flow, &exit.state.active)
}

pub(super) fn flow_may_continue(flow: &Flow, active: &[super::DecisionEdge]) -> bool {
    !matches!(flow, Flow::Abort(_))
        || active
            .iter()
            .any(|edge| edge.test.coverage == super::Coverage::Partial)
}

impl FrozenValue {
    pub(crate) fn exact_text(&self) -> Option<String> {
        match self.value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum: Datum::Text(text),
                origin: _,
            }) => return text.exact_text(),
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
            } => {}
        }
        match self.literal()? {
            GuardValue::String(text) => Some(text.clone()),
            GuardValue::Float(_) => None,
            GuardValue::Null => Some("<no value>".to_owned()),
            value => Some(value.to_string()),
        }
    }
    pub(super) fn json_with(&self, query: &mut JsonQuery<'_>) -> Option<serde_json::Value> {
        json(&self.value, &self.heap, &mut BTreeSet::new(), 0, query)
    }
}
pub(super) struct JsonQuery<'a> {
    pub(super) preserve_nil_collections: bool,
    pub(super) remaining: Option<&'a mut usize>,
    pub(super) outcome: &'a dyn Fn(&super::FrozenTest) -> Option<bool>,
    pub(super) choice: Option<std::rc::Rc<super::FrozenTest>>,
}

impl JsonQuery<'_> {
    fn taken(&mut self, decision: &std::rc::Rc<super::FrozenTest>) -> Option<bool> {
        let taken = decision
            .constant_outcome()
            .or_else(|| (self.outcome)(decision));
        if taken.is_none() {
            self.choice = Some(std::rc::Rc::clone(decision));
        }
        taken
    }
}

fn json_string_slice(
    slice: &super::StringSlice,
    heap: &super::HeapVersion,
    seen: &BTreeSet<super::ObjectId>,
    depth: usize,
    query: &mut JsonQuery<'_>,
) -> Option<serde_json::Value> {
    let super::ObjectVersion {
        data: ObjectData::Strings(slots),
        revision: _,
        multiplicity: _,
    } = heap.object(&slice.backing)?
    else {
        return None;
    };
    let slots = slots.get(slice.start..slice.start.checked_add(slice.length)?)?;
    Some(serde_json::Value::Array(
        slots
            .iter()
            .map(|value| json(value, heap, &mut seen.clone(), depth + 1, query))
            .collect::<Option<Vec<_>>>()?,
    ))
}

fn json_map(
    id: &super::ObjectId,
    heap: &super::HeapVersion,
    seen: &BTreeSet<super::ObjectId>,
    depth: usize,
    query: &mut JsonQuery<'_>,
) -> Option<serde_json::Value> {
    let super::ObjectVersion {
        data:
            ObjectData::Map {
                fields,
                remainder: super::Remainder::Closed,
            },
        revision: _,
        multiplicity: _,
    } = heap.object(id)?
    else {
        return None;
    };
    let mut result = serde_json::Map::new();
    for (key, field) in fields {
        if let Some(remaining) = query.remaining.as_mut()
            && key.len() > 64 * 1024
        {
            **remaining = 0;
            return None;
        }
        let mut field = field;
        let mut field_depth = depth;
        while let super::Field::Select { decision, yes, no } = field {
            field_depth += 1;
            if field_depth > 32 {
                return None;
            }
            if let Some(remaining) = &mut query.remaining {
                **remaining = remaining.checked_sub(1)?;
            }
            field = if query.taken(decision)? { yes } else { no };
        }
        match field {
            super::Field::Present(value) => {
                result.insert(
                    key.clone(),
                    json(value, heap, &mut seen.clone(), depth + 1, query)?,
                );
            }
            super::Field::Absent => {}
            super::Field::Unknown
            | super::Field::Select {
                decision: _,
                yes: _,
                no: _,
            } => return None,
        }
    }
    Some(serde_json::Value::Object(result))
}

fn json_sequence(
    sequence: &super::InterfaceSequence,
    heap: &super::HeapVersion,
    seen: &BTreeSet<super::ObjectId>,
    depth: usize,
    query: &mut JsonQuery<'_>,
) -> Option<serde_json::Value> {
    if sequence.storage.is_none() {
        return Some(if query.preserve_nil_collections {
            serde_json::Value::Array(Vec::new())
        } else {
            serde_json::Value::Null
        });
    }
    Some(serde_json::Value::Array(
        sequence
            .items()?
            .iter()
            .map(|value| json(value, heap, &mut seen.clone(), depth + 1, query))
            .collect::<Option<Vec<_>>>()?,
    ))
}

fn json(
    value: &Value,
    heap: &super::HeapVersion,
    seen: &mut BTreeSet<super::ObjectId>,
    depth: usize,
    query: &mut JsonQuery<'_>,
) -> Option<serde_json::Value> {
    let mut value = value;
    loop {
        if let Some(remaining) = &mut query.remaining {
            **remaining = remaining.checked_sub(1)?;
        }
        if depth > 32 {
            return None;
        }
        let ValueNode::Select { decision, yes, no } = value.node.as_ref() else {
            break;
        };
        // Selecting a member does not add a JSON container level.
        value = if query.taken(decision)? { yes } else { no };
    }
    if let Some(literal) = value.constant() {
        if let (Some(remaining), GuardValue::String(text)) = (query.remaining.as_mut(), literal)
            && text.len() > 64 * 1024
        {
            **remaining = 0;
            return None;
        }
        if matches!(literal, GuardValue::Float(_)) {
            return None;
        }
        return serde_json::to_value(literal).ok();
    }
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(slice),
            origin: _,
        }) => json_string_slice(slice, heap, seen, depth, query),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    target: Some(id),
                    view: _,
                },
            origin: _,
        }) if seen.insert(id.clone()) => json_map(id, heap, seen, depth, query),
        ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(sequence),
            origin: _,
        }) => json_sequence(sequence, heap, seen, depth, query),
        ValueNode::Leaf(Leaf {
            datum: Datum::Map {
                target: None,
                view: _,
            },
            origin: _,
        }) => Some(if query.preserve_nil_collections {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            serde_json::Value::Null
        }),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Text(_)
                | Datum::Scalar {
                    go_type: _,
                    content: _,
                }
                | Datum::Map {
                    target: Some(_),
                    view: _,
                }
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
        } => None,
    }
}

impl PartialEq for ExecutedText {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            program,
            coverage_exact: _,
            rendered_unchanged,
            ordinal,
            bytes,
            span,
            source_path,
            condition,
        } = self;
        let Self {
            program: other_program,
            coverage_exact: _,
            rendered_unchanged: other_unchanged,
            ordinal: other_ordinal,
            bytes: other_bytes,
            span: other_span,
            source_path: other_source,
            condition: other_condition,
        } = other;
        program == other_program
            && rendered_unchanged == other_unchanged
            && ordinal == other_ordinal
            && bytes == other_bytes
            && span == other_span
            && source_path == other_source
            && condition == other_condition
            && self.coverage_exact() == other.coverage_exact()
    }
}

impl super::ExecutedText {
    pub(crate) fn coverage_exact(&self) -> bool {
        self.coverage_exact && self.condition.exact()
    }

    pub(crate) fn span(&self) -> helm_schema_syntax::Span {
        self.span
    }
    pub(crate) fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }
    pub(crate) fn condition(&self) -> Predicate {
        self.condition.predicate()
    }
    pub(crate) fn bytes(&self) -> &str {
        &self.bytes
    }
}

#[cfg(test)]
#[path = "../tests/runtime_output_work.rs"]
mod output_work_tests;

#[cfg(test)]
#[path = "../tests/runtime_scalar_projection.rs"]
mod scalar_projection_tests;

struct RawFormatArm<'value> {
    condition: Predicate,
    value: &'value Value,
}

fn raw_format_inputs(value: &Value, state: &super::State) -> Vec<super::RawFormatInput> {
    let mut pending = vec![RawFormatArm {
        condition: Predicate::True,
        value,
    }];
    let mut inputs = Vec::new();
    let mut proofs = BTreeMap::new();
    let mut remaining = super::MAX_SCALAR_DISPATCH_STATES;
    while let Some(RawFormatArm { condition, value }) = pending.pop() {
        if condition == Predicate::False {
            continue;
        }
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => {
                if decision.coverage != super::Coverage::Exact
                    || decision.when_true.contains_approximation()
                    || decision.when_false.contains_approximation()
                {
                    return Vec::new();
                }
                pending.push(RawFormatArm {
                    condition: Predicate::all(vec![condition.clone(), decision.when_false.clone()])
                        .normalize_boolean(),
                    value: no,
                });
                pending.push(RawFormatArm {
                    condition: Predicate::all(vec![condition, decision.when_true.clone()])
                        .normalize_boolean(),
                    value: yes,
                });
            }
            ValueNode::Leaf(Leaf { datum, origin }) => {
                if remaining == 0 {
                    return Vec::new();
                }
                remaining -= 1;
                let path = original_identity_path(datum, origin, state);
                if let Some(path) = path {
                    let unchanged = *proofs
                        .entry(path.clone())
                        .or_insert_with(|| original_input_is_unchanged(&path, state));
                    if unchanged {
                        inputs.push(super::RawFormatInput { condition, path });
                    }
                }
            }
        }
    }
    inputs
}

fn original_identity_path(
    datum: &Datum,
    origin: &super::Origin,
    state: &super::State,
) -> Option<helm_schema_core::ValuesPath> {
    match datum {
        Datum::Scalar {
            go_type: _,
            content: Some(ScalarContent::Identity(path)),
        } => Some(path.clone()),
        Datum::Map {
            view: _,
            target: Some(object),
        } => state
            .heap
            .arena
            .input_paths
            .borrow()
            .get(object)
            .filter(|path| {
                state
                    .heap
                    .arena
                    .input_objects
                    .borrow()
                    .get(*path)
                    .is_some_and(|backing| backing.object == *object)
            })
            .cloned(),
        Datum::Scalar {
            go_type: _,
            content: None,
        }
        | Datum::Map {
            view: _,
            target: None,
        }
        | Datum::Unknown(super::UnknownValue {
            types: _,
            aliases: _,
        }) => origin.identity.clone(),
        Datum::Text(_)
        | Datum::Scalar {
            go_type: _,
            content:
                Some(
                    ScalarContent::Literal(_)
                    | ScalarContent::Rendered(_)
                    | ScalarContent::PrintfStringIdentity(_)
                    | ScalarContent::SplitLength {
                        value: _,
                        separator: _,
                    }
                    | ScalarContent::Length(_),
                ),
        }
        | Datum::Sequence(_)
        | Datum::Strings(_)
        | Datum::Slot {
            backing: _,
            index: _,
        } => None,
    }
}

fn original_input_is_unchanged(path: &helm_schema_core::ValuesPath, state: &super::State) -> bool {
    let inputs = state.heap.arena.input_objects.borrow();
    let root = helm_schema_core::ValuesPath::default();
    if inputs
        .get(&root)
        .and_then(|backing| state.heap.version.object(&backing.object))
        .is_none()
    {
        return false;
    }
    let mut prefix = root;
    for segment in path.segments() {
        let helm_schema_core::Segment::Literal(key) = segment else {
            return false;
        };
        if let Some(object) = inputs
            .get(&prefix)
            .and_then(|backing| state.heap.version.object(&backing.object))
        {
            let super::ObjectVersion {
                revision: _,
                multiplicity,
                data,
            } = object;
            if *multiplicity != super::Multiplicity::Singleton {
                return false;
            }
            match data {
                ObjectData::Map {
                    fields,
                    remainder: super::Remainder::Input,
                } if !fields.contains_key(key) => {}
                ObjectData::Map {
                    fields: _,
                    remainder: _,
                }
                | ObjectData::Strings(_)
                | ObjectData::Opaque { reachable: _ }
                | ObjectData::Widened { inputs: _ } => return false,
            }
        }
        prefix.push(key.clone());
    }
    if inputs
        .get(path)
        .and_then(|backing| state.heap.version.object(&backing.object))
        .is_some_and(|object| !pristine_input_backing(object))
    {
        return false;
    }
    // ValuesPath orders encoded keys; starting below the first child skips interleaved siblings.
    let mut first_child = path.clone();
    first_child.push("\0");
    inputs
        .range(first_child..)
        .take_while(|(candidate, _)| candidate.is_descendant_of(path))
        .all(|(_, backing)| {
            state
                .heap
                .version
                .object(&backing.object)
                .is_none_or(pristine_input_backing)
        })
}

fn pristine_input_backing(object: &super::ObjectVersion) -> bool {
    matches!(object, super::ObjectVersion {
        revision: _, multiplicity: super::Multiplicity::Singleton,
        data: ObjectData::Map { fields, remainder: super::Remainder::Input },
    } if fields.is_empty())
}

#[cfg(test)]
#[path = "../tests/runtime_raw_format_inputs.rs"]
mod raw_format_input_tests;

#[cfg(test)]
#[path = "../tests/runtime_count_expression.rs"]
mod count_expression_tests;

#[cfg(test)]
#[path = "../tests/runtime_conditions.rs"]
mod condition_tests;
