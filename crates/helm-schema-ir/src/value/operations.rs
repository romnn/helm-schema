use super::{
    AbortReason, Datum, Field, Leaf, MapType, ObjectData, ObjectVersion, Remainder, State, Value,
    ValueNode,
};
use helm_schema_core::GuardValue;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

pub(super) fn transfer(
    invocation: &super::Invocation,
    state: &mut State,
) -> Result<Value, AbortReason> {
    let super::Callee::Function(function) = &invocation.callee;
    let function = function.as_str();
    let values: Vec<_> = invocation
        .operands
        .iter()
        .map(|operand| operand.value.clone())
        .collect();
    let arguments = values.as_slice();
    let influences = inputs(arguments, state);
    let first = arguments.first().cloned().unwrap_or_else(Value::unknown);
    let last = arguments.last().cloned().unwrap_or_else(Value::unknown);
    match function {
        "dict" => {
            let value = state.heap.map(Remainder::Closed);
            if let Some(id) = state.map_target(&value) {
                for pair in arguments.chunks(2) {
                    let Some(key) = pair.first().and_then(|value| {
                        if value.constant() == Some(&GuardValue::Null) {
                            None
                        } else {
                            scalar_text(value)
                        }
                    }) else {
                        invalidate(state);
                        return Ok(value);
                    };
                    let value = pair.get(1).cloned().unwrap_or_else(|| Value::string(""));
                    state.heap.write(&id, &key, Field::Present(value));
                }
            }
            Ok(value)
        }
        "list" | "tuple" => {
            if arguments.len() > 128 {
                tracing::debug!(reason = ?super::BoundReason::StringSlots, "widen sequence");
                return Ok(Value::influenced(influences.clone()));
            }
            Ok(Value::leaf(Datum::Sequence(Some(Rc::new(
                arguments.to_vec(),
            )))))
        }
        "set" | "unset" => {
            let Some(key) =
                arguments
                    .get(1)
                    .and_then(Value::constant)
                    .and_then(|value| match value {
                        GuardValue::String(key) => Some(key),
                        GuardValue::Null
                        | GuardValue::Bool(_)
                        | GuardValue::Int(_)
                        | GuardValue::Float(_) => None,
                    })
            else {
                invalidate(state);
                return Ok(Value::influenced(first.inputs()));
            };
            let Some(id) = state.map_target(&first) else {
                return if matches!(
                    first.node.as_ref(),
                    ValueNode::Leaf(Leaf {
                        datum: Datum::Map {
                            target: None,
                            view: _
                        },
                        origin: _
                    })
                ) {
                    if function == "set" {
                        Err(AbortReason::Type)
                    } else {
                        Ok(first)
                    }
                } else {
                    invalidate(state);
                    Ok(Value::unknown())
                };
            };
            let field = if function == "unset" {
                Field::Absent
            } else {
                Field::Present(arguments.get(2).cloned().unwrap_or_else(Value::unknown))
            };
            state.heap.write(&id, key, field);
            Ok(Value::leaf(Datum::Map {
                view: MapType::Interface,
                target: Some(id),
            }))
        }
        "get" => {
            let Some(GuardValue::String(key)) = arguments.get(1).and_then(Value::constant) else {
                return Ok(Value::influenced(first.inputs()));
            };
            Ok(state.get_field(&first, key))
        }
        "toStrings" | "sortAlpha" => string_slice(function, &first, state),
        "index" => {
            let mut value = first;
            for key in arguments.iter().skip(1) {
                value = match key.constant() {
                    Some(GuardValue::String(key)) => state.field(&value, key),
                    Some(GuardValue::Int(index)) => match value.node.as_ref() {
                        ValueNode::Leaf(Leaf {
                            datum: Datum::Strings(id),
                            origin: _,
                        }) => usize::try_from(*index)
                            .ok()
                            .map_or_else(Value::unknown, |index| {
                                Value::leaf(Datum::Slot {
                                    backing: id.clone(),
                                    index,
                                })
                            }),
                        ValueNode::Leaf(Leaf {
                            datum: Datum::Sequence(Some(items)),
                            origin: _,
                        }) => usize::try_from(*index)
                            .ok()
                            .and_then(|index| items.get(index))
                            .cloned()
                            .unwrap_or_else(Value::unknown),
                        ValueNode::Leaf(Leaf {
                            datum:
                                Datum::Scalar {
                                    go_type: _,
                                    content: _,
                                }
                                | Datum::Map { view: _, target: _ }
                                | Datum::Sequence(None)
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
                        } => Value::influenced(value.inputs()),
                    },
                    Some(GuardValue::Null | GuardValue::Bool(_) | GuardValue::Float(_)) | None => {
                        Value::influenced(value.inputs())
                    }
                };
            }
            Ok(value)
        }
        "first" | "last" => match first.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum: Datum::Strings(id),
                origin: _,
            }) => {
                let Some(ObjectVersion {
                    data: ObjectData::Strings(slots),
                    revision: _,
                    multiplicity: _,
                }) = state.heap.version.object(id)
                else {
                    return Ok(Value::unknown());
                };
                Ok(if function == "first" {
                    slots.first()
                } else {
                    slots.last()
                }
                .cloned()
                .unwrap_or_else(Value::nil))
            }
            ValueNode::Leaf(Leaf {
                datum: Datum::Sequence(Some(items)),
                origin: _,
            }) => Ok(if function == "first" {
                items.first()
            } else {
                items.last()
            }
            .cloned()
            .unwrap_or_else(Value::nil)),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Scalar {
                        go_type: _,
                        content: _,
                    }
                    | Datum::Map { view: _, target: _ }
                    | Datum::Sequence(None)
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
            } => Ok(Value::influenced(first.inputs())),
        },
        "append" | "mustAppend" | "push" | "prepend" | "mustPrepend" => match first.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum: Datum::Sequence(Some(items)),
                origin: _,
            }) => {
                let mut items = items.as_ref().clone();
                if function.contains("repend") {
                    items.insert(0, last);
                } else {
                    items.push(last);
                }
                Ok(Value::leaf(Datum::Sequence(Some(Rc::new(items)))))
            }
            ValueNode::Leaf(Leaf {
                datum:
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
                    | Datum::Unknown(_),
                origin: _,
            })
            | ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => Ok(Value::influenced(first.inputs())),
        },
        "len" => {
            let length = match first.node.as_ref() {
                ValueNode::Leaf(Leaf {
                    datum: Datum::Sequence(Some(items)),
                    origin: _,
                }) => Some(items.len()),
                ValueNode::Leaf(Leaf {
                    datum:
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
                        | Datum::Unknown(_),
                    origin: _,
                })
                | ValueNode::Select {
                    decision: _,
                    yes: _,
                    no: _,
                } => match first.constant() {
                    Some(GuardValue::String(text)) => Some(text.len()),
                    Some(
                        GuardValue::Null
                        | GuardValue::Bool(_)
                        | GuardValue::Int(_)
                        | GuardValue::Float(_),
                    )
                    | None => None,
                },
            };
            Ok(length
                .and_then(|length| i64::try_from(length).ok())
                .map_or_else(
                    || Value::influenced(first.inputs()),
                    |length| Value::literal(GuardValue::Int(length)),
                ))
        }
        "not" | "empty" => Ok(state.truth(&first).map_or_else(
            || Value::influenced(first.inputs()),
            |truth| Value::literal(GuardValue::Bool(!truth)),
        )),
        "toJson" | "mustToJson" | "toPrettyJson" | "toRawJson" | "mustToRawJson" | "toYaml"
        | "toYamlPretty" => {
            if matches!(function, "toYaml" | "toYamlPretty") {
                return Ok(Value::influenced(influences.clone()));
            }
            let frozen = state.heap.freeze(&first);
            let Some(json) = frozen.json() else {
                return Ok(Value::influenced(first.inputs()));
            };
            let text = if matches!(function, "toYaml" | "toYamlPretty") {
                serde_yaml::to_string(&json)
                    .ok()
                    .map(|text| text.trim_end_matches('\n').to_owned())
            } else if function == "toPrettyJson" {
                serde_json::to_string_pretty(&json).ok()
            } else {
                serde_json::to_string(&json).ok()
            };
            let text = text.map(|text| {
                let text = text
                    .replace('\u{2028}', "\\u2028")
                    .replace('\u{2029}', "\\u2029");
                if matches!(function, "toJson" | "mustToJson" | "toPrettyJson") {
                    text.replace('<', "\\u003c")
                        .replace('>', "\\u003e")
                        .replace('&', "\\u0026")
                } else {
                    text
                }
            });
            Ok(text.map_or_else(
                || Value::influenced(influences.clone()),
                |text| Value::string(&text),
            ))
        }
        "fromJson" | "mustFromJson" | "fromYaml" | "fromYamlArray" | "fromJsonArray" => {
            let Some(GuardValue::String(text)) = first.constant() else {
                return Ok(Value::influenced(first.inputs()));
            };
            let parsed = if function.contains("Yaml") {
                serde_yaml::from_str::<serde_json::Value>(text).ok()
            } else {
                serde_json::from_str(text).ok()
            };
            let Some(parsed) = parsed else {
                return Ok(Value::unknown());
            };
            if parsed.is_null() && matches!(function, "fromJson" | "fromYaml") {
                return Ok(Value::leaf(Datum::Map {
                    view: MapType::Interface,
                    target: None,
                }));
            }
            if parsed.is_null() && matches!(function, "fromJsonArray" | "fromYamlArray") {
                return Ok(Value::leaf(Datum::Sequence(None)));
            }
            if matches!(function, "fromJson" | "fromYaml") && !parsed.is_object() {
                return Ok(state.heap.map(Remainder::Open));
            }
            if matches!(function, "fromJsonArray" | "fromYamlArray") && !parsed.is_array() {
                return Ok(Value::unknown());
            }
            Ok(decode(&parsed, state))
        }
        "deepCopy" | "mustDeepCopy" => Ok(copy(&first, state, &mut BTreeSet::new(), 0)),
        "quote" | "squote" => {
            let mut output = Vec::new();
            for value in arguments {
                if value.constant() == Some(&GuardValue::Null) {
                    continue;
                }
                let Some(text) = scalar_text(value) else {
                    return Ok(Value::influenced(influences.clone()));
                };
                if function == "quote"
                    && (!text.is_ascii()
                        || text.chars().any(|character| {
                            character.is_control() || matches!(character, '\u{2028}' | '\u{2029}')
                        }))
                {
                    return Ok(Value::influenced(influences.clone()));
                }
                let quoted = if function == "quote" {
                    serde_json::to_string(&text).ok()
                } else {
                    Some(format!("'{text}'"))
                };
                let Some(quoted) = quoted else {
                    return Ok(Value::unknown());
                };
                output.push(quoted);
            }
            Ok(Value::string(&output.join(" ")))
        }
        "upper" | "lower" | "trim" | "toString" => {
            let Some(text) = scalar_text(&last) else {
                return Ok(Value::influenced(last.inputs()));
            };
            if matches!(function, "upper" | "lower" | "trim") && !text.is_ascii() {
                return Ok(Value::influenced(influences.clone()));
            }
            let text = match function {
                "upper" => text.to_uppercase(),
                "lower" => text.to_lowercase(),
                "trim" => text.trim().to_owned(),
                _ => text,
            };
            Ok(Value::string(&text))
        }
        "print" | "println" => {
            if function == "print"
                && arguments
                    .iter()
                    .any(|value| !matches!(value.constant(), Some(GuardValue::String(_))))
            {
                return Ok(Value::influenced(influences.clone()));
            }
            let mut output = String::new();
            for (index, value) in arguments.iter().enumerate() {
                let Some(text) = scalar_text(value) else {
                    return Ok(Value::influenced(influences.clone()));
                };
                if function == "println" && index > 0 {
                    output.push(' ');
                }
                output.push_str(&text);
            }
            if function == "println" {
                output.push('\n');
            }
            Ok(Value::string(&output))
        }
        "indent" | "nindent" => {
            let Some(GuardValue::Int(width)) = first.constant() else {
                return Ok(Value::influenced(last.inputs()));
            };
            let Some(GuardValue::String(text)) = last.constant() else {
                return Ok(Value::influenced(last.inputs()));
            };
            let Some(width) = usize::try_from(*width).ok().filter(|width| *width <= 4096) else {
                return Ok(Value::unknown());
            };
            let prefix = " ".repeat(width);
            let mut output = text
                .split('\n')
                .map(|line| format!("{prefix}{line}"))
                .collect::<Vec<_>>()
                .join("\n");
            if function == "nindent" {
                output.insert(0, '\n');
            }
            Ok(Value::string(&output))
        }
        _ => {
            invalidate(state);
            Ok(Value::influenced(influences.clone()))
        }
    }
}
pub(super) fn inputs(arguments: &[Value], state: &State) -> BTreeSet<helm_schema_core::ValuesPath> {
    arguments
        .iter()
        .flat_map(|value| state.heap.freeze(value).inputs())
        .collect()
}
pub(super) fn invalidate(state: &mut State) {
    let input_regions = state.heap.freeze(&Value::unknown()).inputs();
    for path in input_regions {
        state.reads.insert(super::RuntimeRead {
            path,
            condition: state.path_predicate(),
            coverage: super::ReadCoverage::UnknownRegion,
        });
    }
    let objects: Vec<_> = state
        .heap
        .version
        .pages
        .values()
        .flat_map(|page| {
            page.iter()
                .map(|(id, object)| (id.clone(), object.as_ref().clone()))
        })
        .collect();
    for (id, object) in objects {
        let ObjectVersion {
            revision: _,
            multiplicity,
            data,
        } = object;
        let data = match data {
            ObjectData::Map {
                fields,
                remainder: _,
            } => ObjectData::Map {
                fields: fields
                    .into_iter()
                    .map(|(key, field)| {
                        let value = match field {
                            Field::Present(value) => Value::influenced(value.inputs()),
                            Field::Absent
                            | Field::Unknown
                            | Field::Select {
                                decision: _,
                                yes: _,
                                no: _,
                            } => Value::unknown(),
                        };
                        (key, Field::Present(value))
                    })
                    .collect(),
                remainder: Remainder::Open,
            },
            ObjectData::Widened { inputs } => ObjectData::Widened { inputs },
            ObjectData::Opaque { reachable } => ObjectData::Opaque { reachable },
            ObjectData::Strings(slots) => ObjectData::Strings(
                slots
                    .into_iter()
                    .map(|value| Value::influenced(value.inputs()))
                    .collect(),
            ),
        };
        state.heap.install(
            id,
            ObjectVersion {
                revision: state.heap.arena.snapshot(),
                multiplicity,
                data,
            },
        );
    }
    for frame in &mut state.frames {
        for value in frame.bindings.values_mut() {
            if !matches!(
                value.node.as_ref(),
                ValueNode::Leaf(Leaf {
                    datum: Datum::Map { .. },
                    ..
                })
            ) {
                *value = Value::influenced(value.inputs());
            }
        }
    }
}
fn decode(value: &serde_json::Value, state: &mut State) -> Value {
    match value {
        serde_json::Value::Object(fields) => {
            let value = state.heap.map(Remainder::Closed);
            if let Some(id) = state.map_target(&value) {
                for (key, value) in fields {
                    let value = decode(value, state);
                    state.heap.write(&id, key, Field::Present(value));
                }
            }
            value
        }
        serde_json::Value::Array(items) => Value::leaf(Datum::Sequence(Some(Rc::new(
            items.iter().map(|value| decode(value, state)).collect(),
        )))),
        serde_json::Value::Null => Value::nil(),
        serde_json::Value::Bool(value) => Value::literal(GuardValue::Bool(*value)),
        serde_json::Value::String(value) => Value::string(value),
        serde_json::Value::Number(_) => Value::unknown(),
    }
}
fn copy(
    value: &Value,
    state: &mut State,
    seen: &mut BTreeSet<super::ObjectId>,
    depth: usize,
) -> Value {
    if depth > 32 {
        return Value::influenced(value.inputs());
    }
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(id),
            origin: _,
        }) => {
            let Some(ObjectVersion {
                data: ObjectData::Strings(slots),
                revision: _,
                multiplicity: _,
            }) = state.heap.version.object(id).cloned()
            else {
                return Value::unknown();
            };
            Value::leaf(Datum::Strings(state.heap.allocate(
                ObjectData::Strings(slots),
                super::Multiplicity::Singleton,
            )))
        }

        ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    view,
                    target: Some(id),
                },
            origin: _,
        }) => {
            if !seen.insert(id.clone()) {
                return Value::unknown();
            }
            let Some(ObjectVersion {
                data: ObjectData::Map { fields, remainder },
                revision: _,
                multiplicity: _,
            }) = state.heap.version.object(id).cloned()
            else {
                return Value::unknown();
            };
            let mut copied = BTreeMap::new();
            for (key, field) in fields {
                copied.insert(
                    key,
                    match field {
                        Field::Present(value) => {
                            Field::Present(copy(&value, state, &mut seen.clone(), depth + 1))
                        }
                        other => other,
                    },
                );
            }
            let id = state.heap.allocate(
                ObjectData::Map {
                    fields: copied,
                    remainder,
                },
                super::Multiplicity::Singleton,
            );
            Value::leaf(Datum::Map {
                view: *view,
                target: Some(id),
            })
        }
        ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(Some(items)),
            origin: _,
        }) => Value::leaf(Datum::Sequence(Some(Rc::new(
            items
                .iter()
                .map(|value| copy(value, state, &mut seen.clone(), depth + 1))
                .collect(),
        )))),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Scalar {
                    go_type: _,
                    content: _,
                }
                | Datum::Map {
                    target: None,
                    view: _,
                }
                | Datum::Sequence(None)
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
        } => {
            if value.identity().is_some() {
                Value::influenced(state.heap.freeze(value).inputs())
            } else {
                state.consumed(value)
            }
        }
    }
}

