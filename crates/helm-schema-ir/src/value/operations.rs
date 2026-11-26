use super::relationship::{Decoder, ValueOperation};
use super::text::TextTerm;
use super::{
    AbortReason, Datum, Field, Leaf, MapType, ObjectData, ObjectVersion, Remainder, State,
    TextPart, Value, ValueNode,
};
use helm_schema_core::{Guard, GuardValue, Predicate, ValuePattern, ValuesPath};
use helm_schema_syntax::{HelmYamlDocument, HelmYamlError, helm_yaml_document};
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
    let observes_contents =
        invocation.callee.semantics().input_read == crate::function_semantics::InputRead::Contents;
    let values: Vec<_> = invocation
        .operands
        .iter()
        .map(|operand| {
            if observes_contents {
                state.consumed_contents(&operand.value)
            } else {
                operand.value.clone()
            }
        })
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
    let result = transfer_known(input, state)?;
    Ok(bounded_result(result, arguments, state))
}

/// A Go template function returns only values it creates or references its arguments reach, so
/// an unknown result of unrecorded reach may reference only those objects.
fn bounded_result(result: Value, arguments: &[Value], state: &mut State) -> Value {
    fn unbounded(value: &Value) -> bool {
        match value.node.as_ref() {
            ValueNode::Select {
                decision: _,
                yes,
                no,
            } => unbounded(yes) || unbounded(no),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Unknown(super::UnknownValue {
                        types: _,
                        aliases: super::Reach::Top,
                    }),
                origin,
            }) => origin.identity.is_none(),
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }) => false,
        }
    }
    fn bounded(value: &Value, reach: &super::Reach) -> Value {
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => {
                Value::select(Rc::clone(decision), bounded(yes, reach), bounded(no, reach))
            }
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Unknown(super::UnknownValue {
                        types,
                        aliases: super::Reach::Top,
                    }),
                origin,
            }) if origin.identity.is_none() => Value {
                node: Rc::new(ValueNode::Leaf(Leaf {
                    datum: Datum::Unknown(super::UnknownValue {
                        types: types.clone(),
                        aliases: reach.clone(),
                    }),
                    origin: origin.clone(),
                })),
            },
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }) => value.clone(),
        }
    }
    if !unbounded(&result) {
        return result;
    }
    let Reached {
        objects,
        bounded: known,
        undecided: _,
    } = reached_objects(arguments, state);
    if !known {
        return result;
    }
    bounded(&result, &super::Reach::Bounded(Rc::new(objects)))
}

/// Records what a call demands of its operands before it executes: catalog parsers abort outside
/// their grammar, and printing and serialization need a known operand.
fn record_operand_demands(function: &str, arguments: &[Value], state: &mut State) {
    record_parser_failures(function, arguments, state);
    if matches!(
        function,
        "quote" | "squote" | "toString" | "toYaml" | "toJson" | "toPrettyJson" | "toRawJson"
    ) {
        for argument in arguments {
            state.record_requirement(
                argument,
                &super::Predicate::True,
                super::query::RequirementKind::KnownAny,
            );
        }
    }
}

fn transfer_known(input: TransferInput<'_>, state: &mut State) -> Result<Value, TransferDemand> {
    let function = input.function;
    record_operand_demands(function, input.arguments, state);
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
        "semverCompare" => Ok(transfer_semver_compare(input, state)),
        "contains" => Ok(transfer_contains(input, state)),
        "regexMatch" | "mustRegexMatch" | "hasPrefix" | "hasSuffix" => {
            Ok(transfer_string_match(input, state))
        }
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
        "regexSplit" => Ok(transfer_regex_split(input, state)),
        "join" => return transfer_join(input, state),
        "pick" | "omit" => Ok(select_map(input.function, input.arguments, state)),
        "pluck" => Ok(pluck(input.arguments, state)),
        "merge" | "mustMerge" | "mergeOverwrite" | "mustMergeOverwrite" => {
            return Ok(merge(
                input.arguments,
                state,
                input.function.contains("Overwrite"),
            ));
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
        "int" | "int64" | "float64" => Ok(transfer_numeric_cast(input, state)),
        "upper" | "lower" | "trim" | "toString" => Ok(transfer_string_transform(input, state)),
        "print" | "println" => Ok(transfer_print(input, state)),
        "printf" => Ok(transfer_printf(input, state)),
        "trunc" | "trimSuffix" | "trimPrefix" | "replace" => Ok(transfer_text_edit(input, state)),
        "sub" => Ok(transfer_subtraction(input, state)),
        "regexReplaceAll"
        | "regexReplaceAllLiteral"
        | "mustRegexReplaceAll"
        | "mustRegexReplaceAllLiteral" => Ok(transfer_regex_replace(input, state)),
        "indent" | "nindent" => Ok(transfer_indent(input, state)),
        "repeat" => Ok(transfer_repeat(input, state)),
        "semver" => transfer_semver(&input, state),
        _ => Ok(transfer_fallback(input, state)),
    }
    .map_err(TransferDemand::Abort)
}

/// A call without an executed transfer: its catalog heap effect, and a result influenced by its
/// operands.
fn transfer_fallback(input: TransferInput<'_>, state: &mut State) -> Value {
    match crate::function_semantics::function_semantics(input.function).heap_effect {
        crate::function_semantics::HeapEffect::ReadOnly => {}
        crate::function_semantics::HeapEffect::Operands => {
            widen_operands(input.arguments, state);
        }
        crate::function_semantics::HeapEffect::Unknown => {
            invalidate(state);
            return Value::influenced(input.influences);
        }
    }
    // Without heap effects the result is fresh or one of what the operands reference.
    super::heap::unknown_member(input.arguments, input.influences)
}

