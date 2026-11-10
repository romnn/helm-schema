//! Bounded text queries over canonical values and their frozen transforms.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use helm_schema_core::{Guard, Predicate, PredicateMemo, ValuePattern};

use super::relationship::{TextFormat, TextTransform, ValueOperation};
use super::scalar_value::{ScalarContent, TruthCondition};
use super::text::TextTerm;
use super::text_prefix::Prefix;
use super::{Coverage, Datum, FrozenValue, Leaf, State, TextBuffer, TextPart, Value, ValueNode};

#[derive(Clone)]
struct TextShape {
    opener: TruthCondition,
    brace: TruthCondition,
    starts: TruthCondition,
    ends: TruthCondition,
    empty: TruthCondition,
    prefix: Option<Prefix>,
}

impl TextShape {
    fn literal(text: &str) -> Self {
        Self {
            opener: truth(text.contains("{{")),
            brace: truth(text.contains('{')),
            starts: truth(text.starts_with('{')),
            ends: truth(text.ends_with('{')),
            empty: truth(text.is_empty()),
            prefix: None,
        }
    }

    fn unknown() -> Self {
        Self {
            opener: TruthCondition::Unknown,
            brace: TruthCondition::Unknown,
            starts: TruthCondition::Unknown,
            ends: TruthCondition::Unknown,
            empty: TruthCondition::Unknown,
            prefix: None,
        }
    }
}

fn truth(value: bool) -> TruthCondition {
    TruthCondition::Exact(if value {
        Predicate::True
    } else {
        Predicate::False
    })
}

pub(super) fn contains(value: &Value, needle: &str, state: &State) -> Value {
    contains_with(value, needle, state, true).unwrap_or_else(Value::unknown)
}

/// `contains`; with `undecided` unset, `None` when no structural fact decides any input.
pub(super) fn contains_with(
    value: &Value,
    needle: &str,
    state: &State,
    undecided: bool,
) -> Option<Value> {
    let inputs = state.heap.arena.input_objects.borrow();
    let mut query = Query {
        inputs: &inputs,
        memo: &state.heap.arena.predicate_memo,
        remaining: 512,
        prefix_only: false,
    };
    let frozen = state.heap.freeze(value);
    let condition = if needle == "{{" {
        query.string_input(&frozen, 0).opener
    } else {
        query.substring(&frozen, needle, 0)
    };
    if condition == TruthCondition::Unknown && !undecided {
        return None;
    }
    let decision = condition_test(&condition, state);
    Some(Value::select(
        decision,
        Value::literal(helm_schema_core::GuardValue::Bool(true)),
        Value::literal(helm_schema_core::GuardValue::Bool(false)),
    ))
}

pub(super) fn truth_test(value: &Value, state: &State) -> Option<Rc<super::FrozenTest>> {
    let inputs = state.heap.arena.input_objects.borrow();
    let mut query = Query {
        inputs: &inputs,
        memo: &state.heap.arena.predicate_memo,
        remaining: 512,
        prefix_only: false,
    };
    let frozen = FrozenValue::at(Rc::clone(&state.heap.version), value.clone());
    let condition = query.string(&frozen, 0).empty;
    if condition == TruthCondition::Unknown {
        return None;
    }
    Some(condition_test(
        &condition.negated_with_memo(&state.heap.arena.predicate_memo),
        state,
    ))
}

fn condition_test(condition: &TruthCondition, state: &State) -> Rc<super::FrozenTest> {
    let when_true = condition.when_true();
    let when_false = condition.when_false_with_memo(&state.heap.arena.predicate_memo);
    let coverage = if condition.predicate().is_some() {
        Coverage::Exact
    } else if *condition == TruthCondition::Unknown
        || when_true.contains_approximation()
        || when_false.contains_approximation()
    {
        Coverage::Partial
    } else {
        Coverage::CertifiedPartial
    };
    state.heap.arena.decision(when_true, when_false, coverage)
}

