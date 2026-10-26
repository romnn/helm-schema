use super::scalar_value::ScalarContent;
use super::{
    Coverage, Datum, Field, FrozenTest, FrozenValue, Leaf, ObjectData, ScalarType, State, Value,
    ValueNode,
};
use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

impl Value {
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
    pub(super) fn input(path: ValuesPath) -> Self {
        Self {
            node: Rc::new(ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(super::UnknownValue {
                    types: super::TypeCoverage::Unresolved,
                    aliases: super::AliasSet::RenderGraph,
                }),
                origin: super::Origin {
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
                    | ScalarContent::Rendered(_)
                    | ScalarContent::PrintfStringIdentity(_)
                    | ScalarContent::SplitLength {
                        value: _,
                        separator: _,
                    }
                    | ScalarContent::Length(_),
                )
                | None => None,
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
        let mut seen = BTreeSet::new();
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
                        Datum::Scalar {
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
        for path in self.heap.freeze(value).inputs() {
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

    pub(super) fn record_invocation_reads(&mut self, invocation: &super::Invocation) {
        use crate::function_semantics::{InputRead, function_semantics};
        let super::Callee::Function(function) = &invocation.callee;
        for operand in &invocation.operands {
            self.record_value_read(&operand.value, super::ReadCoverage::ExactPath);
        }
        match function_semantics(function).input_read {
            InputRead::Value => {}
            InputRead::Contents => {
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

    pub(super) fn path_predicate(&self) -> Predicate {
        if self.heap.arena.runtime_path(&self.active).is_impossible() {
            return Predicate::False;
        }
        Predicate::all(
            self.active
                .iter()
                .map(|edge| edge.test.possible_predicate(edge.taken))
                .collect(),
        )
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
                let Some(backing) =
                    backing.filter(|backing| self.heap.version.object(&backing.object).is_some())
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

    pub(super) fn selector_live_domain(&self, value: &Value) -> Predicate {
        let value = self.consumed(value);
        let mut remaining = super::MAX_SCALAR_DISPATCH_STATES;
        selector_entry_domain(&value, &mut remaining)
    }

    pub(super) fn record_selector_host(&mut self, value: &Value, live: &Predicate) {
        if let ValueNode::Select { decision, yes, no } = value.node.as_ref() {
            let depth = self.active.len();
            for (taken, arm) in [(true, yes), (false, no)] {
                self.active.push(super::DecisionEdge {
                    test: Rc::clone(decision),
                    taken,
                });
                self.record_selector_host(arm, live);
                self.active.truncate(depth);
            }
            return;
        }
        let Some(path) = value.identity() else {
            return;
        };
        if path.segments().next().is_none()
            || path.segments().any(|segment| segment.literal().is_none())
            || self
                .active
                .iter()
                .any(|edge| edge.test.coverage != Coverage::Exact)
        {
            return;
        }
        // Materialized inputs may already have template writes; derived hosts have no raw identity.
        if self
            .heap
            .arena
            .input_objects
            .borrow()
            .get(path)
            .is_some_and(|backing| self.heap.version.object(&backing.object).is_some())
        {
            return;
        }
        let condition =
            Predicate::all(vec![self.path_predicate(), live.clone()]).normalize_boolean();
        if condition == Predicate::False || condition.contains_approximation() {
            return;
        }
        self.reads.insert(super::RuntimeRead {
            coverage: super::ReadCoverage::MemberHost,
            path: path.clone(),
            condition,
        });
    }

    pub(super) fn selector_field(&mut self, value: &Value, key: &str) -> Value {
        let value = self.consumed(value);
        if let ValueNode::Select { decision, yes, no } = value.node.as_ref() {
            return Value::select(
                Rc::clone(decision),
                self.selector_field(yes, key),
                self.selector_field(no, key),
            );
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
        match value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Unknown(super::UnknownValue {
                        types: super::TypeCoverage::InputNonMap,
                        aliases: _,
                    })
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
                let unknown = self.unknown_child_value(id.as_ref(), Some(path), key);
                id.map_or(unknown.clone(), |id| {
                    self.consumed(&field_value(
                        self.heap.version.field(&id, key),
                        &unknown,
                        &Value::nil(),
                    ))
                })
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
                let unknown = self.unknown_child_value(Some(id), path.as_ref(), key);
                self.consumed(&field_value(
                    self.heap.version.field(id, key),
                    &unknown,
                    &Value::nil(),
                ))
            }
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }) => Value::influenced(value.inputs()),
        }
    }
    fn unknown_child_value(
        &self,
        id: Option<&super::ObjectId>,
        path: Option<&ValuesPath>,
        key: &str,
    ) -> Value {
        if let Some(ObjectData::Widened { inputs }) = id
            .and_then(|id| self.heap.version.object(id))
            .map(|object| &object.data)
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
        let untouched = id
            .and_then(|id| self.heap.version.object(id))
            .is_none_or(|object| {
                matches!(
                    object.data,
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
        if let Some(literal) = value.constant() {
            return Some(match literal {
                GuardValue::Null => false,
                GuardValue::Bool(value) => *value,
                GuardValue::String(value) => !value.is_empty(),
                GuardValue::Int(value) => *value != 0,
                GuardValue::Float(value) => value.parse::<f64>().ok()? != 0.0,
            });
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
                .map_length(&value)
                .and_then(|length| match length.constant() {
                    Some(GuardValue::Int(length)) => Some(*length != 0),
                    _ => None,
                }),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Scalar {
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
        let mut budget = TruthBudget {
            visits: 255,
            leaves: 128,
        };
        truth_test(self, value, &mut budget)
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
                    if self.heap.version.object(&id).is_none() {
                        self.heap.install(
                            id.clone(),
                            super::ObjectVersion {
                                revision: self.heap.arena.snapshot(),
                                multiplicity: super::Multiplicity::Singleton,
                                data: ObjectData::Map {
                                    fields: BTreeMap::default(),
                                    remainder: super::Remainder::Input,
                                },
                            },
                        );
                    }
                    return Some(id);
                }
                let id = self.heap.allocate(
                    ObjectData::Map {
                        fields: BTreeMap::default(),
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
                self.heap
                    .arena
                    .input_paths
                    .borrow_mut()
                    .insert(id.clone(), path.clone());
                Some(id)
            }
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
            })
            | ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => None,
        }
    }
}
struct TruthBudget {
    visits: usize,
    leaves: usize,
}

fn truth_test(state: &State, value: &Value, budget: &mut TruthBudget) -> Rc<FrozenTest> {
    if budget.visits == 0 {
        return state
            .heap
            .arena
            .decision(Predicate::False, Predicate::False, Coverage::Partial);
    }
    budget.visits -= 1;
    let value = state.consumed(value);
    if let Some(length) = state.map_length(&value) {
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
                return state.heap.arena.decision(
                    Predicate::False,
                    Predicate::False,
                    Coverage::Partial,
                );
            }
            budget.leaves -= 1;
            let condition = if let Some(truth) = state.truth(&value) {
                if truth {
                    Predicate::True
                } else {
                    Predicate::False
                }
            } else if let Some(path) = value.identity() {
                Predicate::from(Guard::Truthy { path: path.clone() })
            } else {
                return state.heap.arena.decision(
                    Predicate::False,
                    Predicate::False,
                    Coverage::Partial,
                );
            };
            state
                .heap
                .arena
                .decision(condition.clone(), condition.negated(), Coverage::Exact)
        }
    }
}

fn field_value(field: Field, unknown: &Value, absent: &Value) -> Value {
    match field {
        Field::Present(value) => value,
        Field::Absent => absent.clone(),
        Field::Unknown => unknown.clone(),
        Field::Select { decision, yes, no } => Value::select(
            decision,
            field_value(*yes, unknown, absent),
            field_value(*no, unknown, absent),
        ),
    }
}
impl FrozenValue {
    pub(crate) fn literal(&self) -> Option<&GuardValue> {
        self.value.constant()
    }
    pub(crate) fn identity(&self) -> Option<&ValuesPath> {
        self.value.identity()
    }
    pub(crate) fn inputs(&self) -> BTreeSet<ValuesPath> {
        self.collect_inputs(false)
    }

    pub(super) fn reachable_inputs(&self) -> BTreeSet<ValuesPath> {
        self.collect_inputs(true)
    }

    fn collect_inputs(&self, include_unknown_aliases: bool) -> BTreeSet<ValuesPath> {
        let mut inputs = BTreeSet::new();
        let mut values = vec![self.value.clone()];
        let mut objects = Vec::new();
        let mut seen_objects = BTreeSet::new();
        let mut seen_values = BTreeSet::new();
        let mut seen_input_sets = BTreeSet::new();
        let mut expanded_render_graph = false;
        while !values.is_empty() || !objects.is_empty() {
            while let Some(value) = values.pop() {
                if !seen_values.insert(Rc::as_ptr(&value.node)) {
                    continue;
                }
                if let ValueNode::Leaf(Leaf { datum: _, origin }) = value.node.as_ref() {
                    inputs.extend(origin.influences.iter().cloned());
                    inputs.extend(origin.identity.iter().cloned());
                }
                collect_value_children(
                    &value,
                    &self.heap,
                    include_unknown_aliases,
                    &mut values,
                    &mut objects,
                    &mut expanded_render_graph,
                );
            }
            while let Some(id) = objects.pop() {
                if !seen_objects.insert(id.clone()) {
                    continue;
                }
                let Some(object) = self.heap.object(&id) else {
                    continue;
                };
                match &object.data {
                    ObjectData::Map {
                        fields,
                        remainder: _,
                    } => {
                        for field in fields.values() {
                            field_inputs(field, &mut values);
                        }
                    }
                    ObjectData::Strings(slots) => values.extend(slots.iter().cloned()),
                    ObjectData::Widened { inputs: known } => {
                        if seen_input_sets.insert(Rc::as_ptr(known)) {
                            inputs.extend(known.iter().cloned());
                        }
                    }
                    ObjectData::Opaque { reachable } => extend_aliases(
                        reachable,
                        &self.heap,
                        &mut objects,
                        &mut expanded_render_graph,
                    ),
                }
            }
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
                ScalarType::Nil => GoType::Nil,
                ScalarType::Bool => GoType::Bool,
                ScalarType::String => GoType::String,
                ScalarType::Int => GoType::Int,
                ScalarType::Int64 => GoType::Int64,
                ScalarType::Uint64 => GoType::Uint64,
                ScalarType::Float64 => GoType::Float64,
                ScalarType::Uint8 | ScalarType::Complex128 => GoType::Unknown,
            },
            Datum::Map { view: _, target: _ } => GoType::Map,
            Datum::Sequence(_) | Datum::Strings(_) => GoType::Slice,
            Datum::Slot {
                backing: _,
                index: _,
            } => GoType::String,
            Datum::Unknown(super::UnknownValue { types, aliases: _ }) => match types {
                super::TypeCoverage::Map(_) => GoType::Map,
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
            Self::Nil => GoType::Nil,
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
fn alternative_views(
    value: &FrozenValue,
    condition: Predicate,
    coverage_exact: bool,
    result: &mut Vec<super::ValueAlternative>,
) {
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
                let predicate = if decision.coverage == Coverage::Partial {
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
                    &FrozenValue {
                        heap: Rc::clone(&value.heap),
                        value: arm.clone(),
                    },
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
            }) => None,
        }
    }
    pub(super) fn get_field(&mut self, value: &Value, key: &str) -> Value {
        let value = self.consumed(value);
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
                    Value::influenced(child.inputs())
                } else if let Datum::Map {
                    target: Some(id),
                    view: _,
                } = datum
                {
                    let path = self.heap.arena.input_paths.borrow().get(id).cloned();
                    let child = self.unknown_child_value(Some(id), path.as_ref(), key);
                    Value::influenced(child.inputs())
                } else {
                    Value::influenced(self.heap.freeze(&value).inputs())
                };
                self.lookup_field(&value, key)
                    .map_or(unknown.clone(), |field| {
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
                }) | Datum::Scalar {
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
            self.heap.version.object(id).map(|object| &object.data),
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
            field_presence_value(*yes, unknown),
            field_presence_value(*no, unknown),
        ),
    }
}

fn extend_aliases(
    aliases: &super::AliasSet,
    heap: &super::HeapVersion,
    objects: &mut Vec<super::ObjectId>,
    expanded_render_graph: &mut bool,
) {
    match aliases {
        super::AliasSet::Known(ids) => objects.extend(ids.iter().cloned()),
        super::AliasSet::RenderGraph => {
            if !*expanded_render_graph {
                *expanded_render_graph = true;
                objects.extend(heap.pages.values().flat_map(|page| page.keys().cloned()));
            }
        }
    }
}
fn field_inputs(field: &Field, values: &mut Vec<Value>) {
    match field {
        Field::Present(value) => values.push(value.clone()),
        Field::Select {
            decision: _,
            yes,
            no,
        } => {
            field_inputs(yes, values);
            field_inputs(no, values);
        }
        Field::Absent | Field::Unknown => {}
    }
}

impl State {
    pub(super) fn consumed_field(&self, field: &Field) -> Field {
        specialize_field(field, &self.heap.arena.runtime_path(&self.active))
    }

    pub(super) fn map_length(&self, value: &Value) -> Option<Value> {
        let value = self.consumed(value);
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => Some(Value::select(
                Rc::clone(decision),
                self.map_length(yes)?,
                self.map_length(no)?,
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
                let ObjectData::Map {
                    fields,
                    remainder: super::Remainder::Closed,
                } = &self.heap.version.object(id)?.data
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
                datum: _,
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
                    extend_aliases(aliases, heap, objects, expanded_render_graph);
                }
            }
            Datum::Scalar {
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

fn selector_entry_domain(value: &Value, remaining: &mut usize) -> Predicate {
    if *remaining == 0 {
        return Predicate::False;
    }
    *remaining -= 1;
    match value.node.as_ref() {
        ValueNode::Select { decision, yes, no } => {
            if decision.coverage != Coverage::Exact
                || decision.when_true.contains_approximation()
                || decision.when_false.contains_approximation()
            {
                return Predicate::False;
            }
            Predicate::Or(vec![
                Predicate::all(vec![
                    decision.when_true.clone(),
                    selector_entry_domain(yes, remaining),
                ]),
                Predicate::all(vec![
                    decision.when_false.clone(),
                    selector_entry_domain(no, remaining),
                ]),
            ])
            .normalize_boolean()
        }
        ValueNode::Leaf(Leaf { datum, origin }) => {
            if let Some(path) = &origin.identity {
                if path.segments().next().is_none() {
                    return Predicate::True;
                }
                if path.segments().any(|segment| segment.literal().is_none()) {
                    return Predicate::False;
                }
                return super::kinds::input_nil_predicate(path)
                    .negated()
                    .normalize_boolean();
            }
            // Typed nil maps still perform map lookup; only an invalid entry skips the chain.
            if matches!(
                datum,
                Datum::Map { view: _, target: _ }
                    | Datum::Unknown(super::UnknownValue {
                        types: super::TypeCoverage::Map(_),
                        aliases: _
                    })
            ) {
                return Predicate::True;
            }
            match super::kinds::nilness(value) {
                Some(false) => Predicate::True,
                Some(true) | None => Predicate::False,
            }
        }
    }
}
