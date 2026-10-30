use helm_schema_core::Predicate;

/// Runtime types carried by the scalar leaf and function signatures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GoType {
    Nil,
    Bool,
    String,
    Int,
    Int64,
    Uint64,
    Float64,
    Map,
    Slice,
    Unknown,
}

impl GoType {
    pub(crate) fn joined(self, other: Self) -> Self {
        if self == other { self } else { Self::Unknown }
    }
}

pub(super) fn input_nil_predicate(path: &helm_schema_core::ValuesPath) -> Predicate {
    // These distinct atoms are already ordered; this union and its complement are normal forms.
    Predicate::Or(vec![
        Predicate::from(helm_schema_core::Guard::Eq {
            path: path.clone(),
            value: helm_schema_core::GuardValue::Null,
        }),
        Predicate::from(helm_schema_core::Guard::Absent { path: path.clone() }),
    ])
}

pub(super) struct InputTypeDomain {
    pub(super) possible: Predicate,
    pub(super) certain: Predicate,
}

pub(super) struct InputTypeAlternative {
    pub(super) value: super::Value,
    pub(super) condition: Predicate,
}

pub(super) fn input_type_domain(
    path: &helm_schema_core::ValuesPath,
    kind: GoType,
) -> Option<InputTypeDomain> {
    let schema_type = match kind {
        GoType::Nil => {
            return Some(InputTypeDomain {
                possible: input_nil_predicate(path),
                certain: input_nil_predicate(path),
            });
        }
        GoType::Bool => "boolean",
        GoType::String => "string",
        GoType::Map => "object",
        GoType::Slice => "array",
        GoType::Float64 => {
            let number = Predicate::from(helm_schema_core::Guard::TypeIs {
                path: path.clone(),
                schema_type: "number".to_owned(),
            });
            let integer = Predicate::from(helm_schema_core::Guard::TypeIs {
                path: path.clone(),
                schema_type: "integer".to_owned(),
            });
            // File/default numbers may be float64 even when their mathematical value is integral.
            return Some(InputTypeDomain {
                possible: number.clone(),
                certain: Predicate::all(vec![number, integer.negated()]).normalize_boolean(),
            });
        }
        GoType::Int64 => {
            return Some(InputTypeDomain {
                // --set can supply int64 while a file supplies float64 for the same numeric input.
                possible: Predicate::from(helm_schema_core::Guard::TypeIs {
                    path: path.clone(),
                    schema_type: "integer".to_owned(),
                }),
                certain: Predicate::False,
            });
        }
        GoType::Int | GoType::Uint64 => {
            return Some(InputTypeDomain {
                possible: Predicate::False,
                certain: Predicate::False,
            });
        }
        GoType::Unknown => return None,
    };
    let exact = Predicate::from(helm_schema_core::Guard::TypeIs {
        path: path.clone(),
        schema_type: schema_type.to_owned(),
    });
    Some(InputTypeDomain {
        possible: exact.clone(),
        certain: exact,
    })
}

pub(super) fn input_type_alternatives(value: &super::Value) -> Option<Vec<InputTypeAlternative>> {
    let super::ValueNode::Leaf(super::Leaf {
        datum: super::Datum::Unknown(unknown),
        origin: _,
    }) = value.node.as_ref()
    else {
        return None;
    };
    if unknown.types != super::TypeCoverage::Unresolved {
        return None;
    }
    let path = value.identity()?;
    let mut alternatives = Vec::new();
    for kind in [
        GoType::Nil,
        GoType::Bool,
        GoType::String,
        GoType::Int64,
        GoType::Float64,
        GoType::Map,
        GoType::Slice,
    ] {
        let domain = input_type_domain(path, kind)?;
        alternatives.push(InputTypeAlternative {
            value: refined_parameter(value, kind),
            condition: domain.possible,
        });
    }
    Some(alternatives)
}

struct ParameterPlan {
    value: super::Value,
    success: std::rc::Rc<super::FrozenTest>,
}
impl ParameterPlan {
    fn accepted(state: &super::State, value: super::Value) -> Self {
        Self {
            value,
            success: state.heap.arena.decision(
                Predicate::True,
                Predicate::False,
                super::Coverage::Exact,
            ),
        }
    }
    fn rejected(state: &super::State, value: super::Value) -> Self {
        Self {
            value,
            success: state.heap.arena.decision(
                Predicate::False,
                Predicate::True,
                super::Coverage::Exact,
            ),
        }
    }
}