pub(super) fn action_free(
    value: &FrozenValue,
    inputs: &std::collections::BTreeMap<helm_schema_core::ValuesPath, super::InputBacking>,
    memo: &helm_schema_core::PredicateMemo,
    remaining: &mut usize,
) -> Predicate {
    let mut query = Query {
        inputs,
        memo,
        remaining: (*remaining).min(512),
        prefix_only: false,
    };
    let initial = query.remaining;
    let condition = query.string(value, 0).opener;
    *remaining = remaining.saturating_sub(initial - query.remaining);
    condition.when_false_with_memo(memo)
}

pub(super) fn prefix(
    value: &FrozenValue,
    transforms: &[TextTransform],
    remaining: &mut usize,
) -> String {
    let mut query = Query {
        inputs: &BTreeMap::default(),
        memo: &PredicateMemo::default(),
        remaining: *remaining,
        prefix_only: true,
    };
    let mut prefix = query.string(value, 0).prefix;
    for transform in transforms {
        prefix = prefix.map(|prefix| prefix.edited(transform));
    }
    *remaining = query.remaining;
    prefix.map_or_else(String::new, |prefix| prefix.text)
}

struct Query<'a> {
    inputs: &'a std::collections::BTreeMap<helm_schema_core::ValuesPath, super::InputBacking>,
    memo: &'a helm_schema_core::PredicateMemo,
    remaining: usize,
    prefix_only: bool,
}

