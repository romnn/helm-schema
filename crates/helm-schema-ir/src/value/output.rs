use super::scalar_value::ScalarContent;
use super::{Datum, Exit, Flow, FrozenValue, Leaf, ObjectData, Value, ValueNode};
use super::{DocumentExecution, ExecutedSite, ExecutedText, TextBuffer, TextPart};
use crate::abstract_value::AbstractValue;
use helm_schema_core::{GuardValue, Predicate};
use std::collections::{BTreeMap, BTreeSet};

impl TextBuffer {
    fn push(&mut self, part: TextPart) {
        if self.term_count().saturating_add(part.term_count()) > 128 {
            tracing::debug!(reason = ?super::BoundReason::OutputTerms, "widen output");
            self.parts = vec![TextPart::Unknown(
                self.inputs().union(&part.inputs()).cloned().collect(),
            )];
        } else {
            self.parts.push(part);
        }
    }

    fn term_count(&self) -> usize {
        self.parts.iter().map(TextPart::term_count).sum()
    }

    fn inputs(&self) -> BTreeSet<helm_schema_core::ValuesPath> {
        self.parts.iter().flat_map(TextPart::inputs).collect()
    }

    fn is_widened(&self) -> bool {
        self.parts.iter().any(|part| match part {
            TextPart::Bytes(_) | TextPart::Frozen(_) => false,
            TextPart::Unknown(_) => true,
            TextPart::Select { decision, yes, no } => {
                decision.coverage == super::Coverage::Partial || yes.is_widened() || no.is_widened()
            }
        })
    }

