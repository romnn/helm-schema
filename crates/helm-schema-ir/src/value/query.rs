use super::scalar_value::ScalarContent;
use super::{
    Coverage, Datum, Field, FrozenTest, FrozenValue, Leaf, ObjectData, ScalarType, State, Value,
    ValueNode,
};
use ahash::AHashSet;
use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};

#[derive(Clone, Debug)]
pub(super) struct PathPredicateCache {
    active: Weak<Vec<super::DecisionEdge>>,
    predicate: Predicate,
}

const EXPORTED_PATH_CACHE_CAPACITY: usize = 4096;

impl super::Arena {
    pub(super) fn exported_path_predicate(
        &self,
        active: &Rc<Vec<super::DecisionEdge>>,
    ) -> Predicate {
        let key = Rc::as_ptr(active);
        if let Some(PathPredicateCache { active, predicate }) =
            self.exported_paths.borrow().get(&key)
            && std::ptr::eq(active.as_ptr(), key)
        {
            return predicate.clone();
        }
        let predicate = Predicate::all(
            active
                .iter()
                .map(|edge| edge.test.possible_predicate(edge.taken))
                .collect(),
        );
        let mut cache = self.exported_paths.borrow_mut();
        if cache.len() >= EXPORTED_PATH_CACHE_CAPACITY {
            cache.clear();
        }
        // Weak keys preserve allocation identity without retaining execution handles.
        cache.insert(
            key,
            PathPredicateCache {
                active: Rc::downgrade(active),
                predicate: predicate.clone(),
            },
        );
        predicate
    }
}

#[derive(Clone, Copy)]
pub(super) enum MemberAccess {
    Selector,
    StringIndex,
    MapArgument { allows_nil: bool },
}

struct KnownInputObject {
    object: super::ObjectId,
    storage: Rc<super::ObjectVersion>,
}

enum InputRoot<'value> {
    Value(&'value Value),
    Object(KnownInputObject),
}

impl Value {
    pub(super) fn with_influences(mut self, inputs: BTreeSet<ValuesPath>) -> Self {
        if let ValueNode::Leaf(Leaf { datum: _, origin }) = Rc::make_mut(&mut self.node) {
            origin.influences.extend(inputs);
        }
        self
    }

    pub(super) fn literal(value: GuardValue) -> Self {
        let kind = match &value {
            GuardValue::String(_) => ScalarType::String,
            GuardValue::Bool(_) => ScalarType::Bool,
            GuardValue::Int(_) => ScalarType::Int,
            GuardValue::Float(_) => ScalarType::Float64,
            GuardValue::Null => ScalarType::Nil,
        };
        Self::scalar(kind, Some(ScalarContent::Literal(value)))
    }
    pub(super) fn nil() -> Self {
        Self::literal(GuardValue::Null)
    }
    pub(super) fn invalid() -> Self {
        Self::scalar(
            ScalarType::Invalid,
            Some(ScalarContent::Literal(GuardValue::Null)),
        )
    }
    pub(super) fn is_invalid(&self) -> bool {
        matches!(
            self.node.as_ref(),
            ValueNode::Leaf(Leaf {
                datum: Datum::Scalar {
                    go_type: ScalarType::Invalid,
                    content: _
                },
                origin: _,
            })
        )
    }
    pub(super) fn input(path: ValuesPath) -> Self {
        Self {
            node: Rc::new(ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(super::UnknownValue {
                    types: super::TypeCoverage::Unresolved,
                    aliases: super::AliasSet::RenderGraph,
                }),
                origin: super::Origin {
                    derivation: None,
                    identity: Some(path),
                    influences: BTreeSet::new(),
                },
            })),
        }
    }
    pub(super) fn identity(&self) -> Option<&ValuesPath> {
        match self.node.as_ref() {
            ValueNode::Leaf(Leaf { datum: _, origin }) => origin.identity.as_ref(),
            ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => None,
        }
    }
    pub(super) fn constant(&self) -> Option<&GuardValue> {
        match self.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Scalar {
                        go_type: _,
                        content,
                    },
                origin: _,
            }) => match content {
                Some(ScalarContent::Literal(value)) => Some(value),
                Some(
                    ScalarContent::Identity(_)
                    | ScalarContent::StringDomain {
                        pattern: _,
                        min_bytes: _,
                        max_bytes: _,
                    },
                )
                | None => None,
            },
            ValueNode::Leaf(Leaf {
                datum: Datum::Text(text),
                origin: _,
            }) => match text.0.as_ref() {
                super::text::TextTerm::Resolved { term: _, literal } => Some(literal),
                super::text::TextTerm::Json {
                    value: _,
                    pretty: _,
                    escape_html: _,
                    literal,
                } => literal.as_ref(),
                super::text::TextTerm::Yaml {
                    value: _,
                    pretty: _,
                }
                | super::text::TextTerm::Tpl {
                    source: _,
                    result: _,
                }
                | super::text::TextTerm::Indent {
                    value: _,
                    width: _,
                    newline: _,
                }
                | super::text::TextTerm::Capture {
                    graph: _,
                    strip_marker: _,
                }
                | super::text::TextTerm::Quote { value: _ }
                | super::text::TextTerm::Encoded {
                    value: _,
                    alphabet: _,
                } => None,
            },
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map { view: _, target: _ }
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
            } => None,
        }
    }
    pub(super) fn inputs(&self) -> BTreeSet<ValuesPath> {
        let mut inputs = BTreeSet::new();
        let mut pending = vec![self];
        let mut seen = AHashSet::new();
        while let Some(value) = pending.pop() {
            if !seen.insert(Rc::as_ptr(&value.node)) {
                continue;
            }
            match value.node.as_ref() {
                ValueNode::Select {
                    decision: _,
                    yes,
                    no,
                } => {
                    pending.push(yes);
                    pending.push(no);
                }
                ValueNode::Leaf(Leaf { datum, origin }) => {
                    inputs.extend(origin.influences.iter().cloned());
                    inputs.extend(origin.identity.iter().cloned());
                    match datum {
                        Datum::Sequence(super::InterfaceSequence {
                            storage,
                            start: _,
                            length: _,
                            capacity: _,
                        }) => {
                            if let Some(items) = storage {
                                pending.extend(items.iter());
                            }
                        }
                        Datum::Text(_)
                        | Datum::Scalar {
                            go_type: _,
                            content: _,
                        }
                        | Datum::Map { view: _, target: _ }
                        | Datum::Strings(_)
                        | Datum::Slot {
                            backing: _,
                            index: _,
                        }
                        | Datum::Unknown(_) => {}
                    }
                }
            }
        }
        inputs
    }
    pub(super) fn influenced(inputs: BTreeSet<ValuesPath>) -> Self {
        Self {
            node: Rc::new(ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(super::UnknownValue {
                    types: super::TypeCoverage::Unresolved,
                    aliases: super::AliasSet::RenderGraph,
                }),
                origin: super::Origin {
                    derivation: None,
                    identity: None,
                    influences: inputs,
                },
            })),
        }
    }
}

impl State {
    pub(super) fn record_value_read(&mut self, value: &Value, coverage: super::ReadCoverage) {
        let condition = self.path_predicate();
        self.record_value_read_under(value, coverage, &condition);
    }

    fn record_value_read_under(
        &mut self,
        value: &Value,
        coverage: super::ReadCoverage,
        condition: &Predicate,
    ) {
        for path in self.heap.freeze(value).inputs() {
            // A summarized member read does not prove a visit or a non-null collection.
            // The collection read owns authoring openness; member obligations own narrowing.
            if coverage == super::ReadCoverage::ExactPath
                && path.segments().any(|segment| segment.literal().is_none())
            {
                continue;
            }
            self.reads.insert(super::RuntimeRead {
                path,
                condition: condition.clone(),
                coverage,
            });
        }
    }

    pub(super) fn record_field_read(&mut self, value: &Value, key: &str) {
        if key.is_empty() {
            self.record_container_read(value);
            return;
        }
        if matches!(
            self.lookup_field(value, key),
            Some(Field::Present(_) | Field::Absent)
        ) {
            return;
        }
        if let Some(path) = self.untouched_map_child_identity(value, key) {
            self.record_value_read(&Value::input(path), super::ReadCoverage::ExactPath);
        }
    }

