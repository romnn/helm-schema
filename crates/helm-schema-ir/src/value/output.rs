use super::scalar_value::ScalarContent;
use super::{Datum, Exit, Flow, FrozenValue, Leaf, ObjectData, Value, ValueNode};
use super::{DocumentExecution, ExecutedSite, ExecutedText, TextBuffer, TextPart};
use crate::abstract_value::AbstractValue;
use helm_schema_core::{GuardValue, Predicate};
use std::collections::{BTreeMap, BTreeSet};

impl TextBuffer {
    fn immutable_text(parts: Vec<TextPart>) -> Self {
        Self {
            parts,
            counts: BranchCounts::empty(),
            arena: None,
        }
    }

    fn push(&mut self, part: TextPart) {
        self.parts.push(part);
        self.counts.increment();
        if self.counts.is_empty() || self.counts.definitely_below(129) {
            return;
        }
        if self.counts.definitely_at_least(129) {
            self.parts = vec![TextPart::Unknown(self.inputs())];
            self.counts = BranchCounts::constant_on(1, self.counts.domain.clone());
            return;
        }
        if self.counts.is_exhausted() {
            self.parts = vec![TextPart::Unknown(self.inputs())];
            self.counts = BranchCounts::constant_on(1, self.counts.domain.clone());
            return;
        }
        if let Some(arena) = &self.arena {
            let Ok(overflow) = self.counts.overflow(arena) else {
                self.parts = vec![TextPart::Unknown(self.inputs())];
                self.counts = BranchCounts::constant_on(1, self.counts.domain.clone());
                return;
            };
            if overflow.is_impossible() {
                return;
            }
            tracing::debug!(reason = ?super::BoundReason::OutputTerms, "widen output");
            let unknown = Self::immutable_text(vec![TextPart::Unknown(self.inputs())]);
            if overflow == self.counts.domain {
                self.parts = unknown.parts;
                self.counts = BranchCounts::constant_on(1, self.counts.domain.clone());
                return;
            }
            let decision = arena.budget_test(overflow.clone());
            let allowed = arena.runtime_path(&[super::DecisionEdge {
                test: std::rc::Rc::clone(&decision),
                taken: false,
            }]);
            let exact = std::rc::Rc::new(Self::immutable_text(self.parts.clone()));
            let exact = TextRetention::default().retain(&exact, &allowed);
            self.parts = vec![TextPart::Select {
                decision,
                yes: std::rc::Rc::new(unknown),
                no: exact,
            }];
            self.counts.reset_overflow(overflow);
        } else {
            // Variable counts are produced only by joins/restriction, which attach the arena.
            self.parts = vec![TextPart::Unknown(self.inputs())];
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

    fn inputs(&self) -> BTreeSet<helm_schema_core::ValuesPath> {
        let mut inputs = BTreeSet::new();
        let mut pending = vec![self];
        let mut seen = BTreeSet::new();
        while let Some(buffer) = pending.pop() {
            if !seen.insert(std::ptr::from_ref(buffer)) {
                continue;
            }
            for part in &buffer.parts {
                match part {
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
            output.parts.push(TextPart::Select {
                decision,
                yes: std::rc::Rc::new(yes),
                no: std::rc::Rc::new(no),
            });
        }
        output
    }

    pub(super) fn bytes(&mut self, bytes: &str) {
        if !bytes.is_empty() {
            self.push(TextPart::Bytes(bytes.to_owned()));
        }
    }

    pub(super) fn emit(&mut self, value: FrozenValue) {
        self.push(TextPart::Frozen(value));
    }

    pub(crate) fn exact_text(&self) -> Option<String> {
        TextQuery::default().exact_text(self)
    }

    pub(super) fn stripped_value(&self) -> Value {
        strip_no_value(&self.as_value(), &mut 128).unwrap_or_else(|| unknown_text(self.inputs()))
    }

    pub(super) fn as_value(&self) -> Value {
        TextQuery::default().as_value(self)
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
                match part {
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
                            parts.push(TextPart::Select {
                                decision: std::rc::Rc::clone(decision),
                                yes: retained_yes,
                                no: retained_no,
                            });
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
struct TextQuery {
    uncertainty: BTreeMap<*const TextBuffer, bool>,
    widening_domains: BTreeMap<*const TextBuffer, Option<super::guard::RuntimeGuard>>,
    widened: BTreeMap<(*const TextBuffer, u64), bool>,
    text: BTreeMap<*const TextBuffer, Option<String>>,
    values: BTreeMap<*const TextBuffer, Value>,
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
            let widened = match part {
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
        let result = buffer.parts.iter().any(|part| match part {
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
        let result = buffer.parts.iter().any(|part| match part {
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
            let text = match part {
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

    fn as_value(&mut self, buffer: &TextBuffer) -> Value {
        let key = std::ptr::from_ref(buffer);
        if let Some(value) = self.values.get(&key) {
            return value.clone();
        }
        let value = if let Some(text) = self.exact_text(buffer) {
            Value::string(&text)
        } else {
            let mut result = Value::string("");
            for part in &buffer.parts {
                let value = match part {
                    TextPart::Bytes(bytes) => Value::string(bytes),
                    TextPart::Frozen(value) => {
                        let mut leaves = 0;
                        frozen_text_value(value, &mut leaves)
                            .unwrap_or_else(|| unknown_text(value.inputs()))
                    }
                    TextPart::Unknown(inputs) => unknown_text(inputs.clone()),
                    TextPart::Select { decision, yes, no } => Value::select(
                        std::rc::Rc::clone(decision),
                        self.as_value(yes),
                        self.as_value(no),
                    ),
                };
                let mut leaves = 0;
                let Some(joined) = concatenate_text(&result, &value, &mut leaves) else {
                    tracing::debug!(reason = ?super::BoundReason::OutputTerms, "widen scalar output alternatives");
                    result = unknown_text(buffer.inputs());
                    break;
                };
                result = joined;
            }
            result
        };
        self.values.insert(key, value.clone());
        value
    }
}

fn strip_no_value(value: &Value, remaining: &mut usize) -> Option<Value> {
    if *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    match value.node.as_ref() {
        ValueNode::Select { decision, yes, no } => Some(Value::select(
            std::rc::Rc::clone(decision),
            strip_no_value(yes, remaining)?,
            strip_no_value(no, remaining)?,
        )),
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        }) => Some(match value.constant() {
            Some(GuardValue::String(text)) => Value::string(&text.replace("<no value>", "")),
            Some(
                GuardValue::Null | GuardValue::Bool(_) | GuardValue::Int(_) | GuardValue::Float(_),
            )
            | None => unknown_text(value.inputs()),
        }),
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
    std::rc::Rc::ptr_eq(&left.heap, &right.heap) && left.value == right.value
}

fn unknown_text(inputs: BTreeSet<helm_schema_core::ValuesPath>) -> Value {
    Value {
        node: std::rc::Rc::new(ValueNode::Leaf(Leaf {
            datum: Datum::Scalar {
                go_type: super::ScalarType::String,
                content: None,
            },
            origin: super::Origin {
                identity: None,
                influences: inputs,
            },
        })),
    }
}

fn frozen_text_value(value: &FrozenValue, leaves: &mut usize) -> Option<Value> {
    match value.value.node.as_ref() {
        ValueNode::Select { decision, yes, no } => Some(Value::select(
            std::rc::Rc::clone(decision),
            frozen_text_value(
                &FrozenValue {
                    heap: std::rc::Rc::clone(&value.heap),
                    value: yes.clone(),
                },
                leaves,
            )?,
            frozen_text_value(
                &FrozenValue {
                    heap: std::rc::Rc::clone(&value.heap),
                    value: no.clone(),
                },
                leaves,
            )?,
        )),
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        }) => {
            *leaves += 1;
            if *leaves > super::MAX_SCALAR_DISPATCH_STATES {
                return None;
            }
            Some(
                value
                    .exact_text()
                    .map_or_else(|| unknown_text(value.inputs()), |text| Value::string(&text)),
            )
        }
    }
}

fn concatenate_text(left: &Value, right: &Value, leaves: &mut usize) -> Option<Value> {
    match (left.node.as_ref(), right.node.as_ref()) {
        (
            ValueNode::Select { decision, yes, no },
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            })
            | ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            },
        ) => Some(Value::select(
            std::rc::Rc::clone(decision),
            concatenate_text(yes, &right.under(decision, true), leaves)?,
            concatenate_text(no, &right.under(decision, false), leaves)?,
        )),
        (
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }),
            ValueNode::Select { decision, yes, no },
        ) => Some(Value::select(
            std::rc::Rc::clone(decision),
            concatenate_text(left, yes, leaves)?,
            concatenate_text(left, no, leaves)?,
        )),
        (
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }),
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }),
        ) => {
            *leaves += 1;
            if *leaves > super::MAX_SCALAR_DISPATCH_STATES {
                return None;
            }
            if let (Some(GuardValue::String(left)), Some(GuardValue::String(right))) =
                (left.constant(), right.constant())
            {
                Some(Value::string(&format!("{left}{right}")))
            } else {
                Some(unknown_text(
                    left.inputs().union(&right.inputs()).cloned().collect(),
                ))
            }
        }
    }
}

impl ExecutedSite {
    fn same_site(&self, other: &Self) -> bool {
        let Self {
            program,
            coverage_exact,
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
            coverage_exact: other_coverage,
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
            && coverage_exact == other_coverage
            && rendered_unchanged == other_unchanged
            && ordinal == other_ordinal
            && span == other_span
            && source_path == other_source
            && condition == other_condition
            && raw_format_inputs == other_raw_format_inputs
            && plain_scalar_placement == other_plain_scalar_placement
            && same_frozen_value(value, other_value)
            && rendered == other_rendered
    }

    pub(crate) fn coverage_exact(&self) -> bool {
        self.coverage_exact
    }

    pub(crate) fn span(&self) -> helm_schema_syntax::Span {
        self.span
    }
    pub(crate) fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }
    pub(crate) fn condition(&self) -> &Predicate {
        &self.condition
    }
    pub(crate) fn rendered(&self) -> bool {
        self.rendered
    }
    pub(crate) fn value(&self) -> &FrozenValue {
        &self.value
    }
    pub(crate) fn projected_value(&self) -> AbstractValue {
        if self.coverage_exact {
            self.value.projected_value()
        } else {
            AbstractValue::Widened(self.value.inputs())
        }
    }
    pub(crate) fn projected_scalar(&self) -> Option<super::ScalarValueDispatch> {
        if !self.coverage_exact {
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
        if !self.rendered || !self.coverage_exact {
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
struct CountExpression {
    thresholds: std::cell::RefCell<BTreeMap<usize, super::guard::CachedGuard>>,
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
            thresholds: std::cell::RefCell::default(),
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
            thresholds: std::cell::RefCell::default(),
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
            thresholds: std::cell::RefCell::default(),
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
            .thresholds
            .borrow()
            .get(&threshold)
            .and_then(super::guard::CachedGuard::upgrade);
        if let Some(result) = cached {
            self.results.insert(key, result.clone());
            return Ok(result);
        }
        expression.thresholds.borrow_mut().remove(&threshold);
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
        expression
            .thresholds
            .borrow_mut()
            .insert(threshold, super::guard::CachedGuard::new(&result));
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

    fn below_with(
        &self,
        threshold: usize,
        arena: &super::Arena,
        query: &mut CountThreshold,
    ) -> Result<super::guard::RuntimeGuard, super::BoundReason> {
        let expression = match &*self.state.borrow() {
            CountState::Empty => return Ok(super::guard::RuntimeGuard::False),
            CountState::Exact(expression) => std::rc::Rc::clone(expression),
            CountState::Exhausted => return Err(super::BoundReason::BudgetNodes),
        };
        let result = query
            .below(&expression, threshold, arena)
            .and_then(|selected| {
                query.select(
                    &selected,
                    &self.domain,
                    &super::guard::RuntimeGuard::False,
                    arena,
                )
            });
        if result.is_err() {
            self.exhaust();
        }
        result
    }

    #[tracing::instrument(skip_all, level = "debug", target = "helm_schema_ir::value::budget_work", fields(threshold, domain = self.domain.id()))]
    fn below(
        &self,
        threshold: usize,
        arena: &super::Arena,
    ) -> Result<super::guard::RuntimeGuard, super::BoundReason> {
        self.below_with(threshold, arena, &mut CountThreshold::default())
    }

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
        let below = self.below_with(129, arena, &mut query)?;
        let result = query.select(
            &below,
            &super::guard::RuntimeGuard::False,
            &self.domain,
            arena,
        );
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
            thresholds: std::cell::RefCell::default(),
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

pub(super) struct BudgetSlot {
    pub(super) condition: Predicate,
    pub(super) ordinal: usize,
    pub(super) coverage_exact: bool,
}

impl super::State {
    fn reserve_count(
        &self,
        count: &BranchCounts,
        ordinal: usize,
    ) -> Result<(Vec<BudgetSlot>, bool), super::BoundReason> {
        if count.is_empty() {
            return Ok((Vec::new(), false));
        }
        // Storage ordinals preserve event order; only admission consumes the branch-local budget.
        if count.definitely_below(128) {
            return Ok((
                vec![BudgetSlot {
                    condition: self.path_predicate(),
                    ordinal,
                    coverage_exact: true,
                }],
                false,
            ));
        }
        let admitted = count.below(128, &self.heap.arena)?;
        let rejected = admitted != count.domain;
        if admitted.is_impossible() {
            return Ok((Vec::new(), rejected));
        }
        let admitted = self.heap.arena.budget_test(admitted);
        Ok((
            vec![BudgetSlot {
                condition: Predicate::all(vec![
                    self.path_predicate(),
                    admitted.possible_predicate(true),
                ])
                .normalize_boolean(),
                ordinal,
                coverage_exact: admitted.coverage == super::Coverage::Exact,
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
        self.sites = yes.sites.clone();
        self.text_sites = yes.text_sites.clone();
        for site in &no.sites {
            if !self.sites.iter().any(|existing| existing.same_site(site)) {
                self.sites.push(site.clone());
            }
        }
        for site in &no.text_sites {
            if !self.text_sites.contains(site) {
                self.text_sites.push(site.clone());
            }
        }
        self.invocations = yes.invocations.clone();
        self.invocation_records_complete =
            yes.invocation_records_complete && no.invocation_records_complete;
        for invocation in &no.invocations {
            if !self.invocations.contains(invocation) {
                self.invocations.push(invocation.clone());
            }
        }
    }

    pub(super) fn emit_bytes(
        &mut self,
        span: helm_schema_syntax::Span,
        source_path: Option<&str>,
        bytes: &str,
    ) {
        self.output.bytes(bytes);
        if self.output.is_widened_under(
            &self.heap.arena.runtime_path(&self.active),
            &self.heap.arena,
        ) {
            self.output_widened = true;
        }
        if bytes.is_empty() {
            return;
        }
        for slot in self.reserve_observation() {
            self.text_sites.push(ExecutedText {
                program: self.program,
                rendered_unchanged: true,
                coverage_exact: self
                    .active
                    .iter()
                    .all(|edge| edge.test.coverage == super::Coverage::Exact)
                    && slot.coverage_exact,
                ordinal: slot.ordinal,
                bytes: bytes.to_owned(),
                span,
                source_path: source_path.map(str::to_owned),
                condition: slot.condition,
            });
        }
    }
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
        if rendered {
            self.output.emit(value.clone());
        }
        if self.output.is_widened_under(
            &self.heap.arena.runtime_path(&self.active),
            &self.heap.arena,
        ) {
            self.output_widened = true;
        }
        for slot in self.reserve_observation() {
            self.sites.push(ExecutedSite {
                program: self.program,
                rendered_unchanged: true,
                coverage_exact: self
                    .active
                    .iter()
                    .all(|edge| edge.test.coverage == super::Coverage::Exact)
                    && slot.coverage_exact,
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
struct SpanTextPiece {
    ordinal: usize,
    condition: Predicate,
    text: Option<String>,
}

struct SpanTextAlternative {
    condition: Predicate,
    text: Option<String>,
}

fn partition_span_text(
    condition: Predicate,
    mut pieces: Vec<SpanTextPiece>,
    memo: &helm_schema_core::PredicateMemo,
) -> Option<Vec<SpanTextAlternative>> {
    pieces.sort_by_key(|piece| piece.ordinal);
    let mut alternatives = vec![SpanTextAlternative {
        condition,
        text: Some(String::new()),
    }];
    for SpanTextPiece {
        ordinal: _,
        condition: guard,
        text: piece,
    } in pieces
    {
        let mut next = Vec::new();
        for SpanTextAlternative { condition, text } in alternatives {
            let included = memo.normalize(Predicate::all(vec![condition.clone(), guard.clone()]));
            let excluded = memo.normalize(Predicate::all(vec![condition, guard.clone().negated()]));
            if !memo.exactly_implies(&included, &Predicate::False) {
                next.push(SpanTextAlternative {
                    condition: included,
                    text: text.clone().zip(piece.as_ref()).map(|(mut text, piece)| {
                        text.push_str(piece);
                        text
                    }),
                });
            }
            if !memo.exactly_implies(&excluded, &Predicate::False) {
                next.push(SpanTextAlternative {
                    condition: excluded,
                    text,
                });
            }
            if next.len() > super::MAX_SCALAR_DISPATCH_STATES {
                return None;
            }
        }
        alternatives = next;
    }
    Some(alternatives)
}

impl DocumentExecution {
    pub(super) fn new(exits: Vec<Exit>) -> Self {
        let root_programs = exits.iter().map(|exit| exit.state.program).collect();
        let mut execution = Self {
            exits,
            root_programs,
            observed_facts: crate::observed_facts::ObservedFacts::default(),
        };
        for read in execution
            .exits
            .iter()
            .flat_map(|exit| exit.state.reads.iter())
        {
            if read.coverage != super::ReadCoverage::MemberHost {
                continue;
            }
            let invalid_host = Predicate::from(helm_schema_core::Guard::TypeIs {
                path: read.path.clone(),
                schema_type: "object".to_owned(),
            })
            .negated();
            execution
                .observed_facts
                .captures
                .insert(crate::observed_facts::FailCapture {
                    conjunction: helm_schema_core::Conjunction::new([
                        read.condition.clone(),
                        invalid_host,
                    ]),
                    ranged: crate::range_modes::RangeModes::default(),
                    kind: crate::observed_facts::CaptureKind::MemberAccess {
                        handled_kinds: BTreeSet::new(),
                    },
                });
        }
        let aborted = execution.abort_predicate();
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
                    super::ReadCoverage::MemberHost => return None,
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
                    condition: helm_schema_core::GuardDnf::from_conjunction([site
                        .condition
                        .clone()]),
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
    pub(crate) fn text_for_span<'a>(
        &'a self,
        span: helm_schema_syntax::Span,
        source_path: Option<&'a str>,
    ) -> impl Iterator<Item = (Predicate, Option<String>)> + 'a {
        self.exits
            .iter()
            .filter(|exit| matches!(exit.flow, Flow::Normal(_)))
            .flat_map(move |exit| {
                let condition = exit.state.path_predicate();
                let fallback = || vec![(condition.clone(), None)];
                if condition.contains_approximation()
                    || exit.state.observation_count.overflow_under(
                        &exit.state.heap.arena.runtime_path(&exit.state.active),
                        &exit.state.heap.arena,
                    )
                    || exit
                        .state
                        .active
                        .iter()
                        .any(|edge| edge.test.coverage == super::Coverage::Partial)
                {
                    return fallback();
                }
                let memo = helm_schema_core::PredicateMemo::new();
                let mut pieces = Vec::new();
                for site in &exit.state.text_sites {
                    if !self.root_programs.contains(&site.program)
                        || site.source_path.as_deref() != source_path
                        || site.span.start >= span.end
                        || span.start >= site.span.end
                        || memo.exactly_implies(&condition, &site.condition.negated())
                    {
                        continue;
                    }
                    let start = span
                        .start
                        .saturating_sub(site.span.start)
                        .min(site.bytes.len());
                    let end = span
                        .end
                        .saturating_sub(site.span.start)
                        .min(site.bytes.len());
                    if site.condition.contains_approximation() {
                        return fallback();
                    }
                    pieces.push(SpanTextPiece {
                        ordinal: site.ordinal,
                        condition: site.condition.clone(),
                        text: (site.coverage_exact && site.rendered_unchanged)
                            .then(|| site.bytes.get(start..end).map(str::to_owned))
                            .flatten(),
                    });
                    if pieces.len() > super::MAX_SCALAR_DISPATCH_STATES {
                        return fallback();
                    }
                }
                for site in &exit.state.sites {
                    if !self.root_programs.contains(&site.program)
                        || !site.rendered
                        || site.source_path.as_deref() != source_path
                        || site.span.start >= span.end
                        || span.start >= site.span.end
                        || memo.exactly_implies(&condition, &site.condition.negated())
                    {
                        continue;
                    }
                    if site.condition.contains_approximation() {
                        return fallback();
                    }
                    pieces.push(SpanTextPiece {
                        ordinal: site.ordinal,
                        condition: site.condition.clone(),
                        text: (site.coverage_exact && site.rendered_unchanged)
                            .then(|| site.value.exact_text())
                            .flatten(),
                    });
                    if pieces.len() > super::MAX_SCALAR_DISPATCH_STATES {
                        return fallback();
                    }
                }
                let Some(alternatives) = partition_span_text(condition.clone(), pieces, &memo)
                else {
                    return fallback();
                };
                alternatives
                    .into_iter()
                    .map(|SpanTextAlternative { condition, text }| (condition, text))
                    .collect()
            })
    }
    pub(crate) fn text_sites(&self) -> impl Iterator<Item = &super::ExecutedText> {
        self.exits
            .iter()
            .filter(|exit| matches!(exit.flow, Flow::Normal(_)))
            .flat_map(|exit| exit.state.text_sites.iter())
            .filter(|site| self.root_programs.contains(&site.program))
    }
    #[cfg(test)]
    pub(crate) fn all_sites(&self) -> impl Iterator<Item = &ExecutedSite> {
        self.exits.iter().flat_map(|exit| exit.state.sites.iter())
    }
    pub(crate) fn sites(&self) -> impl Iterator<Item = &ExecutedSite> {
        self.exits
            .iter()
            .filter(|exit| matches!(exit.flow, Flow::Normal(_)))
            .flat_map(|exit| exit.state.sites.iter())
            .filter(|site| self.root_programs.contains(&site.program))
    }
    pub(crate) fn text(&self) -> impl Iterator<Item = (Predicate, &TextBuffer)> {
        self.exits.iter().filter_map(|exit| match &exit.flow {
            Flow::Normal(_) => Some((exit.state.path_predicate(), &exit.state.output)),
            Flow::Abort(_) | Flow::Break(_) | Flow::Continue(_) => None,
        })
    }
    pub(crate) fn condition_for_span(
        &self,
        span: helm_schema_syntax::Span,
        source_path: Option<&str>,
    ) -> Predicate {
        let text = self
            .exits
            .iter()
            .filter(|exit| matches!(exit.flow, Flow::Normal(_)))
            .flat_map(|exit| &exit.state.text_sites)
            .filter(|site| {
                self.root_programs.contains(&site.program)
                    && site.source_path.as_deref() == source_path
                    && site.span.start < span.end
                    && span.start < site.span.end
            })
            .map(|site| site.condition.clone());
        let actions = self
            .sites()
            .filter(|site| {
                self.root_programs.contains(&site.program)
                    && site.source_path() == source_path
                    && site.span.start < span.end
                    && span.start < site.span.end
            })
            .map(|site| site.condition.clone());
        Predicate::Or(text.chain(actions).collect()).normalize_boolean()
    }
    pub(crate) fn abort_predicate(&self) -> Predicate {
        let aborted = Predicate::Or(
            self.exits
                .iter()
                .filter(|exit| !exit_may_continue(exit))
                .map(|exit| exit.state.path_predicate())
                .collect(),
        );
        Predicate::all(vec![
            aborted,
            possible_success_predicate(&self.exits).negated(),
        ])
        .normalize_boolean()
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

pub(super) fn possible_success_predicate(exits: &[Exit]) -> Predicate {
    Predicate::Or(
        exits
            .iter()
            .filter(|exit| exit_may_continue(exit))
            .map(|exit| exit.state.path_predicate())
            .collect(),
    )
}

impl FrozenValue {
    pub(crate) fn exact_text(&self) -> Option<String> {
        match self.literal()? {
            GuardValue::String(text) => Some(text.clone()),
            GuardValue::Float(_) => None,
            GuardValue::Null => Some("<no value>".to_owned()),
            value => Some(value.to_string()),
        }
    }
    pub(crate) fn projected_value(&self) -> AbstractValue {
        project(&self.value, &self.heap, &mut BTreeSet::new(), 0)
    }
    pub(super) fn json(&self) -> Option<serde_json::Value> {
        json(&self.value, &self.heap, &mut BTreeSet::new(), 0)
    }
}
fn project_sequence(
    value: &Value,
    sequence: &super::InterfaceSequence,
    heap: &super::HeapVersion,
    seen: &BTreeSet<super::ObjectId>,
    depth: usize,
) -> AbstractValue {
    if sequence.storage.is_none() {
        return AbstractValue::Widened(value.inputs());
    }
    let Some(items) = sequence.items() else {
        return AbstractValue::Widened(value.inputs());
    };
    AbstractValue::List(
        items
            .iter()
            .map(|value| project(value, heap, &mut seen.clone(), depth + 1))
            .collect(),
    )
}

fn project_string_slice(
    value: &Value,
    slice: &super::StringSlice,
    heap: &super::HeapVersion,
    seen: &BTreeSet<super::ObjectId>,
    depth: usize,
) -> AbstractValue {
    let Some(super::ObjectVersion {
        data: ObjectData::Strings(slots),
        revision: _,
        multiplicity: _,
    }) = heap.object(&slice.backing)
    else {
        return AbstractValue::Unknown;
    };
    let Some(slots) = slots.get(slice.start..slice.start.saturating_add(slice.length)) else {
        return AbstractValue::Widened(value.inputs());
    };
    AbstractValue::List(
        slots
            .iter()
            .map(|value| project(value, heap, &mut seen.clone(), depth + 1))
            .collect(),
    )
}

fn project(
    value: &Value,
    heap: &super::HeapVersion,
    seen: &mut BTreeSet<super::ObjectId>,
    depth: usize,
) -> AbstractValue {
    if depth > 32 {
        return AbstractValue::Widened(value.inputs());
    }
    if let Some(path) = value.identity() {
        return AbstractValue::ValuesPath(path.clone());
    }
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(slice),
            origin: _,
        }) => project_string_slice(value, slice, heap, seen, depth),
        ValueNode::Select {
            decision: _,
            yes,
            no,
        } => AbstractValue::Choice(BTreeSet::from([
            project(yes, heap, &mut seen.clone(), depth + 1),
            project(no, heap, &mut seen.clone(), depth + 1),
        ])),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    target: Some(id),
                    view: _,
                },
            origin: _,
        }) if seen.insert(id.clone()) => {
            let Some(super::ObjectVersion {
                data: ObjectData::Map { fields, remainder },
                revision: _,
                multiplicity: _,
            }) = heap.object(id)
            else {
                return AbstractValue::Unknown;
            };
            let entries: BTreeMap<_, _> = fields
                .iter()
                .filter_map(|(key, field)| match field {
                    super::Field::Present(value) => Some((
                        key.clone(),
                        project(value, heap, &mut seen.clone(), depth + 1),
                    )),
                    super::Field::Absent
                    | super::Field::Unknown
                    | super::Field::Select {
                        decision: _,
                        yes: _,
                        no: _,
                    } => None,
                })
                .collect();
            if *remainder == super::Remainder::Closed {
                AbstractValue::Dict(entries)
            } else {
                AbstractValue::Overlay {
                    entries,
                    fallback: Box::new(AbstractValue::Unknown),
                }
            }
        }
        ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(sequence),
            origin: _,
        }) => project_sequence(value, sequence, heap, seen, depth),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Scalar {
                    content: Some(ScalarContent::Literal(GuardValue::String(text))),
                    go_type: _,
                },
            origin: _,
        }) => AbstractValue::StringSet(BTreeSet::from([text.clone()])),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Scalar {
                    go_type: _,
                    content: _,
                }
                | Datum::Map { view: _, target: _ }
                | Datum::Slot {
                    backing: _,
                    index: _,
                }
                | Datum::Unknown(_),
            origin: _,
        }) => AbstractValue::Widened(value.inputs()),
    }
}
fn json_string_slice(
    slice: &super::StringSlice,
    heap: &super::HeapVersion,
    seen: &BTreeSet<super::ObjectId>,
    depth: usize,
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
            .map(|value| json(value, heap, &mut seen.clone(), depth + 1))
            .collect::<Option<Vec<_>>>()?,
    ))
}

fn json(
    value: &Value,
    heap: &super::HeapVersion,
    seen: &mut BTreeSet<super::ObjectId>,
    depth: usize,
) -> Option<serde_json::Value> {
    if depth > 32 {
        return None;
    }
    if let Some(literal) = value.constant() {
        if matches!(literal, GuardValue::Float(_)) {
            return None;
        }
        return serde_json::to_value(literal).ok();
    }
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(slice),
            origin: _,
        }) => json_string_slice(slice, heap, seen, depth),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    target: Some(id),
                    view: _,
                },
            origin: _,
        }) if seen.insert(id.clone()) => {
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
                match field {
                    super::Field::Present(value) => {
                        result.insert(
                            key.clone(),
                            json(value, heap, &mut seen.clone(), depth + 1)?,
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
        ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(sequence),
            origin: _,
        }) => {
            if sequence.storage.is_none() {
                return Some(serde_json::Value::Null);
            }
            Some(serde_json::Value::Array(
                sequence
                    .items()?
                    .iter()
                    .map(|value| json(value, heap, &mut seen.clone(), depth + 1))
                    .collect::<Option<Vec<_>>>()?,
            ))
        }
        ValueNode::Leaf(Leaf {
            datum: Datum::Map {
                target: None,
                view: _,
            },
            origin: _,
        }) => Some(serde_json::Value::Null),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Scalar {
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

impl super::ExecutedText {
    pub(crate) fn coverage_exact(&self) -> bool {
        self.coverage_exact
    }

    pub(crate) fn span(&self) -> helm_schema_syntax::Span {
        self.span
    }
    pub(crate) fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }
    pub(crate) fn condition(&self) -> &Predicate {
        &self.condition
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
        Datum::Scalar {
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
