//! Borrowed input relationships over frozen values, captures and heap members.
//!
//! Work is shared by every branch and member of a request.
//! Exhaustion preserves unknown coverage and never certifies a truncated result.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use helm_schema_core::{Guard, GuardValue, Predicate, PredicateMemo, ValuesPath};

use super::scalar_value::ScalarContent;

use super::{
    Coverage, Datum, Field, FrozenValue, HeapVersion, InputBacking, Leaf, ObjectData, Remainder,
    Value, ValueNode,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DecodeFormat {
    Yaml,
    Json,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Decoder {
    YamlMap,
    YamlSequence,
    JsonMap,
    JsonSequence,
    JsonValue,
}

impl Decoder {
    fn format(self) -> DecodeFormat {
        match self {
            Self::YamlMap | Self::YamlSequence => DecodeFormat::Yaml,
            Self::JsonMap | Self::JsonSequence | Self::JsonValue => DecodeFormat::Json,
        }
    }

    pub(super) fn schema_type(self) -> Option<&'static str> {
        match self {
            Self::YamlMap | Self::JsonMap => Some("object"),
            Self::YamlSequence | Self::JsonSequence => Some("array"),
            Self::JsonValue => None,
        }
    }
}

/// One executed operation, retained on its result's canonical origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Operation {
    Decode(Decoder),
    Stringify,
    Trim,
    Numeric(super::ScalarType),
    Descriptor { kind_only: bool },
    Member(String),
    Copy,
    Omit(BTreeSet<String>),
    Pick(BTreeSet<String>),
    Merge { overwrite: bool },
}

#[derive(Clone, Debug)]
pub(super) struct Derivation {
    pub(super) source: FrozenValue,
    pub(super) operation: Operation,
}

impl PartialEq for Derivation {
    fn eq(&self, other: &Self) -> bool {
        self.operation == other.operation
            && Rc::ptr_eq(&self.source.heap, &other.source.heap)
            && self.source.value == other.source.value
    }
}
impl Eq for Derivation {}

impl Value {
    /// Borrows the executed operation even when its dependency is carried only by guards.
    pub(super) fn operation_preimage(&self) -> Option<&Derivation> {
        match self.node.as_ref() {
            ValueNode::Leaf(Leaf { datum: _, origin }) => origin.derivation.as_deref(),
            ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => None,
        }
    }

    pub(super) fn derived(mut self, source: FrozenValue, operation: Operation) -> Self {
        let inputs = source.inputs();
        if inputs.is_empty() && self.constant().is_some() {
            return self;
        }
        match Rc::make_mut(&mut self.node) {
            ValueNode::Leaf(Leaf { datum: _, origin }) => {
                origin.influences.extend(inputs);
                origin.derivation = Some(Rc::new(Derivation { source, operation }));
            }
            ValueNode::Select {
                decision: _,
                yes,
                no,
            } => {
                *yes = yes.clone().derived(source.clone(), operation.clone());
                *no = no.clone().derived(source, operation);
            }
        }
        self
    }
}

/// A borrowed step in the source preimage, ordered from output toward input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Preimage<'a> {
    Operation(&'a Derivation),
    Text(&'a super::text::FrozenText),
    TemplateSource(&'a FrozenValue),
}

/// A surviving input region at a relative output path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Relationship<'a> {
    pub(crate) condition: Predicate,
    pub(crate) source: Cow<'a, ValuesPath>,
    pub(crate) output_path: Vec<String>,
    pub(crate) omitted_members: BTreeSet<String>,
    /// A source occurrence within composed text; its enclosing placement is not yet proved.
    pub(crate) capture: Option<&'a FrozenValue>,
    pub(super) preimage: Vec<Preimage<'a>>,
}

/// Unknown coverage includes unproved relationships and exhausted requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Relationships<'a> {
    pub(crate) known: Vec<Relationship<'a>>,
    pub(crate) unknown: Predicate,
}