pub(super) fn validate_parameter(
    mut state: super::State,
    value: super::Value,
    expected: Option<GoType>,
    form: super::ArgumentForm,
) -> Vec<super::Exit> {
    if expected == Some(GoType::Map) {
        state.record_member_host(
            &value,
            &Predicate::True,
            super::query::MemberAccess::MapArgument {
                allows_nil: form != super::ArgumentForm::DirectLookup,
            },
        );
    }
    let mut remaining = super::MAX_SCALAR_DISPATCH_STATES;
    let plan = validate_parameter_inner(&mut state, value, expected, form, &mut remaining);
    finish_parameter_plan(state, plan)
}

pub(super) fn validate_type_predicate(
    state: super::State,
    value: super::Value,
    accepted: Predicate,
) -> Vec<super::Exit> {
    let rejected = accepted.negated();
    let success = state
        .heap
        .arena
        .decision(accepted, rejected, super::Coverage::Exact);
    finish_parameter_plan(state, ParameterPlan { value, success })
}

fn finish_parameter_plan(state: super::State, plan: ParameterPlan) -> Vec<super::Exit> {
    let ParameterPlan { value, success } = plan;
    let known = state.heap.arena.runtime_path(&state.active);
    let outcome = success
        .constant_outcome()
        .or_else(|| known.implies_test(&success));
    match outcome {
        Some(true) => super::eval::normal(state, value),
        Some(false) => vec![super::Exit {
            state,
            flow: super::Flow::Abort(super::AbortReason::Type),
        }],
        None => {
            let mut exits = Vec::new();
            for taken in [true, false] {
                let branch = state.fork(std::rc::Rc::clone(&success), taken);
                if branch.path_is_infeasible() {
                    continue;
                }
                exits.push(super::Exit {
                    state: branch,
                    flow: if taken {
                        super::Flow::Normal(super::Normal {
                            value: value.clone(),
                        })
                    } else {
                        super::Flow::Abort(super::AbortReason::Type)
                    },
                });
            }
            exits
        }
    }
}

fn validate_parameter_inner(
    state: &mut super::State,
    value: super::Value,
    expected: Option<GoType>,
    form: super::ArgumentForm,
    remaining: &mut usize,
) -> ParameterPlan {
    use super::{Coverage, ValueNode};

    let Some(expected) = expected.filter(|kind| *kind != GoType::Unknown) else {
        return ParameterPlan::accepted(state, value);
    };
    if *remaining == 0 {
        let value = super::Value::influenced(state.heap.freeze(&value).inputs());
        return ParameterPlan::accepted(state, value);
    }
    *remaining -= 1;
    let consumed = state.consumed(&value);
    let allows_nil = consumed.is_invalid()
        || form != super::ArgumentForm::DirectLookup
            && consumed
                .identity()
                .is_none_or(|path| path.item_parent().is_none());
    if incompatible_parameter_view(&consumed, expected) {
        return ParameterPlan::rejected(state, value);
    }
    let actual = if matches!(expected, GoType::Map | GoType::Slice)
        && matches!(
            consumed.node.as_ref(),
            ValueNode::Select {
                decision: _,
                yes: _,
                no: _
            }
        ) {
        GoType::Unknown
    } else {
        state.heap.freeze(&consumed).go_type()
    };
    if actual != GoType::Unknown {
        return if actual == expected
            || actual == GoType::Nil
                && allows_nil
                && matches!(expected, GoType::Map | GoType::Slice)
        {
            let value = if actual == GoType::Nil {
                parameter_nil(expected).unwrap_or(value)
            } else {
                value
            };
            ParameterPlan::accepted(state, value)
        } else {
            ParameterPlan::rejected(state, value)
        };
    }
    if let ValueNode::Select {
        decision,
        yes: _,
        no: _,
    } = consumed.node.as_ref()
    {
        return validate_selected_parameter(state, value, decision, expected, form, remaining);
    }
    let accepted = if let Some(path) = consumed.identity() {
        let predicate =
            input_type_domain(path, expected).map_or(Predicate::False, |domain| domain.possible);
        if allows_nil && matches!(expected, GoType::Map | GoType::Slice | GoType::Nil) {
            Predicate::Or(vec![predicate, input_nil_predicate(path)]).normalize_boolean()
        } else {
            predicate
        }
    } else {
        return ParameterPlan::accepted(state, value);
    };
    let value = if let Some(path) = consumed.identity() {
        let refined = refined_parameter(&value, expected);
        if let Some(nil) = parameter_nil(expected) {
            let absent = input_nil_predicate(path);
            super::Value::select(
                state
                    .heap
                    .arena
                    .decision(absent.clone(), absent.negated(), Coverage::Exact),
                nil,
                refined,
            )
        } else {
            refined
        }
    } else {
        value
    };
    let rejected = accepted.negated();
    ParameterPlan {
        value,
        success: state
            .heap
            .arena
            .decision(accepted, rejected, Coverage::Exact),
    }
}