impl Query<'_> {
    fn literal(&self, text: &str) -> TextShape {
        let mut shape = TextShape::literal(text);
        if self.prefix_only {
            shape.prefix = Prefix::literal(text);
        }
        shape
    }

    fn substring(&mut self, value: &FrozenValue, needle: &str, depth: usize) -> TruthCondition {
        if !self.charge(depth) {
            return TruthCondition::Unknown;
        }
        if let Some(helm_schema_core::GuardValue::String(text)) = value.literal() {
            return truth(text.contains(needle));
        }
        if let ValueNode::Select { decision, yes, no } = value.value.node.as_ref() {
            let yes = self.substring(
                &FrozenValue::at(Rc::clone(&value.heap), yes.clone()),
                needle,
                depth + 1,
            );
            let no = self.substring(
                &FrozenValue::at(Rc::clone(&value.heap), no.clone()),
                needle,
                depth + 1,
            );
            return self.choose(decision, yes, no);
        }
        if let Some(path) = value.identity() {
            return TruthCondition::Exact(Predicate::from(Guard::MatchesPattern {
                path: path.clone(),
                pattern: helm_schema_core::escape_regex_literal(needle),
                templated: false,
            }));
        }
        if let ValueNode::Leaf(Leaf {
            datum: Datum::Text(text),
            origin: _,
        }) = value.value.node.as_ref()
            && let TextTerm::Quote { value: source } = text.term()
            && self.original_input(source)
            && let Some(path) = source.identity()
        {
            // Every falsy JSON value has one of these quoted spellings; -0 preserves its sign.
            let spellings = [
                "",
                "\"\"",
                "\"0\"",
                "\"-0\"",
                "\"false\"",
                "\"[]\"",
                "\"map[]\"",
            ];
            let all_contain = spellings.iter().all(|text| text.contains(needle));
            let none_contain = spellings.iter().all(|text| !text.contains(needle));
            let falsy = Predicate::truthy_path(path.encode()).negated();
            return TruthCondition::from_subsets_with_memo(
                if all_contain {
                    falsy.clone()
                } else {
                    Predicate::False
                },
                if none_contain {
                    falsy
                } else {
                    Predicate::False
                },
                false,
                self.memo,
            );
        }
        TruthCondition::Unknown
    }

    fn charge(&mut self, depth: usize) -> bool {
        if !self.prefix_only && depth >= 64 || self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }

    fn all(&self, conditions: impl IntoIterator<Item = TruthCondition>) -> TruthCondition {
        TruthCondition::all_with_memo(conditions, self.memo)
    }

    fn any(&self, conditions: impl IntoIterator<Item = TruthCondition>) -> TruthCondition {
        TruthCondition::any_with_memo(conditions, self.memo)
    }

    fn choose(
        &self,
        decision: &super::FrozenTest,
        yes: TruthCondition,
        no: TruthCondition,
    ) -> TruthCondition {
        if yes == no {
            return yes;
        }
        let selected = match decision.coverage {
            // Opaque raw choices do not certify their fields; runtime projections do.
            Coverage::Partial if decision.runtime.is_none() => TruthCondition::Unknown,
            Coverage::Exact | Coverage::Partial | Coverage::CertifiedPartial => {
                TruthCondition::from_subsets_with_memo(
                    decision.when_true.clone(),
                    decision.when_false.clone(),
                    decision.coverage == Coverage::Exact,
                    self.memo,
                )
            }
        };
        self.any([
            self.all([selected.clone(), yes]),
            self.all([selected.negated_with_memo(self.memo), no]),
        ])
    }

    fn select(&self, decision: &super::FrozenTest, yes: TextShape, no: TextShape) -> TextShape {
        let TextShape {
            opener: yes_opener,
            brace: yes_brace,
            starts: yes_starts,
            ends: yes_ends,
            empty: yes_empty,
            prefix: _,
        } = yes;
        let TextShape {
            opener: no_opener,
            brace: no_brace,
            starts: no_starts,
            ends: no_ends,
            empty: no_empty,
            prefix: _,
        } = no;
        TextShape {
            opener: self.choose(decision, yes_opener, no_opener),
            brace: self.choose(decision, yes_brace, no_brace),
            starts: self.choose(decision, yes_starts, no_starts),
            ends: self.choose(decision, yes_ends, no_ends),
            empty: self.choose(decision, yes_empty, no_empty),
            prefix: None,
        }
    }

    fn concatenate(&self, left: TextShape, right: TextShape) -> TextShape {
        let TextShape {
            opener: left_opener,
            brace: left_brace,
            starts: left_starts,
            ends: left_ends,
            empty: left_empty,
            prefix: left_prefix,
        } = left;
        let TextShape {
            opener: right_opener,
            brace: right_brace,
            starts: right_starts,
            ends: right_ends,
            empty: right_empty,
            prefix: right_prefix,
        } = right;
        TextShape {
            opener: self.any([
                left_opener,
                right_opener,
                self.all([left_ends.clone(), right_starts.clone()]),
            ]),
            brace: self.any([left_brace, right_brace]),
            starts: self.any([left_starts, self.all([left_empty.clone(), right_starts])]),
            ends: self.any([right_ends, self.all([right_empty.clone(), left_ends])]),
            empty: self.all([left_empty, right_empty]),
            prefix: left_prefix.map(|prefix| prefix.concatenate(right_prefix)),
        }
    }

    fn string(&mut self, frozen: &FrozenValue, depth: usize) -> TextShape {
        if !self.charge(depth) {
            return TextShape::unknown();
        }
        self.string_node(frozen, depth)
    }

    fn string_node(&mut self, frozen: &FrozenValue, depth: usize) -> TextShape {
        let FrozenValue { value, heap } = frozen;
        if let Some(helm_schema_core::GuardValue::String(text)) = value.constant() {
            return self.literal(text);
        }
        if let ValueNode::Select { decision, yes, no } = value.node.as_ref() {
            if self.prefix_only {
                return TextShape::unknown();
            }
            let yes = self.string(&FrozenValue::at(Rc::clone(heap), yes.clone()), depth + 1);
            let no = self.string(&FrozenValue::at(Rc::clone(heap), no.clone()), depth + 1);
            return self.select(decision, yes, no);
        }
        if let Some(derivation) = value.operation_preimage()
            && let Some(shape) = self.derived_string(derivation, depth)
        {
            return shape;
        }
        let ValueNode::Leaf(Leaf { datum, origin: _ }) = value.node.as_ref() else {
            return TextShape::unknown();
        };
        if self.prefix_only && !matches!(datum, Datum::Text(_)) {
            return TextShape::unknown();
        }
        match datum {
            Datum::Text(text) => self.term(text.term(), depth + 1),
            Datum::Scalar {
                go_type: _,
                content:
                    Some(ScalarContent::StringDomain {
                        pattern,
                        min_bytes,
                        max_bytes,
                    }),
            } => {
                let mut shape = if excludes_brace(pattern) {
                    TextShape::literal("host")
                } else {
                    TextShape::unknown()
                };
                shape.empty = if *min_bytes > 0 {
                    truth(false)
                } else if *max_bytes == 0 {
                    truth(true)
                } else {
                    TruthCondition::Unknown
                };
                shape
            }
            Datum::Scalar {
                go_type,
                content: _,
            }
            | Datum::Unknown(super::UnknownValue {
                types: super::TypeCoverage::Scalar(go_type),
                aliases: _,
            }) if *go_type != super::ScalarType::String => {
                if *go_type == super::ScalarType::Nil {
                    TextShape::literal("<no value>")
                } else {
                    TextShape::literal("number")
                }
            }
            Datum::Scalar {
                go_type: _,
                content: _,
            }
            | Datum::Unknown(_)
            | Datum::Map { view: _, target: _ } => {
                if frozen.go_type() == super::kinds::GoType::String {
                    value
                        .identity()
                        .map_or_else(TextShape::unknown, input_shape)
                } else {
                    TextShape::unknown()
                }
            }

            Datum::Sequence(_)
            | Datum::Strings(_)
            | Datum::Slot {
                backing: _,
                index: _,
            } => TextShape::unknown(),
        }
    }

    fn derived_string(
        &mut self,
        derivation: &super::relationship::Derivation,
        depth: usize,
    ) -> Option<TextShape> {
        match &derivation.operation {
            ValueOperation::Text(transform) => {
                let source = self.string_input(&derivation.source, depth + 1);
                let mut result = self.transform(source, transform);
                if !self.prefix_only
                    && let TextTransform::Truncate(length) = transform
                    && length.unsigned_abs() >= 2
                    && let Some(path) = derivation.source.identity()
                {
                    let width = length.unsigned_abs().saturating_sub(2).min(4096);
                    let pattern = if *length > 0 {
                        format!(r"^[\x00-\x7f]{{0,{width}}}\{{\{{")
                    } else {
                        format!(r"\{{\{{[\x00-\x7f]{{0,{width}}}(?![\s\S])")
                    };
                    let when_true = Predicate::from(Guard::MatchesPattern {
                        path: path.clone(),
                        pattern,
                        templated: false,
                    });
                    // An ASCII prefix/suffix gives an exact byte bound; other strings retain
                    // the independently proved false subset instead of a guessed complement.
                    result.opener = if width == 0 {
                        TruthCondition::Exact(when_true)
                    } else {
                        TruthCondition::from_subsets_with_memo(
                            when_true,
                            result.opener.when_false_with_memo(self.memo),
                            false,
                            self.memo,
                        )
                    };
                }
                return Some(result);
            }
            ValueOperation::Trim => {
                let mut source = self.string_input(&derivation.source, depth + 1);
                source.prefix = source.prefix.map(Prefix::trim);
                source.starts = self.only_false(&source.brace);
                source.ends = source.starts.clone();
                source.empty = TruthCondition::Unknown;
                return Some(source);
            }
            ValueOperation::Format(TextFormat::Value | TextFormat::String) => {
                return Some(self.formatted(&derivation.source, depth + 1));
            }
            ValueOperation::Numeric(_) | ValueOperation::Descriptor { kind_only: _ } => {
                return Some(TextShape::literal("number"));
            }
            ValueOperation::Copy
            | ValueOperation::Capability {
                guard: _,
                decision: _,
            }
            | ValueOperation::Decode(_)
            | ValueOperation::Member(_)
            | ValueOperation::Omit(_)
            | ValueOperation::Pick(_)
            | ValueOperation::Merge { overwrite: _ } => {}
        }
        None
    }

    fn formatted(&mut self, source: &FrozenValue, depth: usize) -> TextShape {
        if self.prefix_only {
            return self.string(source, depth);
        }
        let mut shape = self.string(source, depth);
        if !self.prefix_only && shape.opener == TruthCondition::Unknown {
            shape.opener = self.json(source, depth, &mut BTreeSet::new());
        }
        match source.go_type() {
            super::kinds::GoType::Map
            | super::kinds::GoType::Slice
            | super::kinds::GoType::Bool
            | super::kinds::GoType::Nil
            | super::kinds::GoType::Int
            | super::kinds::GoType::Int64
            | super::kinds::GoType::Uint64
            | super::kinds::GoType::Float64 => shape.empty = truth(false),
            super::kinds::GoType::String => {}
            super::kinds::GoType::Unknown => {
                if self.original_input(source)
                    && let Some(path) = source.identity()
                {
                    shape.empty = TruthCondition::Exact(Predicate::from(Guard::Eq {
                        path: path.clone(),
                        value: helm_schema_core::GuardValue::String(String::new()),
                    }));
                }
            }
        }
        shape
    }

    fn string_input(&mut self, frozen: &FrozenValue, depth: usize) -> TextShape {
        if self.prefix_only {
            return self.string(frozen, depth);
        }
        if let Some(path) = frozen.value.identity() {
            if !self.charge(depth) {
                return TextShape::unknown();
            }
            return input_shape(path);
        }
        self.string(frozen, depth)
    }

    fn only_false(&self, condition: &TruthCondition) -> TruthCondition {
        TruthCondition::from_subsets_with_memo(
            Predicate::False,
            condition.when_false_with_memo(self.memo),
            false,
            self.memo,
        )
    }

    fn transform(&self, mut source: TextShape, transform: &TextTransform) -> TextShape {
        if self.prefix_only {
            return TextShape {
                prefix: source.prefix.map(|prefix| prefix.edited(transform)),
                ..TextShape::unknown()
            };
        }
        match transform {
            TextTransform::Replace { from, to }
                if from.is_empty() && !to.is_empty() && !to.contains('{') =>
            {
                source.opener = truth(false);
                source.starts = truth(false);
                source.ends = truth(false);
                source.empty = truth(false);
                source
            }
            TextTransform::Replace { from, to } if from == to => source,
            TextTransform::Replace { from, to }
                if !from.contains('{') && !to.contains('{') && !to.is_empty() =>
            {
                source
            }
            TextTransform::Replace { from, to } if !from.contains('{') && to.is_empty() => {
                source.opener = TruthCondition::from_subsets_with_memo(
                    source.opener.when_true(),
                    source.brace.when_false_with_memo(self.memo),
                    false,
                    self.memo,
                );
                source.starts = self.only_false(&source.brace);
                source.ends = source.starts.clone();
                source.empty = TruthCondition::Unknown;
                source
            }
            TextTransform::Replace { from: _, to } if !to.contains('{') => {
                source.opener = self.only_false(&source.brace);
                source.brace = source.opener.clone();
                source.starts = source.opener.clone();
                source.ends = source.opener.clone();
                source.empty = TruthCondition::Unknown;
                source
            }
            TextTransform::Replace { from: _, to: _ }
            | TextTransform::RegexReplace {
                pattern: _,
                replacement: _,
            } => TextShape::unknown(),
            TextTransform::Upper | TextTransform::Lower => source,
            TextTransform::TrimPrefix(prefix) if !prefix.contains('{') => {
                source.starts = self.only_false(&source.brace);
                source.empty = TruthCondition::Unknown;
                source
            }
            TextTransform::TrimSuffix(suffix) if !suffix.contains('{') => {
                source.ends = self.only_false(&source.brace);
                source.empty = TruthCondition::Unknown;
                source
            }
            TextTransform::Truncate(length) if length.unsigned_abs() < 2 => {
                source.opener = truth(false);
                if *length == 0 {
                    return TextShape::literal("");
                }
                let edge = if *length > 0 {
                    source.starts
                } else {
                    source.ends
                };
                source.brace = edge.clone();
                source.starts = edge.clone();
                source.ends = edge;
                source
            }
            TextTransform::Truncate(_)
            | TextTransform::TrimPrefix(_)
            | TextTransform::TrimSuffix(_)
            | TextTransform::Cut(_) => {
                // Taking a contiguous substring can destroy an opener, but cannot create one.
                // In particular, byte truncation may split UTF-8 or cut between the two braces.
                // Keep only the proven false domain instead of calling its complement exact.
                source.opener = self.only_false(&source.opener);
                source.brace = self.only_false(&source.brace);
                source.starts = source.brace.clone();
                source.ends = source.brace.clone();
                source.empty = TruthCondition::Unknown;
                source
            }
        }
    }

    fn term(&mut self, term: &TextTerm, depth: usize) -> TextShape {
        if !self.prefix_only && !self.charge(depth) {
            return TextShape::unknown();
        }
        match term {
            TextTerm::Resolved { term: _, literal } => match literal {
                helm_schema_core::GuardValue::String(text) => self.literal(text),
                _ => TextShape::unknown(),
            },
            TextTerm::Encoded {
                value: _,
                alphabet: _,
            } => {
                if self.prefix_only {
                    return TextShape::unknown();
                }
                let mut shape = TextShape::literal("encoded");
                shape.empty = TruthCondition::Unknown;
                shape
            }
            TextTerm::Json {
                value,
                pretty: _,
                escape_html: _,
                literal: _,
            } => self.json_text(value, depth),

            TextTerm::Quote { value } => self.quoted(value, depth),

            TextTerm::Indent {
                value,
                width,
                newline,
            } => {
                if self.prefix_only {
                    return TextShape::unknown();
                }
                let mut shape = self.string(value, depth + 1);
                if *width > 0 || *newline {
                    shape.starts = truth(false);
                    shape.empty = truth(false);
                }
                shape
            }
            TextTerm::Capture {
                graph,
                strip_marker,
            } => {
                if !self.prefix_only
                    && let Some(text) = graph.exact_text()
                {
                    return self.literal(&if *strip_marker {
                        text.replace("<no value>", "")
                    } else {
                        text
                    });
                }
                let mut shape = self.buffer(graph, depth + 1, *strip_marker);
                if self.prefix_only {
                    if *strip_marker {
                        shape.prefix = shape.prefix.map(Prefix::strip_marker);
                    }
                    return shape;
                }
                if *strip_marker {
                    // Marker deletion can join braces that were separated in the input.
                    shape.opener = TruthCondition::from_subsets_with_memo(
                        shape.opener.when_true(),
                        shape.brace.when_false_with_memo(self.memo),
                        false,
                        self.memo,
                    );
                    shape.starts = self.only_false(&shape.brace);
                    shape.ends = shape.starts.clone();
                    shape.empty = TruthCondition::Unknown;
                }
                shape
            }
            TextTerm::Tpl { source: _, result } => self.string(result, depth + 1),
            TextTerm::Yaml {
                value: _,
                pretty: _,
            } => TextShape::unknown(),
        }
    }

    fn quoted(&mut self, value: &FrozenValue, depth: usize) -> TextShape {
        if self.prefix_only {
            return TextShape::unknown();
        }
        let mut shape = TextShape::literal("quoted");
        let source = self.string(value, depth + 1);
        shape.opener = source.opener;
        shape.brace = source.brace;
        shape.empty = super::text::quote_nil_condition(value)
            .map_or(TruthCondition::Unknown, TruthCondition::Exact);
        if self.original_input(value) {
            // Go's scalar/collection formatting inserts separators between nested strings.
            // Quoting preserves braces; a nil interface is the only operand quote omits.
            shape.opener = self.json(value, depth + 1, &mut BTreeSet::new());
        }
        shape
    }

    fn original_input(&mut self, frozen: &FrozenValue) -> bool {
        let Some(path) = frozen.identity() else {
            return false;
        };
        let inputs = self.inputs;
        let mut query = super::relationship::RelationshipQuery::new(inputs, self.remaining);
        let unchanged = query.original_unchanged(path, &frozen.heap);
        self.remaining = self.remaining.saturating_sub(query.work());
        unchanged
    }

    fn json_text(&mut self, value: &FrozenValue, depth: usize) -> TextShape {
        if self.prefix_only {
            return TextShape::unknown();
        }
        let mut shape = TextShape::literal("json");
        shape.opener = self.json(value, depth + 1, &mut BTreeSet::new());
        let inputs = self.inputs;
        let mut encoding = super::relationship::RelationshipQuery::new(inputs, self.remaining);
        let encodable = encoding.serializable_value(&value.value, &value.heap);
        self.remaining = self.remaining.saturating_sub(encoding.work());
        if !encodable {
            shape.opener = self.only_false(&shape.opener);
            shape.empty = TruthCondition::Unknown;
        }
        shape.brace = TruthCondition::Unknown;
        shape.starts = match value.go_type() {
            super::kinds::GoType::Map => truth(true),
            super::kinds::GoType::Unknown => {
                value.identity().map_or(TruthCondition::Unknown, |path| {
                    TruthCondition::Exact(Predicate::from(Guard::TypeIs {
                        path: path.clone(),
                        schema_type: "object".to_owned(),
                    }))
                })
            }
            super::kinds::GoType::Nil
            | super::kinds::GoType::String
            | super::kinds::GoType::Bool
            | super::kinds::GoType::Slice
            | super::kinds::GoType::Int
            | super::kinds::GoType::Int64
            | super::kinds::GoType::Uint64
            | super::kinds::GoType::Float64 => truth(false),
        };
        if !encodable {
            shape.starts = self.only_false(&shape.starts);
        }
        shape
    }

    fn buffer(&mut self, graph: &TextBuffer, depth: usize, strip_marker: bool) -> TextShape {
        if !self.prefix_only && !self.charge(depth) {
            return TextShape::unknown();
        }
        let mut shape = self.literal("");
        for node in &graph.parts {
            if !self.charge(depth) {
                if self.prefix_only {
                    shape.prefix = shape.prefix.map(|prefix| prefix.concatenate(None));
                    return shape;
                }
                return TextShape::unknown();
            }
            let next = match node.content() {
                TextPart::Bytes(text) => self.literal(&if strip_marker && !self.prefix_only {
                    text.replace("<no value>", "")
                } else {
                    text.clone()
                }),
                TextPart::Frozen(value) => self.string(value, depth + 1),
                TextPart::Select { decision, yes, no } => {
                    if self.prefix_only {
                        return self.concatenate(shape, TextShape::unknown());
                    }
                    let yes = self.buffer(yes, depth + 1, strip_marker);
                    let no = self.buffer(no, depth + 1, strip_marker);
                    self.select(decision, yes, no)
                }
                TextPart::Unknown(_) => TextShape::unknown(),
            };
            shape = self.concatenate(shape, next);
            if self.prefix_only && shape.prefix.as_ref().is_none_or(|prefix| !prefix.complete) {
                break;
            }
        }
        shape
    }

    fn json(
        &mut self,
        frozen: &FrozenValue,
        depth: usize,
        active: &mut BTreeSet<super::ObjectId>,
    ) -> TruthCondition {
        super::json_query::value(
            &frozen.value,
            &frozen.heap,
            depth,
            active,
            &mut ActionContents {
                query: self,
                heap: &frozen.heap,
            },
        )
    }

    fn structural_json(&mut self, frozen: &FrozenValue) -> TruthCondition {
        let inputs = self.inputs;
        let mut query = super::relationship::RelationshipQuery::new(inputs, self.remaining);
        let view = query.value(frozen, &[]);
        self.remaining = self.remaining.saturating_sub(query.work());
        if view.unknown != Predicate::False {
            return TruthCondition::Unknown;
        }
        let [payload] = view.known.as_slice() else {
            return TruthCondition::Unknown;
        };
        if payload.condition != Predicate::True
            || !payload.output_path.is_empty()
            || payload.capture.is_some()
            || !payload.omitted_members.is_empty()
            || !super::relationship::structural_preimage(&payload.preimage)
        {
            return TruthCondition::Unknown;
        }
        TruthCondition::Exact(
            Predicate::Or(vec![
                Predicate::from(Guard::Absent {
                    path: payload.source.as_ref().clone(),
                }),
                Predicate::from(Guard::MatchesValue {
                    path: payload.source.as_ref().clone(),
                    pattern: ValuePattern::TemplateActionFree,
                }),
            ])
            .negated(),
        )
    }
}