impl Relationships<'_> {
    fn unknown() -> Self {
        Self {
            known: Vec::new(),
            unknown: Predicate::True,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SelectedDecision {
    id: u64,
    taken: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct NodeKey {
    heap: *const HeapVersion,
    value: *const ValueNode,
    members: Vec<String>,
    decisions: Vec<SelectedDecision>,
}

/// Queries share a work budget across captures, branches, heap fields and claims.
/// No heap or derivation is copied into an independent provenance store.
pub(super) struct RelationshipQuery<'a, 'inputs> {
    inputs: &'inputs BTreeMap<ValuesPath, InputBacking>,
    remaining: usize,
    limit: usize,
    predicates: PredicateMemo,
    decisions: BTreeMap<u64, bool>,
    omission_depth: usize,
    nodes: BTreeMap<NodeKey, Relationships<'a>>,
}

impl<'a, 'inputs> RelationshipQuery<'a, 'inputs> {
    pub(super) fn new(inputs: &'inputs BTreeMap<ValuesPath, InputBacking>, limit: usize) -> Self {
        let limit = limit.min(1024);
        Self {
            inputs,
            remaining: limit,
            limit,
            predicates: PredicateMemo::new(),
            decisions: BTreeMap::new(),
            omission_depth: 0,
            nodes: BTreeMap::new(),
        }
    }
    pub(super) fn fallback_source_path(
        &self,
        value: &Value,
        heap: &HeapVersion,
    ) -> Option<helm_schema_core::ValuesPath> {
        if let Some(path) = fallback_input_path(value) {
            return Some(path.clone());
        }
        let ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    target: Some(object),
                    view: _,
                },
            origin: _,
        }) = value.node.as_ref()
        else {
            return None;
        };
        if !matches!(heap.object(object), Some(super::ObjectVersion {
            multiplicity: super::Multiplicity::Singleton,
            data: super::ObjectData::Map { fields, remainder: super::Remainder::Input },
            revision: _,
        }) if fields.is_empty())
        {
            return None;
        }
        self.inputs
            .iter()
            .find_map(|(path, backing)| (backing.object == *object).then(|| path.clone()))
    }

    pub(super) fn direct_input_path(
        &self,
        datum: &Datum,
        origin: &super::Origin,
    ) -> Option<helm_schema_core::ValuesPath> {
        match datum {
            Datum::Scalar {
                go_type: _,
                content: Some(ScalarContent::Identity(path)),
            } => Some(path.clone()),
            Datum::Map {
                view: _,
                target: Some(object),
            } => self
                .inputs
                .iter()
                .find_map(|(path, backing)| (backing.object == *object).then(|| path.clone())),
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

    pub(super) fn original_unchanged(&mut self, path: &ValuesPath, heap: &HeapVersion) -> bool {
        let root = ValuesPath::default();
        if self
            .inputs
            .get(&root)
            .and_then(|backing| heap.object(&backing.object))
            .is_none()
        {
            return false;
        }
        let mut prefix = root;
        for segment in path.segments() {
            if !self.charge(1) {
                return false;
            }
            let Some(key) = segment.literal() else {
                return false;
            };
            if let Some(object) = self
                .inputs
                .get(&prefix)
                .and_then(|backing| heap.object(&backing.object))
            {
                if object.multiplicity != super::Multiplicity::Singleton {
                    return false;
                }
                if !matches!(&object.data, ObjectData::Map { fields, remainder: Remainder::Input } if !fields.contains_key(key))
                {
                    return false;
                }
            }
            prefix.push(key.to_owned());
        }
        self.unchanged(path, heap)
    }
    pub(super) fn work(&self) -> usize {
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

    pub(super) fn node_value(
        &mut self,
        value: &'a Value,
        heap: &'a HeapVersion,
        members: &[String],
    ) -> Relationships<'a> {
        self.node(value, heap, members, 0)
    }
    /// An empty member path requests the whole payload, including explicit surviving fields.
    #[tracing::instrument(skip_all, level = "debug", name = "input_relationship")]
    pub(super) fn value(
        &mut self,
        frozen: &'a FrozenValue,
        members: &[String],
    ) -> Relationships<'a> {
        self.node(&frozen.value, &frozen.heap, members, 0)
    }

    fn node(
        &mut self,
        value: &'a Value,
        heap: &'a HeapVersion,
        members: &[String],
        depth: usize,
    ) -> Relationships<'a> {
        let key_cost = self.decisions.len()
            + members
                .iter()
                .map(|key| 1 + key.len().div_ceil(64))
                .sum::<usize>()
            + 1;
        if depth >= 32 || !self.charge(key_cost) {
            return Relationships::unknown();
        }
        let key = NodeKey {
            heap: std::ptr::from_ref(heap),
            value: Rc::as_ptr(&value.node),
            members: members.to_vec(),
            decisions: self
                .decisions
                .iter()
                .map(|(id, taken)| SelectedDecision {
                    id: *id,
                    taken: *taken,
                })
                .collect(),
        };
        if let Some(view) = self.nodes.get(&key) {
            let cost = Self::copy_work(view);
            if !self.charge(cost) {
                return Relationships::unknown();
            }
            return self
                .nodes
                .get(&key)
                .cloned()
                .unwrap_or_else(Relationships::unknown);
        }
        let mut view = self.node_inner(value, heap, members, depth);
        if self.charge(Self::copy_work(&view)) {
            self.nodes.insert(key, view.clone());
        } else {
            view.unknown = Predicate::True;
        }
        view
    }

    fn path_work(path: &ValuesPath) -> usize {
        path.segments().fold(1_usize, |work, segment| {
            work.saturating_add(1 + segment.literal().map_or(0, |key| key.len().div_ceil(64)))
        })
    }

    fn copy_work(view: &Relationships<'_>) -> usize {
        view.known.iter().fold(1_usize, |work, arm| {
            work.saturating_add(
                1 + arm.preimage.len() + arm.output_path.len() + arm.omitted_members.len(),
            )
        })
    }

    fn node_inner(
        &mut self,
        value: &'a Value,
        heap: &'a HeapVersion,
        members: &[String],
        depth: usize,
    ) -> Relationships<'a> {
        if depth >= 32 || !self.charge(1) {
            return Relationships::unknown();
        }
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => self.choice(
                decision,
                |query| query.node(yes, heap, members, depth + 1),
                |query| query.node(no, heap, members, depth + 1),
            ),
            ValueNode::Leaf(Leaf { datum, origin }) => {
                self.leaf(datum, origin, heap, members, depth)
            }
        }
    }

    fn leaf(
        &mut self,
        datum: &'a Datum,
        origin: &'a super::Origin,
        heap: &'a HeapVersion,
        members: &[String],
        depth: usize,
    ) -> Relationships<'a> {
        if let Datum::Map {
            target: Some(id),
            view: _,
        } = datum
            && !matches!(
                heap.object(id).map(|object| &object.data),
                Some(ObjectData::Map {
                    fields: _,
                    remainder: Remainder::Closed | Remainder::Open | Remainder::Input
                })
            )
        {
            return Relationships::unknown();
        }
        if let Datum::Unknown(super::UnknownValue {
            types: _,
            aliases: super::AliasSet::Known(objects),
        }) = datum
        {
            for object in objects {
                if !self.charge(1)
                    || matches!(
                        heap.object(object).map(|object| &object.data),
                        Some(ObjectData::Widened { inputs: _ })
                    )
                {
                    return Relationships::unknown();
                }
            }
        }
        if let Datum::Text(text) = datum {
            let mut view = self.text(text.term(), members, depth + 1);
            if !self.charge(Self::copy_work(&view)) {
                return Relationships::unknown();
            }
            for arm in &mut view.known {
                arm.preimage.insert(0, Preimage::Text(text));
            }
            return view;
        }
        if let Datum::Map {
            target: Some(id),
            view: _,
        } = datum
            && let Some(object) = heap.object(id)
            && let ObjectData::Map { fields, remainder } = &object.data
            && let Some((key, rest)) = members.split_first()
        {
            if let Some(field) = fields.get(key) {
                return self.field(field, heap, rest, depth + 1);
            }
            if *remainder == Remainder::Closed && !fields.contains_key(key) {
                return Relationships {
                    known: Vec::new(),
                    unknown: Predicate::False,
                };
            }
        }
        if let Some(path) = &origin.identity
            && let Some((key, rest)) = members.split_first()
            && let Some(object) = self
                .inputs
                .get(path)
                .and_then(|backing| heap.object(&backing.object))
            && let ObjectData::Map {
                fields,
                remainder: _,
            } = &object.data
            && let Some(field) = fields.get(key)
        {
            return self.field(field, heap, rest, depth + 1);
        }
        if let Some(derivation) = &origin.derivation {
            return self.derivation(derivation, datum, heap, members, depth);
        }
        self.input(datum, origin, heap, members, depth)
    }

    fn derivation(
        &mut self,
        derivation: &'a Derivation,
        datum: &'a Datum,
        heap: &'a HeapVersion,
        members: &[String],
        depth: usize,
    ) -> Relationships<'a> {
        if let Operation::Pick(_) = &derivation.operation {
            return self.pick(datum, heap, members, depth);
        }
        if let Operation::Omit(keys) = &derivation.operation
            && members.first().is_some_and(|key| keys.contains(key))
        {
            return Relationships {
                known: Vec::new(),
                unknown: Predicate::False,
            };
        }
        let mut requested = members.to_vec();
        if let Operation::Member(key) = &derivation.operation {
            requested.insert(0, key.clone());
        }
        let mut view = if let Operation::Merge { overwrite: _ } = derivation.operation {
            let mut view = Relationships::unknown();
            if let ValueNode::Leaf(Leaf {
                datum: Datum::Sequence(sequence),
                origin: _,
            }) = derivation.source.value.node.as_ref()
                && let Some(arguments) = sequence.items()
            {
                for argument in arguments {
                    if !self.charge(1) {
                        break;
                    }
                    let operand =
                        self.node(argument, &derivation.source.heap, &requested, depth + 1);
                    view.known.extend(operand.known);
                }
            }
            view
        } else {
            self.node(
                &derivation.source.value,
                &derivation.source.heap,
                &requested,
                depth + 1,
            )
        };
        if members.is_empty()
            && let Operation::Omit(keys) = &derivation.operation
        {
            let key_work = keys.iter().fold(0_usize, |work, key| {
                work.saturating_add(1 + key.len().div_ceil(64))
            });
            let cost = key_work
                .saturating_mul(view.known.len())
                .saturating_add(Self::copy_work(&view));
            if !self.charge(cost) {
                return Relationships::unknown();
            }
            view.known.retain(|arm| {
                arm.output_path
                    .first()
                    .is_none_or(|key| !keys.contains(key))
            });
            for arm in &mut view.known {
                if arm.output_path.is_empty() {
                    arm.omitted_members.extend(keys.iter().cloned());
                }
            }
        }
        if matches!(
            derivation.operation,
            Operation::Omit(_) | Operation::Member(_)
        ) || derivation.operation == Operation::Copy && matches!(datum, Datum::Unknown(_))
        {
            self.refresh_aliases(&mut view, heap);
        }
        if !self.charge(Self::copy_work(&view)) {
            return Relationships::unknown();
        }
        for arm in &mut view.known {
            arm.preimage.insert(0, Preimage::Operation(derivation));
        }
        if members.is_empty()
            && let Datum::Map {
                target: Some(id),
                view: _,
            } = datum
            && let Some(object) = heap.object(id)
            && let ObjectData::Map {
                fields,
                remainder: _,
            } = &object.data
        {
            self.overlay(&mut view, fields, heap, depth + 1);
        }
        view
    }

    fn refresh_aliases(&mut self, view: &mut Relationships<'a>, heap: &HeapVersion) {
        view.known.retain_mut(|arm| {
            let Some(omitted) = self.omissions(&arm.source, heap) else {
                view.unknown = Predicate::True;
                return false;
            };
            arm.omitted_members.extend(omitted);
            if !arm.omitted_members.is_empty() {
                let mapping = Predicate::from(helm_schema_core::Guard::TypeIs {
                    path: arm.source.as_ref().clone(),
                    schema_type: "object".to_owned(),
                });
                arm.condition = self
                    .predicates
                    .normalize(Predicate::all(vec![arm.condition.clone(), mapping.clone()]));
                view.unknown = self
                    .predicates
                    .normalize(Predicate::Or(vec![view.unknown.clone(), mapping.negated()]));
            }
            true
        });
    }

    fn pick(
        &mut self,
        datum: &'a Datum,
        heap: &'a HeapVersion,
        members: &[String],
        depth: usize,
    ) -> Relationships<'a> {
        let Datum::Map {
            target: Some(id),
            view: _,
        } = datum
        else {
            return Relationships::unknown();
        };
        let Some(object) = heap.object(id) else {
            return Relationships::unknown();
        };
        let ObjectData::Map {
            fields,
            remainder: _,
        } = &object.data
        else {
            return Relationships::unknown();
        };
        if let Some((key, rest)) = members.split_first() {
            return fields.get(key).map_or_else(
                || Relationships {
                    known: Vec::new(),
                    unknown: Predicate::False,
                },
                |field| self.field(field, heap, rest, depth + 1),
            );
        }
        let mut view = Relationships {
            known: Vec::new(),
            unknown: Predicate::False,
        };
        self.overlay(&mut view, fields, heap, depth + 1);
        view
    }

    fn input_path(
        &mut self,
        datum: &'a Datum,
        origin: &'a super::Origin,
    ) -> Option<Cow<'a, ValuesPath>> {
        origin
            .identity
            .as_ref()
            .or(match datum {
                Datum::Scalar {
                    content: Some(super::scalar_value::ScalarContent::Identity(path)),
                    go_type: _,
                } => Some(path),
                _ => None,
            })
            .map(Cow::Borrowed)
            .or_else(|| {
                let Datum::Map {
                    target: Some(id),
                    view: _,
                } = datum
                else {
                    return None;
                };
                for path in &origin.influences {
                    if !self.charge(1) {
                        return None;
                    }
                    if self
                        .inputs
                        .get(path)
                        .is_some_and(|backing| &backing.object == id)
                    {
                        return Some(Cow::Borrowed(path));
                    }
                }
                None
            })
    }

    fn input(
        &mut self,
        datum: &'a Datum,
        origin: &'a super::Origin,
        heap: &'a HeapVersion,
        members: &[String],
        depth: usize,
    ) -> Relationships<'a> {
        let path = self.input_path(datum, origin);
        if self.remaining == 0 {
            return Relationships::unknown();
        }
        let mut view = Relationships {
            known: Vec::new(),
            unknown: Predicate::False,
        };
        if let Some(path) = &path {
            let mut source = path.clone();
            for key in members {
                if !self.charge(1) {
                    return Relationships::unknown();
                }
                if !self.charge(Self::path_work(&source)) {
                    return Relationships::unknown();
                }
                source.to_mut().push(key.clone());
            }
            if let Some(omitted_members) = self.omissions(&source, heap) {
                if !self.charge(1) {
                    return Relationships::unknown();
                }
                let condition = if omitted_members.is_empty() {
                    Predicate::True
                } else {
                    Predicate::from(helm_schema_core::Guard::TypeIs {
                        path: source.as_ref().clone(),
                        schema_type: "object".to_owned(),
                    })
                };
                view.unknown = condition.negated();
                view.known.push(Relationship {
                    condition,
                    source,
                    output_path: Vec::new(),
                    omitted_members,
                    preimage: Vec::new(),
                    capture: None,
                });
            } else {
                view.unknown = Predicate::True;
            }
        }
        if members.is_empty()
            && let Datum::Map {
                target: Some(id),
                view: _,
            } = datum
            && let Some(object) = heap.object(id)
            && let ObjectData::Map { fields, remainder } = &object.data
        {
            if path.is_none() && *remainder != Remainder::Closed {
                view.unknown = Predicate::True;
            }
            self.overlay(&mut view, fields, heap, depth + 1);
            return view;
        }
        if path.is_none()
            && !matches!(
                datum,
                Datum::Scalar {
                    content: Some(super::scalar_value::ScalarContent::Literal(_)),
                    go_type: _
                } | Datum::Map {
                    target: None,
                    view: _
                }
            )
        {
            view.unknown = Predicate::True;
        }
        view
    }

    fn overlay(
        &mut self,
        view: &mut Relationships<'a>,
        fields: &'a BTreeMap<String, Field>,
        heap: &'a HeapVersion,
        depth: usize,
    ) {
        for (key, field) in fields {
            if !self.charge(1 + key.len().div_ceil(64) + Self::copy_work(view)) {
                view.unknown = Predicate::True;
                return;
            }
            view.known
                .retain(|arm| arm.output_path.first() != Some(key));
            for arm in &mut view.known {
                if arm.output_path.is_empty() {
                    arm.omitted_members.insert(key.clone());
                }
            }
            let mut member = self.field(field, heap, &[], depth + 1);
            for arm in &mut member.known {
                arm.output_path.insert(0, key.clone());
            }
            view.known.extend(member.known);
            view.unknown = self
                .predicates
                .normalize(Predicate::Or(vec![view.unknown.clone(), member.unknown]));
        }
    }

    fn field(
        &mut self,
        field: &'a Field,
        heap: &'a HeapVersion,
        members: &[String],
        depth: usize,
    ) -> Relationships<'a> {
        if depth >= 32 || !self.charge(1) {
            return Relationships::unknown();
        }
        match field {
            Field::Absent => Relationships {
                known: Vec::new(),
                unknown: Predicate::False,
            },
            Field::Unknown => Relationships::unknown(),
            Field::Present(value) => self.node(value, heap, members, depth + 1),
            Field::Select { decision, yes, no } => self.choice(
                decision,
                |query| query.field(yes, heap, members, depth + 1),
                |query| query.field(no, heap, members, depth + 1),
            ),
        }
    }

    fn choice(
        &mut self,
        decision: &super::FrozenTest,
        yes: impl FnOnce(&mut Self) -> Relationships<'a>,
        no: impl FnOnce(&mut Self) -> Relationships<'a>,
    ) -> Relationships<'a> {
        if let Some(taken) = self.decisions.get(&decision.id) {
            return if *taken { yes(self) } else { no(self) };
        }
        if !self.charge(3) {
            return Relationships::unknown();
        }
        self.decisions.insert(decision.id, true);
        let yes = yes(self);
        self.decisions.insert(decision.id, false);
        let no = no(self);
        self.decisions.remove(&decision.id);
        if decision.coverage != super::Coverage::Exact
            || decision.when_true.contains_approximation()
            || decision.when_false.contains_approximation()
        {
            return self.common(yes, &no);
        }
        let mut view = Relationships {
            known: Vec::new(),
            unknown: Predicate::False,
        };
        for (branch, guard) in [(yes, &decision.when_true), (no, &decision.when_false)] {
            if guard.contains_approximation() {
                view.unknown = Predicate::True;
                continue;
            }
            for mut arm in branch.known {
                if !self.charge(1) {
                    view.unknown = Predicate::True;
                    return view;
                }
                arm.condition = self
                    .predicates
                    .normalize(Predicate::all(vec![guard.clone(), arm.condition]));
                if arm.condition != Predicate::False {
                    view.known.push(arm);
                }
            }
            view.unknown = self.predicates.normalize(Predicate::Or(vec![
                view.unknown,
                Predicate::all(vec![guard.clone(), branch.unknown]),
            ]));
        }
        if decision.coverage != super::Coverage::Exact {
            view.unknown = Predicate::True;
        }
        view
    }

    fn common(&mut self, yes: Relationships<'a>, no: &Relationships<'a>) -> Relationships<'a> {
        let mut view = Relationships::unknown();
        for mut left in yes.known {
            for right in &no.known {
                if !self.charge(1 + left.preimage.len() + right.preimage.len()) {
                    return view;
                }
                if left.source == right.source
                    && left.output_path == right.output_path
                    && left.omitted_members == right.omitted_members
                    && left.preimage == right.preimage
                    && same_capture(left.capture, right.capture)
                {
                    left.condition = self.predicates.normalize(Predicate::all(vec![
                        left.condition,
                        right.condition.clone(),
                    ]));
                    if left.condition != Predicate::False {
                        view.known.push(left);
                    }
                    break;
                }
            }
        }
        view
    }

    fn text(
        &mut self,
        term: &'a super::text::TextTerm,
        members: &[String],
        depth: usize,
    ) -> Relationships<'a> {
        use super::text::TextTerm;
        if depth >= 32 || !self.charge(1) {
            return Relationships::unknown();
        }
        match term {
            TextTerm::Resolved { term, literal: _ } => self.text(term, members, depth + 1),
            TextTerm::Yaml { value, pretty: _ }
            | TextTerm::Json {
                value,
                pretty: _,
                escape_html: _,
                literal: _,
            }
            | TextTerm::Indent {
                value,
                width: _,
                newline: _,
            }
            | TextTerm::Encoded { value, alphabet: _ }
            | TextTerm::Quote { value } => self.node(&value.value, &value.heap, members, depth + 1),
            TextTerm::Capture {
                graph,
                strip_marker: _,
            } => self.buffer(graph, members, depth + 1),
            TextTerm::Tpl { source, result } => {
                let mut view = self.node(&result.value, &result.heap, members, depth + 1);
                let program = self.node(&source.value, &source.heap, &[], depth + 1);
                if !self.charge(Self::copy_work(&program)) {
                    view.unknown = Predicate::True;
                    return view;
                }
                for mut arm in program.known {
                    arm.preimage.insert(0, Preimage::TemplateSource(source));
                    view.known.push(arm);
                }
                view
            }
        }
    }

    fn buffer(
        &mut self,
        buffer: &'a super::TextBuffer,
        members: &[String],
        depth: usize,
    ) -> Relationships<'a> {
        self.capture_parts(buffer, members, depth, true)
    }

    fn capture_parts(
        &mut self,
        buffer: &'a super::TextBuffer,
        members: &[String],
        depth: usize,
        whole: bool,
    ) -> Relationships<'a> {
        if depth >= 32 {
            return Relationships::unknown();
        }
        let mut significant = 0_usize;
        let mut literal = false;
        for part in &buffer.parts {
            if !self.charge(1) {
                return Relationships::unknown();
            }
            match part.content() {
                super::TextPart::Bytes(bytes) => literal |= !bytes.trim().is_empty(),
                super::TextPart::Frozen(_)
                | super::TextPart::Select {
                    decision: _,
                    yes: _,
                    no: _,
                }
                | super::TextPart::Unknown(_) => significant += 1,
            }
        }
        let whole = whole && significant == 1 && !literal;
        if !whole && !members.is_empty() {
            return Relationships::unknown();
        }
        let mut view = Relationships {
            known: Vec::new(),
            unknown: if whole {
                Predicate::False
            } else {
                Predicate::True
            },
        };
        for part in &buffer.parts {
            if !self.charge(1) {
                view.unknown = Predicate::True;
                return view;
            }
            let mut payload = match part.content() {
                super::TextPart::Bytes(_) => continue,
                super::TextPart::Frozen(value) => {
                    let mut payload = self.node(&value.value, &value.heap, members, depth + 1);
                    if !whole {
                        for arm in &mut payload.known {
                            arm.capture = arm.capture.or(Some(value));
                        }
                    }
                    payload
                }
                super::TextPart::Select { decision, yes, no } => self.choice(
                    decision,
                    |query| query.capture_parts(yes, members, depth + 1, whole),
                    |query| query.capture_parts(no, members, depth + 1, whole),
                ),
                super::TextPart::Unknown(_) => Relationships::unknown(),
            };
            if !self.charge(Self::copy_work(&payload)) {
                view.unknown = Predicate::True;
                return view;
            }
            view.known.append(&mut payload.known);
            view.unknown = self
                .predicates
                .normalize(Predicate::Or(vec![view.unknown, payload.unknown]));
        }
        view
    }

    pub(super) fn unchanged(
        &mut self,
        path: &helm_schema_core::ValuesPath,
        heap: &super::HeapVersion,
    ) -> bool {
        self.omissions(path, heap)
            .is_some_and(|omitted| omitted.is_empty())
    }

    pub(super) fn omissions(
        &mut self,
        path: &helm_schema_core::ValuesPath,
        heap: &super::HeapVersion,
    ) -> Option<BTreeSet<String>> {
        if self.omission_depth >= 32 {
            return None;
        }
        self.omission_depth += 1;
        let result = self.omissions_inner(path, heap);
        self.omission_depth -= 1;
        result
    }

    fn input_region_is_known(&mut self, path: &ValuesPath, heap: &HeapVersion) -> Option<()> {
        if !self.charge(Self::path_work(path)) {
            return None;
        }
        let mut prefix = helm_schema_core::ValuesPath::default();
        for segment in path.segments() {
            self.remaining = self.remaining.checked_sub(1)?;
            if self
                .inputs
                .get(&prefix)
                .and_then(|backing| heap.object(&backing.object))
                .is_some_and(|object| {
                    !matches!(
                        &object.data,
                        ObjectData::Map {
                            fields: _,
                            remainder: Remainder::Input
                        }
                    )
                })
            {
                return None;
            }
            match segment {
                helm_schema_core::Segment::Literal(key) => prefix.push(key.clone()),
                helm_schema_core::Segment::EachMember => {
                    // A wildcard cannot bind overwritten or removed original collection members.
                    if self
                        .inputs
                        .get(&prefix)
                        .and_then(|backing| heap.object(&backing.object))
                        .is_some_and(|object| !pristine_input_backing(object))
                    {
                        return None;
                    }
                    prefix.push_each_member();
                }
            }
        }
        Some(())
    }

    fn omissions_inner(
        &mut self,
        path: &helm_schema_core::ValuesPath,
        heap: &super::HeapVersion,
    ) -> Option<BTreeSet<String>> {
        self.input_region_is_known(path, heap)?;
        let mut omitted = BTreeSet::new();
        if let Some(object) = self
            .inputs
            .get(path)
            .and_then(|backing| heap.object(&backing.object))
        {
            let super::ObjectVersion {
                revision: _,
                multiplicity: _,
                data,
            } = object;
            let ObjectData::Map {
                fields,
                remainder: super::Remainder::Input,
            } = data
            else {
                return None;
            };
            for (key, field) in fields {
                self.remaining = self.remaining.checked_sub(1)?;
                if key.contains("<no value>") || !self.serializable_field(field, heap) {
                    return None;
                }
                self.remaining = self.remaining.checked_sub(key.len().div_ceil(64))?;
                omitted.insert(key.clone());
            }
        }
        let mut first_child = path.clone();
        first_child.push("\0");
        for (candidate, backing) in self
            .inputs
            .range(first_child..)
            .take_while(|(candidate, _)| candidate.is_descendant_of(path))
        {
            self.remaining = self.remaining.checked_sub(1)?;
            let Some(object) = heap.object(&backing.object) else {
                continue;
            };
            if pristine_input_backing(object) {
                continue;
            }
            let super::ObjectVersion {
                revision: _,
                multiplicity: _,
                data,
            } = object;
            let ObjectData::Map {
                fields,
                remainder: super::Remainder::Input,
            } = data
            else {
                return None;
            };
            for (key, field) in fields {
                self.remaining = self.remaining.checked_sub(1)?;
                if key.contains("<no value>") || !self.serializable_field(field, heap) {
                    return None;
                }
            }
            let helm_schema_core::Segment::Literal(key) =
                candidate.segments().nth(path.segments().len())?
            else {
                return None;
            };
            self.remaining = self.remaining.checked_sub(key.len().div_ceil(64))?;
            omitted.insert(key.clone());
        }
        Some(omitted)
    }

    fn serializable_field(&mut self, field: &super::Field, heap: &super::HeapVersion) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        match field {
            super::Field::Absent => true,
            super::Field::Unknown => false,
            super::Field::Select {
                decision: _,
                yes,
                no,
            } => self.serializable_field(yes, heap) && self.serializable_field(no, heap),
            super::Field::Present(value) => self.serializable_value(value, heap),
        }
    }

    fn serializable_value(&mut self, value: &Value, heap: &super::HeapVersion) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        match value.node.as_ref() {
            ValueNode::Select {
                decision: _,
                yes,
                no,
            } => self.serializable_value(yes, heap) && self.serializable_value(no, heap),
            ValueNode::Leaf(Leaf { datum: _, origin })
                if origin
                    .identity
                    .as_ref()
                    .is_some_and(|path| self.unchanged(path, heap)) =>
            {
                true
            }
            ValueNode::Leaf(Leaf {
                datum: Datum::Text(_),
                origin: _,
            }) => true,
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Scalar {
                        go_type,
                        content: _,
                    },
                origin: _,
            }) => !matches!(go_type, super::ScalarType::Complex128),
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }) => {
                let mut query = super::output::JsonQuery {
                    preserve_nil_collections: true,
                    remaining: Some(&mut self.remaining),
                    outcome: &|_| None,
                    choice: None,
                };
                super::output::json(value, heap, &mut BTreeSet::new(), 0, &mut query).is_some()
            }
        }
    }
}

