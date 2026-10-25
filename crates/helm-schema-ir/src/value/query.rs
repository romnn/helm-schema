use super::scalar_value::ScalarContent;
use super::{
    Coverage, Datum, Field, FrozenTest, FrozenValue, Leaf, ObjectData, ScalarType, State, Value,
    ValueNode,
};
use helm_schema_core::{Guard, GuardValue, Predicate, ValuesPath};
use std::collections::BTreeSet;
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
                        Datum::Sequence(Some(items)) => pending.extend(items.iter()),
                        Datum::Scalar {
                            go_type: _,
                            content: _,
                        }
                        | Datum::Map { view: _, target: _ }
                        | Datum::Sequence(None)
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
    pub(super) fn path_predicate(&self) -> Predicate {
        Predicate::all(
            self.active
                .iter()
                .filter(|edge| edge.test.coverage == Coverage::Exact)
                .map(|edge| {
                    if edge.taken {
                        edge.test.when_true.clone()
                    } else {
                        edge.test.when_false.clone()
                    }
                })
                .collect(),
        )
    }
    pub(super) fn consumed(&self, value: &Value) -> Value {
        let value = self.heap.version.consume(value);
        let value = if let Some(path) = value.identity() {
            let id = self.heap.arena.input_objects.borrow().get(path).cloned();
            if let Some(id) = id.filter(|id| self.heap.version.object(id).is_some()) {
                Value {
                    node: Rc::new(ValueNode::Leaf(Leaf {
                        datum: Datum::Map {
                            view: super::MapType::Interface,
                            target: Some(id),
                        },
                        origin: super::Origin {
                            identity: None,
                            influences: value.inputs(),
                        },
                    })),
                }
            } else {
                value
            }
        } else {
            value
        };
        self.active
            .iter()
            .fold(value, |value, edge| value.under(&edge.test, edge.taken))
    }
    pub(super) fn field(&mut self, value: &Value, key: &str) -> Value {
        let value = self.consumed(value);
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => Value::select(
                Rc::clone(decision),
                self.field(yes, key),
                self.field(no, key),
            ),
            ValueNode::Leaf(Leaf {
                datum: _,
                origin:
                    super::Origin {
                        identity: Some(path),
                        influences: _,
                    },
            }) => {
                let id = self.heap.arena.input_objects.borrow().get(path).cloned();
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
        let Some(mut path) = path.cloned() else {
            return Value::unknown();
        };
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
                    }
                    | Datum::Sequence(None),
                origin: _,
            }) => Some(false),
            ValueNode::Leaf(Leaf {
                datum: Datum::Strings(id),
                origin: _,
            }) => match self.heap.version.object(id).map(|object| &object.data) {
                Some(ObjectData::Strings(slots)) => Some(!slots.is_empty()),
                Some(
                    ObjectData::Map {
                        fields: _,
                        remainder: _,
                    }
                    | ObjectData::Opaque { reachable: _ }
                    | ObjectData::Widened { inputs: _ },
                )
                | None => None,
            },
            ValueNode::Leaf(Leaf {
                datum: Datum::Sequence(Some(items)),
                origin: _,
            }) => Some(!items.is_empty()),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        target: Some(id),
                        view: _,
                    },
                origin: _,
            }) => match self.heap.version.object(id).map(|object| &object.data) {
                Some(ObjectData::Map {
                    fields,
                    remainder: super::Remainder::Closed,
                }) if fields
                    .values()
                    .all(|field| matches!(field, Field::Present(_) | Field::Absent)) =>
                {
                    Some(
                        fields
                            .values()
                            .any(|field| matches!(field, Field::Present(_))),
                    )
                }
                Some(
                    ObjectData::Map {
                        fields: _,
                        remainder: _,
                    }
                    | ObjectData::Strings(_)
                    | ObjectData::Opaque { reachable: _ }
                    | ObjectData::Widened { inputs: _ },
                )
                | None => None,
            },
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
        let value = self.consumed(value);
        if let ValueNode::Select { decision, yes, no } = value.node.as_ref() {
            if yes.constant() == Some(&GuardValue::Bool(true))
                && no.constant() == Some(&GuardValue::Bool(false))
            {
                return Rc::clone(decision);
            }
        }
        let (when_true, when_false, coverage) = test_predicates(self, &value);
        self.heap.arena.decision(when_true, when_false, coverage)
    }
    pub(super) fn map_target(&mut self, value: &Value) -> Option<super::ObjectId> {
        let value = self.consumed(value);
        match value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum: Datum::Map { target, view: _ },
                origin: _,
            }) => target.clone(),
            ValueNode::Leaf(Leaf {
                datum: _,
                origin:
                    super::Origin {
                        identity: Some(path),
                        influences: _,
                    },
            }) => {
                let existing = self.heap.arena.input_objects.borrow().get(path).cloned();
                if let Some(id) = existing {
                    if self.heap.version.object(&id).is_none() {
                        self.heap.install(
                            id.clone(),
                            super::ObjectVersion {
                                revision: self.heap.arena.snapshot(),
                                multiplicity: super::Multiplicity::Singleton,
                                data: ObjectData::Map {
                                    fields: Default::default(),
                                    remainder: super::Remainder::Input,
                                },
                            },
                        );
                    }
                    return Some(id);
                }
                let id = self.heap.allocate(
                    ObjectData::Map {
                        fields: Default::default(),
                        remainder: super::Remainder::Input,
                    },
                    super::Multiplicity::Singleton,
                );
                self.heap
                    .arena
                    .input_objects
                    .borrow_mut()
                    .insert(path.clone(), id.clone());
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
/// Reads bounded truth subsets from the canonical decision graph.
fn test_predicates(state: &State, value: &Value) -> (Predicate, Predicate, Coverage) {
    const MAX_LEAVES: usize = 128;
    const MAX_VISITS: usize = MAX_LEAVES * 2 - 1;
    let mut pending = vec![(value.clone(), Predicate::True)];
    let mut when_true = Vec::new();
    let mut when_false = Vec::new();
    let mut coverage = Coverage::Exact;
    let mut leaves = 0;
    let mut visits = 0;
    while let Some((value, condition)) = pending.pop() {
        if condition == Predicate::False {
            continue;
        }
        if visits == MAX_VISITS {
            coverage = Coverage::Partial;
            break;
        }
        visits += 1;
        let value = state.consumed(&value);
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => {
                if decision.coverage == Coverage::Partial
                    || decision.when_true.contains_approximation()
                    || decision.when_false.contains_approximation()
                {
                    coverage = Coverage::Partial;
                }
                for (branch, predicate) in [(no, &decision.when_false), (yes, &decision.when_true)]
                {
                    let selected = Predicate::all(vec![condition.clone(), predicate.clone()])
                        .normalize_boolean();
                    if selected != Predicate::False {
                        pending.push((branch.clone(), selected));
                    }
                }
            }
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }) => {
                if leaves == MAX_LEAVES {
                    coverage = Coverage::Partial;
                    break;
                }
                leaves += 1;
                if let Some(truth) = state.truth(&value) {
                    if truth {
                        when_true.push(condition);
                    } else {
                        when_false.push(condition);
                    }
                } else if let Some(path) = value.identity() {
                    let positive = Predicate::from(Guard::Truthy { path: path.clone() });
                    when_true.push(Predicate::all(vec![condition.clone(), positive.clone()]));
                    when_false.push(Predicate::all(vec![condition, positive.negated()]));
                } else {
                    coverage = Coverage::Partial;
                }
            }
        }
    }
    (
        Predicate::Or(when_true).normalize_boolean(),
        Predicate::Or(when_false).normalize_boolean(),
        coverage,
    )
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
        let mut inputs = BTreeSet::new();
        let mut values = vec![self.value.clone()];
        let mut objects = Vec::new();
        let mut seen_objects = BTreeSet::new();
        let mut seen_values = BTreeSet::new();
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
                        | Datum::Strings(id)
                        | Datum::Slot {
                            backing: id,
                            index: _,
                        } => objects.push(id.clone()),
                        Datum::Sequence(Some(items)) => values.extend(items.iter().cloned()),
                        Datum::Unknown(super::UnknownValue { types: _, aliases }) => {
                            extend_aliases(
                                aliases,
                                &self.heap,
                                &mut objects,
                                &mut expanded_render_graph,
                            )
                        }
                        Datum::Scalar {
                            go_type: _,
                            content: _,
                        }
                        | Datum::Map {
                            target: None,
                            view: _,
                        }
                        | Datum::Sequence(None) => {}
                    },
                }
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
                    ObjectData::Widened { inputs: known } => inputs.extend(known.iter().cloned()),
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
        use super::kinds::GoType;
        match self.value.node.as_ref() {
            ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => GoType::Unknown,
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
                    super::TypeCoverage::Unresolved => GoType::Unknown,
                },
            },
        }
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
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        target: Some(id),
                        view: _,
                    },
                origin: _,
            }) => Some(self.heap.version.field(id, key)),
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
            })
            | ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => None,
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
                    let id = self.heap.arena.input_objects.borrow().get(path).cloned();
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
        for path in self.heap.freeze(&result).inputs() {
            self.reads.insert(super::RuntimeRead {
                path,
                condition: self.path_predicate(),
                coverage: super::ReadCoverage::ExactPath,
            });
        }
        result
    }

    pub(super) fn field_presence(&self, value: &Value, key: &str) -> Value {
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
        let unknown = if let Some(path) = path {
            let predicate = Predicate::from(Guard::Absent { path }).negated();
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
        if let Some(path) = value.identity() {
            let id = self.heap.arena.input_objects.borrow().get(path).cloned();
            if let Some(object) = id.as_ref().and_then(|id| self.heap.version.object(id)) {
                if !matches!(
                    object.data,
                    ObjectData::Map {
                        remainder: super::Remainder::Input,
                        ..
                    }
                ) {
                    return None;
                }
            }
            let mut child = path.clone();
            child.push(key);
            return Some(child);
        }
        let value = self.consumed(value);
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