struct ActionContents<'query, 'inputs, 'heap> {
    query: &'query mut Query<'inputs>,
    heap: &'heap Rc<super::HeapVersion>,
}

impl super::json_query::Fact for ActionContents<'_, '_, '_> {
    type Output = TruthCondition;
    fn charge(&mut self, depth: usize) -> bool {
        self.query.charge(depth)
    }
    fn unknown(&self) -> TruthCondition {
        TruthCondition::Unknown
    }
    fn key(&self, key: &str) -> TruthCondition {
        truth(key.contains("{{"))
    }
    fn join(&self, values: Vec<TruthCondition>) -> TruthCondition {
        self.query.any(values)
    }
    fn select(
        &self,
        decision: &super::FrozenTest,
        yes: TruthCondition,
        no: TruthCondition,
    ) -> TruthCondition {
        self.query.choose(decision, yes, no)
    }
    fn decisive(&self, value: &TruthCondition) -> bool {
        *value == truth(true)
    }
    fn examine(
        &mut self,
        value: &Value,
        _heap: &super::HeapVersion,
        depth: usize,
    ) -> Option<TruthCondition> {
        let ValueNode::Leaf(Leaf { datum, origin: _ }) = value.node.as_ref() else {
            return None;
        };
        let frozen = FrozenValue::at(Rc::clone(self.heap), value.clone());
        if self.query.original_input(&frozen)
            && let Some(path) = value.identity()
        {
            return Some(TruthCondition::Exact(
                Predicate::Or(vec![
                    Predicate::from(Guard::Absent { path: path.clone() }),
                    Predicate::from(Guard::MatchesValue {
                        path: path.clone(),
                        pattern: ValuePattern::TemplateActionFree,
                    }),
                ])
                .negated(),
            ));
        }
        match datum {
            Datum::Text(_)
            | Datum::Scalar {
                go_type: _,
                content: _,
            }
            | Datum::Unknown(super::UnknownValue {
                types: super::TypeCoverage::Scalar(_),
                aliases: _,
            }) => Some(self.query.string(&frozen, depth + 1).opener),
            Datum::Map {
                view: _,
                target: None,
            } => Some(truth(false)),
            Datum::Sequence(sequence)
                if sequence
                    .storage
                    .as_ref()
                    .is_some_and(|storage| storage.is_bytes()) =>
            {
                Some(truth(false))
            }
            Datum::Slot {
                backing: _,
                index: _,
            }
            | Datum::Unknown(_) => Some(self.query.structural_json(&frozen)),
            Datum::Map {
                view: _,
                target: Some(_),
            }
            | Datum::Sequence(_)
            | Datum::Strings(_) => None,
        }
    }
}