fn transfer_subtraction(input: TransferInput<'_>, state: &mut State) -> Value {
    let (Some(GuardValue::Int(left)), Some(GuardValue::Int(right))) =
        (input.first.constant(), input.last.constant())
    else {
        return transfer_fallback(input, state);
    };
    Value::scalar(
        super::ScalarType::Int64,
        Some(super::scalar_value::ScalarContent::Literal(
            GuardValue::Int(left.wrapping_sub(*right)),
        )),
    )
    .with_influences(input.influences)
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
        state.record_requirement(
            &value,
            &super::Predicate::True,
            super::query::RequirementKind::MapArgument { allows_nil: false },
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
                    .path_guard(&state.active)
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
                .path_guard(&state.active)
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

/// Records the abort of every strict parser operand outside its grammar (catalog grammar column).
///
/// No abort exit is built here: the failure is recorded as its proved input preimage, and the
/// continuing state keeps the inputs the grammar admits.
fn record_parser_failures(function: &str, arguments: &[Value], state: &mut State) {
    use helm_schema_core::ValuePattern;

    let semantics = crate::function_semantics::function_semantics(function);
    let mut demands = Vec::new();
    if let Some(demand) =
        crate::function_semantics::strict_parser_operand_pattern(semantics, arguments.len())
    {
        demands.push(demand);
    }
    for index in 0..arguments.len() {
        if let Some(grammar) =
            crate::function_semantics::strict_collection_item_pattern(semantics, index)
        {
            demands.push((
                index,
                ValuePattern::Array {
                    prefix: Vec::new(),
                    items: Some(Box::new(ValuePattern::String {
                        pattern: grammar.to_owned(),
                    })),
                },
            ));
        }
    }
    for (index, pattern) in demands {
        let Some(operand) = arguments.get(index) else {
            continue;
        };
        if let Some(preimage) = lexical_preimage(state, operand, &pattern) {
            state.record_failure(&preimage.unmatched);
        }
    }
}

pub(super) fn lexical_preimage(
    state: &State,
    value: &Value,
    pattern: &helm_schema_core::ValuePattern,
) -> Option<super::relationship::LexicalPreimage> {
    let frozen = state.heap.freeze(value);
    let inputs = state.heap.arena.input_objects.borrow();
    let mut query = super::relationship::RelationshipQuery::new(&inputs, 512);
    let ambient = state.heap.arena.path_guard(&state.active);
    query.lexical_preimage(&frozen, pattern, &ambient)
}

/// Compares through the constraint's exact version language, lifted to the version's inputs.
fn transfer_semver_compare(input: TransferInput<'_>, state: &State) -> Value {
    let TransferInput {
        function: _,
        arguments,
        influences,
        first,
        last: _,
    } = input;

    let Some(GuardValue::String(constraint)) = first.constant() else {
        return Value::influenced(influences);
    };
    let Some(version) = arguments.get(1) else {
        return Value::influenced(influences);
    };
    if let Some(GuardValue::String(version)) = version.constant() {
        let result = helm_schema_ast::semver_constraint_matches_version(constraint, version);
        return Value::with_influences(
            result.map_or_else(
                || Value::scalar(super::ScalarType::Bool, None),
                |result| Value::literal(GuardValue::Bool(result)),
            ),
            influences,
        );
    }
    // Spellings outside the parser's grammar abort through the recorded parse failure, so on
    // every live path the constraint's language alone decides the comparison.
    let Some(accepted) = helm_schema_ast::semver_constraint_match_pattern(constraint) else {
        return Value::with_influences(Value::scalar(super::ScalarType::Bool, None), influences);
    };
    let Some(preimage) = lexical_preimage(state, version, &accepted) else {
        return Value::with_influences(Value::scalar(super::ScalarType::Bool, None), influences);
    };
    let super::relationship::LexicalPreimage {
        matched,
        unmatched,
        exact,
    } = preimage;
    super::ordering::certified_truth(state, matched, unmatched, exact, influences)
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
                            reachable: super::heap::values_reach(arguments),
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
            widen_map(&target, &influences, arguments.get(2), state);
            return Ok(Value::with_influences(first, influences));
        }
        invalidate(state);
        return Ok(Value::influenced(influences));
    };
    // `set` and `unset` abort on a non-map receiver, so only a receiver's map arm writes.
    let first = super::layers::map_operand(&first).unwrap_or(first);
    if super::layers::known_non_map(&first) {
        return Err(AbortReason::Type);
    }
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
                state.record_requirement(
                    &value,
                    &super::Predicate::True,
                    super::query::RequirementKind::StringIndex,
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

    if function == "last"
        && let Some(super::relationship::Derivation {
            source,
            operation: super::relationship::ValueOperation::SplitLiteral { separator },
        }) = first.operation_preimage()
    {
        if let Some(GuardValue::String(text)) = source.value.constant()
            && let Some(segment) = text.split(separator.as_str()).last()
        {
            return Ok(Value::string(segment));
        }
        return Ok(Value::scalar(super::ScalarType::String, None).derived(
            source.clone(),
            super::relationship::ValueOperation::SplitSegment(helm_schema_core::SplitSegmentUse {
                separator: separator.clone(),
                last: true,
            }),
        ));
    }

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

fn transfer_regex_split(input: TransferInput<'_>, state: &State) -> Value {
    let TransferInput {
        function: _,
        arguments,
        influences,
        first: _,
        last: _,
    } = input;

    let [pattern, source, limit] = arguments else {
        return Value::influenced(influences);
    };
    let Some(GuardValue::Int(limit)) = limit.constant() else {
        return Value::influenced(influences);
    };
    let Some(GuardValue::String(pattern)) = pattern.constant().filter(|_| *limit < 0) else {
        return Value::influenced(influences);
    };
    let Some(pattern) = helm_schema_core::re2_to_ecma_pattern(pattern) else {
        return Value::influenced(influences);
    };
    let Ok(parsed) = regex_syntax::Parser::new().parse(&pattern) else {
        return Value::influenced(influences);
    };
    let regex_syntax::hir::HirKind::Literal(literal) = parsed.kind() else {
        return Value::influenced(influences);
    };
    let Ok(separator) = std::str::from_utf8(&literal.0) else {
        return Value::influenced(influences);
    };
    if helm_schema_core::escape_regex_literal(separator) != pattern {
        return Value::influenced(influences);
    }
    Value::leaf(Datum::Unknown(super::UnknownValue {
        types: super::TypeCoverage::Sequence,
        aliases: super::Reach::Bounded(Rc::new(BTreeSet::new())),
    }))
    .with_influences(influences)
    .derived(
        state.heap.freeze(source),
        super::relationship::ValueOperation::SplitLiteral {
            separator: separator.to_owned(),
        },
    )
}

fn transfer_join(input: TransferInput<'_>, state: &mut State) -> Result<Value, TransferDemand> {
    let TransferInput {
        function: _,
        arguments: _,
        influences,
        first,
        last,
    } = input;

    // The joined text references no object.
    let text = || super::heap::unknown_member(&[], influences.clone());
    let Some(GuardValue::String(separator)) = first.constant() else {
        return Ok(text());
    };
    let Some(items) = sequence_items(&last, state) else {
        return Ok(text());
    };
    let Some(strings) = string_members(&items, state)? else {
        return Ok(text());
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
    let guard = state.heap.arena.path_guard(&state.active);
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
    super::text_actions::contains_with(&last, needle, state, false)
        .or_else(|| {
            string_language_truth(
                state,
                &last,
                helm_schema_core::escape_regex_literal(needle),
                influences.clone(),
            )
        })
        .unwrap_or_else(|| super::text_actions::contains(&last, needle, state))
}

/// `regexMatch`, `hasPrefix` and `hasSuffix`: membership of the string subject (the last operand)
/// in the language the literal first operand names, lifted to the subject's inputs through its
/// executed text transforms. Every other subject kind aborts the call before it decides.
fn transfer_string_match(input: TransferInput<'_>, state: &mut State) -> Value {
    let (Some(GuardValue::String(operand)), [_, subject]) =
        (input.first.constant(), input.arguments)
    else {
        return transfer_fallback(input, state);
    };
    let pattern = match input.function {
        "hasPrefix" => format!("^{}", helm_schema_core::escape_regex_literal(operand)),
        "hasSuffix" => format!("{}$", helm_schema_core::escape_regex_literal(operand)),
        _ => {
            let Some(pattern) = helm_schema_core::re2_to_ecma_pattern(operand) else {
                return transfer_fallback(input, state);
            };
            pattern
        }
    };
    string_language_truth(state, subject, pattern, input.influences.clone())
        .unwrap_or_else(|| transfer_fallback(input, state))
}

/// The truth of a string subject's membership in a regex language, when its lift decides any
/// input.
pub(super) fn string_language_truth(
    state: &State,
    subject: &Value,
    pattern: String,
    influences: BTreeSet<helm_schema_core::ValuesPath>,
) -> Option<Value> {
    regex::Regex::new(&pattern).ok()?;
    let super::relationship::LexicalPreimage {
        matched,
        unmatched,
        exact,
    } = lexical_preimage(
        state,
        subject,
        &helm_schema_core::ValuePattern::String { pattern },
    )?;
    if !exact && matched == super::Predicate::False && unmatched == super::Predicate::False {
        return None;
    }
    Some(super::ordering::certified_truth(
        state, matched, unmatched, exact, influences,
    ))
}

fn transfer_deserialize(input: TransferInput<'_>, state: &mut State) -> Value {
    let TransferInput {
        function,
        arguments: _,
        influences,
        first,
        last: _,
    } = input;

    let decoder = match function {
        "fromYaml" => Decoder::YamlMap,
        "fromYamlArray" => Decoder::YamlSequence,
        "fromJson" => Decoder::JsonMap,
        "fromJsonArray" => Decoder::JsonSequence,
        _ => Decoder::JsonValue,
    };
    if first.constant().is_none() {
        let frozen = state.heap.freeze(&first);
        if let ValueNode::Leaf(Leaf {
            datum: Datum::Text(text),
            origin: _,
        }) = first.node.as_ref()
        {
            let operand = match (decoder, text.term()) {
                (
                    Decoder::YamlMap,
                    TextTerm::Yaml {
                        value,
                        pretty: false,
                    },
                )
                | (
                    Decoder::JsonMap,
                    TextTerm::Json {
                        value,
                        pretty: _,
                        escape_html: _,
                        literal: None,
                    },
                ) => Some(value),
                _ => None,
            };
            if let Some(operand) = operand {
                let mut projection = DecodeSnapshot {
                    heap: &operand.heap,
                    decoder,
                    yaml_inputs: BTreeSet::new(),
                    budget: super::work::WorkBudget::new(4096),
                };
                if let Some(decoded) = projection.result(&operand.value, &frozen, state) {
                    let stable = projection.yaml_stability();
                    if stable == Predicate::True {
                        return decoded;
                    }
                    let decision = state.heap.arena.decision(
                        stable.clone(),
                        stable.negated(),
                        super::Coverage::Exact,
                    );
                    let unknown = unknown_decoded(function, influences, state)
                        .derived(frozen, ValueOperation::Decode(decoder));
                    return Value::select(decision, decoded, unknown);
                }
            }
        }
        return unknown_decoded(function, influences, state)
            .derived(frozen, ValueOperation::Decode(decoder));
    }
    let Some(GuardValue::String(text)) = first.constant() else {
        return unknown_decoded(function, influences, state);
    };
    decode_literal(decoder, text, state)
        .unwrap_or_else(|| unknown_decoded(function, influences, state))
}

fn decode_literal(decoder: Decoder, text: &str, state: &mut State) -> Option<Value> {
    let parsed = match decoder {
        Decoder::YamlMap | Decoder::YamlSequence => match helm_yaml_document(text) {
            HelmYamlDocument::Decoded { value, unknown } if unknown.is_empty() => Some(value),
            HelmYamlDocument::Failed(HelmYamlError::Rejected) if decoder == Decoder::YamlMap => {
                return Some(decoded_error_map(state));
            }
            HelmYamlDocument::Decoded {
                value: _,
                unknown: _,
            }
            | HelmYamlDocument::Failed(_) => None,
        },
        Decoder::JsonMap | Decoder::JsonSequence | Decoder::JsonValue => {
            serde_json::from_str::<serde_json::Value>(text).ok()
        }
    }?;
    if parsed.is_null() {
        return Some(match decoder {
            Decoder::YamlMap | Decoder::JsonMap => decoded_null_map(decoder, state),
            Decoder::YamlSequence => Value::sequence(Vec::new()),
            Decoder::JsonSequence => Value::nil_sequence(),
            Decoder::JsonValue => Value::nil(),
        });
    }
    match decoder {
        Decoder::YamlMap | Decoder::JsonMap if !parsed.is_object() => {
            Some(decoded_error_map(state))
        }
        Decoder::YamlSequence | Decoder::JsonSequence if !parsed.is_array() => None,
        Decoder::YamlMap
        | Decoder::JsonMap
        | Decoder::YamlSequence
        | Decoder::JsonSequence
        | Decoder::JsonValue => Some(decode(&parsed, state)),
    }
}

fn decoded_null_map(decoder: Decoder, state: &mut State) -> Value {
    if decoder == Decoder::YamlMap {
        state.heap.map(Remainder::Closed)
    } else {
        Value::leaf(Datum::Map {
            view: MapType::Interface,
            target: None,
        })
    }
}

fn decoded_error_map(state: &mut State) -> Value {
    fresh_map(
        MapFields {
            fields: BTreeMap::from([(
                "Error".to_owned(),
                Field::Present(Value::scalar(super::ScalarType::String, None)),
            )]),
            remainder: Remainder::Closed,
        },
        BTreeSet::new(),
        state,
    )
}

struct DecodeSnapshot<'a> {
    heap: &'a super::HeapVersion,
    decoder: Decoder,
    yaml_inputs: BTreeSet<ValuesPath>,
    budget: super::work::WorkBudget,
}

impl DecodeSnapshot<'_> {
    fn result(
        &mut self,
        operand: &Value,
        text: &super::FrozenValue,
        state: &mut State,
    ) -> Option<Value> {
        if let Some(path) = self.pristine_input(operand, state) {
            let decoded = state
                .decoded_input_map(&path)
                .derived(text.clone(), ValueOperation::Decode(self.decoder));
            if known_input_map(operand, &path) {
                return Some(decoded);
            }
            let object = state.input_decision(Guard::TypeIs {
                path: path.clone(),
                schema_type: "object".to_owned(),
            });
            let nil = super::kinds::input_nil_predicate(&path);
            let nil = state
                .heap
                .arena
                .decision(nil.clone(), nil.negated(), super::Coverage::Exact);
            let missing = decoded_null_map(self.decoder, state)
                .with_influences(BTreeSet::from([path.clone()]));
            let error = decoded_error_map(state).with_influences(BTreeSet::from([path]));
            return Some(Value::select(
                object,
                decoded,
                Value::select(nil, missing, error),
            ));
        }
        // An unsupported sibling can cause encoding to fail before any key reaches the decoder.
        let value = self.value(operand, state, 0)?;
        match value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        view: _,
                        target: Some(_),
                    },
                origin: _,
            }) => Some(value.derived(text.clone(), ValueOperation::Decode(self.decoder))),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Scalar {
                        go_type: super::ScalarType::Nil,
                        content: _,
                    },
                origin: _,
            }) => Some(decoded_null_map(self.decoder, state)),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Scalar {
                        go_type: _,
                        content: _,
                    }
                    | Datum::Sequence(_),
                origin: _,
            }) => Some(decoded_error_map(state)),
            _ => None,
        }
    }

    fn yaml_stability(&self) -> Predicate {
        // YAML can interpret an input key as a merge. The existing flow-stability proof excludes
        // merge keys at every depth. Outside its domain the result stays unknown.
        Predicate::all(
            self.yaml_inputs
                .iter()
                .map(|path| {
                    let stable = Predicate::from(Guard::MatchesValue {
                        path: path.clone(),
                        pattern: ValuePattern::YamlFlowStable,
                    });
                    if path.segments().next().is_none() {
                        stable
                    } else {
                        Predicate::Or(vec![
                            Predicate::from(Guard::Absent { path: path.clone() }),
                            stable,
                        ])
                    }
                })
                .collect(),
        )
        .normalize_boolean()
    }

    fn pristine_input(&mut self, value: &Value, state: &State) -> Option<ValuesPath> {
        let path = match value.node.as_ref() {
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Map {
                        view: MapType::Interface | MapType::Values,
                        target: Some(id),
                    },
                origin: _,
            }) => self.heap.input_paths.get(id).cloned(),
            ValueNode::Leaf(Leaf {
                datum:
                    Datum::Unknown(super::UnknownValue {
                        types:
                            super::TypeCoverage::Unresolved
                            | super::TypeCoverage::Map(MapType::Interface | MapType::Values),
                        aliases: _,
                    }),
                origin: _,
            }) => value.identity().cloned(),
            _ => None,
        }?;
        let inputs = state.heap.arena.input_objects.borrow();
        if !super::relationship::RelationshipQuery::new(&inputs, 1024)
            .original_unchanged(&path, self.heap)
        {
            return None;
        }
        if self.decoder == Decoder::YamlMap {
            self.yaml_inputs.insert(path.clone());
        }
        Some(path)
    }

    fn value(&mut self, value: &Value, state: &mut State, depth: usize) -> Option<Value> {
        use super::ScalarType;
        if depth >= 32 {
            self.budget.exhaust();
            return None;
        }
        if !self.budget.charge(1) {
            return None;
        }
        if let Some(path) = self.pristine_input(value, state) {
            let map = state.decoded_input_map(&path);
            return Some(if known_input_map(value, &path) {
                map
            } else {
                state.decoded_input_member(&path, map)
            });
        }
        match value.node.as_ref() {
            ValueNode::Select { decision, yes, no } => Some(Value::select(
                Rc::clone(decision),
                self.value(yes, state, depth + 1)?,
                self.value(no, state, depth + 1)?,
            )),
            ValueNode::Leaf(Leaf { datum, origin: _ }) => match datum {
                Datum::Map { view, target } => match view {
                    MapType::Interface | MapType::Values => match target {
                        Some(id) => self.map(id, state, depth + 1),
                        None => Some(Value::nil()),
                    },
                    MapType::Files
                    | MapType::ApiVersions
                    | MapType::Capabilities
                    | MapType::KubeVersion
                    | MapType::Semver
                    | MapType::String => None,
                },
                Datum::Sequence(sequence) => match &sequence.storage {
                    None => Some(Value::nil()),
                    Some(storage) if storage.is_nil() => Some(Value::nil()),
                    Some(storage) if storage.is_bytes() => None,
                    Some(_) => self.sequence(sequence, state, depth + 1),
                },
                Datum::Scalar {
                    go_type,
                    content: _,
                } => {
                    // Both codecs return float64, and YAML can drop the sign of zero.
                    let number =
                        || Value::scalar(ScalarType::Float64, None).with_influences(value.inputs());
                    match go_type {
                        ScalarType::Invalid | ScalarType::Nil => Some(Value::nil()),
                        ScalarType::Bool | ScalarType::String => Some(value.clone()),
                        ScalarType::Int
                        | ScalarType::Int64
                        | ScalarType::Uint8
                        | ScalarType::Uint64 => Some(number()),
                        ScalarType::Float64 => value.constant().and_then(|literal| {
                            let GuardValue::Float(text) = literal else {
                                return None;
                            };
                            text.parse::<f64>().is_ok_and(f64::is_finite).then(number)
                        }),
                        ScalarType::Complex128 => None,
                    }
                }
                Datum::Text(_) => {
                    Some(Value::scalar(ScalarType::String, None).with_influences(value.inputs()))
                }
                Datum::Unknown(_)
                | Datum::Strings(_)
                | Datum::Slot {
                    backing: _,
                    index: _,
                } => None,
            },
        }
    }

    fn map(&mut self, id: &super::ObjectId, state: &mut State, depth: usize) -> Option<Value> {
        let object = self.heap.borrowed_object(id)?;
        let ObjectData::Map {
            fields,
            remainder: Remainder::Closed,
        } = &object.data
        else {
            return None;
        };
        let mut decoded = BTreeMap::new();
        for (key, field) in fields.iter() {
            if self.decoder == Decoder::YamlMap && key == "<<" {
                return None;
            }
            decoded.insert(key.clone(), self.field(field, state, depth)?);
        }
        Some(fresh_map(
            MapFields {
                fields: decoded,
                remainder: Remainder::Closed,
            },
            BTreeSet::new(),
            state,
        ))
    }

    fn sequence(
        &mut self,
        sequence: &super::InterfaceSequence,
        state: &mut State,
        depth: usize,
    ) -> Option<Value> {
        let items = sequence
            .items()?
            .iter()
            .map(|item| self.value(item, state, depth))
            .collect::<Option<Vec<_>>>()?;
        let mut decoded = Value::sequence(items);
        if let ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(sequence),
            origin: _,
        }) = Rc::make_mut(&mut decoded.node)
        {
            sequence.capacity = None;
        }
        Some(decoded)
    }

    fn field(&mut self, field: &Field, state: &mut State, depth: usize) -> Option<Field> {
        if depth >= 32 {
            self.budget.exhaust();
            return None;
        }
        if !self.budget.charge(1) {
            return None;
        }
        match field {
            Field::Present(value) => Some(Field::Present(self.value(value, state, depth + 1)?)),
            Field::Absent => Some(Field::Absent),
            Field::Unknown => None,
            Field::Select { decision, yes, no } => Some(Field::select(
                Rc::clone(decision),
                self.field(yes, state, depth + 1)?,
                self.field(no, state, depth + 1)?,
            )),
        }
    }
}