    pub(super) fn record_container_read(&mut self, value: &Value) {
        let consumed = self.consumed(value);
        if matches!(
            self.heap.freeze(&consumed).go_type(),
            super::kinds::GoType::Map | super::kinds::GoType::Slice | super::kinds::GoType::Unknown
        ) {
            self.record_value_read(&consumed, super::ReadCoverage::UnknownRegion);
        }
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn record_invocation_reads(&mut self, invocation: &super::Invocation) {
        use crate::function_semantics::InputRead;
        let semantics = invocation.callee.semantics();
        let receiver = match &invocation.callee {
            super::Callee::Function(_) => None,
            super::Callee::Method {
                receiver,
                descriptor: _,
            } => Some(receiver),
        };
        let condition =
            (receiver.is_some() || !invocation.operands.is_empty()).then(|| self.path_predicate());
        if let Some(condition) = &condition {
            if let Some(receiver) = receiver {
                self.record_value_read_under(receiver, super::ReadCoverage::ExactPath, condition);
            }
            for operand in &invocation.operands {
                self.record_value_read_under(
                    &operand.value,
                    super::ReadCoverage::ExactPath,
                    condition,
                );
            }
        }
        self.record_root_fallback_read(invocation);
        if semantics.heap_effect != crate::function_semantics::HeapEffect::ReadOnly
            && self.has_pending_values_fallback()
        {
            // A nested alias may change without changing the returned map's own revision.
            // Keep completed invocation history but expire pending install correlations.
            for record in &mut self.invocations {
                record.values_fallback = None;
            }
        }
        if let Some(condition) = &condition {
            self.record_default_fallback_reads(invocation, condition);
        }
        match semantics.input_read {
            InputRead::Value => {}
            InputRead::Contents => {
                if let Some(receiver) = receiver {
                    self.record_container_read(receiver);
                }
                for operand in &invocation.operands {
                    self.record_container_read(&operand.value);
                }
            }
            InputRead::Keyed => {
                if invocation
                    .operands
                    .iter()
                    .skip(1)
                    .any(|operand| !matches!(operand.value.constant(), Some(GuardValue::String(key)) if !key.is_empty()))
                    && let Some(operand) = invocation.operands.first()
                {
                    self.record_value_read(&operand.value, super::ReadCoverage::UnknownRegion);
                }
            }
        }
    }

    fn record_root_fallback_read(&mut self, invocation: &super::Invocation) {
        if invocation.callee.function_name() != Some("set") {
            return;
        }
        let [target, key, installed] = invocation.operands.as_slice() else {
            return;
        };
        if key.value.constant() != Some(&GuardValue::String("Values".to_owned())) {
            return;
        }
        let Some(target_object) = map_object_id(&target.value) else {
            return;
        };
        // The first frame's dot retains the document context even when `$` is rebound.
        // Helper-local contexts must not withdraw the original document's key closure.
        let original_root = self.frames.first().and_then(|frame| frame.dot.as_ref());
        if original_root.and_then(map_object_id) != Some(target_object) {
            return;
        }
        let opaque_unknown = matches!(
            installed.value.node.as_ref(),
            ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(super::UnknownValue {
                    types: super::TypeCoverage::Unresolved | super::TypeCoverage::Map(_),
                    aliases: _,
                }),
                origin: super::Origin {
                    derivation: _,
                    identity: None,
                    influences: _,
                },
            })
        );
        let opaque_map = map_object_id(&installed.value)
            .and_then(|object| self.heap.version.object(object))
            .is_some_and(|object| {
                matches!(
                    &object.data,
                    ObjectData::Widened { inputs: _ }
                        | ObjectData::Map {
                            fields: _,
                            remainder: super::Remainder::Open
                        }
                )
            });
        if opaque_unknown || opaque_map {
            for path in self.heap.freeze(&installed.value).inputs() {
                self.reads.insert(super::RuntimeRead {
                    coverage: super::ReadCoverage::OpaqueValuesRoot,
                    path,
                    condition: self.path_predicate(),
                });
            }
        }
        let Some(installed_object) = map_object_id(&installed.value) else {
            return;
        };
        if !self.calls.is_empty() || self.read_root().and_then(map_object_id) != Some(target_object)
        {
            return;
        }
        let sources = self
            .invocations
            .iter()
            .filter_map(|record| record.values_fallback.as_ref())
            .filter(|fallback| {
                fallback
                    .result
                    .get(installed_object)
                    .is_some_and(|revision| {
                        self.heap
                            .version
                            .object(installed_object)
                            .is_some_and(|object| &object.revision == revision)
                    })
            })
            .map(|fallback| fallback.source.source_path.clone())
            .collect::<BTreeSet<_>>();
        for path in sources {
            self.reads.insert(super::RuntimeRead {
                coverage: super::ReadCoverage::RootMergeFallback,
                path,
                condition: self.path_predicate(),
            });
        }
    }

    #[tracing::instrument(skip_all, fields(records = self.invocations.len()))]
    fn has_pending_values_fallback(&self) -> bool {
        for index in (0..self.invocations.len()).rev() {
            let Some(record) = self.invocations.get(index) else {
                continue;
            };
            if record.values_fallback.is_some() {
                return true;
            }
            // A joined or incomplete history cannot make a branch-local writer a barrier.
            // Shared ambient identity proves the expiry happened on this continuation.
            if self.invocation_records_complete
                && record.condition.shares_ambient(&self.active)
                && crate::function_semantics::function_semantics(&record.function).heap_effect
                    == crate::function_semantics::HeapEffect::Operands
            {
                return false;
            }
        }
        false
    }

    fn record_default_fallback_reads(
        &mut self,
        invocation: &super::Invocation,
        condition: &Predicate,
    ) {
        let fallback_operands = match invocation.callee.function_name() {
            Some("default") => invocation.operands.get(1..2).unwrap_or(&[]),
            Some("coalesce") => invocation
                .operands
                .get(..invocation.operands.len().saturating_sub(1))
                .unwrap_or(&[]),
            Some(_) | None => return,
        };
        for operand in fallback_operands {
            if let Some(path) = operand.value.identity() {
                self.reads.insert(super::RuntimeRead {
                    coverage: super::ReadCoverage::DefaultFallback,
                    path: path.clone(),
                    condition: condition.clone(),
                });
            }
        }
    }

    /// Whether [`Self::path_predicate`] normalizes to `false`, decided through the analysis-wide
    /// predicate memo.
    pub(super) fn path_is_infeasible(&self) -> bool {
        !self
            .heap
            .arena
            .predicate_memo
            .may_be_true(self.path_predicate())
    }

    pub(super) fn path_predicate(&self) -> Predicate {
        if self.heap.arena.runtime_path(&self.active).is_impossible() {
            return Predicate::False;
        }
        self.heap.arena.exported_path_predicate(&self.active)
    }
    pub(super) fn consumed(&self, value: &Value) -> Value {
        let value = self.heap.version.consume(value);
        let guard = self.heap.arena.runtime_path(&self.active);
        let value = value.under_runtime(&guard);
        let value = self.input_backing_view(&value);
        value.under_runtime(&guard)
    }

    fn input_backing_view(&self, value: &Value) -> Value {
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => Value::select(
                Rc::clone(decision),
                self.input_backing_view(yes),
                self.input_backing_view(no),
            ),
            ValueNode::Leaf(Leaf {
                datum: Datum::Map { view, target },
                origin:
                    super::Origin {
                        derivation: _,
                        identity: Some(_),
                        influences: _,
                    },
            }) => Value {
                node: Rc::new(ValueNode::Leaf(Leaf {
                    datum: Datum::Map {
                        view: *view,
                        target: target.clone(),
                    },
                    origin: super::Origin {
                        derivation: None,
                        identity: None,
                        influences: value.inputs(),
                    },
                })),
            },
            ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(super::UnknownValue { types, aliases }),
                origin,
            }) => {
                let Some(path) = &origin.identity else {
                    return value.clone();
                };
                let backing = self.heap.arena.input_objects.borrow().get(path).cloned();
                let Some(backing) = backing
                    .filter(|backing| self.heap.version.has_allocated_object(&backing.object))
                else {
                    return value.clone();
                };
                let map = Value {
                    node: Rc::new(ValueNode::Leaf(Leaf {
                        datum: Datum::Map {
                            view: super::MapType::Interface,
                            target: Some(backing.object),
                        },
                        origin: super::Origin {
                            derivation: None,
                            identity: None,
                            influences: value.inputs(),
                        },
                    })),
                };
                match types {
                    super::TypeCoverage::Map(_) => map,
                    super::TypeCoverage::Unresolved => {
                        let nonmap = Value {
                            node: Rc::new(ValueNode::Leaf(Leaf {
                                datum: Datum::Unknown(super::UnknownValue {
                                    types: super::TypeCoverage::InputNonMap,
                                    aliases: aliases.clone(),
                                }),
                                origin: origin.clone(),
                            })),
                        };
                        Value::select(backing.map_type, map, nonmap)
                    }
                    super::TypeCoverage::InputNonMap
                    | super::TypeCoverage::Scalar(_)
                    | super::TypeCoverage::Sequence => value.clone(),
                }
            }
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }) => value.clone(),
        }
    }

    pub(super) fn selector_entry_domain(&self, value: &Value) -> Option<Predicate> {
        let value = self.consumed(value);
        let mut remaining = super::MAX_SCALAR_DISPATCH_STATES;
        selector_entry_domain(&value, &mut remaining)
    }

    pub(super) fn evaluated_member_entry(&self, value: &Value) -> Value {
        if !value
            .inputs()
            .iter()
            .any(|path| path.item_parent().is_some())
        {
            return value.clone();
        }
        let mut remaining = super::MAX_SCALAR_DISPATCH_STATES;
        evaluated_member_entry(self, value, &mut remaining)
    }

    pub(super) fn record_member_host(
        &mut self,
        value: &Value,
        live: &Predicate,
        operation: MemberAccess,
    ) {
        let exact = self
            .active
            .iter()
            .all(|edge| edge.test.coverage == Coverage::Exact);
        if !exact && matches!(operation, MemberAccess::MapArgument { .. }) {
            return;
        }
        let frozen = self.heap.freeze(value);
        let ambient = if exact {
            self.path_predicate()
        } else {
            Predicate::all(vec![
                self.path_predicate(),
                Predicate::approximate(
                    "member access execution",
                    value.inputs().iter().map(ValuesPath::encode).collect(),
                ),
            ])
        };
        let mut pending = vec![(&frozen.value, Predicate::True)];
        let mut remaining = super::MAX_SCALAR_DISPATCH_STATES;
        while let Some((value, selected)) = pending.pop() {
            if remaining == 0 {
                return;
            }
            remaining -= 1;
            match value.node.as_ref() {
                ValueNode::Select { decision, yes, no } => {
                    if decision.coverage != Coverage::Exact {
                        continue;
                    }
                    pending.push((
                        yes,
                        Predicate::all(vec![selected.clone(), decision.when_true.clone()]),
                    ));
                    pending.push((
                        no,
                        Predicate::all(vec![selected, decision.when_false.clone()]),
                    ));
                }
                ValueNode::Leaf(Leaf { datum, origin }) => {
                    let Some(source_path) =
                        self.member_host_path(&frozen, datum, origin, operation)
                    else {
                        continue;
                    };
                    let path = &source_path;
                    let member = path.item_parent().is_some();
                    let strict_member_field =
                        matches!(operation, MemberAccess::MapArgument { allows_nil: false })
                            && path
                                .segments()
                                .filter(|segment| segment.is_each_member())
                                .count()
                                == 1;
                    if path.segments().next().is_none()
                        || (!member
                            && !strict_member_field
                            && path.segments().any(|segment| segment.literal().is_none()))
                    {
                        continue;
                    }
                    if !known_nonmap_receiver(datum)
                        && self
                            .heap
                            .arena
                            .input_objects
                            .borrow()
                            .get(path)
                            .and_then(|backing| frozen.heap.object(&backing.object))
                            .is_some_and(|object| {
                                !matches!(
                                    &object.data,
                                    ObjectData::Map {
                                        fields,
                                        remainder: super::Remainder::Input
                                    } if fields.is_empty() || matches!(operation, MemberAccess::MapArgument { .. })
                                )
                            })
                    {
                        continue;
                    }
                    self.record_host_path(
                        path,
                        operation,
                        Predicate::all(vec![ambient.clone(), live.clone(), selected]),
                    );
                }
            }
        }
    }

    fn member_host_path(
        &self,
        frozen: &FrozenValue,
        datum: &Datum,
        origin: &super::Origin,
        operation: MemberAccess,
    ) -> Option<ValuesPath> {
        if let Some(path) = &origin.identity {
            return Some(path.clone());
        }
        let Datum::Map {
            target: Some(id),
            view: _,
        } = datum
        else {
            return None;
        };
        self.input_map_path(
            frozen,
            id,
            matches!(operation, MemberAccess::MapArgument { .. }),
        )
    }

    fn input_map_path(
        &self,
        frozen: &FrozenValue,
        id: &super::ObjectId,
        allow_known_fields: bool,
    ) -> Option<ValuesPath> {
        if !matches!(
            frozen.heap.object(id).as_deref().map(|object| &object.data),
            Some(ObjectData::Map {
                fields,
                remainder: super::Remainder::Input
            }) if fields.is_empty() || allow_known_fields
        ) {
            return None;
        }
        self.heap.arena.input_paths.borrow().get(id).cloned()
    }

    fn record_host_path(
        &mut self,
        path: &ValuesPath,
        operation: MemberAccess,
        condition: Predicate,
    ) {
        let condition = self.heap.arena.predicate_memo.normalize(condition);
        if condition == Predicate::False
            || condition.contains_approximation()
                && matches!(operation, MemberAccess::MapArgument { .. })
        {
            return;
        }
        let member = path.item_parent().is_some();
        let coverage = match operation {
            MemberAccess::MapArgument { allows_nil } => super::ReadCoverage::MapArgument {
                allows_nil: allows_nil && !member,
            },
            MemberAccess::Selector | MemberAccess::StringIndex => {
                super::ReadCoverage::MemberHost { allows_nil: false }
            }
        };
        self.reads.insert(super::RuntimeRead {
            coverage,
            path: path.clone(),
            condition,
        });
    }

    pub(super) fn selector_field(&mut self, value: &Value, key: &str) -> Value {
        let value = self.consumed(value);
        if value.is_invalid() {
            return value;
        }
        if let ValueNode::Select { decision, yes, no } = value.node.as_ref() {
            return Value::select(
                Rc::clone(decision),
                self.selector_field(yes, key),
                self.selector_field(no, key),
            );
        }
        // A read of a written input-root member keeps the authoring key after
        // its value loses input provenance. It adds no input-shape evidence.
        if let ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    view: _,
                    target: Some(id),
                },
            origin: _,
        }) = value.node.as_ref()
            && let Some(path) = self.heap.arena.input_paths.borrow().get(id)
            && path.segments().next().is_none()
        {
            let mut members = Vec::new();
            if let Some(field) = self.lookup_field(&value, key)
                && !field_inputs_with_budget(
                    &field,
                    &mut members,
                    &mut super::heap_query::QueryBudget::new(65_536),
                )
            {
                return Value::influenced(value.inputs());
            }
            let erased = if members.is_empty() {
                self.untouched_map_child_identity(&value, key).is_none()
            } else {
                members.iter().all(|member| {
                    !self.heap.freeze(member).inputs().iter().any(|path| {
                        path.segments()
                            .next()
                            .and_then(helm_schema_core::Segment::literal)
                            == Some(key)
                    })
                })
            };
            if erased {
                self.reads.insert(super::RuntimeRead {
                    path: ValuesPath::from_segments([helm_schema_core::Segment::Literal(
                        key.to_owned(),
                    )]),
                    condition: self.path_predicate(),
                    coverage: super::ReadCoverage::ExactPath,
                });
            }
        }
        if let ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    view: super::MapType::Values,
                    target,
                },
            origin: _,
        }) = value.node.as_ref()
        {
            let influences = self.heap.freeze(&value).inputs();
            let datum = match key {
                "YAML" => Datum::Scalar {
                    go_type: ScalarType::String,
                    content: None,
                },
                "AsMap" => match self.truth(&value) {
                    Some(false) => return self.heap.map(super::Remainder::Closed),
                    Some(true) => Datum::Map {
                        view: super::MapType::Interface,
                        target: target.clone(),
                    },
                    None => Datum::Unknown(super::UnknownValue {
                        types: super::TypeCoverage::Map(super::MapType::Interface),
                        aliases: super::AliasSet::RenderGraph,
                    }),
                },
                "Table" | "Encode" => return Value::influenced(influences),
                _ => return self.field(&value, key),
            };
            return Value {
                node: Rc::new(ValueNode::Leaf(Leaf {
                    datum,
                    origin: super::Origin {
                        derivation: None,
                        identity: None,
                        influences,
                    },
                })),
            };
        }
        self.field(&value, key)
    }
    pub(super) fn field(&mut self, value: &Value, key: &str) -> Value {
        let value = self.consumed(value);
        super::hosts::file_field(&value, key, self).unwrap_or_else(|| match value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Unknown(super::UnknownValue {
                        types: super::TypeCoverage::InputNonMap,
                        aliases: _,
                    })
                    | Datum::Text(_)
                    | Datum::Scalar {
                        go_type: _,
                        content: _,
                    },
                origin: _,
            }) => Value::influenced(value.inputs()),
            ValueNode::Select { decision, yes, no } => Value::select(
                Rc::clone(decision),
                self.field(yes, key),
                self.field(no, key),
            ),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        view: super::MapType::String,
                        target,
                    },
                origin: _,
            }) => {
                let unknown = Value {
                    node: Rc::new(ValueNode::Leaf(Leaf {
                        datum: Datum::Scalar {
                            go_type: ScalarType::String,
                            content: None,
                        },
                        origin: super::Origin {
                            derivation: None,
                            identity: None,
                            influences: self.heap.freeze(&value).inputs(),
                        },
                    })),
                };
                target.as_ref().map_or_else(
                    || Value::string(""),
                    |id| {
                        self.consumed(&field_value(
                            self.heap.version.field(id, key),
                            &unknown,
                            &Value::string(""),
                        ))
                    },
                )
            }
            ValueNode::Leaf(Leaf {
                datum: _,
                origin:
                    super::Origin {
                        derivation: _,
                        identity: Some(path),
                        influences: _,
                    },
            }) => {
                let id = self
                    .heap
                    .arena
                    .input_objects
                    .borrow()
                    .get(path)
                    .map(|backing| backing.object.clone());
                let (field, unknown) = self.read_field(id.as_ref(), Some(path), key);
                self.consumed(&field_value(field, &unknown, &Value::nil()))
            }
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        view: _,
                        target: Some(id),
                    },
                origin: _,
            }) => {
                let path = self.heap.arena.input_paths.borrow().get(id).cloned();
                let (field, unknown) = self.read_field(Some(id), path.as_ref(), key);
                let unknown = if field_needs_unknown(&field) {
                    self.unknown_map_child(&value, path.as_ref(), key, unknown)
                } else {
                    unknown
                };
                self.consumed(&field_value(field, &unknown, &Value::nil()))
            }
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }) => Value::influenced(value.inputs()),
        })
    }
    fn unknown_map_child(
        &self,
        value: &Value,
        path: Option<&ValuesPath>,
        key: &str,
        mut unknown: Value,
    ) -> Value {
        if path.is_none() {
            unknown = Value::with_influences(unknown, value.inputs());
        }
        if let ValueNode::Leaf(Leaf { datum: _, origin }) = value.node.as_ref()
            && origin.derivation.is_some()
        {
            unknown = unknown.derived(
                self.heap.freeze(value),
                super::relationship::ValueOperation::Member(key.to_owned()),
            );
        }
        unknown
    }

    fn unknown_child_value(
        &self,
        id: Option<&super::ObjectId>,
        path: Option<&ValuesPath>,
        key: &str,
    ) -> Value {
        let object = id.and_then(|id| self.heap.version.object(id));
        Self::unknown_child_in_storage(object.as_deref().map(|object| &object.data), path, key)
    }

    fn read_field(
        &self,
        id: Option<&super::ObjectId>,
        path: Option<&ValuesPath>,
        key: &str,
    ) -> (Field, Value) {
        let object = id.and_then(|id| self.heap.version.object(id));
        let data = object.as_deref().map(|object| &object.data);
        let field = match data {
            Some(ObjectData::Map { fields, remainder }) => fields
                .get(key)
                .cloned()
                .unwrap_or_else(|| remainder.missing()),
            Some(
                ObjectData::Strings(_)
                | ObjectData::Opaque { reachable: _ }
                | ObjectData::Widened { inputs: _ },
            )
            | None => Field::Unknown,
        };
        let unknown = if field_needs_unknown(&field) {
            Self::unknown_child_in_storage(data, path, key)
        } else {
            Value::unknown()
        };
        (field, unknown)
    }

    fn unknown_child_in_storage(
        data: Option<&ObjectData>,
        path: Option<&ValuesPath>,
        key: &str,
    ) -> Value {
        if let Some(
            ObjectData::Widened { inputs }
            | ObjectData::Map {
                fields: _,
                remainder: super::Remainder::Unknown { inputs },
            },
        ) = data
        {
            return Value::influenced(inputs.as_ref().clone());
        }
        let Some(mut path) = path.cloned() else {
            return Value::unknown();
        };
        if key.is_empty() {
            return Value::influenced(BTreeSet::from([path]));
        }
        path.push(key);
        let untouched = data.is_none_or(|data| {
            matches!(
                data,
                ObjectData::Map {
                    remainder: super::Remainder::Input,
                    ..
                }
            )
        });
        if untouched {
            Value::input(path)
        } else {
            Value::influenced(BTreeSet::from([path]))
        }
    }

    pub(super) fn truth(&self, value: &Value) -> Option<bool> {
        let value = self.consumed(value);
        self.truth_of_consumed(&value)
    }

    fn truth_of_consumed(&self, value: &Value) -> Option<bool> {
        if let Some(literal) = value.constant() {
            return Some(match literal {
                GuardValue::Null => false,
                GuardValue::Bool(value) => *value,
                GuardValue::String(value) => !value.is_empty(),
                GuardValue::Int(value) => *value != 0,
                GuardValue::Float(value) => value.parse::<f64>().ok()? != 0.0,
            });
        }
        if let ValueNode::Leaf(Leaf {
            datum:
                Datum::Scalar {
                    go_type: ScalarType::String,
                    content:
                        Some(ScalarContent::StringDomain {
                            pattern: _,
                            min_bytes,
                            max_bytes,
                        }),
                },
            origin: _,
        }) = value.node.as_ref()
        {
            if *min_bytes > 0 {
                return Some(true);
            }
            if *max_bytes == 0 {
                return Some(false);
            }
        }
        match value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        target: None,
                        view: _,
                    },
                origin: _,
            }) => Some(false),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Strings(super::StringSlice {
                        backing: _,
                        start: _,
                        length,
                        capacity: _,
                    })
                    | Datum::Sequence(super::InterfaceSequence {
                        storage: _,
                        start: _,
                        length,
                        capacity: _,
                    }),
                origin: _,
            }) => Some(*length != 0),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        target: Some(_),
                        view: _,
                    },
                origin: _,
            }) => self
                .map_length_of_consumed(value)
                .and_then(|length| match length.constant() {
                    Some(GuardValue::Int(length)) => Some(*length != 0),
                    _ => None,
                }),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Text(_)
                    | Datum::Scalar {
                        go_type: _,
                        content: _,
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
    pub(super) fn test(&self, value: &Value) -> Rc<FrozenTest> {
        self.test_with_unknown(value, &mut None)
    }

    pub(super) fn test_with_unknown(
        &self,
        value: &Value,
        unknown: &mut Option<Rc<FrozenTest>>,
    ) -> Rc<FrozenTest> {
        let mut budget = TruthBudget {
            visits: 255,
            leaves: 128,
            unknown,
        };
        let value = self.consumed(value);
        truth_test(self, &value, &mut budget)
    }
    pub(super) fn map_target(&mut self, value: &Value) -> Option<super::ObjectId> {
        let value = self.consumed(value);
        match value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum: Datum::Map { target, view: _ },
                origin: _,
            }) => target.clone(),
            ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(super::UnknownValue { types, aliases: _ }),
                origin:
                    super::Origin {
                        derivation: _,
                        identity: Some(path),
                        influences: _,
                    },
            }) if matches!(types, super::TypeCoverage::Map(_))
                || matches!(types, super::TypeCoverage::Unresolved)
                    && path.segments().next().is_none() =>
            {
                let existing = self
                    .heap
                    .arena
                    .input_objects
                    .borrow()
                    .get(path)
                    .map(|backing| backing.object.clone());
                if let Some(id) = existing {
                    if !self.heap.version.has_allocated_object(&id) {
                        self.heap.install(
                            id.clone(),
                            super::ObjectVersion {
                                revision: self.heap.arena.snapshot(),
                                multiplicity: super::Multiplicity::Singleton,
                                data: ObjectData::Map {
                                    fields: Rc::new(BTreeMap::default()),
                                    remainder: super::Remainder::Input,
                                },
                            },
                        );
                    }
                    return Some(id);
                }
                let id = self.heap.allocate(
                    ObjectData::Map {
                        fields: Rc::new(BTreeMap::default()),
                        remainder: super::Remainder::Input,
                    },
                    super::Multiplicity::Singleton,
                );
                self.heap.arena.input_objects.borrow_mut().insert(
                    path.clone(),
                    super::InputBacking {
                        object: id.clone(),
                        map_type: {
                            let object = if path.segments().next().is_none() {
                                Predicate::True
                            } else {
                                Predicate::from(Guard::TypeIs {
                                    path: path.clone(),
                                    schema_type: "object".to_owned(),
                                })
                            };
                            self.heap.arena.decision(
                                object.clone(),
                                object.negated(),
                                Coverage::Exact,
                            )
                        },
                    },
                );
                self.heap.register_input(id.clone(), path.clone());
                Some(id)
            }
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Text(_)
                    | Datum::Scalar {
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
            })
            | ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => None,
        }
    }
}
struct TruthBudget<'a> {
    visits: usize,
    leaves: usize,
    unknown: &'a mut Option<Rc<FrozenTest>>,
}