fn incompatible_parameter_view(value: &super::Value, expected: GoType) -> bool {
    use super::ValueNode;
    match value.node.as_ref() {
        ValueNode::Leaf(super::Leaf {
            datum:
                super::Datum::Map {
                    view: super::MapType::String,
                    target: _,
                }
                | super::Datum::Unknown(super::UnknownValue {
                    types: super::TypeCoverage::Map(super::MapType::String),
                    aliases: _,
                }),
            origin: _,
        }) => expected == GoType::Map,
        ValueNode::Leaf(super::Leaf {
            datum: super::Datum::Strings(_),
            origin: _,
        }) => expected == GoType::Slice,
        ValueNode::Leaf(super::Leaf {
            datum: _,
            origin: _,
        })
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => false,
    }
}

fn validate_selected_parameter(
    state: &mut super::State,
    value: super::Value,
    decision: &std::rc::Rc<super::FrozenTest>,
    expected: GoType,
    form: super::ArgumentForm,
    remaining: &mut usize,
) -> ParameterPlan {
    use std::rc::Rc;
    let depth = state.active.len();
    let mut plans = Vec::new();
    for taken in [true, false] {
        std::rc::Rc::make_mut(&mut state.active).push(super::DecisionEdge {
            test: Rc::clone(decision),
            taken,
        });
        let plan = if state.path_is_infeasible() {
            ParameterPlan::rejected(state, super::Value::unknown())
        } else {
            validate_parameter_inner(
                state,
                value.under(decision, taken),
                Some(expected),
                form,
                remaining,
            )
        };
        std::rc::Rc::make_mut(&mut state.active).truncate(depth);
        plans.push(plan);
    }
    let mut plans = plans.into_iter();
    let Some(yes) = plans.next() else {
        return ParameterPlan::accepted(state, value);
    };
    let Some(no) = plans.next() else {
        return yes;
    };
    let success = state
        .heap
        .arena
        .choose_test(Rc::clone(decision), yes.success, no.success);
    let value = state.join_local_values(Rc::clone(decision), &yes.value, &no.value);
    ParameterPlan { value, success }
}

fn parameter_nil(expected: GoType) -> Option<super::Value> {
    match expected {
        GoType::Map => Some(super::Value::leaf(super::Datum::Map {
            view: super::MapType::Interface,
            target: None,
        })),
        GoType::Slice => Some(super::Value::nil_sequence()),
        GoType::Nil
        | GoType::Bool
        | GoType::String
        | GoType::Int
        | GoType::Int64
        | GoType::Uint64
        | GoType::Float64
        | GoType::Unknown => None,
    }
}

fn refined_parameter(value: &super::Value, expected: GoType) -> super::Value {
    use super::{Datum, Leaf, ScalarType, TypeCoverage, UnknownValue, Value, ValueNode};
    let ValueNode::Leaf(Leaf { datum: _, origin }) = value.node.as_ref() else {
        return value.clone();
    };
    let scalar = match expected {
        GoType::Nil => Some(ScalarType::Nil),
        GoType::Bool => Some(ScalarType::Bool),
        GoType::String => Some(ScalarType::String),
        GoType::Int => Some(ScalarType::Int),
        GoType::Int64 => Some(ScalarType::Int64),
        GoType::Uint64 => Some(ScalarType::Uint64),
        GoType::Float64 => Some(ScalarType::Float64),
        GoType::Map | GoType::Slice | GoType::Unknown => None,
    };
    let datum = if let Some(go_type) = scalar {
        Datum::Scalar {
            go_type,
            content: None,
        }
    } else {
        Datum::Unknown(UnknownValue {
            types: match expected {
                GoType::Map => TypeCoverage::Map(super::MapType::Interface),
                GoType::Slice => TypeCoverage::Sequence,
                GoType::Nil
                | GoType::Bool
                | GoType::String
                | GoType::Int
                | GoType::Int64
                | GoType::Uint64
                | GoType::Float64
                | GoType::Unknown => TypeCoverage::Unresolved,
            },
            aliases: super::AliasSet::RenderGraph,
        })
    };
    Value {
        node: std::rc::Rc::new(ValueNode::Leaf(Leaf {
            datum,
            origin: origin.clone(),
        })),
    }
}

