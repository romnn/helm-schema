use super::{
    AbortReason, Datum, Field, Leaf, MapType, ObjectData, ObjectVersion, Remainder, State, Value,
    ValueNode,
};
use helm_schema_core::{GuardValue, ValuesPath};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

#[derive(Debug, thiserror::Error)]
pub(super) enum TransferDemand {
    #[error("transfer aborted: {0:?}")]
    Abort(AbortReason),
    #[error("transfer requires a member choice")]
    Choice(Rc<super::FrozenTest>),
}

pub(super) fn transfer(
    invocation: &super::Invocation,
    state: &mut State,
) -> Result<Value, TransferDemand> {
    let Some(function) = invocation.callee.function_name() else {
        return Ok(Value::unknown());
    };
    let values: Vec<_> = invocation
        .operands
        .iter()
        .map(|operand| operand.value.clone())
        .collect();
    let arguments = values.as_slice();
    let influences = inputs(arguments, state);
    let first = arguments.first().cloned().unwrap_or_else(Value::unknown);
    let last = arguments.last().cloned().unwrap_or_else(Value::unknown);
    let input = TransferInput {
        function,
        arguments,
        influences,
        first,
        last,
    };
    match crate::function_semantics::function_semantics(function).behavior {
        crate::function_semantics::OutputBehavior::Encoded(alphabet) => {
            return Ok(Value::frozen_text(
                super::text::TextTerm::Encoded {
                    value: state.heap.freeze(&input.first),
                    alphabet,
                },
                input.influences,
            ));
        }
        crate::function_semantics::OutputBehavior::Opaque
        | crate::function_semantics::OutputBehavior::NonEmpty
        | crate::function_semantics::OutputBehavior::Transform
        | crate::function_semantics::OutputBehavior::Stringify
        | crate::function_semantics::OutputBehavior::NumericCast
        | crate::function_semantics::OutputBehavior::Arithmetic => {}
    }
    match function {
        "semverCompare" => Ok(transfer_semver_compare(input)),
        "contains" => Ok(transfer_contains(input, state)),
        "dict" => Ok(transfer_dictionary(input, state)),
        "list" | "tuple" => Ok(transfer_sequence(input)),
        "set" | "unset" => transfer_write_map(input, state),
        "get" => {
            let Some(GuardValue::String(key)) = input.arguments.get(1).and_then(Value::constant)
            else {
                return Ok(Value::influenced(input.first.inputs()));
            };
            Ok(state.get_field(&input.first, key))
        }
        "dig" => return transfer_dig(input, state),
        "toStrings" | "sortAlpha" => return string_slice(input.function, &input.first, state),
        "index" => transfer_index(input, state),
        "first" | "last" => transfer_sequence_edge(input, state),
        "append" | "mustAppend" | "push" | "prepend" | "mustPrepend" | "reverse"
        | "mustReverse" | "rest" | "mustRest" | "initial" | "mustInitial" | "concat" => {
            transfer_sequence_edit(input, state)
        }
        "split" | "splitn" => Ok(split_map(input.function, input.arguments, state)),
        "splitList" => Ok(transfer_split_list(input, state)),
        "join" => return transfer_join(input, state),
        "pick" | "omit" => Ok(select_map(input.function, input.arguments, state)),
        "pluck" => Ok(pluck(input.arguments, state)),
        "merge" | "mustMerge" | "mergeOverwrite" | "mustMergeOverwrite" => {
            return merge(input.arguments, state, input.function.contains("Overwrite"));
        }
        "slice" | "mustSlice" => slice(input.arguments, state),
        "keys" => {
            let mut length = Value::literal(GuardValue::Int(0));
            for argument in input.arguments {
                let Some(next) = state.map_length(argument) else {
                    return Ok(Value::influenced(input.influences));
                };
                length = super::query::add_lengths(&length, &next, &mut 255);
            }
            Ok(keys_with_length(&length, state, &mut 255))
        }
        "len" => Ok(transfer_length(input, state)),
        "toJson" | "mustToJson" | "toPrettyJson" | "toRawJson" | "mustToRawJson" | "toYaml"
        | "toYamlPretty" => return transfer_serialize(input, state),
        "fromJson" | "mustFromJson" | "fromYaml" | "fromYamlArray" | "fromJsonArray" => {
            Ok(transfer_deserialize(input, state))
        }
        "deepCopy" | "mustDeepCopy" => Ok(copy(&input.first, state, &mut BTreeSet::new(), 0)),
        "quote" | "squote" => Ok(transfer_quote(input, state)),
        "int" | "int64" | "float64" => Ok(transfer_numeric_cast(input)),
        "upper" | "lower" | "trim" | "toString" => Ok(transfer_string_transform(input)),
        "print" | "println" => Ok(transfer_print(input)),
        "indent" | "nindent" => Ok(transfer_indent(input, state)),
        _ => {
            match crate::function_semantics::function_semantics(input.function).heap_effect {
                crate::function_semantics::HeapEffect::ReadOnly => {}
                crate::function_semantics::HeapEffect::Operands => {
                    widen_operands(input.arguments, state);
                }
                crate::function_semantics::HeapEffect::Unknown => invalidate(state),
            }
            Ok(Value::influenced(input.influences.clone()))
        }
    }
    .map_err(TransferDemand::Abort)
}
struct TransferInput<'a> {
    function: &'a str,
    arguments: &'a [Value],
    influences: BTreeSet<helm_schema_core::ValuesPath>,
    first: Value,
    last: Value,
}

fn transfer_dig(input: TransferInput<'_>, state: &mut State) -> Result<Value, TransferDemand> {
    let Some((subject, prefix)) = input.arguments.split_last() else {
        return Err(TransferDemand::Abort(AbortReason::Type));
    };
    let Some((fallback, keys)) = prefix.split_last().filter(|(_, keys)| !keys.is_empty()) else {
        return Err(TransferDemand::Abort(AbortReason::Type));
    };
    if keys.len() > super::MAX_SCALAR_DISPATCH_STATES
        || keys
            .iter()
            .any(|key| !matches!(key.constant(), Some(GuardValue::String(_))))
    {
        return Ok(Value::influenced(input.influences));
    }
    let mut value = subject.clone();
    for key in keys {
        value = state.consumed(&value);
        if let ValueNode::Select {
            decision,
            yes: _,
            no: _,
        } = value.node.as_ref()
        {
            return Err(TransferDemand::Choice(Rc::clone(decision)));
        }
        state.record_member_host(
            &value,
            &super::Predicate::True,
            super::query::MemberAccess::MapArgument { allows_nil: false },
        );
        match state.heap.freeze(&value).go_type() {
            super::kinds::GoType::Map => {}
            super::kinds::GoType::Unknown => {
                let Some(path) = value.identity() else {
                    return Ok(Value::influenced(input.influences));
                };
                let accepted = super::Predicate::from(helm_schema_core::Guard::TypeIs {
                    path: path.clone(),
                    schema_type: "object".to_owned(),
                });
                let decision = state.heap.arena.decision(
                    accepted.clone(),
                    accepted.negated(),
                    super::Coverage::Exact,
                );
                match state
                    .heap
                    .arena
                    .runtime_path(&state.active)
                    .implies_test(&decision)
                {
                    Some(true) => {}
                    Some(false) => return Err(TransferDemand::Abort(AbortReason::Type)),
                    None => return Err(TransferDemand::Choice(decision)),
                }
            }
            super::kinds::GoType::Nil
            | super::kinds::GoType::Bool
            | super::kinds::GoType::String
            | super::kinds::GoType::Slice
            | super::kinds::GoType::Int
            | super::kinds::GoType::Int64
            | super::kinds::GoType::Uint64
            | super::kinds::GoType::Float64 => {
                return Err(TransferDemand::Abort(AbortReason::Type));
            }
        }
        let Some(GuardValue::String(key)) = key.constant() else {
            return Ok(Value::influenced(input.influences));
        };
        let present = state.field_presence(&value, key);
        let decision = state.test(&present);
        match decision.constant_outcome().or_else(|| {
            state
                .heap
                .arena
                .runtime_path(&state.active)
                .implies_test(&decision)
        }) {
            Some(true) => value = state.field(&value, key),
            Some(false) => return Ok(fallback.clone()),
            None => return Err(TransferDemand::Choice(decision)),
        }
    }
    Ok(value)
}

fn transfer_sequence(input: TransferInput<'_>) -> Value {
    let TransferInput {
        arguments,
        influences,
        function: _,
        first: _,
        last: _,
    } = input;
    if arguments.len() > 128 {
        tracing::debug!(reason = ?super::BoundReason::StringSlots, "widen sequence");
        Value::influenced(influences)
    } else {
        Value::sequence(arguments.to_vec())
    }
}

fn transfer_semver_compare(input: TransferInput<'_>) -> Value {
    let TransferInput {
        function: _,
        arguments,
        influences,
        first,
        last: _,
    } = input;

    let (Some(GuardValue::String(constraint)), Some(GuardValue::String(version))) =
        (first.constant(), arguments.get(1).and_then(Value::constant))
    else {
        return Value::influenced(influences);
    };
    let result = helm_schema_ast::semver_constraint_matches_version(constraint, version);
    Value::with_influences(
        result.map_or_else(
            || Value::scalar(super::ScalarType::Bool, None),
            |result| Value::literal(GuardValue::Bool(result)),
        ),
        influences,
    )
}

fn transfer_dictionary(input: TransferInput<'_>, state: &mut State) -> Value {
    let TransferInput {
        function: _,
        arguments,
        influences,
        first: _,
        last: _,
    } = input;

    let value = state.heap.map(Remainder::Closed);
    if let Some(id) = state.map_target(&value) {
        for pair in arguments.chunks(2) {
            let Some(key) = pair.first().and_then(scalar_text) else {
                state.heap.install(
                    id.clone(),
                    ObjectVersion {
                        revision: state.heap.arena.snapshot(),
                        multiplicity: super::Multiplicity::Singleton,
                        data: ObjectData::Widened {
                            inputs: Rc::new(influences.clone()),
                        },
                    },
                );
                return value;
            };
            let value = pair.get(1).cloned().unwrap_or_else(|| Value::string(""));
            state.heap.write(&id, &key, Field::Present(value));
        }
    }
    value
}