impl TruthBudget<'_> {
    fn unknown(&mut self, state: &State) -> Rc<FrozenTest> {
        // Only one leaf supplies this query's Boolean result. Fresh fallback choices have no
        // other observers, so one independent bit preserves every (existing decisions, result) pair.
        Rc::clone(self.unknown.get_or_insert_with(|| {
            state
                .heap
                .arena
                .decision(Predicate::False, Predicate::False, Coverage::Partial)
        }))
    }
}

// The caller specializes the whole tree once; residual captures are unknown leaves.
fn truth_test(state: &State, value: &Value, budget: &mut TruthBudget<'_>) -> Rc<FrozenTest> {
    if budget.visits == 0 {
        return budget.unknown(state);
    }
    budget.visits -= 1;
    if matches!(
        value.node.as_ref(),
        ValueNode::Leaf(Leaf {
            datum: Datum::Text(_),
            origin: _
        })
    ) && let Some(test) = super::text_actions::truth_test(value, state)
    {
        return test;
    }
    if let Some(super::relationship::Derivation {
        source: _,
        operation: super::relationship::ValueOperation::Capability { guard: _, decision },
    }) = value.operation_preimage()
    {
        return Rc::clone(decision);
    }
    if let ValueNode::Leaf(Leaf {
        datum: Datum::Map {
            target: Some(id),
            view: _,
        },
        origin: _,
    }) = value.node.as_ref()
        && let Some(path) = state.input_map_path(&state.heap.freeze(value), id, false)
    {
        if budget.leaves == 0 {
            return budget.unknown(state);
        }
        budget.leaves -= 1;
        let condition = Predicate::from(Guard::Truthy { path });
        return state
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    }
    if let Some(length) = state.map_length_of_consumed(value) {
        return truth_test(state, &length, budget);
    }
    match value.node.as_ref() {
        ValueNode::Select { decision, yes, no } => {
            let yes = truth_test(state, yes, budget);
            let no = truth_test(state, no, budget);
            state.heap.arena.choose_test(Rc::clone(decision), yes, no)
        }
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        }) => {
            if budget.leaves == 0 {
                return budget.unknown(state);
            }
            budget.leaves -= 1;
            let condition = if let Some(truth) = state.truth_of_consumed(value) {
                if truth {
                    Predicate::True
                } else {
                    Predicate::False
                }
            } else if let Some(path) = value.identity() {
                Predicate::from(Guard::Truthy { path: path.clone() })
            } else {
                return budget.unknown(state);
            };
            state
                .heap
                .arena
                .decision(condition.clone(), condition.negated(), Coverage::Exact)
        }
    }
}