    pub(super) fn join(decision: std::rc::Rc<super::FrozenTest>, yes: &Self, no: &Self) -> Self {
        let prefix = yes
            .parts
            .iter()
            .zip(&no.parts)
            .take_while(|(yes, no)| yes.same_part(no))
            .count();
        let mut output = Self {
            parts: yes.parts.iter().take(prefix).cloned().collect(),
        };
        let yes = Self {
            parts: yes.parts.iter().skip(prefix).cloned().collect(),
        };
        let no = Self {
            parts: no.parts.iter().skip(prefix).cloned().collect(),
        };
        if !yes.parts.is_empty() || !no.parts.is_empty() {
            output.push(TextPart::Select {
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
        let mut output = String::new();
        for part in &self.parts {
            let text = match part {
                TextPart::Bytes(bytes) => bytes.clone(),
                TextPart::Frozen(value) => value.exact_text()?,
                TextPart::Unknown(_) => return None,
                TextPart::Select {
                    decision: _,
                    yes,
                    no,
                } => {
                    let yes = yes.exact_text()?;
                    let no = no.exact_text()?;
                    if yes != no {
                        return None;
                    }
                    yes
                }
            };
            output.push_str(&text);
        }
        Some(output)
    }

    pub(super) fn stripped_value(&self) -> Value {
        strip_no_value(&self.as_value(), &mut 128).unwrap_or_else(|| unknown_text(self.inputs()))
    }

    pub(super) fn as_value(&self) -> Value {
        if let Some(text) = self.exact_text() {
            return Value::string(&text);
        }
        let mut result = Value::string("");
        for part in &self.parts {
            let value = match part {
                TextPart::Bytes(bytes) => Value::string(bytes),
                TextPart::Frozen(value) => {
                    let mut leaves = 0;
                    frozen_text_value(value, &mut leaves)
                        .unwrap_or_else(|| unknown_text(value.inputs()))
                }
                TextPart::Unknown(inputs) => unknown_text(inputs.clone()),
                TextPart::Select { decision, yes, no } => {
                    Value::select(std::rc::Rc::clone(decision), yes.as_value(), no.as_value())
                }
            };
            let mut leaves = 0;
            let Some(joined) = concatenate_text(&result, &value, &mut leaves) else {
                tracing::debug!(reason = ?super::BoundReason::OutputTerms, "widen scalar output alternatives");
                return unknown_text(self.inputs());
            };
            result = joined;
        }
        result
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
    fn term_count(&self) -> usize {
        match self {
            Self::Bytes(_) | Self::Frozen(_) | Self::Unknown(_) => 1,
            Self::Select {
                decision: _,
                yes,
                no,
            } => 1 + yes.term_count() + no.term_count(),
        }
    }

    fn inputs(&self) -> BTreeSet<helm_schema_core::ValuesPath> {
        match self {
            Self::Bytes(_) => BTreeSet::new(),
            Self::Frozen(value) => value.inputs(),
            Self::Unknown(inputs) => inputs.clone(),
            Self::Select {
                decision: _,
                yes,
                no,
            } => yes.inputs().union(&no.inputs()).cloned().collect(),
        }
    }

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
            rendered: other_rendered,
        } = other;
        program == other_program
            && coverage_exact == other_coverage
            && rendered_unchanged == other_unchanged
            && ordinal == other_ordinal
            && span == other_span
            && source_path == other_source
            && condition == other_condition
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
        if let Some(literal) = self.value.literal() {
            return Some(super::ScalarValueDispatch::constant(literal.clone()));
        }
        self.value
            .identity()
            .map(|path| super::ScalarValueDispatch::identity(path.clone()))
    }

    pub(crate) fn rendered_scalar(&self) -> Option<super::ScalarValueDispatch> {
        if self.rendered && self.rendered_unchanged {
            self.projected_scalar()
        } else {
            None
        }
    }
}
impl super::State {
    pub(super) fn merge_observations(
        &mut self,
        decision: std::rc::Rc<super::FrozenTest>,
        yes: &Self,
        no: &Self,
    ) {
        self.output = TextBuffer::join(decision, &yes.output, &no.output);
        self.output_widened = yes.output_widened || no.output_widened || self.output.is_widened();
        self.sites = yes.sites.clone();
        self.text_sites = yes.text_sites.clone();
        for site in &no.sites {
            if self.sites.iter().any(|existing| existing.same_site(site)) {
                continue;
            }
            if self.sites.len() + self.text_sites.len() >= 128 {
                self.output_widened = true;
                break;
            }
            self.sites.push(site.clone());
        }
        for site in &no.text_sites {
            if self.text_sites.contains(site) {
                continue;
            }
            if self.sites.len() + self.text_sites.len() >= 128 {
                self.output_widened = true;
                break;
            }
            self.text_sites.push(site.clone());
        }
        self.invocations = yes.invocations.clone();
        self.invocation_records_complete =
            yes.invocation_records_complete && no.invocation_records_complete;
        for invocation in &no.invocations {
            if self.invocations.contains(invocation) {
                continue;
            }
            if self.invocations.len() >= 128 {
                self.invocation_records_complete = false;
                break;
            }
            self.invocations.push(invocation.clone());
        }
    }

    pub(super) fn emit_bytes(
        &mut self,
        span: helm_schema_syntax::Span,
        source_path: Option<&str>,
        bytes: &str,
    ) {
        self.output.bytes(bytes);
        if self.output.is_widened() {
            self.output_widened = true;
        }
        if !bytes.is_empty() && self.text_sites.len() + self.sites.len() < 128 {
            self.text_sites.push(ExecutedText {
                program: self.program,
                rendered_unchanged: true,
                coverage_exact: self
                    .active
                    .iter()
                    .all(|edge| edge.test.coverage == super::Coverage::Exact),
                ordinal: self.text_sites.len() + self.sites.len(),
                bytes: bytes.to_owned(),
                span,
                source_path: source_path.map(str::to_owned),
                condition: self.path_predicate(),
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
        if rendered {
            self.output.emit(value.clone());
        }
        if self.output.is_widened() {
            self.output_widened = true;
        }
        if self.sites.len() + self.text_sites.len() >= 128 {
            self.output_widened = true;
            return;
        }
        self.sites.push(ExecutedSite {
            program: self.program,
            rendered_unchanged: true,
            coverage_exact: self
                .active
                .iter()
                .all(|edge| edge.test.coverage == super::Coverage::Exact),
            ordinal: self.text_sites.len() + self.sites.len(),
            span,
            source_path: source_path.map(str::to_owned),
            condition: self.path_predicate(),
            value,
            rendered,
        });
    }
}
impl DocumentExecution {
    pub(super) fn new(exits: Vec<Exit>) -> Self {
        let root_programs = exits.iter().map(|exit| exit.state.program).collect();
        let mut execution = Self {
            exits,
            root_programs,
            observed_facts: crate::observed_facts::ObservedFacts::default(),
        };
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
            .map(|read| crate::fragment_eval::ValueRead {
                values_path: read.path.clone(),
                kind: match read.coverage {
                    super::ReadCoverage::ExactPath => crate::ValueKind::WidenedDependency,
                    super::ReadCoverage::UnknownRegion => crate::ValueKind::WidenedRegion,
                },
                condition: helm_schema_core::GuardDnf::from_conjunction([read.condition.clone()]),
                resource: None,
                provenance: Vec::new(),
                dependency: true,
            })
            .collect()
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
            .map(move |exit| {
                if exit.state.output_widened
                    || exit
                        .state
                        .active
                        .iter()
                        .any(|edge| edge.test.coverage == super::Coverage::Partial)
                {
                    return (exit.state.path_predicate(), None);
                }
                let mut pieces: Vec<(usize, Option<String>)> = Vec::new();
                let mut conditions = BTreeSet::new();
                let mut coverage_exact = true;
                for site in &exit.state.text_sites {
                    if !self.root_programs.contains(&site.program)
                        || site.source_path.as_deref() != source_path
                        || site.span.start >= span.end
                        || span.start >= site.span.end
                    {
                        continue;
                    }
                    coverage_exact &= site.coverage_exact && site.rendered_unchanged;
                    conditions.insert(site.condition.clone());
                    let start = span
                        .start
                        .saturating_sub(site.span.start)
                        .min(site.bytes.len());
                    let end = span
                        .end
                        .saturating_sub(site.span.start)
                        .min(site.bytes.len());
                    pieces.push((site.ordinal, site.bytes.get(start..end).map(str::to_owned)));
                }
                for site in &exit.state.sites {
                    if !self.root_programs.contains(&site.program)
                        || !site.rendered
                        || site.source_path.as_deref() != source_path
                        || site.span.start >= span.end
                        || span.start >= site.span.end
                    {
                        continue;
                    }
                    coverage_exact &= site.coverage_exact && site.rendered_unchanged;
                    conditions.insert(site.condition.clone());
                    pieces.push((site.ordinal, site.value.exact_text()));
                }
                let condition = exit.state.path_predicate();
                // A span query has one result, so it cannot discard a differently guarded text arm.
                if !coverage_exact || conditions.iter().any(|guard| guard != &condition) {
                    return (condition, None);
                }
                pieces.sort_by_key(|(ordinal, _)| *ordinal);
                let text = pieces
                    .into_iter()
                    .try_fold(String::new(), |mut result, (_, piece)| {
                        result.push_str(&piece?);
                        Some(result)
                    });
                (condition, text)
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
        }) => {
            let Some(super::ObjectVersion {
                data: ObjectData::Strings(slots),
                revision: _,
                multiplicity: _,
            }) = heap.object(&slice.backing)
            else {
                return AbstractValue::Unknown;
            };
            let Some(slots) = slots.get(slice.start..slice.start.saturating_add(slice.length))
            else {
                return AbstractValue::Widened(value.inputs());
            };
            AbstractValue::List(
                slots
                    .iter()
                    .map(|value| project(value, heap, &mut seen.clone(), depth + 1))
                    .collect(),
            )
        }
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
        }) => {
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
        }) => {
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