fn transfer_write_map(input: TransferInput<'_>, state: &mut State) -> Result<Value, AbortReason> {
    let TransferInput {
        function,
        arguments,
        influences,
        first,
        last: _,
    } = input;

    let Some(key) = arguments
        .get(1)
        .and_then(Value::constant)
        .and_then(|value| match value {
            GuardValue::String(key) => Some(key),
            GuardValue::Null | GuardValue::Bool(_) | GuardValue::Int(_) | GuardValue::Float(_) => {
                None
            }
        })
    else {
        if let Some(target) = state.map_target(&first) {
            widen_map(&target, &influences, state);
            return Ok(Value::with_influences(first, influences));
        }
        invalidate(state);
        return Ok(Value::influenced(influences));
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
    let returned_inputs = state.heap.freeze(&first).inputs();
    Ok(Value::with_influences(
        Value::leaf(Datum::Map {
            view: MapType::Interface,
            target: Some(id),
        }),
        returned_inputs,
    ))
}

fn transfer_index(input: TransferInput<'_>, state: &mut State) -> Result<Value, AbortReason> {
    let TransferInput {
        function: _,
        arguments,
        influences,
        first,
        last: _,
    } = input;

    let mut value = first;
    for key in arguments.iter().skip(1) {
        value = match key.constant() {
            Some(GuardValue::String(key)) => {
                state.record_member_host(
                    &value,
                    &super::Predicate::True,
                    super::query::MemberAccess::StringIndex,
                );
                match state.heap.freeze(&value).go_type() {
                    super::kinds::GoType::Map | super::kinds::GoType::Unknown => {}
                    super::kinds::GoType::Nil
                    | super::kinds::GoType::Bool
                    | super::kinds::GoType::String
                    | super::kinds::GoType::Slice
                    | super::kinds::GoType::Int
                    | super::kinds::GoType::Int64
                    | super::kinds::GoType::Uint64
                    | super::kinds::GoType::Float64 => return Err(AbortReason::Type),
                }
                state.field(&value, key)
            }
            Some(GuardValue::Int(index)) => match value.node.as_ref() {
                ValueNode::Leaf(Leaf {
                    datum: Datum::Strings(slice),
                    origin: _,
                }) => {
                    let Ok(index) = usize::try_from(*index) else {
                        return Err(AbortReason::Type);
                    };
                    if index >= slice.length {
                        return Err(AbortReason::Type);
                    }
                    Value::leaf(Datum::Slot {
                        backing: slice.backing.clone(),
                        index: slice.start + index,
                    })
                }
                ValueNode::Leaf(Leaf {
                    datum: Datum::Sequence(sequence),
                    origin: _,
                }) => {
                    let Ok(index) = usize::try_from(*index) else {
                        return Err(AbortReason::Type);
                    };
                    if index >= sequence.length {
                        return Err(AbortReason::Type);
                    }
                    sequence
                        .items()
                        .and_then(|items| items.get(index))
                        .cloned()
                        .unwrap_or_else(|| Value::influenced(influences.clone()))
                }
                ValueNode::Leaf(Leaf {
                    datum: _,
                    origin: _,
                })
                | ValueNode::Select {
                    decision: _,
                    yes: _,
                    no: _,
                } => Value::influenced(influences.clone()),
            },
            Some(GuardValue::Null | GuardValue::Bool(_) | GuardValue::Float(_)) | None => {
                Value::influenced(influences.clone())
            }
        };
    }
    state.record_value_read(&value, super::ReadCoverage::ExactPath);
    Ok(value)
}

fn transfer_sequence_edge(
    input: TransferInput<'_>,
    state: &mut State,
) -> Result<Value, AbortReason> {
    let TransferInput {
        function,
        arguments: _,
        influences,
        first,
        last: _,
    } = input;

    if let ValueNode::Leaf(Leaf {
        datum: Datum::Strings(slice),
        origin: _,
    }) = first.node.as_ref()
    {
        if slice.length == 0 {
            return Ok(Value::nil());
        }
        let index = slice.start
            + if function == "first" {
                0
            } else {
                slice.length - 1
            };
        return Ok(state.consumed(&Value::leaf(Datum::Slot {
            backing: slice.backing.clone(),
            index,
        })));
    }
    let Some(items) = sequence_items(&first, state) else {
        if known_non_sequence(&first, state) {
            return Err(AbortReason::Type);
        }
        return Ok(Value::influenced(influences.clone()));
    };
    Ok(if function == "first" {
        items.first()
    } else {
        items.last()
    }
    .cloned()
    .unwrap_or_else(Value::nil))
}

fn transfer_sequence_edit(
    input: TransferInput<'_>,
    state: &mut State,
) -> Result<Value, AbortReason> {
    let TransferInput {
        function,
        arguments,
        influences,
        first,
        last,
    } = input;

    if function == "concat" && arguments.is_empty() {
        return Ok(Value::nil_sequence());
    }
    let Some(mut items) = sequence_items(&first, state) else {
        if known_non_sequence(&first, state) {
            return Err(AbortReason::Type);
        }
        return Ok(Value::influenced(influences.clone()));
    };
    match function {
        "append" | "mustAppend" | "push" => items.push(last),
        "prepend" | "mustPrepend" => items.insert(0, last),
        "reverse" | "mustReverse" => items.reverse(),
        "rest" | "mustRest" => {
            if items.is_empty() {
                return Ok(Value::nil_sequence());
            }
            if !items.is_empty() {
                items.remove(0);
            }
        }
        "initial" | "mustInitial" => {
            if items.is_empty() {
                return Ok(Value::nil_sequence());
            }
            items.truncate(items.len().saturating_sub(1));
        }
        "concat" => {
            for value in arguments.iter().skip(1) {
                let Some(next) = sequence_items(value, state) else {
                    if known_non_sequence(value, state) {
                        return Err(AbortReason::Type);
                    }
                    return Ok(Value::influenced(influences.clone()));
                };
                items.extend(next);
            }
        }
        _ => return Ok(Value::influenced(influences.clone())),
    }
    if items.len() > 128 {
        return Ok(Value::influenced(influences.clone()));
    }
    if function == "concat" && items.is_empty() {
        return Ok(Value::nil_sequence());
    }
    let mut result = Value::sequence(items);
    if matches!(
        function,
        "append" | "mustAppend" | "push" | "prepend" | "mustPrepend" | "concat"
    ) && let ValueNode::Leaf(Leaf {
        datum: Datum::Sequence(sequence),
        origin: _,
    }) = Rc::make_mut(&mut result.node)
    {
        sequence.capacity = None;
    }
    Ok(result)
}

fn transfer_split_list(input: TransferInput<'_>, state: &mut State) -> Value {
    let TransferInput {
        function: _,
        arguments: _,
        influences,
        first,
        last,
    } = input;

    let Some(GuardValue::String(separator)) = first.constant() else {
        return Value::influenced(influences.clone());
    };
    let Some(GuardValue::String(text)) = last.constant() else {
        return Value::influenced(influences.clone());
    };
    let slots: Vec<Value> = split_parts(text, separator, None)
        .into_iter()
        .map(|text| Value::string(&text))
        .collect();
    let length = slots.len();
    let backing = state
        .heap
        .allocate(ObjectData::Strings(slots), super::Multiplicity::Singleton);
    Value::strings(backing, length)
}

fn transfer_join(input: TransferInput<'_>, state: &mut State) -> Result<Value, TransferDemand> {
    let TransferInput {
        function: _,
        arguments: _,
        influences,
        first,
        last,
    } = input;

    let Some(GuardValue::String(separator)) = first.constant() else {
        return Ok(Value::influenced(influences.clone()));
    };
    let Some(items) = sequence_items(&last, state) else {
        return Ok(Value::influenced(influences.clone()));
    };
    let Some(strings) = string_members(&items, state)? else {
        return Ok(Value::influenced(influences.clone()));
    };
    Ok(Value::string(&strings.join(separator)))
}

fn transfer_length(input: TransferInput<'_>, state: &mut State) -> Value {
    let TransferInput {
        function: _,
        arguments: _,
        influences,
        first,
        last: _,
    } = input;

    if let Some(length) = state.map_length(&first) {
        return length;
    }
    let length = if let ValueNode::Leaf(Leaf {
        datum: Datum::Sequence(sequence),
        origin: _,
    }) = first.node.as_ref()
    {
        Some(sequence.length)
    } else if let ValueNode::Leaf(Leaf {
        datum: Datum::Strings(slice),
        origin: _,
    }) = first.node.as_ref()
    {
        Some(slice.length)
    } else if let Some(GuardValue::String(text)) = first.constant() {
        Some(text.len())
    } else {
        None
    };
    length
        .and_then(|length| i64::try_from(length).ok())
        .map_or_else(
            || Value::influenced(influences.clone()),
            |length| Value::literal(GuardValue::Int(length)),
        )
}

fn transfer_serialize(
    input: TransferInput<'_>,
    state: &mut State,
) -> Result<Value, TransferDemand> {
    let TransferInput {
        function,
        arguments: _,
        influences,
        first,
        last: _,
    } = input;

    if matches!(function, "toYaml" | "toYamlPretty") {
        return Ok(Value::frozen_text(
            super::text::TextTerm::Yaml {
                value: state.heap.freeze(&first),
                pretty: function == "toYamlPretty",
            },
            influences.clone(),
        ));
    }
    let frozen = state.heap.freeze(&first);
    let guard = state.heap.arena.runtime_path(&state.active);
    let mut remaining = 4096;
    let mut query = super::output::JsonQuery {
        preserve_nil_collections: false,
        remaining: Some(&mut remaining),
        outcome: &|test| guard.implies_test(test),
        choice: None,
    };
    let Some(json) = frozen.json_with(&mut query) else {
        return match query.choice {
            Some(decision) => Err(TransferDemand::Choice(decision)),
            None => Ok(Value::frozen_text(
                super::text::TextTerm::Json {
                    value: frozen,
                    pretty: function == "toPrettyJson",
                    escape_html: matches!(function, "toJson" | "mustToJson" | "toPrettyJson"),
                    literal: None,
                },
                influences.clone(),
            )),
        };
    };
    let text = if function == "toPrettyJson" {
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
    Ok(Value::frozen_text(
        super::text::TextTerm::Json {
            value: frozen,
            pretty: function == "toPrettyJson",
            escape_html: matches!(function, "toJson" | "mustToJson" | "toPrettyJson"),
            literal: text.map(GuardValue::String),
        },
        influences.clone(),
    ))
}

fn transfer_contains(input: TransferInput<'_>, state: &State) -> Value {
    let TransferInput {
        function: _,
        arguments: _,
        influences,
        first,
        last,
    } = input;
    let Some(GuardValue::String(needle)) = first.constant() else {
        return Value::influenced(influences);
    };
    if needle.is_empty() {
        return Value::literal(GuardValue::Bool(true));
    }
    contains_text(needle, &last, state, &influences, &mut 128)
}

fn contains_text(
    needle: &str,
    last: &Value,
    state: &State,
    influences: &BTreeSet<ValuesPath>,
    remaining: &mut usize,
) -> Value {
    use helm_schema_core::{Guard, Predicate};
    let Some(next) = remaining.checked_sub(1) else {
        return Value::influenced(influences.clone());
    };
    *remaining = next;
    if let ValueNode::Select { decision, yes, no } = last.node.as_ref() {
        return Value::select(
            Rc::clone(decision),
            contains_text(needle, yes, state, influences, remaining),
            contains_text(needle, no, state, influences, remaining),
        );
    }
    if let Some(GuardValue::String(text)) = last.constant() {
        return Value::literal(GuardValue::Bool(text.contains(needle)));
    }
    let predicate = if let Some(path) = last.identity() {
        Some(Predicate::from(Guard::MatchesPattern {
            path: path.clone(),
            pattern: regex::escape(needle),
            templated: false,
        }))
    } else if needle == "{{" {
        if let Some(source) = json_source(last, remaining) {
            return json_source_contains(source, state, influences, remaining);
        }
        None
    } else {
        None
    };
    let Some(predicate) = predicate else {
        return Value::influenced(influences.clone());
    };
    let decision = state.heap.arena.decision(
        predicate.clone(),
        predicate.negated(),
        super::Coverage::Exact,
    );
    Value::select(
        decision,
        Value::literal(GuardValue::Bool(true)),
        Value::literal(GuardValue::Bool(false)),
    )
}

fn json_source<'a>(value: &'a Value, remaining: &mut usize) -> Option<&'a super::FrozenValue> {
    let ValueNode::Leaf(Leaf {
        datum: Datum::Text(text),
        origin: _,
    }) = value.node.as_ref()
    else {
        return None;
    };
    let mut term = &text.0;
    loop {
        *remaining = remaining.checked_sub(1)?;
        match term.as_ref() {
            super::text::TextTerm::Json {
                value,
                pretty: _,
                escape_html: _,
                literal: _,
            } => return Some(value),
            super::text::TextTerm::Resolved {
                term: nested,
                literal: _,
            } => term = nested,
            super::text::TextTerm::Yaml {
                value: _,
                pretty: _,
            }
            | super::text::TextTerm::Capture {
                graph: _,
                strip_marker: _,
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
            | super::text::TextTerm::Quote { value: _ }
            | super::text::TextTerm::Encoded {
                value: _,
                alphabet: _,
            } => return None,
        }
    }
}

fn json_source_contains(
    source: &super::FrozenValue,
    state: &State,
    influences: &BTreeSet<ValuesPath>,
    remaining: &mut usize,
) -> Value {
    use helm_schema_core::{Guard, Predicate, ValuePattern};
    let Some(next) = remaining.checked_sub(1) else {
        return Value::influenced(influences.clone());
    };
    *remaining = next;
    if let Some(literal) = source.literal() {
        return Value::literal(GuardValue::Bool(
            matches!(literal, GuardValue::String(text) if text.contains("{{")),
        ));
    }
    if let ValueNode::Select { decision, yes, no } = source.value.node.as_ref() {
        let yes = super::FrozenValue {
            heap: Rc::clone(&source.heap),
            value: yes.clone(),
        };
        let no = super::FrozenValue {
            heap: Rc::clone(&source.heap),
            value: no.clone(),
        };
        return Value::select(
            Rc::clone(decision),
            json_source_contains(&yes, state, influences, remaining),
            json_source_contains(&no, state, influences, remaining),
        );
    }
    let inputs = state.heap.arena.input_objects.borrow();
    let mut query = super::output::PayloadQuery::with_limit(
        &inputs,
        super::output::PayloadShape::Mapping,
        *remaining,
    );
    let view = query.json_source(source);
    *remaining = remaining.saturating_sub(query.work());
    if view.unknown != Predicate::False {
        return Value::influenced(influences.clone());
    }
    let [payload] = view.known.as_slice() else {
        return Value::influenced(influences.clone());
    };
    if payload.condition != Predicate::True {
        return Value::influenced(influences.clone());
    }
    let predicate = Predicate::Or(vec![
        Predicate::from(Guard::Absent {
            path: payload.path.clone(),
        }),
        Predicate::from(Guard::MatchesValue {
            path: payload.path.clone(),
            pattern: ValuePattern::TemplateActionFree,
        }),
    ])
    .negated();
    let decision = state.heap.arena.decision(
        predicate.clone(),
        predicate.negated(),
        super::Coverage::Exact,
    );
    Value::select(
        decision,
        Value::literal(GuardValue::Bool(true)),
        Value::literal(GuardValue::Bool(false)),
    )
}

fn transfer_deserialize(input: TransferInput<'_>, state: &mut State) -> Value {
    let TransferInput {
        function,
        arguments: _,
        influences,
        first,
        last: _,
    } = input;

    let Some(GuardValue::String(text)) = first.constant() else {
        return unknown_decoded(function, influences.clone(), state);
    };
    let parsed = if matches!(function, "fromYaml" | "fromYamlArray") {
        parse_yaml(text)
    } else {
        serde_json::from_str::<serde_yaml::Value>(text).ok()
    };
    let Some(parsed) = parsed else {
        return unknown_decoded(function, influences.clone(), state);
    };
    if parsed.is_null() {
        return match function {
            "fromYaml" => state.heap.map(Remainder::Closed),
            "fromYamlArray" => Value::sequence(Vec::new()),
            "fromJson" => Value::leaf(Datum::Map {
                view: MapType::Interface,
                target: None,
            }),
            "fromJsonArray" => Value::nil_sequence(),
            _ => Value::nil(),
        };
    }
    if matches!(function, "fromJson" | "fromYaml") && !parsed.is_mapping() {
        return Value::with_influences(state.heap.map(Remainder::Open), influences.clone());
    }
    if matches!(function, "fromJsonArray" | "fromYamlArray") && !parsed.is_sequence() {
        return unknown_decoded(function, influences.clone(), state);
    }
    decode(
        &parsed,
        state,
        !matches!(function, "fromYaml" | "fromYamlArray"),
    )
}

fn transfer_quote(input: TransferInput<'_>, state: &State) -> Value {
    let TransferInput {
        function,
        arguments,
        influences,
        first: _,
        last: _,
    } = input;

    let mut output = Vec::new();
    for value in arguments {
        if value.constant() == Some(&GuardValue::Null) {
            continue;
        }
        let Some(text) = scalar_text(value) else {
            return if function == "quote" && arguments.len() == 1 {
                Value::frozen_text(
                    super::text::TextTerm::Quote {
                        value: state.heap.freeze(value),
                    },
                    influences.clone(),
                )
            } else {
                Value::influenced(influences.clone())
            };
        };
        if function == "quote"
            && (!text.is_ascii()
                || text.chars().any(|character| {
                    character.is_control() || matches!(character, '\u{2028}' | '\u{2029}')
                }))
        {
            return Value::influenced(influences.clone());
        }
        let quoted = if function == "quote" {
            serde_json::to_string(&text).ok()
        } else {
            Some(format!("'{text}'"))
        };
        let Some(quoted) = quoted else {
            return Value::unknown();
        };
        output.push(quoted);
    }
    Value::string(&output.join(" "))
}

fn transfer_numeric_cast(input: TransferInput<'_>) -> Value {
    let TransferInput {
        function,
        arguments: _,
        influences,
        first,
        last: _,
    } = input;

    let kind = match function {
        "int" => super::ScalarType::Int,
        "int64" => super::ScalarType::Int64,
        "float64" => super::ScalarType::Float64,
        _ => return Value::influenced(influences.clone()),
    };
    let content = match first.constant() {
        Some(GuardValue::Null) => Some(GuardValue::Int(0)),
        Some(GuardValue::Bool(value)) => Some(GuardValue::Int(i64::from(*value))),
        Some(GuardValue::Int(value))
            if function != "float64" || value.unsigned_abs() <= 9_007_199_254_740_991 =>
        {
            Some(GuardValue::Int(*value))
        }
        Some(GuardValue::Float(value)) if function != "float64" => {
            truncated_float(value).map(GuardValue::Int)
        }
        Some(GuardValue::Float(value)) => Some(GuardValue::Float(value.clone())),
        Some(GuardValue::Int(_) | GuardValue::String(_)) | None => None,
    };
    let content = content.map(|value| {
        if function == "float64" {
            match value {
                GuardValue::Int(value) => GuardValue::Float(value.to_string()),
                value => value,
            }
        } else {
            value
        }
    });
    Value {
        node: Rc::new(ValueNode::Leaf(Leaf {
            datum: Datum::Scalar {
                go_type: kind,
                content: content.map(super::scalar_value::ScalarContent::Literal),
            },
            origin: super::Origin {
                identity: None,
                influences,
            },
        })),
    }
}

fn transfer_string_transform(input: TransferInput<'_>) -> Value {
    let TransferInput {
        function,
        arguments: _,
        influences,
        first: _,
        last,
    } = input;

    if function == "toString"
        && matches!(
            last.node.as_ref(),
            ValueNode::Leaf(Leaf {
                datum: Datum::Text(_),
                origin: _,
            })
        )
    {
        return last;
    }
    let Some(text) = scalar_text(&last) else {
        return Value::influenced(last.inputs());
    };
    if matches!(function, "upper" | "lower" | "trim") && !text.is_ascii() {
        return Value::influenced(influences.clone());
    }
    let text = match function {
        "upper" => text.to_uppercase(),
        "lower" => text.to_lowercase(),
        "trim" => text.trim().to_owned(),
        _ => text,
    };
    Value::string(&text)
}

fn transfer_print(input: TransferInput<'_>) -> Value {
    let TransferInput {
        function,
        arguments,
        influences,
        first: _,
        last: _,
    } = input;

    if function == "print"
        && arguments
            .iter()
            .any(|value| !matches!(value.constant(), Some(GuardValue::String(_))))
    {
        return Value::influenced(influences.clone());
    }
    let mut output = String::new();
    for (index, value) in arguments.iter().enumerate() {
        let Some(text) = scalar_text(value) else {
            return Value::influenced(influences.clone());
        };
        if function == "println" && index > 0 {
            output.push(' ');
        }
        output.push_str(&text);
    }
    if function == "println" {
        output.push('\n');
    }
    Value::string(&output)
}

fn transfer_indent(input: TransferInput<'_>, state: &State) -> Value {
    let TransferInput {
        function,
        arguments: _,
        influences: _,
        first,
        last,
    } = input;

    let Some(GuardValue::Int(width)) = first.constant() else {
        return Value::influenced(last.inputs());
    };
    let Some(width) = usize::try_from(*width).ok().filter(|width| *width <= 4096) else {
        return Value::unknown();
    };
    if let Some(GuardValue::String(text)) = last.constant() {
        return Value::string(&super::text::indent(text, width, function == "nindent"));
    }
    let value = state.heap.freeze(&last);
    let inputs = value.inputs();
    Value::frozen_text(
        super::text::TextTerm::Indent {
            value,
            width,
            newline: function == "nindent",
        },
        inputs,
    )
}

pub(super) fn inputs(arguments: &[Value], state: &State) -> BTreeSet<helm_schema_core::ValuesPath> {
    arguments
        .iter()
        .flat_map(|value| state.heap.freeze(value).inputs())
        .collect()
}

pub(super) fn widen_map(
    target: &super::ObjectId,
    influences: &BTreeSet<helm_schema_core::ValuesPath>,
    state: &mut State,
) {
    let input_path = state.heap.arena.input_paths.borrow().get(target).cloned();
    if let Some(path) = input_path {
        state.reads.insert(super::RuntimeRead {
            path,
            condition: state.path_predicate(),
            coverage: super::ReadCoverage::UnknownRegion,
        });
    }
    let value = Value::leaf(Datum::Map {
        view: MapType::Interface,
        target: Some(target.clone()),
    });
    let inputs = state
        .heap
        .freeze(&value)
        .inputs()
        .union(influences)
        .cloned()
        .collect();
    let Some(object) = state.heap.version.object(target) else {
        return;
    };
    let multiplicity = object.multiplicity;
    state.heap.install(
        target.clone(),
        ObjectVersion {
            revision: state.heap.arena.snapshot(),
            multiplicity,
            data: ObjectData::Widened {
                inputs: Rc::new(inputs),
            },
        },
    );
}
pub(super) fn invalidate(state: &mut State) {
    let input_regions = Rc::new(state.heap.freeze(&Value::unknown()).reachable_inputs());
    for path in input_regions.iter() {
        state.reads.insert(super::RuntimeRead {
            path: path.clone(),
            condition: state.path_predicate(),
            coverage: super::ReadCoverage::UnknownRegion,
        });
    }
    let objects: Vec<_> = state
        .heap
        .version
        .pages
        .values()
        .flat_map(|page| page.keys().cloned())
        .collect();
    for id in objects {
        let Some(object) = state.heap.version.object(&id) else {
            continue;
        };
        let multiplicity = object.multiplicity;
        // An unknown writer can insert or delete keys as well as replace their contents.
        state.heap.install(
            id,
            ObjectVersion {
                revision: state.heap.arena.snapshot(),
                multiplicity,
                data: ObjectData::Widened {
                    inputs: Rc::clone(&input_regions),
                },
            },
        );
    }
    for frame in &mut state.frames {
        let frame = Rc::make_mut(frame);
        for value in frame.bindings.values_mut() {
            if !matches!(
                value.node.as_ref(),
                ValueNode::Leaf(Leaf {
                    datum: Datum::Map { view: _, target: _ },
                    origin: _,
                })
            ) {
                *value = Value::influenced(input_regions.union(&value.inputs()).cloned().collect());
            }
        }
    }
}
struct YamlScalarReplacement {
    range: std::ops::Range<usize>,
    spelling: &'static str,
}

fn parse_yaml(text: &str) -> Option<serde_yaml::Value> {
    use helm_schema_syntax::{DialectDivergence, plain_scalar_divergence};

    let tree = helm_schema_syntax::parse_yaml(text)?;
    if tree.root_node().has_error() {
        return None;
    }
    let mut pending = vec![tree.root_node()];
    let mut replacements = Vec::new();
    while let Some(node) = pending.pop() {
        match node.kind() {
            // Tags can override implicit resolution; retain no exact result without that policy.
            "tag" => return None,
            "single_quote_scalar" | "double_quote_scalar" => {
                let spelling = text.get(node.byte_range())?;
                // serde_yaml's merge expansion no longer retains a key's quoted style.
                if serde_yaml::from_str::<String>(spelling).ok().as_deref() == Some("<<") {
                    return None;
                }
                continue;
            }
            "plain_scalar" => {
                let spelling = text.get(node.start_byte()..node.end_byte())?;
                let replacement = match plain_scalar_divergence(spelling, false) {
                    Some(DialectDivergence::Boolean) => {
                        if matches!(
                            spelling,
                            "y" | "Y" | "yes" | "Yes" | "YES" | "on" | "On" | "ON"
                        ) {
                            "true"
                        } else {
                            "false"
                        }
                    }
                    // The decoder deliberately withholds exact numeric content. A numeric node
                    // keeps divergent YAML 1.1 spellings from becoming exact YAML 1.2 strings.
                    Some(DialectDivergence::Number | DialectDivergence::ImpreciseInteger) => ".nan",
                    Some(DialectDivergence::SpecialFloat) => return None,
                    Some(DialectDivergence::MergeKey | DialectDivergence::NonStringKey) | None => {
                        continue;
                    }
                };
                replacements.push(YamlScalarReplacement {
                    range: node.byte_range(),
                    spelling: replacement,
                });
                continue;
            }
            _ => {}
        }
        let mut cursor = node.walk();
        pending.extend(node.named_children(&mut cursor));
    }
    replacements.sort_by_key(|replacement| replacement.range.start);
    let mut normalized = String::new();
    let mut cursor = 0;
    for YamlScalarReplacement { range, spelling } in replacements {
        normalized.push_str(text.get(cursor..range.start)?);
        normalized.push_str(spelling);
        cursor = range.end;
    }
    normalized.push_str(text.get(cursor..)?);
    let mut parsed = serde_yaml::from_str::<serde_yaml::Value>(&normalized).ok()?;
    parsed.apply_merge().ok()?;
    yaml_json_compatible(&parsed).then_some(parsed)
}

fn yaml_json_compatible(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Mapping(fields) => {
            let mut keys = BTreeSet::new();
            fields.iter().all(|(key, value)| {
                yaml_key(key).is_some_and(|key| keys.insert(key)) && yaml_json_compatible(value)
            })
        }
        serde_yaml::Value::Sequence(items) => items.iter().all(yaml_json_compatible),
        serde_yaml::Value::Tagged(_) => false,
        serde_yaml::Value::Null
        | serde_yaml::Value::Bool(_)
        | serde_yaml::Value::Number(_)
        | serde_yaml::Value::String(_) => true,
    }
}