fn known_input_map(value: &Value, path: &ValuesPath) -> bool {
    path.segments().next().is_none()
        || matches!(
            value.node.as_ref(),
            ValueNode::Leaf(Leaf {
                datum: Datum::Map {
                    view: _,
                    target: Some(_)
                } | Datum::Unknown(super::UnknownValue {
                    types: super::TypeCoverage::Map(_),
                    aliases: _
                }),
                origin: _,
            })
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

    if function == "quote" && arguments.len() > 1 {
        return quote_many(arguments, influences, state);
    }
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
            return Value::frozen_text(
                super::text::TextTerm::Quote {
                    value: state.heap.freeze(value),
                },
                influences.clone(),
            );
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

fn quote_many(arguments: &[Value], influences: BTreeSet<ValuesPath>, state: &State) -> Value {
    if arguments.len() > super::MAX_SCALAR_DISPATCH_STATES {
        return Value::influenced(influences);
    }
    let mut output = super::TextBuffer::default();
    let mut emitted = Value::literal(GuardValue::Bool(false));
    for argument in arguments {
        let frozen = state.heap.freeze(argument);
        let nil = super::text::quote_nil_condition(&frozen);
        if nil == Some(super::Predicate::True) {
            continue;
        }
        let quoted = transfer_quote(
            TransferInput {
                function: "quote",
                arguments: std::slice::from_ref(argument),
                influences: frozen.inputs(),
                first: argument.clone(),
                last: argument.clone(),
            },
            state,
        );
        let mut next = super::TextBuffer::default();
        let separator = Value::select(state.test(&emitted), Value::string(" "), Value::string(""));
        next.emit(state.heap.freeze(&separator));
        next.emit(state.heap.freeze(&quoted));
        let next = super::text::capture(&next, false);
        let next = if nil == Some(super::Predicate::False) {
            emitted = Value::literal(GuardValue::Bool(true));
            next
        } else {
            let decision = match nil {
                Some(predicate) => state.heap.arena.decision(
                    predicate.clone(),
                    predicate.negated(),
                    super::Coverage::Exact,
                ),
                None => state.heap.arena.decision(
                    super::Predicate::False,
                    super::Predicate::False,
                    super::Coverage::Partial,
                ),
            };
            emitted = Value::select(
                Rc::clone(&decision),
                emitted,
                Value::literal(GuardValue::Bool(true)),
            );
            Value::select(decision, Value::string(""), next)
        };
        output.emit(state.heap.freeze(&next));
    }
    super::text::capture(&output, false).with_influences(influences)
}

fn transfer_numeric_cast(input: TransferInput<'_>, state: &State) -> Value {
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
                derivation: None,
                identity: None,
                influences,
            },
        })),
    }
    .derived(
        state.heap.freeze(&first),
        super::relationship::ValueOperation::Numeric(kind),
    )
}