fn excludes_brace(pattern: &str) -> bool {
    use regex_syntax::hir::{Class, Hir, HirKind};
    fn contains(hir: &Hir) -> bool {
        match hir.kind() {
            HirKind::Empty | HirKind::Look(_) => false,
            HirKind::Literal(literal) => literal.0.contains(&b'{'),
            HirKind::Class(Class::Unicode(class)) => class
                .iter()
                .any(|range| range.start() <= '{' && range.end() >= '{'),
            HirKind::Class(Class::Bytes(class)) => class
                .iter()
                .any(|range| range.start() <= b'{' && range.end() >= b'{'),
            HirKind::Repetition(repetition) => contains(&repetition.sub),
            HirKind::Capture(capture) => contains(&capture.sub),
            HirKind::Concat(parts) | HirKind::Alternation(parts) => parts.iter().any(contains),
        }
    }
    regex_syntax::Parser::new()
        .parse(pattern)
        .is_ok_and(|hir| !contains(&hir))
}

fn input_shape(path: &helm_schema_core::ValuesPath) -> TextShape {
    let matches = |pattern: &str| {
        TruthCondition::Exact(Predicate::from(Guard::MatchesPattern {
            path: path.clone(),
            pattern: pattern.to_owned(),
            templated: false,
        }))
    };
    TextShape {
        opener: matches(r"\{\{"),
        brace: matches(r"\{"),
        starts: matches(r"^\{"),
        ends: matches(r"\{(?![\s\S])"),
        empty: matches(r"^(?![\s\S])"),
        prefix: None,
    }
}
