//! Syntax spelling classes composed from the frozen output graph.

use super::super::text::TextTerm;
use super::super::{
    Coverage, Datum, FrozenValue, HeapVersion, Leaf, TextBuffer, TextPart, Value, ValueNode,
};
use helm_schema_core::{Predicate, PredicateMemo};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PayloadShape {
    Mapping,
    Scalar,
    Sequence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Spelling {
    pub(crate) condition: Predicate,
    pub(crate) bytes: String,
    pub(crate) flow_node: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Spellings {
    pub(crate) known: Vec<Spelling>,
    pub(crate) unknown: Predicate,
}
impl Spellings {
    fn unknown() -> Self {
        Self {
            known: Vec::new(),
            unknown: Predicate::True,
        }
    }
}

pub(crate) struct SpellingTransfer {
    shape: PayloadShape,
    remaining: usize,
    limit: usize,
    depth: usize,
    memo: PredicateMemo,
    values: BTreeMap<(*const HeapVersion, *const ValueNode), Spellings>,
}
impl SpellingTransfer {
    pub(crate) fn new(shape: PayloadShape, limit: usize) -> Self {
        let limit = limit.min(1024);
        Self {
            shape,
            remaining: limit,
            limit,
            depth: 0,
            memo: PredicateMemo::new(),
            values: BTreeMap::new(),
        }
    }
    pub(crate) fn work(&self) -> usize {
        self.limit - self.remaining
    }
    fn charge(&mut self, cost: usize) -> bool {
        if let Some(rest) = self.remaining.checked_sub(cost) {
            self.remaining = rest;
            true
        } else {
            self.remaining = 0;
            false
        }
    }
    #[tracing::instrument(skip_all)]
    pub(crate) fn value(&mut self, value: &FrozenValue) -> Spellings {
        let mut view = self.frozen(value);
        view.known.retain(|arm| {
            if arm.bytes.trim().is_empty() {
                view.unknown = self.memo.normalize(Predicate::Or(vec![
                    view.unknown.clone(),
                    arm.condition.clone(),
                ]));
                false
            } else {
                true
            }
        });
        view
    }
    fn frozen(&mut self, value: &FrozenValue) -> Spellings {
        if let Some(helm_schema_core::GuardValue::String(bytes)) = value.value.constant()
            && bytes.trim().is_empty()
        {
            return if bytes.len() <= 64 * 1024 && self.charge(bytes.len().div_ceil(64) + 1) {
                Self::whitespace(bytes)
            } else {
                Spellings::unknown()
            };
        }
        self.node(&value.value, &value.heap)
    }
    fn node(&mut self, value: &Value, heap: &HeapVersion) -> Spellings {
        if self.depth >= 128 {
            return Spellings::unknown();
        }
        self.depth += 1;
        let result = self.node_inner(value, heap);
        self.depth -= 1;
        result
    }
    fn node_inner(&mut self, value: &Value, heap: &HeapVersion) -> Spellings {
        if !self.charge(1) {
            return Spellings::unknown();
        }
        let key = (std::ptr::from_ref(heap), std::rc::Rc::as_ptr(&value.node));
        if let Some(cached) = self.values.get(&key) {
            let cost = cached.known.iter().fold(1_usize, |cost, arm| {
                cost.saturating_add(1 + arm.bytes.len().div_ceil(64))
            });
            if !self.charge(cost) {
                return Spellings::unknown();
            }
            return self
                .values
                .get(&key)
                .cloned()
                .unwrap_or_else(Spellings::unknown);
        }
        let result = match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => {
                if std::rc::Rc::ptr_eq(&yes.node, &no.node) {
                    return self.node(yes, heap);
                }
                let yes = self.node(yes, heap);
                let no = self.node(no, heap);
                self.choice(decision, yes, no)
            }
            ValueNode::Leaf(Leaf {
                datum: Datum::Text(text),
                origin: _,
            }) => self.text(text.term()),
            ValueNode::Leaf(Leaf { datum: _, origin }) => {
                if let Some(derivation) = &origin.derivation {
                    if derivation.operation == super::super::relationship::ValueOperation::Trim {
                        let mut view = self.frozen(&derivation.source);
                        for arm in &mut view.known {
                            arm.bytes = arm.bytes.trim().to_owned();
                        }
                        view
                    } else {
                        Spellings::unknown()
                    }
                } else {
                    Spellings::unknown()
                }
            }
        };
        if self.charge(result.known.len() + 1) {
            self.values.insert(key, result.clone());
        }
        result
    }
    fn serialized(&self, json: bool) -> Spellings {
        let spelling = if json {
            "{\"00000000\": 00000000}"
        } else {
            match self.shape {
                PayloadShape::Mapping => "00000000: 00000000",
                PayloadShape::Scalar => "00000000",
                PayloadShape::Sequence => "- 00000000",
            }
        };
        Spellings {
            known: vec![Spelling {
                condition: Predicate::True,
                bytes: spelling.to_owned(),
                flow_node: json,
            }],
            unknown: Predicate::False,
        }
    }
    fn text(&mut self, term: &TextTerm) -> Spellings {
        if self.depth >= 128 {
            return Spellings::unknown();
        }
        self.depth += 1;
        let result = self.text_inner(term);
        self.depth -= 1;
        result
    }
    fn text_inner(&mut self, term: &TextTerm) -> Spellings {
        if !self.charge(1) {
            return Spellings::unknown();
        }
        match term {
            TextTerm::Resolved { term, literal: _ } => self.text(term),
            TextTerm::Yaml {
                value: _,
                pretty: _,
            } => self.serialized(false),
            TextTerm::Json {
                value: _,
                pretty: _,
                escape_html: _,
                literal: _,
            } => self.serialized(true),
            TextTerm::Indent {
                value,
                width,
                newline,
            } => {
                let mut view = self.frozen(value);
                if *width > 1024 {
                    return Spellings::unknown();
                }
                for arm in &mut view.known {
                    let size = arm
                        .bytes
                        .len()
                        .saturating_add((arm.bytes.lines().count() + 1).saturating_mul(*width));
                    if size > 64 * 1024 || !self.charge(size.div_ceil(64)) {
                        return Spellings::unknown();
                    }
                    arm.bytes = super::super::text::indent(&arm.bytes, *width, *newline);
                }
                view
            }
            TextTerm::Capture {
                graph,
                strip_marker: _,
            } => self.buffer(graph),
            TextTerm::Tpl { source, result } => {
                let mut view = self.frozen(result);
                let source = self.frozen(source);
                for arm in source.known {
                    if !view.known.contains(&arm) {
                        view.known.push(arm);
                    }
                }
                view.unknown = self
                    .memo
                    .normalize(Predicate::all(vec![view.unknown, source.unknown]));
                view
            }
            TextTerm::Quote { value: _ }
            | TextTerm::Encoded {
                value: _,
                alphabet: _,
            } => Spellings::unknown(),
        }
    }
    fn buffer(&mut self, graph: &TextBuffer) -> Spellings {
        if self.depth >= 128 {
            return Spellings::unknown();
        }
        self.depth += 1;
        let result = self.buffer_inner(graph);
        self.depth -= 1;
        result
    }
    fn buffer_inner(&mut self, graph: &TextBuffer) -> Spellings {
        let mut result = Self::whitespace("");
        for node in &graph.parts {
            if !self.charge(1) {
                return Spellings::unknown();
            }
            let next = match node.content() {
                TextPart::Bytes(bytes) if bytes.trim().is_empty() => {
                    if bytes.len() > 64 * 1024 || !self.charge(bytes.len().div_ceil(64) + 1) {
                        return Spellings::unknown();
                    }
                    Self::whitespace(bytes)
                }
                TextPart::Frozen(value) => self.frozen(value),
                TextPart::Select { decision, yes, no } => {
                    if std::rc::Rc::ptr_eq(yes, no) {
                        self.buffer(yes)
                    } else {
                        let yes = self.buffer(yes);
                        let no = self.buffer(no);
                        self.choice(decision, yes, no)
                    }
                }
                TextPart::Bytes(_) | TextPart::Unknown(_) => return Spellings::unknown(),
            };
            result = self.concatenate(result, &next);
        }
        result
    }

    fn whitespace(bytes: &str) -> Spellings {
        Spellings {
            known: vec![Spelling {
                condition: Predicate::True,
                bytes: bytes.to_owned(),
                flow_node: false,
            }],
            unknown: Predicate::False,
        }
    }

    fn concatenate(&mut self, left: Spellings, right: &Spellings) -> Spellings {
        let mut result = Spellings {
            known: Vec::new(),
            unknown: self
                .memo
                .normalize(Predicate::Or(vec![left.unknown, right.unknown.clone()])),
        };
        for prefix in left.known {
            for suffix in &right.known {
                if !self.charge(1) || result.known.len() >= 32 {
                    result.unknown = Predicate::True;
                    return result;
                }
                let condition = self.memo.normalize(Predicate::all(vec![
                    prefix.condition.clone(),
                    suffix.condition.clone(),
                ]));
                if condition == Predicate::False {
                    continue;
                }
                let prefix_payload = !prefix.bytes.trim().is_empty();
                let suffix_payload = !suffix.bytes.trim().is_empty();
                let size = prefix.bytes.len().saturating_add(suffix.bytes.len());
                if prefix_payload && suffix_payload
                    || size > 64 * 1024
                    || !self.charge(size.div_ceil(64) + 1)
                {
                    result.unknown = self
                        .memo
                        .normalize(Predicate::Or(vec![result.unknown, condition]));
                    continue;
                }
                let bytes = format!("{}{}", prefix.bytes, suffix.bytes);
                let flow_node = if prefix_payload {
                    prefix.flow_node
                } else {
                    suffix.flow_node
                };
                if let Some(known) = result
                    .known
                    .iter_mut()
                    .find(|known| known.bytes == bytes && known.flow_node == flow_node)
                {
                    known.condition = self
                        .memo
                        .normalize(Predicate::Or(vec![known.condition.clone(), condition]));
                } else {
                    result.known.push(Spelling {
                        condition,
                        bytes,
                        flow_node,
                    });
                }
            }
        }
        result
    }
    fn choice(
        &mut self,
        decision: &super::super::FrozenTest,
        yes: Spellings,
        no: Spellings,
    ) -> Spellings {
        if !self.charge(1) {
            return Spellings::unknown();
        }
        if decision.coverage != Coverage::Exact
            || decision.when_true.contains_approximation()
            || decision.when_false.contains_approximation()
        {
            let mut common = Spellings::unknown();
            for mut arm in yes.known {
                if let Some(other) = no
                    .known
                    .iter()
                    .find(|other| other.bytes == arm.bytes && other.flow_node == arm.flow_node)
                {
                    arm.condition = self
                        .memo
                        .normalize(Predicate::all(vec![arm.condition, other.condition.clone()]));
                    common.known.push(arm);
                }
            }
            return common;
        }
        let mut view = Spellings {
            known: Vec::new(),
            unknown: Predicate::False,
        };
        for (branch, guard) in [(yes, &decision.when_true), (no, &decision.when_false)] {
            view.unknown = self.memo.normalize(Predicate::Or(vec![
                view.unknown,
                Predicate::all(vec![guard.clone(), branch.unknown]),
            ]));
            for mut arm in branch.known {
                if !self.charge(1) {
                    view.unknown = Predicate::True;
                    return view;
                }
                arm.condition = self
                    .memo
                    .normalize(Predicate::all(vec![guard.clone(), arm.condition]));
                if arm.condition != Predicate::False {
                    view.known.push(arm);
                }
            }
        }
        view
    }
}