fn string_slice(function: &str, first: &Value, state: &mut State) -> Result<Value, AbortReason> {
    let value = if matches!(
        first.node.as_ref(),
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(_),
            origin: _
        })
    ) {
        first.clone()
    } else {
        let values = match first.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum: Datum::Sequence(Some(items)),
                origin: _,
            }) => items.as_ref().clone(),
            ValueNode::Leaf(Leaf {
                datum:
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
                    | Datum::Unknown(_),
                origin: _,
            })
            | ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            } => return Ok(Value::influenced(first.inputs())),
        };
        let strings = values
            .iter()
            .map(|value| scalar_text(value).map(|text| Value::string(&text)))
            .collect::<Option<Vec<_>>>();
        let Some(strings) = strings else {
            return Ok(Value::influenced(first.inputs()));
        };
        Value::leaf(Datum::Strings(state.heap.allocate(
            ObjectData::Strings(strings),
            super::Multiplicity::Singleton,
        )))
    };
    if function == "sortAlpha" {
        if let ValueNode::Leaf(Leaf {
            datum: Datum::Strings(id),
            origin: _,
        }) = value.node.as_ref()
        {
            let Some(ObjectVersion {
                data: ObjectData::Strings(slots),
                revision: _,
                multiplicity: _,
            }) = state.heap.version.object(id)
            else {
                return Ok(Value::unknown());
            };
            let sorted = slots
                .iter()
                .map(|value| state.heap.freeze(value).exact_text())
                .collect::<Option<Vec<_>>>();
            let Some(mut sorted) = sorted else {
                invalidate(state);
                return Ok(Value::unknown());
            };
            sorted.sort();
            for (index, text) in sorted.iter().enumerate() {
                state.heap.write_slot(id, index, Value::string(text));
            }
        }
    }
    Ok(value)
}

fn scalar_text(value: &Value) -> Option<String> {
    if matches!(value.constant(), Some(GuardValue::Float(_))) {
        return None;
    }
    value.constant().map(|value| match value {
        GuardValue::Null => "<nil>".to_owned(),
        GuardValue::String(text) => text.clone(),
        GuardValue::Bool(value) => value.to_string(),
        GuardValue::Int(value) => value.to_string(),
        GuardValue::Float(value) => value.to_string(),
    })
}