fn pristine_input_backing(object: &super::ObjectVersion) -> bool {
    matches!(object, super::ObjectVersion {
        revision: _, multiplicity: _,
        data: ObjectData::Map { fields, remainder: super::Remainder::Input },
    } if fields.is_empty())
}

/// Serialization followed by its matching decoder preserves structured input coverage.
pub(super) fn structural_preimage(preimage: &[Preimage<'_>]) -> bool {
    let mut decode = None;
    for step in preimage {
        match step {
            Preimage::TemplateSource(_) => return false,
            Preimage::Operation(derivation) => match &derivation.operation {
                Operation::Decode(format) if decode.is_none() => decode = Some(*format),
                Operation::Copy | Operation::Omit(_) => {}
                Operation::Decode(_)
                | Operation::Trim
                | Operation::Pick(_)
                | Operation::Merge { overwrite: _ }
                | Operation::Stringify
                | Operation::Numeric(_)
                | Operation::Descriptor { kind_only: _ }
                | Operation::Member(_) => return false,
            },
            Preimage::Text(text) => match text.term() {
                super::text::TextTerm::Yaml {
                    value: _,
                    pretty: _,
                } if decode.map(Decoder::format) == Some(DecodeFormat::Yaml) => decode = None,
                super::text::TextTerm::Json {
                    value: _,
                    pretty: _,
                    escape_html: _,
                    literal: _,
                } if decode.map(Decoder::format) == Some(DecodeFormat::Json) => decode = None,
                super::text::TextTerm::Capture {
                    graph: _,
                    strip_marker: _,
                }
                | super::text::TextTerm::Indent {
                    value: _,
                    width: _,
                    newline: _,
                }
                | super::text::TextTerm::Resolved {
                    term: _,
                    literal: _,
                } => {}
                _ => return false,
            },
        }
    }
    decode.is_none()
}

impl super::DocumentExecution {
    /// Queries whole payloads or relative members at the value's frozen execution snapshot.
    /// A single budget covers all returned claims; unknown coverage must remain open.
    pub(crate) fn relationships<'a>(
        &self,
        value: &'a FrozenValue,
        members: &[String],
        budget: usize,
    ) -> Relationships<'a> {
        let Some(exit) = self.exits.first() else {
            return Relationships::unknown();
        };
        let inputs = exit.state.heap.arena.input_objects.borrow();
        RelationshipQuery::new(&inputs, budget).value(value, members)
    }
}