pub(super) fn required_subject(state: super::State, subject: super::Value) -> Vec<super::Exit> {
    use super::{AbortReason, Coverage, Exit, Flow, Normal};
    use helm_schema_core::{Guard, GuardValue};
    use std::rc::Rc;

    match state.heap.freeze(&subject).go_type() {
        GoType::Bool
        | GoType::Int
        | GoType::Int64
        | GoType::Uint64
        | GoType::Float64
        | GoType::Map
        | GoType::Slice => return super::eval::normal(state, subject),
        GoType::Nil | GoType::String | GoType::Unknown => {}
    }
    let empty = match subject.constant() {
        Some(GuardValue::Null) => Some(Predicate::True),
        Some(GuardValue::String(text)) => Some(if text.is_empty() {
            Predicate::True
        } else {
            Predicate::False
        }),
        Some(GuardValue::Bool(_) | GuardValue::Int(_) | GuardValue::Float(_)) => {
            Some(Predicate::False)
        }
        None => subject.identity().map(|path| {
            Predicate::Or(vec![
                Predicate::from(Guard::Eq {
                    path: path.clone(),
                    value: GuardValue::Null,
                }),
                Predicate::from(Guard::Absent { path: path.clone() }),
                Predicate::from(Guard::Eq {
                    path: path.clone(),
                    value: GuardValue::String(String::new()),
                }),
            ])
            .normalize_boolean()
        }),
    };
    let Some(empty) = empty else {
        return super::eval::normal(state, subject);
    };
    let decision = state
        .heap
        .arena
        .decision(empty.clone(), empty.negated(), Coverage::Exact);
    let mut exits = Vec::new();
    for taken in [true, false] {
        let branch = state.fork(Rc::clone(&decision), taken);
        if !branch.path_is_infeasible() {
            exits.push(Exit {
                state: branch,
                flow: if taken {
                    Flow::Abort(AbortReason::Required)
                } else {
                    Flow::Normal(Normal {
                        value: subject.clone(),
                    })
                },
            });
        }
    }
    exits
}

pub(super) fn descriptor(
    state: &super::State,
    function: &str,
    arguments: &[super::Value],
) -> super::Value {
    use super::{Coverage, Value};
    use helm_schema_core::{Guard, GuardValue};
    let Some(value) = arguments.last() else {
        return Value::unknown();
    };
    let kind_only = matches!(function, "kindOf" | "kindIs");
    let name = descriptor_name(value, kind_only);
    if matches!(function, "kindOf" | "typeOf") {
        return name.map_or_else(
            || refined_parameter(&Value::influenced(value.inputs()), GoType::String),
            Value::string,
        );
    }
    let Some(GuardValue::String(expected)) = arguments.first().and_then(Value::constant) else {
        return Value::influenced(value.inputs());
    };
    if let Some(name) = name {
        return Value::literal(GuardValue::Bool(name == expected));
    }
    if let Some(path) = value.identity() {
        let schema_type = match expected.as_str() {
            "bool" => Some("boolean"),
            "string" => Some("string"),
            "map" if kind_only => Some("object"),
            "map[string]interface {}" if !kind_only => Some("object"),
            "slice" if kind_only => Some("array"),
            "[]interface {}" if !kind_only => Some("array"),
            _ => None,
        };
        let predicate = if let Some(schema_type) = schema_type {
            Some(Predicate::from(Guard::TypeIs {
                path: path.clone(),
                schema_type: schema_type.to_owned(),
            }))
        } else if expected == if kind_only { "invalid" } else { "<nil>" } {
            Some(
                Predicate::Or(vec![
                    Predicate::from(Guard::Eq {
                        path: path.clone(),
                        value: GuardValue::Null,
                    }),
                    Predicate::from(Guard::Absent { path: path.clone() }),
                ])
                .normalize_boolean(),
            )
        } else if matches!(expected.as_str(), "float64" | "int64") {
            let kind = if expected == "float64" {
                GoType::Float64
            } else {
                GoType::Int64
            };
            if let Some(domain) = input_type_domain(path, kind) {
                let unknown = refined_parameter(&Value::influenced(value.inputs()), GoType::Bool);
                let false_arm = Value::select(
                    state.heap.arena.decision(
                        domain.possible.negated(),
                        domain.possible.clone(),
                        super::Coverage::Exact,
                    ),
                    Value::literal(GuardValue::Bool(false)),
                    unknown,
                );
                return Value::select(
                    state.heap.arena.decision(
                        domain.certain.clone(),
                        domain.certain.negated(),
                        super::Coverage::Exact,
                    ),
                    Value::literal(GuardValue::Bool(true)),
                    false_arm,
                );
            }
            None
        } else {
            Some(Predicate::False)
        };
        if let Some(predicate) = predicate {
            return Value::select(
                state
                    .heap
                    .arena
                    .decision(predicate.clone(), predicate.negated(), Coverage::Exact),
                Value::literal(GuardValue::Bool(true)),
                Value::literal(GuardValue::Bool(false)),
            );
        }
    }
    Value::influenced(value.inputs())
}