fn decode(value: &serde_yaml::Value, state: &mut State, json_numbers: bool) -> Value {
    match value {
        serde_yaml::Value::Mapping(fields) => {
            let mut keyed = BTreeMap::new();
            for (key, value) in fields {
                let Some(key) = yaml_key(key) else {
                    return state.heap.map(Remainder::Open);
                };
                if keyed.insert(key, value).is_some() {
                    return state.heap.map(Remainder::Open);
                }
            }
            let value = state.heap.map(Remainder::Closed);
            if let Some(id) = state.map_target(&value) {
                for (key, value) in keyed {
                    let value = decode(value, state, json_numbers);
                    state.heap.write(&id, &key, Field::Present(value));
                }
            }
            value
        }
        serde_yaml::Value::Sequence(items) => {
            let mut value = Value::sequence(
                items
                    .iter()
                    .map(|value| decode(value, state, json_numbers))
                    .collect(),
            );
            if let ValueNode::Leaf(Leaf {
                datum: Datum::Sequence(sequence),
                origin: _,
            }) = Rc::make_mut(&mut value.node)
            {
                sequence.capacity = None;
            }
            value
        }
        serde_yaml::Value::Null => Value::nil(),
        serde_yaml::Value::Bool(value) => Value::literal(GuardValue::Bool(*value)),
        serde_yaml::Value::String(value) => Value::string(value),
        // The Go type is exact; numeric content still lacks Go-compatible serialization.
        serde_yaml::Value::Number(_) if json_numbers => {
            Value::scalar(super::ScalarType::Float64, None)
        }
        serde_yaml::Value::Number(_) | serde_yaml::Value::Tagged(_) => Value::unknown(),
    }
}