fn map_object_id(value: &Value) -> Option<&super::ObjectId> {
    let ValueNode::Leaf(Leaf {
        datum: Datum::Map { target, view: _ },
        origin: _,
    }) = value.node.as_ref()
    else {
        return None;
    };
    target.as_ref()
}

fn field_needs_unknown(field: &Field) -> bool {
    match field {
        Field::Unknown => true,
        Field::Absent | Field::Present(_) => false,
        Field::Select {
            decision: _,
            yes,
            no,
        } => field_needs_unknown(yes) || field_needs_unknown(no),
    }
}

fn field_value(field: Field, unknown: &Value, absent: &Value) -> Value {
    match field {
        Field::Present(value) => value,
        Field::Absent => absent.clone(),
        Field::Unknown => unknown.clone(),
        Field::Select { decision, yes, no } => Value::select(
            decision,
            field_value(yes.as_ref().clone(), unknown, absent),
            field_value(no.as_ref().clone(), unknown, absent),
        ),
    }
}
impl FrozenValue {
    pub(crate) fn literal(&self) -> Option<&GuardValue> {
        self.value.constant()
    }
    pub(crate) fn identity(&self) -> Option<&ValuesPath> {
        let mut remaining = super::MAX_SCALAR_DISPATCH_STATES;
        selector_source_identity(&self.value, &mut remaining)
    }
    pub(crate) fn inputs(&self) -> BTreeSet<ValuesPath> {
        self.collect_inputs(false)
    }