fn transfer_string_transform(input: TransferInput<'_>, state: &State) -> Value {
    let TransferInput {
        function,
        arguments: _,
        influences,
        first: _,
        last,
    } = input;

    if function == "toString"
        && let ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(sequence),
            origin: _,
        }) = last.node.as_ref()
        && let Some(bytes) = super::hosts::sequence_bytes(sequence)
    {
        return std::str::from_utf8(bytes)
            .map_or_else(|_| Value::influenced(influences.clone()), Value::string);
    }
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
    if function == "toString" {
        let value = scalar_text(&last).map_or_else(
            || Value::scalar(super::ScalarType::String, None).with_influences(influences),
            |text| Value::string(&text),
        );
        return value.derived(
            state.heap.freeze(&last),
            super::relationship::ValueOperation::Format(super::relationship::TextFormat::Value),
        );
    }
    if function == "trim" {
        let value = scalar_text(&last).map_or_else(
            || Value::scalar(super::ScalarType::String, None).with_influences(influences),
            |text| Value::string(text.trim()),
        );
        return value.derived(
            state.heap.freeze(&last),
            super::relationship::ValueOperation::Trim,
        );
    }
    let transform = if function == "upper" {
        super::relationship::TextTransform::Upper
    } else {
        super::relationship::TextTransform::Lower
    };
    scalar_text(&last)
        .and_then(|text| transform.apply(&text, 64 * 1024))
        .map_or_else(
            || Value::scalar(super::ScalarType::String, None).with_influences(influences),
            |text| Value::string(&text),
        )
        .derived(
            state.heap.freeze(&last),
            super::relationship::ValueOperation::Text(transform),
        )
}

fn transfer_print(input: TransferInput<'_>, state: &State) -> Value {
    let TransferInput {
        function,
        arguments,
        influences,
        first: _,
        last: _,
    } = input;

    let mut graph = super::TextBuffer::default();
    let mut previous_type = super::kinds::GoType::String;
    for (index, value) in arguments.iter().enumerate() {
        let current_type = state.heap.freeze(value).go_type();
        let is_string = current_type == super::kinds::GoType::String;
        let previous_string = previous_type == super::kinds::GoType::String;
        if function == "print"
            && index > 0
            && !previous_string
            && !is_string
            && [previous_type, current_type].contains(&super::kinds::GoType::Unknown)
        {
            // Sprint separates adjacent nonstrings; an unknown operand may be a string.
            return Value::scalar(super::ScalarType::String, None).with_influences(influences);
        }
        if index > 0 && (function == "println" || !previous_string && !is_string) {
            graph.bytes(" ");
        }
        if let Some(text) = scalar_text(value) {
            graph.bytes(&text);
        } else {
            graph.emit(state.heap.freeze(value));
        }
        previous_type = current_type;
    }
    if function == "println" {
        graph.bytes("\n");
    }
    super::text::capture(&graph, false).with_influences(influences)
}

fn transfer_printf(input: TransferInput<'_>, state: &State) -> Value {
    use super::kinds::GoType;
    use super::relationship::{TextFormat, ValueOperation};
    use helm_schema_ast::{PrintfPart, PrintfVerb, parse_supported_printf_format};

    let TransferInput {
        function: _,
        arguments,
        influences,
        first,
        last: _,
    } = input;

    let Some(GuardValue::String(format)) = first.constant() else {
        return Value::influenced(influences);
    };
    let Some(parts) = parse_supported_printf_format(format) else {
        return Value::influenced(influences);
    };
    let mut graph = super::TextBuffer::default();
    let mut arguments = arguments.iter().skip(1);
    for part in parts {
        match part {
            PrintfPart::Literal(text) => graph.bytes(text),
            PrintfPart::Substitution(verb) => {
                let Some(value) = arguments.next() else {
                    return Value::influenced(influences);
                };
                let frozen = state.heap.freeze(value);
                match (verb, frozen.go_type()) {
                    (PrintfVerb::String | PrintfVerb::Value, GoType::String)
                    | (PrintfVerb::Decimal, GoType::Int | GoType::Int64 | GoType::Uint64) => {
                        if let Some(text) = scalar_text(value) {
                            graph.bytes(&text);
                        } else {
                            graph.emit(frozen);
                        }
                    }
                    (PrintfVerb::String, _) => {
                        graph.emit(state.heap.freeze(&printf_string(value, state)));
                    }
                    (PrintfVerb::Value, _) => {
                        let formatted = scalar_text(&frozen.value).map_or_else(
                            || Value::scalar(super::ScalarType::String, None),
                            |text| Value::string(&text),
                        );
                        let formatted =
                            formatted.derived(frozen, ValueOperation::Format(TextFormat::Value));
                        graph.emit(state.heap.freeze(&formatted));
                    }
                    (PrintfVerb::Decimal, _) => return Value::influenced(influences),
                }
            }
        }
    }
    if arguments.next().is_some() {
        return Value::influenced(influences);
    }
    super::text::capture(&graph, false)
}

fn printf_string(value: &Value, state: &State) -> Value {
    printf_string_with_budget(value, state, &mut 128)
}

fn printf_string_with_budget(value: &Value, state: &State, remaining: &mut usize) -> Value {
    use helm_schema_core::{Guard, Predicate};
    if *remaining == 0 {
        return Value::influenced(value.inputs());
    }
    *remaining -= 1;
    if let ValueNode::Select { decision, yes, no } = value.node.as_ref() {
        let yes = printf_string_with_budget(yes, state, remaining);
        let no = printf_string_with_budget(no, state, remaining);
        return Value::select(Rc::clone(decision), yes, no);
    }
    if state.heap.freeze(value).go_type() == super::kinds::GoType::String {
        return value.clone();
    }
    // Formatted text references no object.
    let Some(path) = value.identity() else {
        return super::heap::unknown_member(&[], value.inputs());
    };
    let string = Predicate::from(Guard::TypeIs {
        path: path.clone(),
        schema_type: "string".to_owned(),
    });
    let scalar = Predicate::Or(vec![
        Predicate::from(Guard::Absent { path: path.clone() }),
        Predicate::from(Guard::TypeIs {
            path: path.clone(),
            schema_type: "null".to_owned(),
        }),
        Predicate::from(Guard::TypeIs {
            path: path.clone(),
            schema_type: "boolean".to_owned(),
        }),
        Predicate::from(Guard::TypeIs {
            path: path.clone(),
            schema_type: "number".to_owned(),
        }),
    ]);
    // Scalar mismatches have a mandatory diagnostic prefix.
    // Containers always print delimiters, and their leaves recursively consume the verb.
    let mut diagnostic = super::TextBuffer::default();
    diagnostic.bytes("%!s(");
    diagnostic.emit(state.heap.freeze(&Value::influenced(value.inputs())));
    diagnostic.bytes(")");
    let empty_sequence = helm_schema_core::Predicate::from(helm_schema_core::Guard::MatchesValue {
        path: path.clone(),
        pattern: helm_schema_core::ValuePattern::Array {
            prefix: Vec::new(),
            items: None,
        },
    });
    let container = Value::select(
        state.heap.arena.decision(
            empty_sequence.clone(),
            empty_sequence.negated(),
            super::Coverage::Exact,
        ),
        Value::string("[]"),
        Value::scalar(
            super::ScalarType::String,
            Some(super::scalar_value::ScalarContent::StringDomain {
                pattern: r"[\s\S]*",
                min_bytes: 1,
                max_bytes: usize::MAX,
            }),
        )
        .with_influences(value.inputs()),
    )
    .derived(
        state.heap.freeze(value),
        super::relationship::ValueOperation::Format(super::relationship::TextFormat::String),
    );
    let mismatch = Value::select(
        state
            .heap
            .arena
            .decision(scalar.clone(), scalar.negated(), super::Coverage::Exact),
        super::text::capture(&diagnostic, false),
        container,
    );
    Value::select(
        state
            .heap
            .arena
            .decision(string.clone(), string.negated(), super::Coverage::Exact),
        super::kinds::refined_parameter(value, super::kinds::GoType::String),
        mismatch,
    )
}

fn transfer_text_edit(input: TransferInput<'_>, state: &State) -> Value {
    use super::relationship::{TextTransform, ValueOperation};
    let transform = match (input.function, input.first.constant()) {
        ("trunc", Some(GuardValue::Int(length))) => TextTransform::Truncate(*length),
        ("trimSuffix", Some(GuardValue::String(suffix))) => {
            TextTransform::TrimSuffix(suffix.clone())
        }
        ("trimPrefix", Some(GuardValue::String(prefix))) => {
            TextTransform::TrimPrefix(prefix.clone())
        }
        ("replace", Some(GuardValue::String(from))) => {
            let Some(GuardValue::String(to)) = input.arguments.get(1).and_then(Value::constant)
            else {
                return Value::influenced(input.influences);
            };
            TextTransform::Replace {
                from: from.clone(),
                to: to.clone(),
            }
        }
        _ => return Value::influenced(input.influences),
    };
    let frozen = state.heap.freeze(&input.last);
    let text = scalar_text(&input.last).and_then(|text| transform.apply(&text, 64 * 1024));
    if text.is_none() && edit_keeps_every_spelling(&transform, &frozen.value) {
        // The whole operand is the result: its own choices, parts and provenance stay intact.
        return input.last.with_influences(input.influences);
    }
    let value = text.map_or_else(
        || Value::scalar(super::ScalarType::String, None).with_influences(input.influences),
        |text| Value::string(&text),
    );
    value.derived(frozen, ValueOperation::Text(transform))
}