fn yaml_key(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(value) => Some(value.clone()),
        serde_yaml::Value::Bool(value) => Some(value.to_string()),
        serde_yaml::Value::Number(value) => value
            .as_i64()
            .map(|value| value.to_string())
            .or_else(|| value.as_u64().map(|value| value.to_string())),
        serde_yaml::Value::Null
        | serde_yaml::Value::Sequence(_)
        | serde_yaml::Value::Mapping(_)
        | serde_yaml::Value::Tagged(_) => None,
    }
}

fn copy(
    value: &Value,
    state: &mut State,
    seen: &mut BTreeSet<super::ObjectId>,
    depth: usize,
) -> Value {
    let influences = state.heap.freeze(value).inputs();
    if depth > 32 {
        return Value::influenced(influences);
    }
    let copied = match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(slice),
            origin: _,
        }) => copy_string_slice(slice, state, influences.clone()),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    view,
                    target: Some(id),
                },
            origin: _,
        }) => {
            if !seen.insert(id.clone()) {
                return Value::influenced(influences);
            }
            let Some(ObjectVersion {
                data: ObjectData::Map { fields, remainder },
                revision: _,
                multiplicity: _,
            }) = state.heap.version.object(id).cloned()
            else {
                return Value::influenced(influences);
            };
            let copied = fields
                .into_iter()
                .map(|(key, field)| (key, copy_field(field, state, seen, depth + 1)))
                .collect();
            let target = state.heap.allocate(
                ObjectData::Map {
                    fields: copied,
                    remainder,
                },
                super::Multiplicity::Singleton,
            );
            Value::leaf(Datum::Map {
                view: *view,
                target: Some(target),
            })
        }
        ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(sequence),
            origin: _,
        }) => {
            if sequence.storage.is_none() {
                return Value::with_influences(Value::nil_sequence(), influences);
            }
            let Some(items) = sequence.items() else {
                return Value::influenced(influences);
            };
            let mut copied: Vec<_> = items
                .iter()
                .map(|value| copy(value, state, &mut seen.clone(), depth + 1))
                .collect();
            if let Some(capacity) = sequence.capacity {
                copied.resize_with(capacity, Value::nil);
            }
            Value::leaf(Datum::Sequence(super::InterfaceSequence {
                storage: Some(Rc::new(copied)),
                start: 0,
                length: sequence.length,
                capacity: sequence.capacity,
            }))
        }
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Text(_)
                | Datum::Scalar {
                    go_type: _,
                    content: _,
                }
                | Datum::Map {
                    target: None,
                    view: _,
                }
                | Datum::Slot {
                    backing: _,
                    index: _,
                }
                | Datum::Unknown(_),
            origin: _,
        }) => copy_unstructured(value, state, &influences),
        ValueNode::Select { decision, yes, no } => Value::select(
            Rc::clone(decision),
            copy(yes, state, &mut seen.clone(), depth + 1),
            copy(no, state, &mut seen.clone(), depth + 1),
        ),
    };
    Value::with_influences(copied, influences)
}