fn descriptor_name(value: &super::Value, kind_only: bool) -> Option<&'static str> {
    use super::{Datum, Leaf, MapType, ScalarType, ValueNode};
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf { datum, origin: _ }) => match datum {
            Datum::Scalar {
                go_type,
                content: _,
            } => Some(match go_type {
                ScalarType::Invalid | ScalarType::Nil => {
                    if kind_only {
                        "invalid"
                    } else {
                        "<nil>"
                    }
                }
                ScalarType::Bool => "bool",
                ScalarType::String => "string",
                ScalarType::Int => "int",
                ScalarType::Int64 => "int64",
                ScalarType::Uint8 => "uint8",
                ScalarType::Uint64 => "uint64",
                ScalarType::Float64 => "float64",
                ScalarType::Complex128 => "complex128",
            }),
            Datum::Map { view, target: _ } => Some(if kind_only {
                "map"
            } else {
                match view {
                    MapType::Interface => "map[string]interface {}",
                    MapType::String => "map[string]string",
                    MapType::Values => "common.Values",
                }
            }),
            Datum::Sequence(_) => Some(if kind_only { "slice" } else { "[]interface {}" }),
            Datum::Strings(_) => Some(if kind_only { "slice" } else { "[]string" }),
            Datum::Text(_)
            | Datum::Slot {
                backing: _,
                index: _,
            } => Some("string"),
            Datum::Unknown(_) => None,
        },
        ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => None,
    }
}

pub(super) fn nilness(value: &super::Value) -> Option<bool> {
    use super::{Datum, Leaf, ScalarType, TypeCoverage, UnknownValue, ValueNode};
    match value.node.as_ref() {
        ValueNode::Select {
            decision: _,
            yes,
            no,
        } => {
            let yes = nilness(yes)?;
            (Some(yes) == nilness(no)).then_some(yes)
        }
        ValueNode::Leaf(Leaf { datum, origin: _ }) => match datum {
            Datum::Scalar {
                go_type,
                content: _,
            } => Some(matches!(go_type, ScalarType::Invalid | ScalarType::Nil)),
            Datum::Map { view: _, target } => Some(target.is_none()),
            Datum::Sequence(super::InterfaceSequence {
                storage,
                start: _,
                length: _,
                capacity: _,
            }) => Some(storage.is_none()),
            Datum::Text(_)
            | Datum::Strings(_)
            | Datum::Slot {
                backing: _,
                index: _,
            } => Some(false),
            Datum::Unknown(UnknownValue { types, aliases: _ }) => match types {
                TypeCoverage::Scalar(kind) => {
                    Some(matches!(kind, ScalarType::Invalid | ScalarType::Nil))
                }
                TypeCoverage::Map(_)
                | TypeCoverage::Sequence
                | TypeCoverage::InputNonMap
                | TypeCoverage::Unresolved => None,
            },
        },
    }
}