/// Whether a whole-text edit returns every possible spelling of `value` unchanged: Sprig's
/// `trunc` slices bytes and keeps a text no longer than its width, and a trim keeps a text that
/// cannot carry its affix.
fn edit_keeps_every_spelling(
    transform: &super::relationship::TextTransform,
    value: &Value,
) -> bool {
    use super::relationship::TextTransform;
    let mut budget = 256;
    match transform {
        TextTransform::Truncate(width) => usize::try_from(width.unsigned_abs())
            .is_ok_and(|width| max_bytes(value, &mut budget).is_some_and(|bound| bound <= width)),
        TextTransform::TrimPrefix(affix) => {
            affix.is_empty() || lacks_affix(value, affix, TextEnd::Start, &mut budget)
        }
        TextTransform::TrimSuffix(affix) => {
            affix.is_empty() || lacks_affix(value, affix, TextEnd::End, &mut budget)
        }
        TextTransform::Upper
        | TextTransform::Lower
        | TextTransform::Replace { from: _, to: _ }
        | TextTransform::Cut(_)
        | TextTransform::RegexReplace {
            pattern: _,
            replacement: _,
        } => false,
    }
}

/// An upper bound on the bytes of every spelling, or `None` when there is none.
fn max_bytes(value: &Value, budget: &mut usize) -> Option<usize> {
    *budget = budget.checked_sub(1)?;
    if let Some(GuardValue::String(text)) = value.constant() {
        return Some(text.len());
    }
    let datum = match value.node.as_ref() {
        ValueNode::Select {
            decision: _,
            yes,
            no,
        } => return Some(max_bytes(yes, budget)?.max(max_bytes(no, budget)?)),
        ValueNode::Leaf(Leaf { datum, origin: _ }) => datum,
    };
    match datum {
        Datum::Scalar {
            go_type: _,
            content:
                Some(super::scalar_value::ScalarContent::StringDomain {
                    pattern: _,
                    min_bytes: _,
                    max_bytes,
                }),
        } => Some(*max_bytes),
        Datum::Text(text) => match text.term() {
            super::text::TextTerm::Resolved { term: _, literal } => Some(literal.to_string().len()),
            super::text::TextTerm::Capture {
                graph,
                strip_marker: _,
            } => buffer_max_bytes(graph, budget),
            super::text::TextTerm::Encoded {
                value: _,
                alphabet: _,
            }
            | super::text::TextTerm::Quote { value: _ }
            | super::text::TextTerm::Yaml {
                value: _,
                pretty: _,
            }
            | super::text::TextTerm::Json {
                value: _,
                pretty: _,
                escape_html: _,
                literal: _,
            }
            | super::text::TextTerm::Tpl {
                source: _,
                result: _,
            }
            | super::text::TextTerm::Indent {
                value: _,
                width: _,
                newline: _,
            } => None,
        },
        Datum::Scalar {
            go_type: _,
            content:
                None
                | Some(
                    super::scalar_value::ScalarContent::Literal(_)
                    | super::scalar_value::ScalarContent::Identity(_),
                ),
        }
        | Datum::Map { view: _, target: _ }
        | Datum::Sequence(_)
        | Datum::Strings(_)
        | Datum::Slot {
            backing: _,
            index: _,
        }
        | Datum::Unknown(_) => None,
    }
}

/// The sum of the parts' bounds; stripping Helm's marker only shortens a text.
fn buffer_max_bytes(buffer: &super::TextBuffer, budget: &mut usize) -> Option<usize> {
    buffer.parts.iter().try_fold(0_usize, |total, node| {
        *budget = budget.checked_sub(1)?;
        let part = match node.content() {
            TextPart::Bytes(text) => text.len(),
            TextPart::Frozen(value) => max_bytes(&value.value, budget)?,
            TextPart::Select {
                decision: _,
                yes,
                no,
            } => buffer_max_bytes(yes, budget)?.max(buffer_max_bytes(no, budget)?),
            TextPart::Unknown(_) | TextPart::Repeat(_) => return None,
        };
        total.checked_add(part)
    })
}

#[derive(Clone, Copy)]
enum TextEnd {
    Start,
    End,
}

/// Whether no spelling of `value` starts or ends with `affix`, decided on its first or last
/// part. A part shorter than the affix leaves the neighbouring part undecided, so it decides
/// nothing.
fn lacks_affix(value: &Value, affix: &str, end: TextEnd, budget: &mut usize) -> bool {
    let Some(rest) = budget.checked_sub(1) else {
        return false;
    };
    *budget = rest;
    if let Some(GuardValue::String(text)) = value.constant() {
        return bytes_lack_affix(text, affix, end);
    }
    let datum = match value.node.as_ref() {
        ValueNode::Select {
            decision: _,
            yes,
            no,
        } => {
            return lacks_affix(yes, affix, end, budget) && lacks_affix(no, affix, end, budget);
        }
        ValueNode::Leaf(Leaf { datum, origin: _ }) => datum,
    };
    match datum {
        Datum::Scalar {
            go_type: _,
            content:
                Some(super::scalar_value::ScalarContent::StringDomain {
                    pattern,
                    min_bytes,
                    max_bytes: _,
                }),
        } => *min_bytes >= affix.len() && domain_lacks_affix(pattern, affix, end),
        Datum::Text(text) => match text.term() {
            super::text::TextTerm::Resolved { term: _, literal } => {
                bytes_lack_affix(&literal.to_string(), affix, end)
            }
            super::text::TextTerm::Capture {
                graph,
                strip_marker: false,
            } => buffer_lacks_affix(graph, affix, end, budget),
            super::text::TextTerm::Capture {
                graph: _,
                strip_marker: true,
            }
            | super::text::TextTerm::Encoded {
                value: _,
                alphabet: _,
            }
            | super::text::TextTerm::Quote { value: _ }
            | super::text::TextTerm::Yaml {
                value: _,
                pretty: _,
            }
            | super::text::TextTerm::Json {
                value: _,
                pretty: _,
                escape_html: _,
                literal: _,
            }
            | super::text::TextTerm::Tpl {
                source: _,
                result: _,
            }
            | super::text::TextTerm::Indent {
                value: _,
                width: _,
                newline: _,
            } => false,
        },
        Datum::Scalar {
            go_type: _,
            content:
                None
                | Some(
                    super::scalar_value::ScalarContent::Literal(_)
                    | super::scalar_value::ScalarContent::Identity(_),
                ),
        }
        | Datum::Map { view: _, target: _ }
        | Datum::Sequence(_)
        | Datum::Strings(_)
        | Datum::Slot {
            backing: _,
            index: _,
        }
        | Datum::Unknown(_) => false,
    }
}

fn buffer_lacks_affix(
    buffer: &super::TextBuffer,
    affix: &str,
    end: TextEnd,
    budget: &mut usize,
) -> bool {
    let Some(rest) = budget.checked_sub(1) else {
        return false;
    };
    *budget = rest;
    let part = match end {
        TextEnd::Start => buffer.parts.iter().next(),
        TextEnd::End => buffer.parts.iter().next_back(),
    };
    let Some(part) = part else {
        return false;
    };
    match part.content() {
        TextPart::Bytes(text) => bytes_lack_affix(text, affix, end),
        TextPart::Frozen(value) => lacks_affix(&value.value, affix, end, budget),
        TextPart::Select {
            decision: _,
            yes,
            no,
        } => {
            buffer_lacks_affix(yes, affix, end, budget)
                && buffer_lacks_affix(no, affix, end, budget)
        }
        TextPart::Unknown(_) | TextPart::Repeat(_) => false,
    }
}

fn bytes_lack_affix(text: &str, affix: &str, end: TextEnd) -> bool {
    text.len() >= affix.len()
        && match end {
            TextEnd::Start => !text.starts_with(affix),
            TextEnd::End => !text.ends_with(affix),
        }
}

/// Whether no word of a full-match language starts or ends with `affix`.
fn domain_lacks_affix(pattern: &str, affix: &str, end: TextEnd) -> bool {
    let width = affix.chars().count();
    // Each word's first or last `width` characters (words are ASCII here).
    let ends = helm_schema_core::pattern_end_states(
        pattern,
        String::new(),
        |seen, character| {
            let mut seen = seen.clone();
            match end {
                TextEnd::End => seen.push(character),
                TextEnd::Start if seen.len() < width => seen.push(character),
                TextEnd::Start => {}
            }
            if seen.len() > width {
                seen.remove(0);
            }
            Some(seen)
        },
        4096,
    );
    ends.is_some_and(|ends| {
        ends.iter()
            .all(|seen| seen.as_deref().is_some_and(|seen| seen != affix))
    })
}