fn copy_unstructured(
    value: &Value,
    state: &mut State,
    influences: &BTreeSet<helm_schema_core::ValuesPath>,
) -> Value {
    if value.identity().is_some() {
        if matches!(
            value.node.as_ref(),
            ValueNode::Leaf(Leaf {
                datum: Datum::Unknown(super::UnknownValue {
                    types: super::TypeCoverage::Map(_),
                    aliases: _
                }),
                origin: _,
            })
        ) || value
            .identity()
            .is_some_and(|path| path.segments().next().is_none())
        {
            state.heap.map(Remainder::Open)
        } else {
            Value::influenced(influences.clone())
        }
    } else {
        state.consumed(value)
    }
}

fn copy_field(
    field: Field,
    state: &mut State,
    seen: &mut BTreeSet<super::ObjectId>,
    depth: usize,
) -> Field {
    match field {
        Field::Absent => Field::Absent,
        Field::Unknown => Field::Unknown,
        Field::Present(value) => Field::Present(copy(&value, state, &mut seen.clone(), depth)),
        Field::Select { decision, yes, no } => Field::select(
            decision,
            copy_field(*yes, state, &mut seen.clone(), depth + 1),
            copy_field(*no, state, &mut seen.clone(), depth + 1),
        ),
    }
}

fn string_slice(function: &str, first: &Value, state: &mut State) -> Result<Value, TransferDemand> {
    let influences = state.heap.freeze(first).inputs();
    let value = if matches!(
        first.node.as_ref(),
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(_),
            origin: _
        })
    ) {
        first.clone()
    } else {
        let values = if let Some(values) = sequence_items(first, state) {
            values
        } else if first.constant() == Some(&GuardValue::Null) {
            Vec::new()
        } else if first.constant().is_some() {
            vec![first.clone()]
        } else if known_non_sequence(first, state) {
            let backing = state.heap.allocate(
                ObjectData::Widened {
                    inputs: Rc::new(influences.clone()),
                },
                super::Multiplicity::Singleton,
            );
            return Ok(Value::with_influences(
                Value::strings(backing, 1),
                influences,
            ));
        } else {
            if function == "sortAlpha" {
                invalidate(state);
            }
            return Ok(Value::influenced(influences));
        };
        let capacity = values.len();
        let Some(strings) = string_members(&values, state)? else {
            return Ok(Value::influenced(influences));
        };
        let mut strings: Vec<_> = strings.iter().map(|text| Value::string(text)).collect();
        let length = strings.len();
        strings.resize_with(capacity, || Value::string(""));
        let backing = state
            .heap
            .allocate(ObjectData::Strings(strings), super::Multiplicity::Singleton);
        Value::with_influences(
            Value::leaf(Datum::Strings(super::StringSlice {
                backing,
                start: 0,
                length,
                capacity: Some(capacity),
            })),
            influences.clone(),
        )
    };
    if function == "sortAlpha"
        && let ValueNode::Leaf(Leaf {
            datum: Datum::Strings(slice),
            origin: _,
        }) = value.node.as_ref()
    {
        let Some(ObjectVersion {
            data: ObjectData::Strings(slots),
            revision: _,
            multiplicity: _,
        }) = state.heap.version.object(&slice.backing)
        else {
            return Ok(value);
        };
        let sorted = slots
            .get(slice.start..slice.start.saturating_add(slice.length))
            .map(|slots| string_members(slots, state))
            .transpose()?
            .flatten();
        let Some(mut sorted) = sorted else {
            let multiplicity = state
                .heap
                .version
                .object(&slice.backing)
                .map_or(super::Multiplicity::Summary, |object| object.multiplicity);
            state.heap.install(
                slice.backing.clone(),
                ObjectVersion {
                    revision: state.heap.arena.snapshot(),
                    multiplicity,
                    data: ObjectData::Widened {
                        inputs: Rc::new(influences),
                    },
                },
            );
            return Ok(value);
        };
        sorted.sort();
        for (index, text) in sorted.iter().enumerate() {
            state
                .heap
                .write_slot(&slice.backing, slice.start + index, Value::string(text));
        }
    }
    Ok(value)
}

fn selected_member(value: &Value, state: &State) -> Value {
    match value.node.as_ref() {
        ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => state.consumed(value),
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        }) => value.clone(),
    }
}

fn string_members(values: &[Value], state: &State) -> Result<Option<Vec<String>>, TransferDemand> {
    let mut strings = Vec::new();
    for value in values {
        let value = selected_member(value, state);
        if let ValueNode::Select {
            decision,
            yes: _,
            no: _,
        } = value.node.as_ref()
        {
            return Err(TransferDemand::Choice(Rc::clone(decision)));
        }
        if value.constant() == Some(&GuardValue::Null) {
            continue;
        }
        let Some(text) = scalar_text(&value) else {
            return Ok(None);
        };
        strings.push(text);
    }
    Ok(Some(strings))
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
        GuardValue::Float(value) => value.clone(),
    })
}

struct MapFields {
    fields: BTreeMap<String, Field>,
    remainder: Remainder,
}

fn map_fields(value: &Value, state: &State) -> Option<MapFields> {
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Map {
                target: None,
                view: _,
            },
            origin: _,
        }) => Some(MapFields {
            fields: BTreeMap::new(),
            remainder: Remainder::Closed,
        }),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    target: Some(id),
                    view: _,
                },
            origin: _,
        }) => match &state.heap.version.object(id)?.data {
            ObjectData::Map { fields, remainder } => Some(MapFields {
                fields: fields
                    .iter()
                    .map(|(key, field)| (key.clone(), state.consumed_field(field)))
                    .collect(),
                remainder: remainder.clone(),
            }),
            ObjectData::Strings(_)
            | ObjectData::Opaque { reachable: _ }
            | ObjectData::Widened { inputs: _ } => None,
        },
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        })
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => None,
    }
}

fn sequence_items(value: &Value, state: &State) -> Option<Vec<Value>> {
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(sequence),
            origin: _,
        }) => Some(
            sequence
                .items()?
                .iter()
                .map(|value| state.consumed(value))
                .collect(),
        ),
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(slice),
            origin: _,
        }) => {
            let ObjectData::Strings(slots) = &state.heap.version.object(&slice.backing)?.data
            else {
                return None;
            };
            Some(
                slots
                    .get(slice.start..slice.start.checked_add(slice.length)?)?
                    .iter()
                    .map(|value| state.consumed(value))
                    .collect(),
            )
        }
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        })
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => None,
    }
}