fn same_capture(left: Option<&FrozenValue>, right: Option<&FrozenValue>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => std::ptr::eq(left, right),
        (None, None) => true,
        (Some(_), None) | (None, Some(_)) => false,
    }
}

// Parameter validation accepts nil maps by an exact nil/present selection.
// Retain its raw source only when both selector predicates prove that same partition.
fn fallback_input_path(value: &Value) -> Option<&helm_schema_core::ValuesPath> {
    if let Some(path) = value.identity() {
        return Some(path);
    }
    let ValueNode::Select { decision, yes, no } = value.node.as_ref() else {
        return None;
    };
    let nil_map = |value: &Value| {
        matches!(
            value.node.as_ref(),
            ValueNode::Leaf(Leaf {
                datum: Datum::Map { target: None, view: _ },
                origin,
            }) if origin.identity.is_none() && origin.influences.is_empty()
        )
    };
    let (path, nil_when_true) = if nil_map(yes) {
        (no.identity()?, true)
    } else if nil_map(no) {
        (yes.identity()?, false)
    } else {
        return None;
    };
    let nil = Predicate::Or(vec![
        Predicate::from(Guard::Eq {
            path: path.clone(),
            value: GuardValue::Null,
        }),
        Predicate::from(Guard::Absent { path: path.clone() }),
    ])
    .normalize_boolean();
    let (when_nil, when_present) = if nil_when_true {
        (&decision.when_true, &decision.when_false)
    } else {
        (&decision.when_false, &decision.when_true)
    };
    (decision.coverage == Coverage::Exact && when_nil == &nil && when_present == &nil.negated())
        .then_some(path)
}