/// `semver VERSION`: a parsed `*semver.Version` whose accessor methods read as fields.
fn transfer_semver(input: &TransferInput<'_>, state: &mut State) -> Result<Value, AbortReason> {
    let parsed = match input.first.constant() {
        Some(GuardValue::String(text)) => Some((
            helm_schema_ast::parse_semver_version(text).ok_or(AbortReason::Type)?,
            text.clone(),
        )),
        Some(
            GuardValue::Null | GuardValue::Bool(_) | GuardValue::Int(_) | GuardValue::Float(_),
        ) => return Err(AbortReason::Type),
        None => None,
    };
    let scalar = |kind, content: Option<GuardValue>| {
        Value::scalar(
            kind,
            content.map(super::scalar_value::ScalarContent::Literal),
        )
        .with_influences(input.influences.clone())
    };
    let mut fields = Vec::new();
    for (index, name) in ["Major", "Minor", "Patch"].into_iter().enumerate() {
        let component = parsed
            .as_ref()
            .and_then(|(parsed, _)| parsed.core.get(index))
            .and_then(|component| i64::try_from(*component).ok())
            .map(GuardValue::Int);
        fields.push((name, scalar(super::ScalarType::Uint64, component)));
    }
    for (name, text) in [
        (
            "Prerelease",
            parsed.as_ref().map(|(parsed, _)| parsed.prerelease.clone()),
        ),
        (
            "Metadata",
            parsed.as_ref().map(|(parsed, _)| parsed.metadata.clone()),
        ),
        (
            "Original",
            parsed.as_ref().map(|(_, original)| original.clone()),
        ),
    ] {
        fields.push((
            name,
            scalar(super::ScalarType::String, text.map(GuardValue::String)),
        ));
    }
    let value = state.heap.map(Remainder::Closed);
    if let Some(target) = state.map_target(&value) {
        for (name, field) in fields {
            state.heap.write(&target, name, Field::Present(field));
        }
    }
    Ok(super::hosts::typed_map(
        value,
        MapType::Semver,
        &state.heap.arena,
    ))
}

/// `strings.Repeat` over literal operands; a negative count panics and aborts the call.
fn transfer_repeat(input: TransferInput<'_>, state: &mut State) -> Value {
    if let (Some(GuardValue::Int(count)), Some(GuardValue::String(text))) =
        (input.first.constant(), input.last.constant())
        && let Ok(count) = usize::try_from(*count)
        && text.len().saturating_mul(count) <= 64 * 1024
    {
        return Value::string(&text.repeat(count));
    }
    transfer_fallback(input, state)
}

/// `regexReplaceAll REGEX text REPLACEMENT`: the subject is the middle operand.
fn transfer_regex_replace(input: TransferInput<'_>, state: &State) -> Value {
    use super::relationship::{TextTransform, ValueOperation};

    let (Some(GuardValue::String(pattern)), Some(subject), Some(GuardValue::String(replacement))) = (
        input.first.constant(),
        input.arguments.get(1),
        input.last.constant(),
    ) else {
        return Value::influenced(input.influences);
    };
    // The literal variants never expand `$` references, so they replace with the text itself.
    let replacement = if input.function.ends_with("Literal") {
        replacement.replace('$', "$$")
    } else {
        replacement.clone()
    };
    Value::scalar(super::ScalarType::String, None)
        .with_influences(input.influences)
        .derived(
            state.heap.freeze(subject),
            ValueOperation::Text(TextTransform::RegexReplace {
                pattern: pattern.clone(),
                replacement,
            }),
        )
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

#[tracing::instrument(skip_all, level = "debug", fields(arguments = arguments.len()))]
pub(super) fn inputs(arguments: &[Value], state: &State) -> BTreeSet<helm_schema_core::ValuesPath> {
    arguments
        .iter()
        .flat_map(|value| state.heap.freeze(value).inputs())
        .collect()
}

pub(super) fn widen_map(
    target: &super::ObjectId,
    influences: &BTreeSet<helm_schema_core::ValuesPath>,
    stored: Option<&Value>,
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
                reachable: super::heap::data_reach(&object.data)
                    .union(&super::heap::values_reach(stored)),
            },
        },
    );
}
/// The unknown write of the code running on the current frames (ADR 0072 §5).
///
/// Helm executes every file and helper against its own context, so an unknown writer reaches
/// only what the frames reach; another chart instance's values and allocations keep their
/// contents.
pub(super) fn invalidate(state: &mut State) {
    let roots: Vec<Value> = state
        .frames
        .iter()
        .flat_map(|frame| frame.bindings.values().chain(frame.dot.iter()).cloned())
        .collect();
    let input_regions = unknown_write(&roots, &roots, state);
    forget_frame_scalars(state, &input_regions);
}

/// Widens what `targets` reach, every object depending on the inputs `sources` reach.
///
/// Helm runs an unknown program against its data argument alone, so it can write only what
/// that argument reaches when the call begins (`engine.go` `tpl`); whatever it stores may then
/// reference any of those objects. Where an unknown part may reference objects nothing records,
/// the writer may reach any object.
fn unknown_write(
    targets: &[Value],
    sources: &[Value],
    state: &mut State,
) -> Rc<BTreeSet<helm_schema_core::ValuesPath>> {
    let reached = reached_objects(targets, state);
    let owners = Vec::from_iter(executing_values_path(state));
    write_reached(reached, sources, &owners, state)
}

/// Ends the analysis assumptions of the chart instances whose roots are `contexts`, which made
/// every assumption the path holds. Each unknown program they assumed action-free runs against its
/// data argument alone (`engine.go` `tpl`), which reaches only what its instance's root reaches.
/// The documents on which the assumptions hold continue as they are; on the others those roots'
/// reach takes an unknown write. The two continuations join, so later instances claim their facts
/// on every document.
pub(super) fn discharge_assumptions(contexts: &[Value], state: State) -> Vec<State> {
    let Some(test) = state.heap.arena.assumption_test(&state.active) else {
        return vec![state];
    };
    let base = state
        .active
        .without(super::path::LinkKind::Assume)
        .unwrap_or_else(|| state.active.clone());
    let mut held = state.clone();
    held.active = base.pushed(
        super::DecisionEdge {
            test: Rc::clone(&test),
            taken: true,
        },
        super::path::LinkKind::Branch,
    );
    let mut broken = state;
    broken.active = base.pushed(
        super::DecisionEdge { test, taken: false },
        super::path::LinkKind::Branch,
    );
    let reached = reached_objects(contexts, &mut broken);
    let owners = contexts
        .iter()
        .map(|context| instance_values_path(context, &broken))
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default();
    write_reached(reached, contexts, &owners, &mut broken);
    let normal = |state| super::Exit {
        state,
        flow: super::Flow::Normal(super::Normal {
            value: Value::nil(),
        }),
    };
    super::control::reconverge(vec![normal(held), normal(broken)])
        .into_iter()
        .map(|exit| exit.state)
        .collect()
}

/// [`unknown_write`] over what its targets were already found to reach.
fn write_reached(
    reached: Reached,
    sources: &[Value],
    owners: &[helm_schema_core::ValuesPath],
    state: &mut State,
) -> Rc<BTreeSet<helm_schema_core::ValuesPath>> {
    let Reached {
        mut objects,
        bounded,
        undecided: _,
    } = reached;
    let mut influences = inputs(sources, state);
    let reachable = if bounded {
        super::Reach::Bounded(Rc::new(objects.clone()))
    } else {
        // An unknown part may reference objects nothing records. Until every summary keeps its
        // references, such a writer is scoped to the instances that run it: it may reach any
        // object except another instance's values maps and render-context maps.
        if owners.is_empty() {
            return invalidate_all(state);
        }
        let owned = |path: &helm_schema_core::ValuesPath| {
            owners
                .iter()
                .any(|own| path == own || path.is_descendant_of(own))
        };
        let outside = |path: &helm_schema_core::ValuesPath| !owned(path);
        let input_paths = state.heap.arena.input_paths.borrow();
        let instance_objects = state.heap.arena.instance_objects.borrow();
        for id in state.heap.version.store.objects() {
            let foreign = input_paths.get(&id).is_some_and(outside)
                || instance_objects.get(&id).is_some_and(outside);
            if !foreign {
                objects.insert(id);
            }
        }
        drop(input_paths);
        drop(instance_objects);
        influences.extend(
            state
                .heap
                .freeze(&Value::unknown())
                .reachable_inputs()
                .into_iter()
                .filter(|path| owned(path)),
        );
        super::Reach::Top
    };
    // Objects one earlier writer widened share its input set; each set is merged once.
    let mut merged = ahash::AHashSet::new();
    for id in &objects {
        if let Some(object) = state.heap.version.object(id)
            && let ObjectData::Widened {
                inputs,
                reachable: _,
            } = &object.data
            && merged.insert(Rc::as_ptr(inputs))
        {
            influences.extend(inputs.iter().cloned());
        }
    }
    // A template writes only `map[string]interface{}` maps; the host structs keep their fields.
    let read_only = state.heap.arena.read_only_objects.borrow();
    let written = objects
        .into_iter()
        .filter(|id| !read_only.contains(id))
        .collect();
    drop(read_only);
    let influences = Rc::new(influences);
    widen_objects(written, &influences, &reachable, state);
    influences
}

/// The effect of an unknown program run against `argument`, reading `sources`: one
/// continuation per outcome of the open decisions that select what the argument reaches, each
/// writing only what its own path reaches (Helm runs `tpl` and `include` against their data
/// argument alone, `engine.go`).
pub(super) fn opaque_call(argument: &Value, sources: &[Value], state: State) -> Vec<State> {
    const MAX_REACH_FORKS: usize = 3;
    let mut pending = vec![(state, 0)];
    let mut done = Vec::new();
    while let Some((mut state, depth)) = pending.pop() {
        let reached = reached_objects(std::slice::from_ref(argument), &mut state);
        match reached.undecided.first() {
            Some(decision) if reached.bounded && depth < MAX_REACH_FORKS => {
                for taken in [true, false] {
                    let fork = state.fork(Rc::clone(decision), taken);
                    if !fork.path_is_infeasible() {
                        pending.push((fork, depth + 1));
                    }
                }
            }
            Some(_) | None => {
                let owners = Vec::from_iter(executing_values_path(&state));
                write_reached(reached, sources, &owners, &mut state);
                done.push(state);
            }
        }
    }
    done
}