fn fresh_map(
    map: MapFields,
    influences: BTreeSet<helm_schema_core::ValuesPath>,
    state: &mut State,
) -> Value {
    let MapFields { fields, remainder } = map;
    let target = state.heap.allocate(
        ObjectData::Map { fields, remainder },
        super::Multiplicity::Singleton,
    );
    Value {
        node: Rc::new(ValueNode::Leaf(Leaf {
            datum: Datum::Map {
                view: MapType::Interface,
                target: Some(target),
            },
            origin: super::Origin {
                identity: None,
                influences,
            },
        })),
    }
}

fn select_map(function: &str, arguments: &[Value], state: &mut State) -> Value {
    let influences = inputs(arguments, state);
    let Some(first) = arguments.first() else {
        return Value::influenced(influences);
    };
    let Some(mut map) = map_fields(first, state) else {
        return Value::influenced(influences);
    };
    let keys = arguments
        .iter()
        .skip(1)
        .map(|value| match value.constant() {
            Some(GuardValue::String(key)) => Some(key.clone()),
            Some(
                GuardValue::Null | GuardValue::Bool(_) | GuardValue::Int(_) | GuardValue::Float(_),
            )
            | None => None,
        })
        .collect::<Option<BTreeSet<_>>>();
    let Some(keys) = keys else {
        return Value::influenced(influences);
    };
    if function == "omit" {
        for key in keys {
            map.fields.insert(key, Field::Absent);
        }
    } else {
        map.fields.retain(|key, _| keys.contains(key));
        for key in keys {
            if !map.fields.contains_key(&key) && map.remainder != Remainder::Closed {
                map.fields.insert(key, Field::Unknown);
            }
        }
        map.remainder = Remainder::Closed;
    }
    fresh_map(map, influences, state)
}

fn pluck(arguments: &[Value], state: &mut State) -> Value {
    let influences = inputs(arguments, state);
    let Some(GuardValue::String(key)) = arguments.first().and_then(Value::constant) else {
        for value in arguments.iter().skip(1) {
            state.record_container_read(value);
        }
        return Value::influenced(influences);
    };
    let mut items = Vec::new();
    for value in arguments.iter().skip(1) {
        state.record_field_read(value, key);
        let Some(map) = map_fields(value, state) else {
            return Value::influenced(influences);
        };
        match map.fields.get(key) {
            Some(Field::Present(value)) => items.push(state.consumed(value)),
            Some(Field::Absent) => {}
            None if map.remainder == Remainder::Closed => {}
            Some(
                Field::Select {
                    decision: _,
                    yes: _,
                    no: _,
                }
                | Field::Unknown,
            )
            | None => {
                return Value::influenced(influences);
            }
        }
    }
    if items.len() > 128 {
        return Value::influenced(influences);
    }
    Value::sequence(items)
}

fn slice(arguments: &[Value], state: &mut State) -> Result<Value, AbortReason> {
    let influences = inputs(arguments, state);
    let Some(first) = arguments.first() else {
        return Ok(Value::influenced(influences));
    };
    if first.constant() == Some(&GuardValue::Null) {
        return Err(AbortReason::Type);
    }
    let Some(SequenceBounds {
        length,
        capacity,
        known_capacity,
    }) = sequence_bounds(first, state)
    else {
        return if known_non_sequence(first, state) {
            Err(AbortReason::Type)
        } else {
            Ok(Value::influenced(influences))
        };
    };
    if length == 0 {
        return Ok(Value::nil());
    }
    let known_capacity = capacity.unwrap_or(known_capacity.max(length));
    let mut indices = [0, length];
    for (index, value) in arguments.iter().skip(1).take(2).enumerate() {
        let number = match value.constant() {
            Some(GuardValue::Int(value)) => Some(*value),
            Some(GuardValue::String(value)) => {
                let digits = value
                    .strip_prefix('-')
                    .or_else(|| value.strip_prefix('+'))
                    .unwrap_or(value);
                if digits.len() > 1 && digits.starts_with('0') {
                    None
                } else {
                    value.parse::<i64>().ok()
                }
            }
            Some(GuardValue::Bool(value)) => Some(i64::from(*value)),
            Some(GuardValue::Null) => Some(0),
            Some(GuardValue::Float(value)) => truncated_float(value),
            None => None,
        };
        let Some(number) = number else {
            return Ok(Value::influenced(influences));
        };
        let Ok(number) = usize::try_from(number) else {
            return Err(AbortReason::Type);
        };
        let Some(slot) = indices.get_mut(index) else {
            return Ok(Value::influenced(influences));
        };
        *slot = number;
    }
    let [start, end] = indices;
    if start > end || capacity.is_some_and(|capacity| end > capacity) {
        return Err(AbortReason::Type);
    }
    if end > known_capacity {
        return Ok(Value::influenced(influences));
    }
    Ok(match first.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(sequence),
            origin: _,
        }) => Value::with_influences(
            Value::leaf(Datum::Sequence(super::InterfaceSequence {
                storage: sequence.storage.clone(),
                start: sequence.start + start,
                length: end - start,
                capacity: sequence.capacity.map(|capacity| capacity - start),
            })),
            influences,
        ),
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(slice),
            origin: _,
        }) => Value::with_influences(
            Value::leaf(Datum::Strings(super::StringSlice {
                backing: slice.backing.clone(),
                start: slice.start + start,
                length: end - start,
                capacity: slice.capacity.map(|capacity| capacity - start),
            })),
            influences,
        ),
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        })
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => Value::influenced(influences),
    })
}

fn merge(arguments: &[Value], state: &mut State, overwrite: bool) -> Result<Value, TransferDemand> {
    let influences = inputs(arguments, state);
    let Some(first) = arguments.first() else {
        return Ok(Value::influenced(influences));
    };
    let nil_destination = matches!(
        first.node.as_ref(),
        ValueNode::Leaf(Leaf {
            datum: Datum::Map {
                target: None,
                view: _
            },
            origin: _,
        })
    );
    let mut destination = first.clone();
    if arguments.len() == 1 {
        return Ok(interface_map_view(first, state));
    }
    if arguments
        .iter()
        .skip(1)
        .any(|value| map_fields(value, state).is_none())
    {
        invalidate(state);
        return Ok(Value::influenced(influences));
    }
    if nil_destination
        && arguments.iter().skip(1).all(|value| {
            matches!(
                value.node.as_ref(),
                ValueNode::Leaf(Leaf {
                    datum: Datum::Map {
                        target: None,
                        view: _
                    },
                    origin: _,
                })
            )
        })
    {
        return Ok(Value::with_influences(
            Value::leaf(Datum::Map {
                view: MapType::Interface,
                target: None,
            }),
            influences.clone(),
        ));
    }
    let scalar_overwrite = state.map_target(&destination).is_some_and(|target| {
        can_overwrite_scalar_sources(&destination, &target, arguments, state, overwrite)
    });
    let mut graph = BTreeSet::new();
    let disjoint = scalar_overwrite
        || disjoint_map_keys(arguments, state)
        || std::iter::once(&destination)
            .chain(arguments.iter().skip(1))
            .try_fold(true, |disjoint, value| {
                if disjoint {
                    disjoint_closed_graph(value, state, &mut graph, 0)
                } else {
                    Ok(false)
                }
            })?;
    if nil_destination {
        destination = state.heap.map(Remainder::Closed);
    }
    let Some(target) = state.map_target(&destination) else {
        invalidate(state);
        return Ok(Value::influenced(influences));
    };
    if scalar_overwrite {
        if !overwrite_scalar_sources(&target, arguments, state) {
            return Ok(Value::influenced(influences));
        }
        return Ok(interface_map_view(&destination, state));
    }
    if !disjoint {
        // Mergo walks map keys in unspecified order; aliased paths can interfere through the heap.
        let mut affected = arguments.to_vec();
        affected.push(destination.clone());
        widen_operands(&affected, state);
        return Ok(interface_map_view(&destination, state));
    }
    for source in arguments.iter().skip(1) {
        if !merge_maps(&target, source, state, overwrite, 0) {
            let mut affected = arguments.to_vec();
            affected.push(destination.clone());
            widen_operands(&affected, state);
            break;
        }
    }
    Ok(interface_map_view(&destination, state))
}

// With disjoint root keys Mergo only inserts fields; it never traverses their aliased values.
fn disjoint_map_keys(arguments: &[Value], state: &State) -> bool {
    let mut keys = BTreeSet::new();
    let mut objects = BTreeSet::new();
    for value in arguments {
        let ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    target: Some(id),
                    view: _,
                },
            origin: _,
        }) = value.node.as_ref()
        else {
            return false;
        };
        if !objects.insert(id) {
            return false;
        }
        let Some(ObjectVersion {
            data:
                ObjectData::Map {
                    fields,
                    remainder: Remainder::Closed,
                },
            multiplicity: super::Multiplicity::Singleton,
            revision: _,
        }) = state.heap.version.object(id)
        else {
            return false;
        };
        for (key, field) in fields {
            if !matches!(field, Field::Present(_) | Field::Absent) || !keys.insert(key) {
                return false;
            }
        }
    }
    true
}