    pub(super) fn reachable_inputs(&self) -> BTreeSet<ValuesPath> {
        self.collect_inputs(true)
    }

    fn collect_inputs(&self, include_unknown_aliases: bool) -> BTreeSet<ValuesPath> {
        self.collect_inputs_with_budget(
            include_unknown_aliases,
            &mut super::heap_query::QueryBudget::new(65_536),
        )
    }

    pub(super) fn collect_inputs_with_budget(
        &self,
        include_unknown_aliases: bool,
        budget: &mut super::heap_query::QueryBudget,
    ) -> BTreeSet<ValuesPath> {
        if let ValueNode::Leaf(Leaf { datum, origin }) = self.value.node.as_ref()
            && (matches!(
                datum,
                Datum::Text(_)
                    | Datum::Scalar {
                        go_type: _,
                        content: _
                    }
                    | Datum::Map {
                        target: None,
                        view: _
                    }
                    | Datum::Sequence(super::InterfaceSequence {
                        storage: None,
                        start: _,
                        length: _,
                        capacity: _
                    })
            ) || !include_unknown_aliases && matches!(datum, Datum::Unknown(_)))
        {
            if !budget.charge(1)
                || !budget.charge(origin.influences.len() + usize::from(origin.identity.is_some()))
                || budget.remaining == 0
            {
                budget.exhaust();
                return BTreeSet::from([ValuesPath::default()]);
            }
            let mut inputs = origin.influences.clone();
            inputs.extend(origin.identity.iter().cloned());
            return inputs;
        }
        let cacheable = matches!(
            self.value.node.as_ref(),
            ValueNode::Leaf(Leaf {
                datum: Datum::Map {
                    target: Some(_),
                    view: _
                } | Datum::Strings(_)
                    | Datum::Sequence(_),
                origin: _,
            })
        );
        let arena = if cacheable {
            self.heap.arena.upgrade()
        } else {
            None
        };
        let key = super::heap_cache::InputQueryKey {
            version: self.heap.id.ordinal,
            value: Rc::as_ptr(&self.value.node),
            include_unknown_aliases,
        };
        if let Some(arena) = &arena
            && let Some(entry) = arena.input_queries.borrow_mut().get(&key)
        {
            return if budget.charge(entry.inputs.len() + 1) {
                entry.inputs.clone()
            } else {
                BTreeSet::from([ValuesPath::default()])
            };
        }
        let inputs = Self::collect_seeded_inputs(
            &self.heap,
            InputRoot::Value(&self.value),
            include_unknown_aliases,
            budget,
        );
        if !budget.is_exhausted()
            && let Some(arena) = arena
        {
            arena
                .input_queries
                .borrow_mut()
                .insert(key, &self.value.node, &inputs);
        }
        inputs
    }

    pub(super) fn object_inputs(
        heap: &Rc<super::HeapVersion>,
        object: &super::ObjectId,
        storage: &Rc<super::ObjectVersion>,
        budget: &mut super::heap_query::QueryBudget,
    ) -> BTreeSet<ValuesPath> {
        let terminal = match &storage.data {
            ObjectData::Map { fields, remainder } if fields.is_empty() => match remainder {
                super::Remainder::Unknown { inputs } => Some((Some(inputs), 2)),
                super::Remainder::Closed | super::Remainder::Input | super::Remainder::Open => {
                    Some((None, 2))
                }
            },
            ObjectData::Strings(slots) if slots.is_empty() => Some((None, 2)),
            ObjectData::Opaque {
                reachable: super::AliasSet::Known(objects),
            } if objects.is_empty() => Some((None, 2)),
            ObjectData::Widened { inputs } => Some((Some(inputs), inputs.len().saturating_add(2))),
            ObjectData::Map {
                fields: _,
                remainder: _,
            }
            | ObjectData::Strings(_)
            | ObjectData::Opaque { reachable: _ } => None,
        };
        if let Some((inputs, work)) = terminal {
            if !budget.charge(work) || budget.remaining == 0 {
                budget.exhaust();
                return BTreeSet::from([ValuesPath::default()]);
            }
            return inputs.map_or_else(BTreeSet::new, |inputs| inputs.as_ref().clone());
        }
        Self::collect_seeded_inputs(
            heap,
            InputRoot::Object(KnownInputObject {
                object: object.clone(),
                storage: Rc::clone(storage),
            }),
            true,
            budget,
        )
    }