/// The values path of the chart instance whose file is executing.
fn executing_values_path(state: &State) -> Option<helm_schema_core::ValuesPath> {
    instance_values_path(state.frames.first()?.bindings.get("$")?, state)
}

/// The values path of the chart instance whose root map is `root`.
fn instance_values_path(root: &Value, state: &State) -> Option<helm_schema_core::ValuesPath> {
    let ValueNode::Leaf(Leaf {
        datum: Datum::Map {
            view: _,
            target: Some(target),
        },
        origin: _,
    }) = root.node.as_ref()
    else {
        return None;
    };
    state
        .heap
        .arena
        .instance_objects
        .borrow()
        .get(target)
        .cloned()
}

/// Frame values that are not map references now depend on everything the writer read.
fn forget_frame_scalars(state: &mut State, input_regions: &BTreeSet<helm_schema_core::ValuesPath>) {
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
                // The value still references only what it did.
                *value = super::heap::unknown_member(
                    std::slice::from_ref(value),
                    input_regions.union(&value.inputs()).cloned().collect(),
                );
            }
        }
    }
}

/// An unknown write to every object, reading every input the heap reaches.
pub(super) fn invalidate_all(state: &mut State) -> Rc<BTreeSet<helm_schema_core::ValuesPath>> {
    let input_regions = Rc::new(state.heap.freeze(&Value::unknown()).reachable_inputs());
    for path in input_regions.iter() {
        state.reads.insert(super::RuntimeRead {
            path: path.clone(),
            condition: state.path_predicate(),
            coverage: super::ReadCoverage::UnknownRegion,
        });
    }
    // A template writes only `map[string]interface{}` maps; the host structs keep their fields.
    let kept = state
        .heap
        .arena
        .read_only_objects
        .borrow()
        .iter()
        .filter_map(|id| Some((id.clone(), state.heap.version.object(id)?)))
        .collect::<Vec<_>>();
    // An unknown writer can insert or delete keys as well as replace their contents.
    state.heap.unknown_write(Rc::clone(&input_regions));
    for (id, object) in kept {
        state.heap.install(id, object.as_ref().clone());
    }
    input_regions
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
        serde_json::Value::Array(items) => {
            let mut value =
                Value::sequence(items.iter().map(|value| decode(value, state)).collect());
            if let ValueNode::Leaf(Leaf {
                datum: Datum::Sequence(sequence),
                origin: _,
            }) = Rc::make_mut(&mut value.node)
            {
                sequence.capacity = None;
            }
            value
        }
        serde_json::Value::Null => Value::nil(),
        serde_json::Value::Bool(value) => Value::literal(GuardValue::Bool(*value)),
        serde_json::Value::String(value) => Value::string(value),
        // The Go type is exact; numeric content still lacks Go-compatible serialization.
        serde_json::Value::Number(_) => Value::scalar(super::ScalarType::Float64, None),
    }
}

fn copy(
    value: &Value,
    state: &mut State,
    seen: &mut BTreeSet<super::ObjectId>,
    depth: usize,
) -> Value {
    let value = state.consumed_contents(value);
    let frozen = state.heap.freeze(&value);
    let influences = frozen.inputs();
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
            }) = state.heap.version.object(id).as_deref().cloned()
            else {
                return Value::influenced(influences);
            };
            let copied = fields
                .iter()
                .map(|(key, field)| {
                    (
                        key.clone(),
                        copy_field(field.clone(), state, seen, depth + 1),
                    )
                })
                .collect();
            let remainder = state.copy_remainder(&frozen, &remainder);
            let target = state.heap.allocate(
                ObjectData::Map {
                    fields: Rc::new(copied),
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
        }) => copy_sequence(sequence, state, seen, depth, influences.clone()),
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
        }) => copy_unstructured(&value, state, &influences),
        ValueNode::Select { decision, yes, no } => Value::select(
            Rc::clone(decision),
            copy(yes, state, &mut seen.clone(), depth + 1),
            copy(no, state, &mut seen.clone(), depth + 1),
        ),
    };
    Value::with_influences(copied, influences)
        .derived(frozen, super::relationship::ValueOperation::Copy)
}

fn copy_sequence(
    sequence: &super::InterfaceSequence,
    state: &mut State,
    seen: &BTreeSet<super::ObjectId>,
    depth: usize,
    influences: BTreeSet<ValuesPath>,
) -> Value {
    if sequence
        .storage
        .as_ref()
        .is_some_and(|storage| storage.is_bytes())
    {
        return Value::with_influences(Value::leaf(Datum::Sequence(sequence.clone())), influences);
    }
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
        storage: Some(Rc::new(super::SequenceStorage::Interfaces {
            values: copied,
        })),
        start: 0,
        length: sequence.length,
        capacity: sequence.capacity,
    }))
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
            state
                .copy_open_map(value)
                .unwrap_or_else(|| state.heap.map(Remainder::Open))
        } else if let Some(copy) = state.copy_open_map(value) {
            copy
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
            copy_field(yes.as_ref().clone(), state, &mut seen.clone(), depth + 1),
            copy_field(no.as_ref().clone(), state, &mut seen.clone(), depth + 1),
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
                    reachable: super::heap::values_reach([first]),
                },
                super::Multiplicity::Singleton,
            );
            return Ok(Value::with_influences(
                Value::strings(backing, 1),
                influences,
            ));
        } else {
            // `sortAlpha` sorts a `[]string` operand in place (`sprig` `strslice`).
            if function == "sortAlpha" {
                widen_operands(std::slice::from_ref(first), state);
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
    if function == "sortAlpha" {
        sort_strings_in_place(&value, influences, state)?;
    }
    Ok(value)
}

/// Sorts a `[]string` in place, as `sortAlpha` does (`sprig` `strslice`).
fn sort_strings_in_place(
    value: &Value,
    influences: BTreeSet<helm_schema_core::ValuesPath>,
    state: &mut State,
) -> Result<(), TransferDemand> {
    let ValueNode::Leaf(Leaf {
        datum: Datum::Strings(slice),
        origin: _,
    }) = value.node.as_ref()
    else {
        return Ok(());
    };
    let storage = state.heap.version.object(&slice.backing);
    let Some(ObjectVersion {
        data: ObjectData::Strings(slots),
        revision: _,
        multiplicity: _,
    }) = storage.as_deref()
    else {
        return Ok(());
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
                    // String slots reference no object.
                    reachable: super::Reach::Bounded(Rc::default()),
                },
            },
        );
        return Ok(());
    };
    sorted.sort();
    for (index, text) in sorted.iter().enumerate() {
        state
            .heap
            .write_slot(&slice.backing, slice.start + index, Value::string(text));
    }
    Ok(())
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
    let value = state.consumed_contents(value);
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
            | ObjectData::Widened {
                inputs: _,
                reachable: _,
            } => None,
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