fn disjoint_closed_graph(
    value: &Value,
    state: &State,
    objects: &mut BTreeSet<super::ObjectId>,
    depth: usize,
) -> Result<bool, TransferDemand> {
    if depth > 32 {
        return Ok(false);
    }
    let value = selected_member(value, state);
    Ok(match value.node.as_ref() {
        ValueNode::Leaf(Leaf { datum, origin: _ }) => match datum {
            Datum::Text(_)
            | Datum::Scalar {
                go_type: _,
                content: _,
            }
            | Datum::Map {
                view: _,
                target: None,
            } => true,
            Datum::Map {
                view: _,
                target: Some(id),
            } => {
                if !objects.insert(id.clone()) {
                    return Ok(false);
                }
                let Some(ObjectVersion {
                    data:
                        ObjectData::Map {
                            fields,
                            remainder: Remainder::Closed,
                        },
                    revision: _,
                    multiplicity: super::Multiplicity::Singleton,
                }) = state.heap.version.object(id)
                else {
                    return Ok(false);
                };
                fields.values().try_fold(true, |disjoint, field| {
                    if !disjoint {
                        return Ok(false);
                    }
                    match if matches!(
                        field,
                        Field::Select {
                            decision: _,
                            yes: _,
                            no: _
                        }
                    ) {
                        state.consumed_field(field)
                    } else {
                        field.clone()
                    } {
                        Field::Absent => Ok(true),
                        Field::Present(value) => {
                            disjoint_closed_graph(&value, state, objects, depth + 1)
                        }
                        Field::Select {
                            decision,
                            yes: _,
                            no: _,
                        } => Err(TransferDemand::Choice(decision)),
                        Field::Unknown => Ok(false),
                    }
                })?
            }
            Datum::Sequence(sequence) => sequence
                .storage
                .iter()
                .flat_map(|items| items.iter())
                .try_fold(true, |disjoint, value| {
                    if disjoint {
                        disjoint_closed_graph(value, state, objects, depth + 1)
                    } else {
                        Ok(false)
                    }
                })?,
            Datum::Strings(slice) => objects.insert(slice.backing.clone()),
            Datum::Slot {
                backing: _,
                index: _,
            }
            | Datum::Unknown(_) => false,
        },
        ValueNode::Select {
            decision,
            yes: _,
            no: _,
        } => return Err(TransferDemand::Choice(Rc::clone(decision))),
    })
}

fn merge_maps(
    target: &super::ObjectId,
    source: &Value,
    state: &mut State,
    overwrite: bool,
    depth: usize,
) -> bool {
    if depth > 32 {
        return false;
    }
    let Some(source) = map_fields(source, state) else {
        return false;
    };
    let Some(ObjectVersion {
        data:
            ObjectData::Map {
                fields,
                remainder: Remainder::Closed,
            },
        revision: _,
        multiplicity: super::Multiplicity::Singleton,
    }) = state.heap.version.object(target).cloned()
    else {
        return false;
    };
    for (key, field) in source.fields {
        let source = match field {
            Field::Absent => continue,
            Field::Present(value) => value,
            Field::Select {
                decision: _,
                yes: _,
                no: _,
            }
            | Field::Unknown => return false,
        };
        if !overwrite
            && matches!(
                source.node.as_ref(),
                ValueNode::Leaf(Leaf {
                    datum: Datum::Scalar {
                        go_type: super::ScalarType::Invalid | super::ScalarType::Nil,
                        content: _
                    },
                    origin: _,
                })
            )
        {
            continue;
        }
        let destination = match fields.get(&key) {
            Some(Field::Present(value)) => Some(selected_member(value, state)),
            Some(Field::Absent) | None => None,
            Some(
                Field::Select {
                    decision: _,
                    yes: _,
                    no: _,
                }
                | Field::Unknown,
            ) => return false,
        };
        if let Some(destination) = destination {
            if let (
                ValueNode::Leaf(Leaf {
                    datum:
                        Datum::Map {
                            target: Some(nested),
                            view: _,
                        },
                    origin: _,
                }),
                ValueNode::Leaf(Leaf {
                    datum: Datum::Map { target: _, view: _ },
                    origin: _,
                }),
            ) = (destination.node.as_ref(), source.node.as_ref())
            {
                if !merge_maps(nested, &source, state, overwrite, depth + 1) {
                    return false;
                }
                continue;
            }
            if !overwrite {
                match state.truth(&destination) {
                    Some(true) => continue,
                    Some(false) => {}
                    None => return false,
                }
            }
        }
        state.heap.write(target, &key, Field::Present(source));
    }
    true
}

pub(super) fn widen_operands(arguments: &[Value], state: &mut State) {
    let influences = Rc::new(inputs(arguments, state));
    let mut pending = arguments.to_vec();
    let mut objects = BTreeSet::new();
    while let Some(value) = pending.pop() {
        let mut targets = BTreeSet::new();
        if mutation_targets(&value, &mut pending, &mut targets) {
            invalidate(state);
            return;
        }
        for target in targets {
            if !objects.insert(target.clone()) {
                continue;
            }
            let Some(object) = state.heap.version.object(&target) else {
                continue;
            };
            match &object.data {
                ObjectData::Map { fields, remainder } => {
                    if matches!(remainder, Remainder::Unknown { inputs: _ }) {
                        invalidate(state);
                        return;
                    }
                    let mut fields: Vec<_> = fields.values().collect();
                    while let Some(field) = fields.pop() {
                        match field {
                            Field::Absent | Field::Unknown => {}
                            Field::Present(value) => pending.push(value.clone()),
                            Field::Select {
                                decision: _,
                                yes,
                                no,
                            } => fields.extend([yes.as_ref(), no.as_ref()]),
                        }
                    }
                }
                ObjectData::Strings(slots) => pending.extend(slots.iter().cloned()),
                ObjectData::Opaque { reachable } => match reachable {
                    super::AliasSet::Known(ids) => pending.extend(ids.iter().map(|id| {
                        Value::leaf(Datum::Map {
                            view: MapType::Interface,
                            target: Some(id.clone()),
                        })
                    })),
                    super::AliasSet::RenderGraph => {
                        invalidate(state);
                        return;
                    }
                },
                ObjectData::Widened { inputs: _ } => {
                    // Lost fields may still alias objects elsewhere in the render graph.
                    invalidate(state);
                    return;
                }
            }
        }
    }
    for id in objects {
        let Some(object) = state.heap.version.object(&id) else {
            continue;
        };
        let multiplicity = object.multiplicity;
        state.heap.install(
            id,
            ObjectVersion {
                revision: state.heap.arena.snapshot(),
                multiplicity,
                data: ObjectData::Widened {
                    inputs: Rc::clone(&influences),
                },
            },
        );
    }
    for path in influences.iter() {
        state.reads.insert(super::RuntimeRead {
            path: path.clone(),
            condition: state.path_predicate(),
            coverage: super::ReadCoverage::UnknownRegion,
        });
    }
}

fn known_non_sequence(value: &Value, state: &State) -> bool {
    use super::kinds::GoType;
    match state.heap.freeze(value).go_type() {
        GoType::Nil
        | GoType::Bool
        | GoType::String
        | GoType::Int
        | GoType::Int64
        | GoType::Uint64
        | GoType::Float64
        | GoType::Map => true,
        GoType::Slice | GoType::Unknown => false,
    }
}

fn unknown_decoded(
    function: &str,
    influences: BTreeSet<helm_schema_core::ValuesPath>,
    state: &mut State,
) -> Value {
    if function == "fromYaml" {
        return Value::with_influences(state.heap.map(Remainder::Open), influences);
    }
    if function == "fromJson" {
        let nil = Value::with_influences(
            Value::leaf(Datum::Map {
                view: MapType::Interface,
                target: None,
            }),
            influences.clone(),
        );
        let map = Value::with_influences(state.heap.map(Remainder::Open), influences);
        let decision = state.heap.arena.decision(
            helm_schema_core::Predicate::False,
            helm_schema_core::Predicate::False,
            super::Coverage::Partial,
        );
        return Value::select(decision, nil, map);
    }
    let region = state.heap.allocate(
        ObjectData::Opaque {
            reachable: super::AliasSet::Known(BTreeSet::new()),
        },
        super::Multiplicity::Singleton,
    );
    Value {
        node: Rc::new(ValueNode::Leaf(Leaf {
            datum: Datum::Unknown(super::UnknownValue {
                types: if matches!(function, "fromJsonArray" | "fromYamlArray") {
                    super::TypeCoverage::Sequence
                } else {
                    super::TypeCoverage::Unresolved
                },
                aliases: super::AliasSet::Known(BTreeSet::from([region])),
            }),
            origin: super::Origin {
                identity: None,
                influences,
            },
        })),
    }
}

fn closed_scalar_source(value: &Value, state: &State, remaining: &mut usize) -> bool {
    let map = match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Map {
                view: _,
                target: None,
            },
            origin: _,
        }) => return true,
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    view: _,
                    target: Some(id),
                },
            origin: _,
        }) => {
            let Some(ObjectVersion {
                revision: _,
                multiplicity: super::Multiplicity::Singleton,
                data:
                    ObjectData::Map {
                        fields,
                        remainder: Remainder::Closed,
                    },
            }) = state.heap.version.object(id)
            else {
                return false;
            };
            fields
        }
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        })
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => return false,
    };
    map.values().all(|field| match field {
        Field::Absent => true,
        Field::Present(value) => immutable_scalar(value, remaining),
        Field::Unknown
        | Field::Select {
            decision: _,
            yes: _,
            no: _,
        } => false,
    })
}

fn immutable_scalar(value: &Value, remaining: &mut usize) -> bool {
    if *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Text(_)
                | Datum::Scalar {
                    go_type: _,
                    content: _,
                },
            origin: _,
        }) => true,
        ValueNode::Select {
            decision: _,
            yes,
            no,
        } => immutable_scalar(yes, remaining) && immutable_scalar(no, remaining),
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        }) => false,
    }
}

