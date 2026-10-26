use std::rc::Rc;

use super::{
    AliasSet, Callee, Datum, Field, Invocation, Leaf, MapType, ObjectData, Origin, Remainder,
    ScalarType, State, TypeCoverage, UnknownValue, Value, ValueNode,
};
use crate::function_semantics::{Method, MethodReceiver};

pub(super) fn method(receiver: &Value, name: &str) -> Method {
    let mut pending = vec![receiver];
    let mut resolved = None;
    for _ in 0..128 {
        let Some(value) = pending.pop() else {
            return resolved.unwrap_or_else(|| Method::Foreign(name.to_owned()));
        };
        let family = match value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        view: MapType::Values,
                        target: _,
                    }
                    | Datum::Unknown(UnknownValue {
                        types: TypeCoverage::Map(MapType::Values),
                        aliases: _,
                    }),
                origin: _,
            }) => MethodReceiver::Values,
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Scalar {
                        go_type: _,
                        content: _,
                    }
                    | Datum::Map {
                        view: MapType::Interface | MapType::String,
                        target: _,
                    }
                    | Datum::Sequence(_)
                    | Datum::Strings(_)
                    | Datum::Slot {
                        backing: _,
                        index: _,
                    }
                    | Datum::Unknown(UnknownValue {
                        types:
                            TypeCoverage::Unresolved
                            | TypeCoverage::InputNonMap
                            | TypeCoverage::Scalar(_)
                            | TypeCoverage::Sequence
                            | TypeCoverage::Map(MapType::Interface | MapType::String),
                        aliases: _,
                    }),
                origin: _,
            }) => MethodReceiver::Other,
            ValueNode::Select {
                decision: _,
                yes,
                no,
            } => {
                pending.extend([yes, no]);
                continue;
            }
        };
        let next = Method::resolve(family, name);
        if resolved.as_ref().is_some_and(|previous| previous != &next) {
            return Method::Foreign(name.to_owned());
        }
        resolved = Some(next);
    }
    Method::Foreign(name.to_owned())
}

pub(super) fn transfer(invocation: &Invocation, state: &mut State) -> Value {
    let Callee::Method {
        receiver,
        descriptor,
    } = &invocation.callee
    else {
        return Value::unknown();
    };
    let inputs = state.heap.freeze(receiver).inputs();
    match descriptor {
        Method::ValuesAsMap => {
            let target = state.map_target(receiver);
            let nonempty = target.as_ref().is_some_and(|target| {
                match state.heap.version.object(target).map(|object| &object.data) {
                    Some(ObjectData::Map {
                        fields,
                        remainder: _,
                    }) => fields
                        .values()
                        .any(|field| matches!(state.consumed_field(field), Field::Present(_))),
                    Some(
                        ObjectData::Strings(_)
                        | ObjectData::Opaque { reachable: _ }
                        | ObjectData::Widened { inputs: _ },
                    )
                    | None => false,
                }
            });
            let converted = match target {
                Some(target) => Value::with_influences(
                    Value::leaf(Datum::Map {
                        view: MapType::Interface,
                        target: Some(target),
                    }),
                    inputs.clone(),
                ),
                None => unknown_map(MapType::Interface, inputs.clone()),
            };
            match if nonempty {
                Some(true)
            } else {
                state.truth(receiver)
            } {
                Some(true) => converted,
                Some(false) => Value::with_influences(state.heap.map(Remainder::Closed), inputs),
                None => {
                    let decision = state.test(receiver);
                    let empty = Value::with_influences(state.heap.map(Remainder::Closed), inputs);
                    Value::select(decision, converted, empty)
                }
            }
        }
        Method::ValuesTable => unknown_map(MapType::Values, inputs),
        Method::ValuesYaml => {
            Value::with_influences(Value::scalar(ScalarType::String, None), inputs)
        }
        Method::Foreign(_) => {
            let mut reachable: Vec<_> = invocation
                .operands
                .iter()
                .map(|operand| operand.value.clone())
                .collect();
            reachable.push(receiver.clone());
            let inputs = super::operations::inputs(&reachable, state);
            super::operations::widen_operands(&reachable, state);
            Value::influenced(inputs)
        }
    }
}

fn unknown_map(
    view: MapType,
    inputs: std::collections::BTreeSet<helm_schema_core::ValuesPath>,
) -> Value {
    Value {
        node: Rc::new(ValueNode::Leaf(Leaf {
            datum: Datum::Unknown(UnknownValue {
                types: TypeCoverage::Map(view),
                aliases: AliasSet::RenderGraph,
            }),
            origin: Origin {
                identity: None,
                influences: inputs,
            },
        })),
    }
}