    fn collect_seeded_inputs(
        heap: &super::HeapVersion,
        root: InputRoot<'_>,
        include_unknown_aliases: bool,
        budget: &mut super::heap_query::QueryBudget,
    ) -> BTreeSet<ValuesPath> {
        let mut inputs = BTreeSet::new();
        let mut values = Vec::new();
        let mut objects = Vec::new();
        let mut known = match root {
            InputRoot::Value(value) => {
                values.push(value.clone());
                None
            }
            InputRoot::Object(known) => {
                // Preserve the synthetic value-root charge without allocating a value.
                if !budget.charge(1) {
                    return BTreeSet::from([ValuesPath::default()]);
                }
                objects.push(known.object.clone());
                Some(known)
            }
        };
        let mut seen_objects = AHashSet::new();
        let mut seen_values = AHashSet::new();
        let mut seen_input_sets = AHashSet::new();
        let mut expanded_render_graph = false;
        while !values.is_empty() || !objects.is_empty() {
            while let Some(value) = values.pop() {
                if !seen_values.insert(Rc::as_ptr(&value.node)) {
                    continue;
                }
                if !budget.charge(1) {
                    return BTreeSet::from([ValuesPath::default()]);
                }
                if let ValueNode::Leaf(Leaf { datum: _, origin }) = value.node.as_ref() {
                    if !budget
                        .charge(origin.influences.len() + usize::from(origin.identity.is_some()))
                    {
                        return BTreeSet::from([ValuesPath::default()]);
                    }
                    inputs.extend(origin.influences.iter().cloned());
                    inputs.extend(origin.identity.iter().cloned());
                }
                collect_value_children(
                    &value,
                    heap,
                    include_unknown_aliases,
                    &mut values,
                    &mut objects,
                    &mut expanded_render_graph,
                    budget,
                );
            }
            while let Some(id) = objects.pop() {
                if !seen_objects.insert(id.ordinal) {
                    continue;
                }
                if !budget.charge(1) {
                    return BTreeSet::from([ValuesPath::default()]);
                }
                let storage = if known.as_ref().is_some_and(|known| known.object == id) {
                    known.take().map(|known| known.storage)
                } else {
                    heap.query_object(&id, budget)
                };
                let Some(object) = storage else {
                    continue;
                };
                match &object.data {
                    ObjectData::Map { fields, remainder } => {
                        if let super::Remainder::Unknown { inputs: known } = remainder
                            && seen_input_sets.insert(Rc::as_ptr(known))
                        {
                            inputs.extend(known.iter().cloned());
                        }
                        for field in fields.values() {
                            if !field_inputs_with_budget(field, &mut values, budget) {
                                return BTreeSet::from([ValuesPath::default()]);
                            }
                        }
                    }
                    ObjectData::Strings(slots) => values.extend(slots.iter().cloned()),
                    ObjectData::Widened { inputs: known } => {
                        if seen_input_sets.insert(Rc::as_ptr(known)) {
                            if !budget.charge(known.len()) {
                                return BTreeSet::from([ValuesPath::default()]);
                            }
                            inputs.extend(known.iter().cloned());
                        }
                    }
                    ObjectData::Opaque { reachable } => extend_aliases(
                        reachable,
                        heap,
                        &mut objects,
                        &mut expanded_render_graph,
                        budget,
                    ),
                }
            }
        }
        if budget.remaining == 0 {
            budget.exhaust();
            return BTreeSet::from([ValuesPath::default()]);
        }
        inputs
    }
}

impl FrozenValue {
    pub(crate) fn alternatives(&self) -> Vec<super::ValueAlternative> {
        let mut result = Vec::new();
        alternative_views(self, Predicate::True, true, &mut result);
        result
    }
    pub(crate) fn go_type(&self) -> super::kinds::GoType {
        let mut remaining = super::MAX_SCALAR_DISPATCH_STATES;
        runtime_type(&self.value, &mut remaining)
    }
}

fn runtime_type(value: &Value, remaining: &mut usize) -> super::kinds::GoType {
    use super::kinds::GoType;
    if *remaining == 0 {
        return GoType::Unknown;
    }
    *remaining -= 1;
    match value.node.as_ref() {
        ValueNode::Select {
            decision: _,
            yes,
            no,
        } => runtime_type(yes, remaining).joined(runtime_type(no, remaining)),
        ValueNode::Leaf(Leaf { datum, origin: _ }) => match datum {
            Datum::Scalar {
                go_type,
                content: _,
            } => match go_type {
                ScalarType::Invalid | ScalarType::Nil => GoType::Nil,
                ScalarType::Bool => GoType::Bool,
                ScalarType::String => GoType::String,
                ScalarType::Int => GoType::Int,
                ScalarType::Int64 => GoType::Int64,
                ScalarType::Uint64 => GoType::Uint64,
                ScalarType::Float64 => GoType::Float64,
                ScalarType::Uint8 | ScalarType::Complex128 => GoType::Unknown,
            },
            Datum::Map { view, target: _ } => super::hosts::go_type(*view),
            Datum::Sequence(_) | Datum::Strings(_) => GoType::Slice,
            Datum::Text(_)
            | Datum::Slot {
                backing: _,
                index: _,
            } => GoType::String,
            Datum::Unknown(super::UnknownValue { types, aliases: _ }) => match types {
                super::TypeCoverage::Map(view) => super::hosts::go_type(*view),
                super::TypeCoverage::Sequence => GoType::Slice,
                super::TypeCoverage::Scalar(kind) => kind.query_type(),
                super::TypeCoverage::Unresolved | super::TypeCoverage::InputNonMap => {
                    GoType::Unknown
                }
            },
        },
    }
}
impl ScalarType {
    fn query_type(self) -> super::kinds::GoType {
        use super::kinds::GoType;
        match self {
            Self::Invalid | Self::Nil => GoType::Nil,
            Self::Bool => GoType::Bool,
            Self::String => GoType::String,
            Self::Int => GoType::Int,
            Self::Int64 => GoType::Int64,
            Self::Uint64 => GoType::Uint64,
            Self::Float64 => GoType::Float64,
            Self::Uint8 | Self::Complex128 => GoType::Unknown,
        }
    }
}
impl super::ValueAlternative {
    pub(crate) fn condition(&self) -> &Predicate {
        &self.condition
    }
    pub(crate) fn value(&self) -> &FrozenValue {
        &self.value
    }
    pub(crate) fn coverage_exact(&self) -> bool {
        self.coverage_exact
    }
}
// A skipped nil ancestor and its absent descendant have the same source content.
// Consuming operations still see the invalid entry in the value DAG.
fn selector_source_identity<'a>(value: &'a Value, remaining: &mut usize) -> Option<&'a ValuesPath> {
    if *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf { datum: _, origin }) => origin.identity.as_ref(),
        ValueNode::Select { decision, yes, no } => {
            if decision.coverage != Coverage::Exact
                || decision.when_true.contains_approximation()
                || decision.when_false.contains_approximation()
                || !no.is_invalid()
            {
                return None;
            }
            let path = selector_source_identity(yes, remaining)?;
            let present = ancestor_nil_domain(&decision.when_true, path, remaining)?;
            (present.normalize_boolean() == Predicate::True
                && decision.when_false.clone().normalize_boolean()
                    == decision.when_true.negated().normalize_boolean())
            .then_some(path)
        }
    }
}

/// Projects ancestor nil tests assuming the descendant exists, or abstains at the query bound.
pub(super) fn without_ancestor_nil(predicate: &Predicate, path: &ValuesPath) -> Option<Predicate> {
    let mut remaining = super::MAX_SCALAR_DISPATCH_STATES;
    ancestor_nil_domain(predicate, path, &mut remaining).map(Predicate::normalize_boolean)
}

fn ancestor_nil_domain(
    predicate: &Predicate,
    path: &ValuesPath,
    remaining: &mut usize,
) -> Option<Predicate> {
    use helm_schema_core::PredicateKind;
    if *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    Some(match predicate.kind() {
        PredicateKind::Guard(
            Guard::Absent { path: ancestor }
            | Guard::Eq {
                path: ancestor,
                value: GuardValue::Null,
            },
        ) if ancestor.segments().len() < path.segments().len()
            && ancestor
                .segments()
                .zip(path.segments())
                .all(|(left, right)| left == right) =>
        {
            Predicate::False
        }
        PredicateKind::And(parts) => Predicate::all(
            parts
                .iter()
                .map(|part| ancestor_nil_domain(part, path, remaining))
                .collect::<Option<Vec<_>>>()?,
        ),
        PredicateKind::Or(parts) => Predicate::Or(
            parts
                .iter()
                .map(|part| ancestor_nil_domain(part, path, remaining))
                .collect::<Option<Vec<_>>>()?,
        ),
        PredicateKind::Not(inner) => ancestor_nil_domain(inner, path, remaining)?.negated(),
        PredicateKind::True
        | PredicateKind::False
        | PredicateKind::Guard(_)
        | PredicateKind::Approximate {
            marker: _,
            paths: _,
            role: _,
            sound_subset: _,
        } => predicate.clone(),
    })
}