fn split_map(function: &str, arguments: &[Value], state: &mut State) -> Value {
    let influences = inputs(arguments, state);
    let unknown =
        |state: &mut State| string_map(BTreeMap::new(), Remainder::Open, influences.clone(), state);
    let count = if function == "splitn" {
        let Some(GuardValue::Int(count)) = arguments.get(1).and_then(Value::constant) else {
            return unknown(state);
        };
        if *count == 0 {
            return string_map(BTreeMap::new(), Remainder::Closed, influences, state);
        }
        if *count < 0 {
            None
        } else {
            let Ok(count) = usize::try_from(*count) else {
                return unknown(state);
            };
            Some(count)
        }
    } else {
        None
    };
    let separator = arguments
        .first()
        .and_then(Value::constant)
        .and_then(|value| match value {
            GuardValue::String(text) => Some(text.as_str()),
            GuardValue::Null | GuardValue::Bool(_) | GuardValue::Int(_) | GuardValue::Float(_) => {
                None
            }
        });
    let text_value = arguments.last();
    let text = text_value
        .and_then(Value::constant)
        .and_then(|value| match value {
            GuardValue::String(text) => Some(text.as_str()),
            GuardValue::Null | GuardValue::Bool(_) | GuardValue::Int(_) | GuardValue::Float(_) => {
                None
            }
        });
    if count == Some(1)
        && (text.is_some_and(|text| !text.is_empty())
            || separator.is_some_and(|separator| !separator.is_empty()))
        && let Some(text) = text_value
    {
        return string_map(
            BTreeMap::from([("_0".to_owned(), Field::Present(text.clone()))]),
            Remainder::Closed,
            influences,
            state,
        );
    }
    let (Some(separator), Some(text)) = (separator, text) else {
        return unknown(state);
    };
    let fields = split_parts(text, separator, count)
        .into_iter()
        .enumerate()
        .map(|(index, text)| (format!("_{index}"), Field::Present(Value::string(&text))))
        .collect();
    string_map(fields, Remainder::Closed, influences, state)
}

fn string_map(
    fields: BTreeMap<String, Field>,
    remainder: Remainder,
    influences: BTreeSet<helm_schema_core::ValuesPath>,
    state: &mut State,
) -> Value {
    let target = state.heap.allocate(
        ObjectData::Map { fields, remainder },
        super::Multiplicity::Singleton,
    );
    Value::with_influences(
        Value::leaf(Datum::Map {
            view: MapType::String,
            target: Some(target),
        }),
        influences,
    )
}

fn split_parts(text: &str, separator: &str, limit: Option<usize>) -> Vec<String> {
    if let Some(0) = limit {
        return Vec::new();
    }
    if separator.is_empty() {
        let mut parts = Vec::new();
        for (offset, character) in text.char_indices() {
            if limit.is_some_and(|limit| parts.len() + 1 == limit) {
                parts.push(text.get(offset..).unwrap_or_default().to_owned());
                break;
            }
            parts.push(character.to_string());
        }
        parts
    } else if let Some(limit) = limit {
        text.splitn(limit, separator).map(str::to_owned).collect()
    } else {
        text.split(separator).map(str::to_owned).collect()
    }
}

fn keys_with_length(length: &Value, state: &mut State, remaining: &mut usize) -> Value {
    if *remaining == 0 {
        return Value::unknown();
    }
    *remaining -= 1;
    match length.node.as_ref() {
        ValueNode::Select { decision, yes, no } => Value::select(
            Rc::clone(decision),
            keys_with_length(yes, state, remaining),
            keys_with_length(no, state, remaining),
        ),
        ValueNode::Leaf(_) => {
            let Some(GuardValue::Int(length)) = length.constant() else {
                return Value::unknown();
            };
            let Ok(length) = usize::try_from(*length) else {
                return Value::unknown();
            };
            if length > 128 {
                return Value::unknown();
            }
            // Go map iteration does not establish an order for the returned keys.
            let slots = vec![Value::scalar(super::ScalarType::String, None); length];
            let backing = state
                .heap
                .allocate(ObjectData::Strings(slots), super::Multiplicity::Singleton);
            Value::strings(backing, length)
        }
    }
}

fn truncated_float(value: &str) -> Option<i64> {
    let number = value.parse::<f64>().ok()?;
    if !number.is_finite() || number.abs() > 9_007_199_254_740_991.0 {
        return None;
    }
    format!("{:.0}", number.trunc()).parse().ok()
}

fn copy_string_slice(
    slice: &super::StringSlice,
    state: &mut State,
    influences: BTreeSet<helm_schema_core::ValuesPath>,
) -> Value {
    let Some(ObjectVersion {
        data: ObjectData::Strings(slots),
        revision: _,
        multiplicity: _,
    }) = state.heap.version.object(&slice.backing)
    else {
        let backing = state.heap.allocate(
            ObjectData::Widened {
                inputs: Rc::new(influences.clone()),
            },
            super::Multiplicity::Singleton,
        );
        return Value::with_influences(
            Value::leaf(Datum::Strings(super::StringSlice {
                backing,
                start: 0,
                length: slice.length,
                capacity: slice.capacity,
            })),
            influences,
        );
    };
    let Some(visible) = slots.get(slice.start..slice.start.saturating_add(slice.length)) else {
        return Value::influenced(influences);
    };
    let mut slots = visible.to_vec();
    if let Some(capacity) = slice.capacity {
        slots.resize_with(capacity, || Value::string(""));
    }
    let backing = state
        .heap
        .allocate(ObjectData::Strings(slots), super::Multiplicity::Singleton);
    Value::leaf(Datum::Strings(super::StringSlice {
        backing,
        start: 0,
        length: slice.length,
        capacity: slice.capacity,
    }))
}

struct SequenceBounds {
    length: usize,
    capacity: Option<usize>,
    known_capacity: usize,
}

fn sequence_bounds(value: &Value, state: &State) -> Option<SequenceBounds> {
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(sequence),
            origin: _,
        }) => Some(SequenceBounds {
            length: sequence.length,
            capacity: sequence.capacity,
            known_capacity: sequence
                .storage
                .as_ref()
                .map_or(0, |storage| storage.len().saturating_sub(sequence.start)),
        }),
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(slice),
            origin: _,
        }) => Some(SequenceBounds {
            length: slice.length,
            capacity: slice.capacity,
            known_capacity: state
                .heap
                .version
                .object(&slice.backing)
                .and_then(|object| match &object.data {
                    ObjectData::Strings(slots) => Some(slots.len().saturating_sub(slice.start)),
                    ObjectData::Map {
                        fields: _,
                        remainder: _,
                    }
                    | ObjectData::Opaque { reachable: _ }
                    | ObjectData::Widened { inputs: _ } => None,
                })
                .unwrap_or(0),
        }),
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        })
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => None,
    }
}

fn interface_map_view(first: &Value, state: &mut State) -> Value {
    let view = match first.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Map { view: _, target },
            origin: _,
        }) => Value::leaf(Datum::Map {
            view: MapType::Interface,
            target: target.clone(),
        }),
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _,
        })
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => state.map_target(first).map_or_else(
            || first.clone(),
            |target| {
                Value::leaf(Datum::Map {
                    view: MapType::Interface,
                    target: Some(target),
                })
            },
        ),
    };
    Value::with_influences(view, state.heap.freeze(first).inputs())
}

fn overwrite_scalar_sources(
    target: &super::ObjectId,
    arguments: &[Value],
    state: &mut State,
) -> bool {
    for source in arguments.iter().skip(1) {
        let Some(source) = map_fields(source, state) else {
            return false;
        };
        for (key, field) in source.fields {
            match field {
                Field::Absent => {}
                Field::Present(value) => {
                    state.heap.write(target, &key, Field::Present(value));
                }
                Field::Unknown
                | Field::Select {
                    decision: _,
                    yes: _,
                    no: _,
                } => return false,
            }
        }
    }
    true
}

fn mutation_targets(
    value: &Value,
    pending: &mut Vec<Value>,
    targets: &mut BTreeSet<super::ObjectId>,
) -> bool {
    match value.node.as_ref() {
        ValueNode::Select {
            decision: _,
            yes,
            no,
        } => {
            pending.extend([yes.clone(), no.clone()]);
        }
        ValueNode::Leaf(Leaf { datum, origin: _ }) => match datum {
            Datum::Text(_)
            | Datum::Scalar {
                go_type: _,
                content: _,
            }
            | Datum::Map {
                view: _,
                target: None,
            } => {}
            Datum::Map {
                view: _,
                target: Some(id),
            }
            | Datum::Slot {
                backing: id,
                index: _,
            } => {
                targets.insert(id.clone());
            }
            Datum::Strings(slice) => {
                targets.insert(slice.backing.clone());
            }
            Datum::Sequence(sequence) => pending.extend(
                sequence
                    .storage
                    .iter()
                    .flat_map(|items| items.iter().cloned()),
            ),
            Datum::Unknown(unknown) => match &unknown.aliases {
                super::AliasSet::Known(ids) => targets.extend(ids.iter().cloned()),
                super::AliasSet::RenderGraph => {
                    return true;
                }
            },
        },
    }
    false
}

fn can_overwrite_scalar_sources(
    destination: &Value,
    target: &super::ObjectId,
    arguments: &[Value],
    state: &State,
    overwrite: bool,
) -> bool {
    let concrete_destination = matches!(
        destination.node.as_ref(),
        ValueNode::Leaf(Leaf {
            datum: Datum::Map {
                target: Some(_),
                view: _
            },
            origin: _,
        })
    ) || destination
        .identity()
        .is_some_and(|path| path.segments().next().is_none());
    let singleton_destination = matches!(
        state.heap.version.object(target),
        Some(ObjectVersion {
            revision: _,
            multiplicity: super::Multiplicity::Singleton,
            data: ObjectData::Map {
                fields: _,
                remainder: _
            },
        })
    );
    let mut remaining = 128;
    overwrite
        && concrete_destination
        && singleton_destination
        && arguments
            .iter()
            .skip(1)
            .all(|source| closed_scalar_source(source, state, &mut remaining))
}