pub(super) fn sequence_items(value: &Value, state: &State) -> Option<Vec<Value>> {
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
            let storage = state.heap.version.object(&slice.backing)?;
            let ObjectData::Strings(slots) = &storage.data else {
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
        ObjectData::Map {
            fields: Rc::new(fields.into()),
            remainder,
        },
        super::Multiplicity::Singleton,
    );
    Value {
        node: Rc::new(ValueNode::Leaf(Leaf {
            datum: Datum::Map {
                view: MapType::Interface,
                target: Some(target),
            },
            origin: super::Origin {
                derivation: None,
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
    // Picking unknown members needs presence guards; omitted remainders stay lazy.
    let first = if function == "pick" {
        let first = state.consumed(first);
        if state.map_target(&first).is_some() {
            state.consumed(&first)
        } else {
            first
        }
    } else {
        first.clone()
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
    let operation = if function == "omit" {
        super::relationship::ValueOperation::Omit(keys.clone())
    } else {
        super::relationship::ValueOperation::Pick(keys.clone())
    };
    let frozen = state.heap.freeze(&first);
    let Some(mut map) = map_fields(&first, state) else {
        return Value::influenced(influences).derived(frozen, operation);
    };
    if function == "omit" {
        for key in keys {
            map.fields.insert(key, Field::Absent);
        }
    } else {
        map.fields.retain(|key, _| keys.contains(key));
        for key in keys {
            if matches!(map.fields.get(&key), Some(Field::Unknown))
                || !map.fields.contains_key(&key) && map.remainder != Remainder::Closed
            {
                let present = state.field_presence(&first, &key);
                let value = state.field(&first, &key);
                map.fields.insert(
                    key,
                    Field::select(state.test(&present), Field::Present(value), Field::Absent),
                );
            }
        }
        map.remainder = Remainder::Closed;
    }
    fresh_map(map, influences, state).derived(frozen, operation)
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

fn merge(arguments: &[Value], state: &mut State, overwrite: bool) -> Value {
    let influences = inputs(arguments, state);
    let Some(first) = arguments.first() else {
        return Value::influenced(influences);
    };
    if arguments.len() == 1 {
        return interface_map_view(first, state);
    }
    let nil_map = |value: &Value| {
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
    };
    let nil_destination = nil_map(first);
    if nil_destination && arguments.iter().skip(1).all(nil_map) {
        return Value::with_influences(
            Value::leaf(Datum::Map {
                view: MapType::Interface,
                target: None,
            }),
            influences,
        );
    }
    let destination = if nil_destination {
        state.heap.map(Remainder::Closed)
    } else {
        first.clone()
    };
    let Some(target) = state.map_target(&destination) else {
        invalidate(state);
        return Value::influenced(influences);
    };
    for source in arguments.iter().skip(1) {
        // A source sharing the destination's object would be walked while it changes.
        let shared = state
            .map_target(source)
            .is_some_and(|object| object == target);
        if shared || !state.merge_into(&target, source, overwrite, 0, &Rc::default()) {
            // Mergo writes only the destination; the sources stay as they are.
            let mut sources = arguments.to_vec();
            sources.push(destination.clone());
            widen_targets(std::slice::from_ref(&destination), &sources, state);
            break;
        }
    }
    // The merged map is influenced by every operand, not only by its destination.
    interface_map_view(&destination, state).with_influences(influences)
}

pub(super) fn widen_operands(arguments: &[Value], state: &mut State) {
    widen_targets(arguments, arguments, state);
}

/// Widens the objects reachable from `targets`; their unknown contents depend on every input
/// of `sources`.
fn widen_targets(targets: &[Value], sources: &[Value], state: &mut State) {
    unknown_write(targets, sources, state);
}

/// Replaces each object by an unknown one depending on `influences`, recording the read.
fn widen_objects(
    objects: BTreeSet<super::ObjectId>,
    influences: &Rc<BTreeSet<helm_schema_core::ValuesPath>>,
    reachable: &super::Reach,
    state: &mut State,
) {
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
                    inputs: Rc::clone(influences),
                    reachable: reachable.clone(),
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

/// The objects a member of `collection` may reference: everything the collection reaches on the
/// current path. An input collection whose map was never allocated stays unbounded: allocating
/// it here would change what later reads of it see.
pub(super) fn member_aliases(collection: &Value, state: &mut State) -> super::Reach {
    let target = match collection.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Unknown(super::UnknownValue {
                    types: _,
                    aliases: super::Reach::Top,
                }),
            origin:
                super::Origin {
                    derivation: _,
                    identity: Some(path),
                    influences: _,
                },
        }) => {
            let Some(object) = state
                .heap
                .arena
                .input_objects
                .borrow()
                .get(path)
                .map(|backing| backing.object.clone())
            else {
                return super::Reach::Top;
            };
            Value::leaf(Datum::Map {
                view: MapType::Interface,
                target: Some(object),
            })
        }
        ValueNode::Leaf(_)
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => collection.clone(),
    };
    let Reached {
        objects,
        bounded,
        undecided: _,
    } = reached_objects(std::slice::from_ref(&target), state);
    if bounded {
        super::Reach::Bounded(Rc::new(objects))
    } else {
        super::Reach::Top
    }
}

/// What a writer's argument reaches on the current path.
struct Reached {
    objects: BTreeSet<super::ObjectId>,
    /// False when an unknown part may reference objects nothing records.
    bounded: bool,
    /// The decisions the path leaves open that select a reached reference.
    undecided: Vec<Rc<super::FrozenTest>>,
}

/// The objects `targets` may reach on the current path through known fields, slots, merge
/// layers, the references unknown contents keep and the input maps allocated beneath an input
/// map.
fn reached_objects(targets: &[Value], state: &mut State) -> Reached {
    let guard = state.heap.arena.path_guard(&state.active);
    let mut reach = Reached {
        objects: BTreeSet::new(),
        bounded: true,
        undecided: Vec::new(),
    };
    let mut pending: Vec<Value> = targets.to_vec();
    while let Some(value) = pending.pop() {
        // A choice the path decides holds only its own arm; a choice between different
        // references leaves what is reached open.
        if let ValueNode::Select { decision, yes, no } = value.node.as_ref() {
            match guard.implies_test(decision) {
                Some(true) => pending.push(yes.clone()),
                Some(false) => pending.push(no.clone()),
                None => {
                    if super::heap::values_reach([yes]) != super::heap::values_reach([no]) {
                        reach.undecide(decision);
                    }
                    pending.extend([yes.clone(), no.clone()]);
                }
            }
            continue;
        }
        let mut targets = BTreeSet::new();
        // An input value references the input map at its own path, if it is one.
        if let ValueNode::Leaf(Leaf {
            datum:
                Datum::Unknown(super::UnknownValue {
                    types: super::TypeCoverage::Unresolved | super::TypeCoverage::Map(_),
                    aliases: super::Reach::Top,
                }),
            origin:
                super::Origin {
                    derivation: None,
                    identity: Some(path),
                    influences: _,
                },
        }) = value.node.as_ref()
        {
            let path = path.clone();
            targets.insert(state.input_object(&path));
        } else {
            let bounded = super::heap::value_references(&value, &mut pending, &mut targets);
            if reach.bounded && !bounded {
                // The first unbounded producer of each reach, for attributing genuine Top.
                tracing::debug!(target: "helm_schema_ir::top_reach", node = ?value.node, "unbounded value");
            }
            reach.bounded &= bounded;
        }
        for target in targets {
            if !reach.objects.insert(target.clone()) {
                continue;
            }
            let Some(object) = state.heap.version.object(&target) else {
                continue;
            };
            let bounded = reach_object(
                &target,
                &object.data,
                state,
                &guard,
                &mut reach,
                &mut pending,
            );
            if reach.bounded && !bounded {
                tracing::debug!(target: "helm_schema_ir::top_reach", ?target, data = ?object.data, "unbounded object");
            }
            reach.bounded &= bounded;
        }
    }
    reach
}

/// The objects a field's values may reference.
fn field_reach(field: &Field) -> super::Reach {
    let mut values = Vec::new();
    let mut pending = vec![field];
    while let Some(field) = pending.pop() {
        match field {
            Field::Absent | Field::Unknown => {}
            Field::Present(value) => values.push(value),
            Field::Select {
                decision: _,
                yes,
                no,
            } => pending.extend([yes.as_ref(), no.as_ref()]),
        }
    }
    super::heap::values_reach(values)
}

impl Reached {
    fn undecide(&mut self, decision: &Rc<super::FrozenTest>) {
        if !self.undecided.iter().any(|known| known.id == decision.id) {
            self.undecided.push(Rc::clone(decision));
        }
    }
}

/// Queues what one object reaches; false when an unknown part may reference unnamed objects.
fn reach_object(
    target: &super::ObjectId,
    data: &ObjectData,
    state: &State,
    guard: &super::guard::RuntimeGuard,
    reach: &mut Reached,
    pending: &mut Vec<Value>,
) -> bool {
    let map = |id| {
        Value::leaf(Datum::Map {
            view: MapType::Interface,
            target: Some(id),
        })
    };
    let children = || {
        state
            .allocated_children(target)
            .into_iter()
            .map(|(_, child)| map(child))
    };
    match data {
        ObjectData::Map { fields, remainder } => {
            let mut bounded = true;
            match remainder {
                // The input maps already allocated beneath an input map are its members.
                Remainder::Input => pending.extend(children()),
                // Unwritten keys of a layered map may alias what its layers reach.
                Remainder::Layered(layers) => {
                    if let Some(aliases) = state.layer_aliases(layers) {
                        pending.extend(aliases.into_iter().map(map));
                    } else {
                        bounded = false;
                    }
                    pending.extend(children());
                }
                Remainder::Unknown { inputs: _ } => {
                    bounded = false;
                    pending.extend(children());
                }
                Remainder::Closed | Remainder::Open => {}
            }
            let mut fields: Vec<_> = fields.values().collect();
            while let Some(field) = fields.pop() {
                match field {
                    Field::Absent | Field::Unknown => {}
                    Field::Present(value) => pending.push(value.clone()),
                    // A member the path selects holds only its own arm.
                    Field::Select { decision, yes, no } => match guard.implies_test(decision) {
                        Some(true) => fields.push(yes.as_ref()),
                        Some(false) => fields.push(no.as_ref()),
                        None => {
                            if field_reach(yes) != field_reach(no) {
                                reach.undecide(decision);
                            }
                            fields.extend([yes.as_ref(), no.as_ref()]);
                        }
                    },
                }
            }
            bounded
        }
        ObjectData::Strings(slots) => {
            pending.extend(slots.iter().cloned());
            true
        }
        ObjectData::Opaque { reachable } => match reachable {
            super::Reach::Bounded(ids) => {
                pending.extend(ids.iter().cloned().map(map));
                true
            }
            super::Reach::Top => false,
        },
        // Unknown contents keep the references they may hold.
        ObjectData::Widened {
            inputs: _,
            reachable,
        } => {
            pending.extend(children());
            match reachable {
                super::Reach::Bounded(ids) => {
                    pending.extend(ids.iter().cloned().map(map));
                    true
                }
                super::Reach::Top => false,
            }
        }
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
            reachable: super::Reach::Bounded(Rc::new(BTreeSet::new())),
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
                aliases: super::Reach::Bounded(Rc::new(BTreeSet::from([region]))),
            }),
            origin: super::Origin {
                derivation: None,
                identity: None,
                influences,
            },
        })),
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
    if let (Some(separator), None, Some(subject)) = (
        separator.filter(|separator| !separator.is_empty()),
        text,
        text_value,
    ) {
        // Every split of a string by a nonempty separator has a first part: the text before it.
        let first = Value::scalar(super::ScalarType::String, None)
            .with_influences(influences.clone())
            .derived(
                state.heap.freeze(subject),
                super::relationship::ValueOperation::Text(super::relationship::TextTransform::Cut(
                    separator.to_owned(),
                )),
            );
        // The other pieces are strings: they reference no object.
        return string_map(
            BTreeMap::from([("_0".to_owned(), Field::Present(first))]),
            super::layers::unknown_members(
                &Rc::new(influences.clone()),
                &super::Reach::Bounded(Rc::default()),
            ),
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
        ObjectData::Map {
            fields: Rc::new(fields.into()),
            remainder,
        },
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
    let storage = state.heap.version.object(&slice.backing);
    let Some(ObjectVersion {
        data: ObjectData::Strings(slots),
        revision: _,
        multiplicity: _,
    }) = storage.as_deref()
    else {
        let backing = state.heap.allocate(
            ObjectData::Widened {
                inputs: Rc::new(influences.clone()),
                // String slots reference no object.
                reachable: super::Reach::Bounded(Rc::default()),
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
                    | ObjectData::Widened {
                        inputs: _,
                        reachable: _,
                    } => None,
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