fn alternative_views(
    value: &FrozenValue,
    condition: Predicate,
    coverage_exact: bool,
    result: &mut Vec<super::ValueAlternative>,
) {
    if matches!(
        value.value.node.as_ref(),
        ValueNode::Select {
            decision: _,
            yes: _,
            no: _
        }
    ) && value.identity().is_some()
    {
        result.push(super::ValueAlternative {
            condition,
            value: value.clone(),
            coverage_exact,
        });
        return;
    }
    match value.value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        }) => result.push(super::ValueAlternative {
            condition,
            value: value.clone(),
            coverage_exact,
        }),
        ValueNode::Select { decision, yes, no } => {
            for (taken, arm) in [(true, yes), (false, no)] {
                let predicate = if decision.coverage != Coverage::Exact {
                    Predicate::True
                } else if taken {
                    decision.when_true.clone()
                } else {
                    decision.when_false.clone()
                };
                let condition =
                    Predicate::all(vec![condition.clone(), predicate]).normalize_boolean();
                if condition == Predicate::False {
                    continue;
                }
                alternative_views(
                    &FrozenValue::at(Rc::clone(&value.heap), arm.clone()),
                    condition,
                    coverage_exact && decision.coverage == Coverage::Exact,
                    result,
                );
            }
        }
    }
}

impl State {
    pub(super) fn lookup_field(&self, value: &Value, key: &str) -> Option<Field> {
        let value = self.consumed(value);
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => Some(Field::select(
                Rc::clone(decision),
                self.lookup_field(yes, key).unwrap_or(Field::Unknown),
                self.lookup_field(no, key).unwrap_or(Field::Unknown),
            )),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        target: Some(id),
                        view: _,
                    },
                origin: _,
            }) => Some(self.consumed_field(&self.heap.version.field(id, key))),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        target: None,
                        view: _,
                    },
                origin: _,
            }) => Some(Field::Absent),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Text(_)
                    | Datum::Scalar {
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
            }) => None,
        }
    }
    pub(super) fn get_field(&mut self, value: &Value, key: &str) -> Value {
        let value = self.consumed(value);
        let stored = matches!(value.node.as_ref(), ValueNode::Leaf(_))
            .then(|| self.lookup_field(&value, key))
            .flatten();
        if let Some(field) = &stored
            && !field_needs_unknown(field)
        {
            if key.is_empty()
                && matches!(
                    value.node.as_ref(),
                    ValueNode::Leaf(
                        Leaf {
                            datum: Datum::Map {
                                target: Some(_),
                                view: _
                            },
                            origin: _,
                        } | Leaf {
                            datum: _,
                            origin: super::Origin {
                                derivation: _,
                                identity: Some(_),
                                influences: _
                            }
                        }
                    )
                )
            {
                self.record_container_read(&value);
            }
            let result = self.consumed(&field_value(
                field.clone(),
                &Value::unknown(),
                &Value::string(""),
            ));
            self.record_value_read(&result, super::ReadCoverage::ExactPath);
            return result;
        }
        let result = match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => Value::select(
                Rc::clone(decision),
                self.get_field(yes, key),
                self.get_field(no, key),
            ),
            ValueNode::Leaf(Leaf { datum, origin }) => {
                let unknown = if let Some(path) = &origin.identity {
                    let id = self
                        .heap
                        .arena
                        .input_objects
                        .borrow()
                        .get(path)
                        .map(|backing| backing.object.clone());
                    let child = self.unknown_child_value(id.as_ref(), Some(path), key);
                    {
                        let missing =
                            Value::scalar(ScalarType::String, None).with_influences(child.inputs());
                        let present = self.field_presence(&value, key);
                        Value::select(self.test(&present), child, missing)
                    }
                } else if let Datum::Map {
                    target: Some(id),
                    view: _,
                } = datum
                {
                    let path = self.heap.arena.input_paths.borrow().get(id).cloned();
                    let child = self.unknown_child_value(Some(id), path.as_ref(), key);
                    let mut child = child;
                    if path.is_none() {
                        let mut inputs = child.inputs();
                        inputs.extend(value.inputs());
                        child = child.with_influences(inputs);
                    }
                    {
                        let missing =
                            Value::scalar(ScalarType::String, None).with_influences(child.inputs());
                        let present = self.field_presence(&value, key);
                        Value::select(self.test(&present), child, missing)
                    }
                } else {
                    Value::influenced(self.heap.freeze(&value).inputs())
                };
                stored.map_or(unknown.clone(), |field| {
                    self.consumed(&field_value(field, &unknown, &Value::string("")))
                })
            }
        };
        self.record_value_read(&result, super::ReadCoverage::ExactPath);
        result
    }

    pub(super) fn field_presence(&mut self, value: &Value, key: &str) -> Value {
        self.record_field_read(value, key);
        let value = self.consumed(value);
        if let ValueNode::Select { decision, yes, no } = value.node.as_ref() {
            return Value::select(
                Rc::clone(decision),
                self.field_presence(yes, key),
                self.field_presence(no, key),
            );
        }
        let field = self.lookup_field(&value, key);
        let path = match &field {
            Some(Field::Select { .. }) => self.untouched_map_child_identity(&value, key),
            Some(Field::Unknown) | None => self.untouched_child_identity(&value, key),
            Some(Field::Present(_)) => return Value::literal(GuardValue::Bool(true)),
            Some(Field::Absent) => return Value::literal(GuardValue::Bool(false)),
        };
        let unknown = if let Some(path) = path.and_then(|path| path.parent()) {
            let predicate = Predicate::from(Guard::HasKey {
                path,
                key: key.to_owned(),
            });
            let decision =
                self.heap
                    .arena
                    .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
            Value::select(
                decision,
                Value::literal(GuardValue::Bool(true)),
                Value::literal(GuardValue::Bool(false)),
            )
        } else {
            Value::influenced(self.heap.freeze(&value).inputs())
        };
        field.map_or(unknown.clone(), |field| {
            self.consumed(&field_presence_value(field, &unknown))
        })
    }

    pub(super) fn untouched_child_identity(&self, value: &Value, key: &str) -> Option<ValuesPath> {
        if matches!(
            self.lookup_field(value, key),
            Some(Field::Present(_) | Field::Absent | Field::Select { .. })
        ) {
            return None;
        }
        self.untouched_map_child_identity(value, key)
    }

    fn untouched_map_child_identity(&self, value: &Value, key: &str) -> Option<ValuesPath> {
        let value = self.consumed(value);
        if matches!(
            value.node.as_ref(),
            ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(super::UnknownValue {
                    types: super::TypeCoverage::InputNonMap,
                    aliases: _,
                }) | Datum::Text(_)
                    | Datum::Scalar {
                        go_type: _,
                        content: _
                    },
                origin: _,
            })
        ) {
            return None;
        }
        if key.is_empty() {
            return None;
        }
        if let Some(path) = value.identity() {
            let id = self
                .heap
                .arena
                .input_objects
                .borrow()
                .get(path)
                .map(|backing| backing.object.clone());
            if let Some(object) = id.as_ref().and_then(|id| self.heap.version.object(id))
                && !matches!(
                    object.data,
                    ObjectData::Map {
                        fields: _,
                        remainder: super::Remainder::Input,
                    }
                )
            {
                return None;
            }
            let mut child = path.clone();
            child.push(key);
            return Some(child);
        }
        let ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    target: Some(id),
                    view: _,
                },
            origin: _,
        }) = value.node.as_ref()
        else {
            return None;
        };
        if !matches!(
            self.heap
                .version
                .object(id)
                .as_deref()
                .map(|object| &object.data),
            Some(ObjectData::Map {
                remainder: super::Remainder::Input,
                ..
            })
        ) {
            return None;
        }
        let mut path = self.heap.arena.input_paths.borrow().get(id)?.clone();
        path.push(key);
        Some(path)
    }
}

fn field_presence_value(field: Field, unknown: &Value) -> Value {
    match field {
        Field::Present(_) => Value::literal(GuardValue::Bool(true)),
        Field::Absent => Value::literal(GuardValue::Bool(false)),
        Field::Unknown => unknown.clone(),
        Field::Select { decision, yes, no } => Value::select(
            decision,
            field_presence_value(yes.as_ref().clone(), unknown),
            field_presence_value(no.as_ref().clone(), unknown),
        ),
    }
}

fn extend_aliases(
    aliases: &super::AliasSet,
    heap: &super::HeapVersion,
    objects: &mut Vec<super::ObjectId>,
    expanded_render_graph: &mut bool,
    budget: &mut super::heap_query::QueryBudget,
) {
    match aliases {
        super::AliasSet::Known(ids) => objects.extend(ids.iter().cloned()),
        super::AliasSet::RenderGraph => {
            if !*expanded_render_graph {
                *expanded_render_graph = true;
                if let Some(ids) = heap.query_object_ids(budget) {
                    objects.extend(ids);
                } else {
                    budget.exhaust();
                }
            }
        }
    }
}
fn field_inputs_with_budget(
    field: &Field,
    values: &mut Vec<Value>,
    budget: &mut super::heap_query::QueryBudget,
) -> bool {
    match field {
        Field::Present(value) => {
            if !budget.charge(1) {
                return false;
            }
            values.push(value.clone());
            return true;
        }
        Field::Absent | Field::Unknown => return budget.charge(1),
        Field::Select {
            decision: _,
            yes: _,
            no: _,
        } => {}
    }
    let mut pending = vec![field];
    let mut seen = AHashSet::new();
    while let Some(field) = pending.pop() {
        if !seen.insert(std::ptr::from_ref(field)) {
            continue;
        }
        if !budget.charge(1) {
            return false;
        }
        match field {
            Field::Present(value) => values.push(value.clone()),
            Field::Select {
                decision: _,
                yes,
                no,
            } => pending.extend([yes.as_ref(), no.as_ref()]),
            Field::Unknown | Field::Absent => {}
        }
    }
    true
}

impl State {
    pub(super) fn consumed_field(&self, field: &Field) -> Field {
        specialize_field(field, &self.heap.arena.runtime_path(&self.active))
    }

    pub(super) fn map_length(&self, value: &Value) -> Option<Value> {
        let value = self.consumed(value);
        self.map_length_of_consumed(&value)
    }

    fn map_length_of_consumed(&self, value: &Value) -> Option<Value> {
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => Some(Value::select(
                Rc::clone(decision),
                self.map_length_of_consumed(yes)?,
                self.map_length_of_consumed(no)?,
            )),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        target: None,
                        view: _,
                    },
                origin: _,
            }) => Some(Value::literal(GuardValue::Int(0))),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        target: Some(id),
                        view: _,
                    },
                origin: _,
            }) => {
                let storage = self.heap.version.object(id)?;
                let ObjectData::Map {
                    fields,
                    remainder: super::Remainder::Closed,
                } = &storage.data
                else {
                    return None;
                };
                let mut length = Value::literal(GuardValue::Int(0));
                for field in fields.values() {
                    let presence = field_length(&self.consumed_field(field));
                    length = add_lengths(&length, &presence, &mut 255);
                }
                Some(length)
            }
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Text(_)
                    | Datum::Scalar {
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
            }) => None,
        }
    }
}

fn specialize_field(field: &Field, guard: &super::guard::RuntimeGuard) -> Field {
    match field {
        Field::Absent => Field::Absent,
        Field::Unknown => Field::Unknown,
        Field::Present(value) => Field::Present(value.clone()),
        Field::Select { decision, yes, no } => match guard.implies_test(decision) {
            Some(true) => specialize_field(yes, guard),
            Some(false) => specialize_field(no, guard),
            None => Field::select(
                Rc::clone(decision),
                specialize_field(yes, guard),
                specialize_field(no, guard),
            ),
        },
    }
}

fn field_length(field: &Field) -> Value {
    match field {
        Field::Absent => Value::literal(GuardValue::Int(0)),
        Field::Present(_) => Value::literal(GuardValue::Int(1)),
        Field::Unknown => Value::unknown(),
        Field::Select { decision, yes, no } => {
            Value::select(Rc::clone(decision), field_length(yes), field_length(no))
        }
    }
}

pub(super) fn add_lengths(left: &Value, right: &Value, remaining: &mut usize) -> Value {
    if *remaining == 0 {
        return Value::unknown();
    }
    *remaining -= 1;
    match (left.node.as_ref(), right.node.as_ref()) {
        (ValueNode::Select { decision, yes, no }, _) => Value::select(
            Rc::clone(decision),
            add_lengths(yes, &right.under(decision, true), remaining),
            add_lengths(no, &right.under(decision, false), remaining),
        ),
        (_, ValueNode::Select { decision, yes, no }) => Value::select(
            Rc::clone(decision),
            add_lengths(left, yes, remaining),
            add_lengths(left, no, remaining),
        ),
        (ValueNode::Leaf(_), ValueNode::Leaf(_)) => match (left.constant(), right.constant()) {
            (Some(GuardValue::Int(left)), Some(GuardValue::Int(right))) => left
                .checked_add(*right)
                .map_or_else(Value::unknown, |length| {
                    Value::literal(GuardValue::Int(length))
                }),
            _ => Value::unknown(),
        },
    }
}

fn collect_value_children(
    value: &Value,
    heap: &super::HeapVersion,
    include_unknown_aliases: bool,
    values: &mut Vec<Value>,
    objects: &mut Vec<super::ObjectId>,
    expanded_render_graph: &mut bool,
    budget: &mut super::heap_query::QueryBudget,
) {
    match value.node.as_ref() {
        ValueNode::Select {
            decision: _,
            yes,
            no,
        } => {
            values.push(yes.clone());
            values.push(no.clone());
        }
        ValueNode::Leaf(Leaf { datum, origin: _ }) => match datum {
            Datum::Map {
                target: Some(id),
                view: _,
            }
            | Datum::Strings(super::StringSlice {
                backing: id,
                start: _,
                length: _,
                capacity: _,
            })
            | Datum::Slot {
                backing: id,
                index: _,
            } => objects.push(id.clone()),
            Datum::Sequence(super::InterfaceSequence {
                storage,
                start: _,
                length: _,
                capacity: _,
            }) => {
                if let Some(items) = storage {
                    values.extend(items.iter().cloned());
                }
            }
            Datum::Unknown(super::UnknownValue { types: _, aliases }) => {
                // Mutation reachability does not describe the value's input provenance.
                if include_unknown_aliases {
                    extend_aliases(aliases, heap, objects, expanded_render_graph, budget);
                }
            }
            Datum::Text(_)
            | Datum::Scalar {
                go_type: _,
                content: _,
            }
            | Datum::Map {
                target: None,
                view: _,
            } => {}
        },
    }
}

fn known_nonmap_receiver(datum: &Datum) -> bool {
    matches!(
        datum,
        Datum::Scalar {
            go_type: _,
            content: _
        } | Datum::Text(_)
            | Datum::Sequence(_)
            | Datum::Strings(_)
            | Datum::Slot {
                backing: _,
                index: _
            }
            | Datum::Unknown(super::UnknownValue {
                types: super::TypeCoverage::InputNonMap
                    | super::TypeCoverage::Scalar(_)
                    | super::TypeCoverage::Sequence,
                aliases: _,
            })
    )
}

fn evaluated_member_entry(state: &State, value: &Value, remaining: &mut usize) -> Value {
    if *remaining == 0 {
        return Value::influenced(state.heap.freeze(value).inputs());
    }
    *remaining -= 1;
    match value.node.as_ref() {
        ValueNode::Select { decision, yes, no } => Value::select(
            Rc::clone(decision),
            evaluated_member_entry(state, yes, remaining),
            evaluated_member_entry(state, no, remaining),
        ),
        ValueNode::Leaf(Leaf { datum: _, origin }) => {
            let Some(path) = origin
                .identity
                .as_ref()
                .filter(|path| path.item_parent().is_some())
            else {
                return value.clone();
            };
            let nil = Predicate::from(Guard::Absent { path: path.clone() });
            let decision = state
                .heap
                .arena
                .decision(nil.clone(), nil.negated(), Coverage::Exact);
            Value::select(decision, Value::invalid(), value.clone())
        }
    }
}

fn selector_entry_domain(value: &Value, remaining: &mut usize) -> Option<Predicate> {
    if *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    match value.node.as_ref() {
        ValueNode::Select { decision, yes, no } => {
            if decision.coverage != Coverage::Exact
                || decision.when_true.contains_approximation()
                || decision.when_false.contains_approximation()
            {
                return None;
            }
            let yes = selector_entry_domain(yes, remaining)?;
            let no = selector_entry_domain(no, remaining)?;
            Some(
                Predicate::Or(vec![
                    Predicate::all(vec![decision.when_true.clone(), yes]),
                    Predicate::all(vec![decision.when_false.clone(), no]),
                ])
                .normalize_boolean(),
            )
        }
        ValueNode::Leaf(Leaf { datum, origin }) => {
            if let Some(path) = &origin.identity {
                if path.segments().next().is_none() || path.item_parent().is_some() {
                    return Some(Predicate::True);
                }
                if path.segments().any(|segment| segment.literal().is_none()) {
                    return None;
                }
                return Some(super::kinds::input_nil_predicate(path).negated());
            }
            // Typed nil maps still perform map lookup; only an invalid entry skips the chain.
            if matches!(
                datum,
                Datum::Map { view: _, target: _ }
                    | Datum::Unknown(super::UnknownValue {
                        types: super::TypeCoverage::Map(_),
                        aliases: _,
                    })
            ) {
                return Some(Predicate::True);
            }
            super::kinds::nilness(value).map(|nil| {
                if nil {
                    Predicate::False
                } else {
                    Predicate::True
                }
            })
        }
    }
}
