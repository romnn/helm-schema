use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{
    Effects, EvalResult, SelectionPolarity, SelectionReachability, SelectionTruthSource,
};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::scalar_value::{ScalarValueDispatch, TruthCondition};
use helm_schema_core::{GuardValue, Predicate};

use super::strict_operands::{
    record_range_key_string_consumer_effects, record_raw_range_key_string_consumer_paths,
    record_string_call_consumers, record_string_consumer_effects, string_invocation_operand_facts,
};
use super::value_facts::{identity_value_paths, split_transformed_value, value_strings};
use super::{eval_all_args, eval_unknown_call, merge_arg_effects, merge_arg_values};

/// `default FALLBACK PRIMARY` and `PRIMARY | default FALLBACK` are one rule:
/// the primary's identity paths become defaulted (typed by a literal
/// fallback), and the value is the choice of primary and fallback values.
pub(super) fn eval_default(
    mut primary: EvalResult,
    fallback_args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let primary_dispatch = primary.scalar_dispatch.clone();
    let fallback_reachability = default_primary_selection(&primary);
    let primary_reachability = fallback_reachability.complement();
    let primary_paths = identity_value_paths(primary.value.as_ref());
    primary.selection_reachability = Some(primary_reachability.clone());
    apply_default_primary_formatter_reachability(
        primary.value.as_ref(),
        &primary_reachability,
        &mut primary.effects,
    );
    let primary_identity = direct_raw_identity_path(primary.value.as_ref());
    let mut effects = primary.effects;
    effects.add_default_paths(primary_paths.clone());
    // Only a LITERAL fallback types the path: `default "x" .Values.name`
    // documents a string-shaped input. A call fallback (`default (include
    // …) .Values.ns`) only proves the fallback renders text; the path
    // itself accepts whatever the render site accepts. The hint rides its
    // own channel: `default` itself never consumes the raw value — every
    // Helm-empty input selects the fallback and renders — so the fallback's
    // kind types only the truthy arm and must not close the base against
    // Helm-empty inputs.
    if let Some(schema_type) = fallback_args
        .first()
        .map(TemplateExpr::deparen)
        .filter(|expr| matches!(expr, TemplateExpr::Literal(_)))
        .and_then(literal_schema_type)
    {
        effects.add_fallback_type_hints(primary_paths.clone(), schema_type);
    }
    let mut values = if fallback_reachability.is_always() {
        Vec::new()
    } else {
        primary.value.into_iter().collect::<Vec<_>>()
    };
    let mut fallback_paths = BTreeSet::new();
    let mut fallback_dispatch = None;
    for fallback in fallback_args {
        let mut result = eval_expr_with_helper_calls(fallback, env, resolver);
        if fallback_reachability.is_never() {
            effects.merge(result.effects.execution_only());
            continue;
        }
        // An opaque primary still selects its fallback conditionally.
        // Keeping an unlowerable selection marker prevents later consumers
        // from mistaking the fallback for an unconditional raw operand.
        super::conjoin_result_reachability(
            &mut result,
            &fallback_reachability,
            "default fallback after opaque primary",
            primary_paths.clone(),
        );
        fallback_dispatch = result.scalar_dispatch.clone();
        fallback_paths.extend(identity_value_paths(result.value.as_ref()));
        effects.merge(result.effects);
        if let Some(value) = result.value {
            values.push(value);
        }
    }
    if let Some(primary_path) = primary_identity {
        let overlaps_fallback = fallback_paths.remove(&primary_path);
        if !overlaps_fallback {
            let meta = effects
                .local_output_meta
                .entry(primary_path.clone())
                .or_default();
            meta.input_identity = true;
            // The legacy capture lane still uses raw truthiness here. For a
            // plain `%s` formatter it is the capture's faithfulness boundary;
            // for an opaque formatter it keeps eager strict-consumer effects
            // separate from selected output. The carrier owns the actual
            // output reachability until Step 6b.4 removes this projection.
            let primary_predicates = BTreeSet::from([Predicate::truthy_path(primary_path.clone())]);
            meta.conjoin_branches(&primary_predicates);
            // A scalar literal fallback is the binding's exact value on
            // every Helm-falsy input; equality decoding needs the literal
            // itself to spell the fallback arm. Floats abstain (their
            // file-vs-`--set` channels compare differently).
            if let [fallback] = fallback_args {
                meta.default_fallback = match fallback.deparen() {
                    TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
                        Some(GuardValue::string(value.clone()))
                    }
                    TemplateExpr::Literal(Literal::Bool(value)) => Some(GuardValue::Bool(*value)),
                    TemplateExpr::Literal(Literal::Int(value)) => Some(GuardValue::Int(*value)),
                    _ => None,
                };
            }
        }
    }
    if fallback_reachability.has_proven_selection_condition() {
        for path in fallback_paths {
            let meta = effects.local_output_meta.entry(path).or_default();
            meta.input_identity = true;
        }
    }
    // `values` holds the primary first and the fallback second exactly when
    // both resolved, which is the ordered first-truthy selection; a missing
    // arm leaves only the other value, where the chain collapses to it (the
    // same result the unordered choice produced).
    let result = EvalResult::with_effects(AbstractValue::first_truthy(values), effects);
    finish_default_dispatch(
        result,
        &fallback_reachability,
        primary_dispatch,
        fallback_dispatch,
        fallback_args,
    )
}

fn literal_schema_type(expr: &TemplateExpr) -> Option<&'static str> {
    match expr {
        TemplateExpr::Literal(Literal::String(_) | Literal::RawString(_)) => Some("string"),
        TemplateExpr::Literal(Literal::Int(_)) => Some("integer"),
        TemplateExpr::Literal(Literal::Float(_)) => Some("number"),
        TemplateExpr::Literal(Literal::Bool(_)) => Some("boolean"),
        _ => None,
    }
}

fn finish_default_dispatch(
    result: EvalResult,
    fallback_reachability: &SelectionReachability,
    primary_dispatch: Option<ScalarValueDispatch>,
    fallback_dispatch: Option<ScalarValueDispatch>,
    fallback_args: &[TemplateExpr],
) -> EvalResult {
    if fallback_reachability.is_always()
        && let Some(fallback) = fallback_dispatch
    {
        return result.with_scalar_dispatch(fallback);
    }
    if fallback_reachability.is_never()
        && let Some(primary) = primary_dispatch
    {
        return result.with_scalar_dispatch(primary);
    }
    if let (Some(primary), Some(fallback), [_]) =
        (primary_dispatch, fallback_dispatch, fallback_args)
        && let Some(dispatch) = ScalarValueDispatch::select_default(&primary, &fallback)
    {
        return result.with_scalar_dispatch(dispatch);
    }
    result
}

fn apply_default_primary_formatter_reachability(
    value: Option<&AbstractValue>,
    reachability: &SelectionReachability,
    effects: &mut Effects,
) {
    let Some(value) = value else {
        return;
    };
    let mut formatter_meta = value.plain_slot_string_format_meta();
    if formatter_meta.is_empty() {
        return;
    }
    let predicates = reachability
        .output_selection_conjunction("default primary after formatter output", value.paths());
    for (path, meta) in &mut formatter_meta {
        meta.conjoin_branches(&predicates);
        effects
            .local_output_meta
            .entry(path.clone())
            .or_default()
            .merge(meta);
    }
}

pub(crate) fn default_primary_selection(result: &EvalResult) -> SelectionReachability {
    if let Some(dispatch) = result.scalar_dispatch.as_ref()
        && (dispatch.has_printf_string_identity()
            || result.value.as_ref().is_some_and(|value| {
                !value
                    .paths()
                    .is_disjoint(&result.effects.derived_text_paths)
            }))
    {
        return SelectionReachability::from((dispatch, SelectionPolarity::Falsy));
    }
    let Some(value) = result.value.as_ref() else {
        return SelectionReachability::approximate(None, SelectionTruthSource::RawInput);
    };
    if let Some(truthy) = known_literal_truthiness(value) {
        return if truthy {
            SelectionReachability::never(SelectionTruthSource::RawInput)
        } else {
            SelectionReachability::always(SelectionTruthSource::RawInput)
        };
    }
    match value {
        AbstractValue::ValuesPath(_) | AbstractValue::JsonDecodedPath(_) => {
            result.exact_input_identity().map_or_else(
                || SelectionReachability::approximate(None, SelectionTruthSource::RawInput),
                |path| {
                    SelectionReachability::exact(
                        Predicate::truthy_path(path).negated(),
                        SelectionTruthSource::RawInput,
                    )
                },
            )
        }
        AbstractValue::OutputPath(_, meta) if meta.is_input_identity() => {
            result.exact_input_identity().map_or_else(
                || SelectionReachability::approximate(None, SelectionTruthSource::RawInput),
                |path| {
                    SelectionReachability::exact(
                        Predicate::truthy_path(path).negated(),
                        SelectionTruthSource::RawInput,
                    )
                },
            )
        }
        AbstractValue::FirstTruthy(candidates) => candidates
            .iter()
            .map(AbstractValue::direct_values_identity)
            .collect::<Option<Vec<_>>>()
            .map(|paths| {
                SelectionReachability::exact(
                    Predicate::all(
                        paths
                            .into_iter()
                            .map(Predicate::truthy_path)
                            .map(|predicate| predicate.negated())
                            .collect(),
                    ),
                    SelectionTruthSource::RawInput,
                )
            })
            .unwrap_or_else(|| {
                SelectionReachability::approximate(None, SelectionTruthSource::RawInput)
            }),
        _ => SelectionReachability::approximate(None, SelectionTruthSource::RawInput),
    }
}

fn known_literal_truthiness(value: &AbstractValue) -> Option<bool> {
    match value {
        AbstractValue::StringSet(values) if values.iter().all(String::is_empty) => Some(false),
        AbstractValue::StringSet(values) if values.iter().all(|value| !value.is_empty()) => {
            Some(true)
        }
        AbstractValue::Choice(values) => {
            let mut values = values.iter();
            let first = known_literal_truthiness(values.next()?)?;
            for value in values {
                if known_literal_truthiness(value)? != first {
                    return None;
                }
            }
            Some(first)
        }
        _ => None,
    }
}

pub(super) fn direct_raw_identity_path(value: Option<&AbstractValue>) -> Option<String> {
    match value? {
        AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path) => {
            Some(path.clone())
        }
        _ => None,
    }
}

pub(super) fn eval_coalesce(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = Effects::default();
    let mut values = Vec::new();
    let mut default_paths = BTreeSet::new();
    let mut candidate_truths = Vec::with_capacity(args.len());
    let mut candidates = Vec::with_capacity(args.len());
    let mut candidate_dispatches = Vec::with_capacity(args.len());
    for arg in args {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        candidate_truths.push(result.truth.clone());
        default_paths.extend(identity_value_paths(result.value.as_ref()));
        let reachability = empty_fold_candidate_reachability(&result)
            .unwrap_or_else(|| result.output_reachability(SelectionPolarity::Truthy));
        candidate_dispatches.push(result.scalar_dispatch.clone());
        candidates.push((result, reachability));
    }
    // `coalesce` selects only non-empty candidates. Downstream strict consumers therefore see
    // each source path only while it is truthy, just as they see a `default` primary.
    effects.add_default_paths(default_paths);
    let mut previous_falsy = Vec::new();
    for (mut result, reachability) in candidates {
        let involved_paths = identity_value_paths(result.value.as_ref());
        let selected = reachability.conjoin_predicates(previous_falsy.iter().cloned());
        super::conjoin_result_reachability(
            &mut result,
            &selected,
            "coalesce candidate selection",
            involved_paths.clone(),
        );
        previous_falsy.push(
            reachability
                .complement()
                .output_selection_predicate("coalesce prior candidate selection", involved_paths),
        );
        effects.merge(result.effects);
        if let Some(value) = result.value {
            values.push(value);
        }
    }
    // A constant final fallback rescues the Helm-empty rendering of a
    // STRINGIFIED first arm (cilium's `coalesce $stringValueKPR "false"`):
    // equality decoding may then admit the empty spellings for the fallback
    // literal. Bounded to the fully explained two-arm shape; a raw first
    // arm abstains because its Helm-emptiness spans false/0/nil/empty
    // collections, not just the empty string.
    if let [first, fallback] = values.as_slice()
        && let AbstractValue::StringSet(literals) = fallback
        && literals.len() == 1
        && let Some(literal) = literals
            .iter()
            .next()
            .filter(|literal| !literal.is_empty())
            .cloned()
        && let Some(rescues) = empty_rescue_paths(first, &effects)
    {
        for path in rescues {
            effects
                .local_output_meta
                .entry(path.0)
                .or_default()
                .empty_rescue = Some(crate::helper_meta::EmptyRescue {
                fallback: literal.clone(),
                spellings: path.1,
            });
        }
    }
    let mut result = EvalResult::with_effects(AbstractValue::choice(values), effects);
    result.truth = TruthCondition::any(candidate_truths);
    if let Some(dispatch) = candidate_dispatches
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .and_then(|dispatches| {
            let mut dispatches = dispatches.into_iter().rev();
            let mut selected = dispatches.next()?;
            for primary in dispatches {
                selected = ScalarValueDispatch::select_default(&primary, &selected)?;
            }
            Some(selected)
        })
    {
        result = result.with_scalar_dispatch(dispatch);
    }
    result
}

fn empty_fold_candidate_reachability(result: &EvalResult) -> Option<SelectionReachability> {
    let rescues = empty_rescue_paths(result.value.as_ref()?, &result.effects)?;
    let [(path, spellings)] = rescues.as_slice() else {
        return None;
    };
    let falsy = Predicate::Or(
        spellings
            .iter()
            .cloned()
            .map(|value| {
                Predicate::from(helm_schema_core::Guard::Eq {
                    path: path.clone(),
                    value,
                })
            })
            .collect(),
    )
    .normalize_boolean();
    Some(SelectionReachability::exact(
        falsy.negated(),
        SelectionTruthSource::RenderedScalar,
    ))
}

/// The per-path [`crate::helper_meta::EmptyRescue`] spellings for a
/// `coalesce` first argument, provided every alternative is explained: a
/// STRINGIFIED identity (its rendering is empty exactly for the raw empty
/// string), or the empty-string literal a recorded fold diverts to. One
/// unexplained alternative (an empty literal without fold spellings, a raw
/// identity, derived text) abstains — its states reach the fallback for
/// spellings the rescue could not name.
fn empty_rescue_paths(
    value: &AbstractValue,
    effects: &Effects,
) -> Option<Vec<(String, BTreeSet<GuardValue>)>> {
    let arms: Vec<&AbstractValue> = match value {
        AbstractValue::Choice(choices) => choices.iter().collect(),
        AbstractValue::FirstTruthy(candidates) => candidates.iter().collect(),
        other => vec![other],
    };
    let is_empty_literal = |arm: &AbstractValue| matches!(arm, AbstractValue::StringSet(set) if set.len() == 1 && set.contains(""));
    let has_empty_literal_arm = arms.iter().any(|arm| is_empty_literal(arm));
    let stringified_in_effects = |path: &str| {
        effects.stringified_paths.contains(path)
            || effects
                .local_output_meta
                .get(path)
                .is_some_and(|meta| meta.stringified)
    };
    let mut rescues = Vec::new();
    for arm in arms {
        let (path, meta) = match arm {
            arm if is_empty_literal(arm) => continue,
            AbstractValue::OutputPath(path, meta) => (path, Some(meta)),
            AbstractValue::ValuesPath(path) => (path, None),
            _ => return None,
        };
        // `empty_fold_spellings` is produced only for the exact
        // stringified-local-to-empty normalization recognized by the
        // control-flow join. It therefore preserves the stringification
        // proof even when the joined OutputPath no longer carries the
        // original transform flags.
        let stringified = meta
            .is_some_and(|meta| meta.stringified || meta.empty_fold_spellings.is_some())
            || stringified_in_effects(path);
        if !stringified {
            return None;
        }
        let mut spellings = BTreeSet::from([GuardValue::string("")]);
        match meta.and_then(|meta| meta.empty_fold_spellings.as_ref()) {
            Some(fold) => spellings.extend(fold.iter().cloned()),
            // An empty-literal alternative without a recorded divert means
            // unknown raw values reach the fallback.
            None if has_empty_literal_arm => return None,
            None => {}
        }
        rescues.push((path.clone(), spellings));
    }
    (!rescues.is_empty()).then_some(rescues)
}

pub(super) fn eval_dict(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut map = BTreeMap::new();
    let mut field_scalar_dispatches = BTreeMap::new();
    let mut effects = Effects::default();
    for pair in args.chunks_exact(2) {
        let [key, value] = pair else {
            continue;
        };
        let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) = key else {
            continue;
        };
        let value = eval_expr_with_helper_calls(value, env, resolver);
        if let Some(dispatch) = &value.scalar_dispatch {
            field_scalar_dispatches.insert(key.clone(), dispatch.clone());
        }
        effects.merge(value.effects);
        map.insert(key.clone(), value.value.unwrap_or(AbstractValue::Unknown));
    }
    let mut result = EvalResult::with_effects(Some(AbstractValue::Dict(map)), effects);
    result.field_scalar_dispatches = field_scalar_dispatches;
    result
}

pub(super) fn eval_pick(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let Some((subject, key_args)) = args.split_first() else {
        return eval_all_args(args, env, resolver);
    };
    let mut subject = eval_expr_with_helper_calls(subject, env, resolver);
    let mut keys = BTreeSet::new();
    for arg in key_args {
        let key = eval_expr_with_helper_calls(arg, env, resolver);
        keys.extend(value_strings(key.value.as_ref()));
        subject.effects.merge(key.effects);
    }
    let value = subject.value.map(|value| {
        let value = value.into_parsed_map();
        let entries = keys
            .into_iter()
            .filter_map(|key| {
                value
                    .apply_to_path(std::slice::from_ref(&key))
                    .map(|picked| (key, picked))
            })
            .collect();
        AbstractValue::Dict(entries)
    });
    // The selected structure above owns the returned value. Raw output
    // paths from the map-producing helper are eager dependencies, not a
    // second whole-map splice beside the selected keys.
    EvalResult::with_effects(value, subject.effects.execution_only())
}

pub(super) fn eval_list(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut items = Vec::new();
    let mut effects = Effects::default();
    for arg in args {
        let item = eval_expr_with_helper_calls(arg, env, resolver);
        effects.merge(item.effects);
        items.push(item.value.unwrap_or(AbstractValue::Unknown));
    }
    EvalResult::with_effects(Some(AbstractValue::List(items)), effects)
}

pub(super) fn eval_concat(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut items = Vec::new();
    let mut effects = Effects::default();
    for arg in args {
        let mut result = eval_expr_with_helper_calls(arg, env, resolver);
        let value = result
            .value
            .take()
            .map(|value| value.with_output_meta(&result.effects.local_output_meta));
        effects.merge(result.effects);
        match value {
            Some(AbstractValue::List(mut values)) => items.append(&mut values),
            Some(value) => {
                if let Some(item) = value.fragment_range_item() {
                    items.push(item);
                }
            }
            None => {}
        }
    }
    EvalResult::with_effects(Some(AbstractValue::List(items)), effects)
}

pub(super) fn eval_prepend(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let [list, item] = args else {
        return eval_all_args(args, env, resolver);
    };
    let mut list = eval_expr_with_helper_calls(list, env, resolver);
    let item = eval_expr_with_helper_calls(item, env, resolver);
    list.effects.merge(item.effects);
    let mut items = item.value.into_iter().collect::<Vec<_>>();
    match list.value {
        Some(AbstractValue::List(mut values)) => items.append(&mut values),
        Some(value) => {
            if let Some(item) = value.fragment_range_item() {
                items.push(item);
            }
        }
        None => {}
    }
    EvalResult::with_effects(Some(AbstractValue::List(items)), list.effects)
}

/// `pluck KEY MAP` whose KEY is the current ranged key of the SAME map
/// selects exactly the current member: the result is the singleton list
/// holding that member's identity (signoz's `pluck . $dict | first` member
/// read inside `range keys .`). Other shapes keep widened-call semantics.
pub(super) fn eval_pluck(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    if let [key_expr, map_expr] = args {
        let key = eval_expr_with_helper_calls(key_expr, env, resolver);
        if let Some(AbstractValue::RangeKey(key_source)) = &key.value {
            let map = eval_expr_with_helper_calls(map_expr, env, resolver);
            let member = match &map.value {
                Some(
                    value
                    @ (AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path)),
                ) if path == key_source => value.fragment_range_item(),
                _ => None,
            };
            if let Some(member) = member {
                let mut effects = key.effects;
                effects.merge(map.effects);
                return EvalResult::with_effects(Some(AbstractValue::List(vec![member])), effects);
            }
        }
    }
    eval_unknown_call(args, Effects::default(), env, resolver)
}

pub(super) fn eval_first_result(result: EvalResult) -> EvalResult {
    let value = match result.value {
        Some(AbstractValue::List(items)) => items.first().cloned(),
        Some(AbstractValue::SplitList {
            source_paths,
            separator,
            total_text_preimage,
        }) => Some(AbstractValue::SplitSegment {
            source_paths,
            separator,
            last: false,
            total_text_preimage,
        }),
        Some(value) => value.fragment_range_item(),
        None => None,
    };
    EvalResult::with_effects(value, result.effects)
}

pub(super) fn eval_last_result(result: EvalResult) -> EvalResult {
    let value = match result.value {
        Some(AbstractValue::List(items)) => items.last().cloned(),
        Some(AbstractValue::SplitList {
            source_paths,
            separator,
            total_text_preimage,
        }) => Some(AbstractValue::SplitSegment {
            source_paths,
            separator,
            last: true,
            total_text_preimage,
        }),
        Some(value) => value.fragment_range_item(),
        None => None,
    };
    EvalResult::with_effects(value, result.effects)
}

pub(super) fn eval_reverse_result(result: EvalResult) -> EvalResult {
    let value = match result.value {
        Some(AbstractValue::List(mut items)) => {
            items.reverse();
            Some(AbstractValue::List(items))
        }
        other => other,
    };
    EvalResult::with_effects(value, result.effects)
}

pub(super) fn eval_split_list(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let [separator, subject] = args else {
        return eval_all_args(args, env, resolver);
    };
    let TemplateExpr::Literal(Literal::String(separator) | Literal::RawString(separator)) =
        separator.deparen()
    else {
        return eval_all_args(args, env, resolver);
    };
    let mut result = eval_expr_with_helper_calls(subject, env, resolver);
    let source_paths = result
        .value
        .as_ref()
        .map(AbstractValue::paths)
        .unwrap_or_default();
    let total_text_preimage = source_paths.iter().all(|path| {
        result.effects.shape_erased_paths.contains(path)
            || result
                .effects
                .local_output_meta
                .get(path)
                .is_some_and(|meta| meta.shape_erased || meta.derived_text)
    });
    // The subject must be a Go string at runtime whatever the split
    // produces: the literal-split fast path below is value refinement on
    // top of that contract, not a replacement for it.
    record_string_consumer_effects(
        &identity_value_paths(result.value.as_ref()),
        &mut result.effects,
    );
    let value = result.value.clone();
    record_range_key_string_consumer_effects(value.as_ref(), &mut result.effects);
    let Some(strings) = result.value.as_ref().map(AbstractValue::strings) else {
        let value = (!source_paths.is_empty()).then_some(AbstractValue::SplitList {
            source_paths,
            separator: separator.clone(),
            total_text_preimage,
        });
        return EvalResult::with_effects(value, result.effects);
    };
    if strings.is_empty() {
        let value = (!source_paths.is_empty()).then_some(AbstractValue::SplitList {
            source_paths,
            separator: separator.clone(),
            total_text_preimage,
        });
        return EvalResult::with_effects(value, result.effects);
    }

    let split_values = split_string_set(separator, strings);
    EvalResult::with_effects(split_values, result.effects)
}

pub(super) fn eval_regex_split(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let [pattern, subject, limit] = args else {
        return eval_all_args(args, env, resolver);
    };
    let mut subject = eval_expr_with_helper_calls(subject, env, resolver);
    let source_paths = subject
        .value
        .as_ref()
        .map(AbstractValue::paths)
        .unwrap_or_default();
    let total_text_preimage = source_paths.iter().all(|path| {
        subject.effects.shape_erased_paths.contains(path)
            || subject
                .effects
                .local_output_meta
                .get(path)
                .is_some_and(|meta| meta.shape_erased || meta.derived_text)
    });
    for arg in [pattern, limit] {
        subject
            .effects
            .merge(eval_expr_with_helper_calls(arg, env, resolver).effects);
    }
    record_string_call_consumers("regexSplit", args, env, resolver, &mut subject.effects);

    let separator = match pattern.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value))
            if is_literal_regex(value) =>
        {
            value.clone()
        }
        _ => {
            return EvalResult::with_effects(AbstractValue::widened(source_paths), subject.effects);
        }
    };
    let value = (!source_paths.is_empty()).then_some(AbstractValue::SplitList {
        source_paths,
        separator,
        total_text_preimage,
    });
    EvalResult::with_effects(value, subject.effects)
}

fn is_literal_regex(pattern: &str) -> bool {
    !pattern.is_empty()
        && !pattern.chars().any(|character| {
            matches!(
                character,
                '\\' | '.' | '^' | '$' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}'
            )
        })
}

pub(super) fn eval_nonempty_split(
    args: &[TemplateExpr],
    piped: Option<EvalResult>,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let (separator, mut subject, piped_for_facts) = match (piped, args) {
        (None, [separator, subject]) => (
            eval_expr_with_helper_calls(separator, env, resolver),
            eval_expr_with_helper_calls(subject, env, resolver),
            None,
        ),
        (Some(subject), [separator]) => {
            let piped_for_facts = subject.clone();
            (
                eval_expr_with_helper_calls(separator, env, resolver),
                subject,
                Some(piped_for_facts),
            )
        }
        (None, _) => return eval_all_args(args, env, resolver),
        (Some(mut subject), _) => {
            merge_arg_effects(args, env, resolver, &mut subject.effects);
            return subject;
        }
    };
    subject.effects.merge(separator.effects);
    let mut effects = subject.effects;
    if let Some(piped) = piped_for_facts.as_ref() {
        let (string_paths, raw_range_key_paths) =
            string_invocation_operand_facts("split", args, Some(piped), env, resolver);
        record_string_consumer_effects(&string_paths, &mut effects);
        super::strict_operands::record_nil_strict_identity_operand(
            piped.value.as_ref(),
            &mut effects,
        );
        record_raw_range_key_string_consumer_paths(&raw_range_key_paths, &mut effects);
    } else {
        record_string_call_consumers("split", args, env, resolver, &mut effects);
    }
    let separator = value_strings(separator.value.as_ref());
    let value = single_string(separator).and_then(|separator| {
        // A raw-identity subject keeps its path through `._0` qualified by
        // the separator as a lexical escape before the legacy map
        // degrade.
        subject
            .value
            .as_ref()
            .and_then(|value| split_transformed_value(value, &effects, &separator))
            .or_else(|| nonempty_split_map(subject.value.as_ref(), &separator))
    });
    EvalResult::with_effects(value, effects)
}

pub(super) fn nonempty_split_map(
    source: Option<&AbstractValue>,
    separator: &str,
) -> Option<AbstractValue> {
    let strings = source.map(AbstractValue::strings).unwrap_or_default();
    if strings.is_empty() {
        return Some(AbstractValue::Overlay {
            entries: BTreeMap::from([("_0".to_string(), AbstractValue::Unknown)]),
            fallback: Box::new(AbstractValue::Unknown),
        });
    }
    AbstractValue::choice(
        strings
            .into_iter()
            .map(|value| {
                AbstractValue::Dict(
                    value
                        .split(separator)
                        .enumerate()
                        .map(|(index, part)| {
                            (
                                format!("_{index}"),
                                AbstractValue::StringSet(BTreeSet::from([part.to_string()])),
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

fn single_string(strings: BTreeSet<String>) -> Option<String> {
    let mut strings = strings.into_iter();
    let first = strings.next()?;
    strings.next().is_none().then_some(first)
}

pub(super) fn is_nonempty_string_literal(expr: &TemplateExpr) -> bool {
    matches!(
        expr.deparen(),
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value))
            if !value.is_empty()
    )
}

pub(super) fn split_string_set(
    separator: &str,
    strings: BTreeSet<String>,
) -> Option<AbstractValue> {
    if separator.is_empty() {
        return None;
    }

    let choices = strings
        .into_iter()
        .map(|value| {
            AbstractValue::List(
                value
                    .split(separator)
                    .map(|part| AbstractValue::StringSet(BTreeSet::from([part.to_string()])))
                    .collect(),
            )
        })
        .collect();
    AbstractValue::choice(choices)
}

pub(super) fn eval_append(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = Effects::default();
    let mut items = match args
        .first()
        .map(|expr| eval_expr_with_helper_calls(expr, env, resolver))
    {
        Some(result) => {
            effects.merge(result.effects);
            match result.value {
                Some(AbstractValue::List(items)) => items,
                Some(value) => value.fragment_range_item().into_iter().collect(),
                None => Vec::new(),
            }
        }
        None => Vec::new(),
    };
    if let Some((_, rest)) = args.split_first() {
        merge_arg_values(rest, env, resolver, &mut items, &mut effects);
    }
    EvalResult::with_effects(Some(AbstractValue::List(items)), effects)
}

pub(super) fn eval_omit(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let Some((base, key_args)) = args.split_first() else {
        return eval_all_args(args, env, resolver);
    };
    let mut base = eval_expr_with_helper_calls(base, env, resolver);
    let mut keys = BTreeSet::new();
    for arg in key_args {
        let key = eval_expr_with_helper_calls(arg, env, resolver);
        keys.extend(value_strings(key.value.as_ref()));
        base.effects.merge(key.effects);
    }
    let value = base.value.map(|value| value.omit_keys(&keys));
    // A values-backed base keeps its path identity (structural entries are
    // filtered in place), so the removal is recorded as an effect: sink
    // typing for the removed members must not bind through this render
    // (external-secrets' OpenShift `adaptSecurityContext` omit).
    if let Some(path) = value
        .as_ref()
        .and_then(AbstractValue::direct_values_identity)
    {
        base.effects
            .local_output_meta
            .entry(path.clone())
            .or_default()
            .input_identity = true;
        base.effects
            .omitted_map_keys
            .entry(path)
            .or_default()
            .extend(keys.iter().cloned());
    }
    EvalResult::with_effects(value, base.effects)
}

pub(super) fn eval_merge(
    function: &str,
    args: &[TemplateExpr],
    piped: EvalResult,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut piped_meta = piped.effects.local_output_meta.clone();
    piped_meta.retain(|_, meta| !meta.omitted_keys.is_empty());
    let piped_values = piped
        .value
        .map(|value| value.with_output_meta(&piped_meta))
        .into_iter()
        .collect::<Vec<_>>();
    let mut effects = piped.effects;
    let operand_count = args.len() + piped_values.len();
    let mut values = Vec::new();
    for arg in args {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        let mut output_meta = result.effects.local_output_meta.clone();
        output_meta.retain(|_, meta| !meta.omitted_keys.is_empty());
        if let Some(value) = result.value {
            values.push(value.with_output_meta(&output_meta));
        }
        effects.merge(result.effects);
    }
    // A Go pipeline passes the piped subject as the LAST argument.
    values.extend(piped_values);
    // An overwrite-merge whose destination is the LITERAL values root
    // mutates the shared map in place: members under each source prefix
    // overwrite their effective-root twins for the rest of the render
    // (istiod's `mustMergeOverwrite $.Values (index $.Values "pilot")`
    // descope). Only the direct `$.Values`/`.Values` spelling qualifies —
    // a copied destination (`deepCopy $.Values`) mutates nothing shared.
    if matches!(function, "mergeOverwrite" | "mustMergeOverwrite")
        && args.first().is_some_and(expr_is_direct_values_root)
    {
        for source in values.iter().skip(1) {
            if let Some(path) = source.unique_path().filter(|path| !path.is_empty()) {
                effects.values_root_overlay_prefixes.insert(path);
            }
        }
    }
    // A definitely-empty literal destination (`mergeOverwrite (dict) a b`)
    // supplies no keys in any state, so it neither shadows nor contributes:
    // dropping it keeps the remaining operands eligible for the ordered
    // layer form (KPS's fresh-dict annotation merges).
    let dropped_empty_literals = values
        .iter()
        .filter(|value| matches!(value, AbstractValue::Dict(entries) if entries.is_empty()))
        .count();
    values.retain(|value| !matches!(value, AbstractValue::Dict(entries) if entries.is_empty()));
    let operand_count = operand_count - dropped_empty_literals;
    // Each identity-bearing operand's splice rows tolerate Helm-falsy
    // inputs: the strict map contract rides the operand's own fail
    // implication, not the merged value's render. Recorded even when the
    // ordered-layer form below abstains — a fold site's operands carry the
    // same contract split (airflow's worker-family labels merges). An
    // operand that is ITSELF a layered merge records every layer identity:
    // a truthy non-map in any visible layer terminates the outer merge the
    // same way, and collapsing to one path would drop the member-level
    // contract (airflow's per-set labels under the merged worker context).
    for value in &values {
        if let Some(path) = value.merge_layer_identity().filter(|path| !path.is_empty()) {
            effects.merge_operand_paths.insert(path);
        } else if let AbstractValue::MergedLayers(layers) = value {
            for layer in layers {
                if let Some(path) = layer.merge_layer_identity().filter(|path| !path.is_empty()) {
                    effects.merge_operand_paths.insert(path);
                }
            }
        }
    }
    if let Some(layers) = merge_layer_order(function, operand_count, &values) {
        return EvalResult::with_effects(Some(AbstractValue::MergedLayers(layers)), effects);
    }
    EvalResult::with_effects(AbstractValue::merge_all(values), effects)
}

/// Whether the expression IS the shared values root (`$.Values` or
/// `.Values` at document scope), not a copy or a subtree.
fn expr_is_direct_values_root(expr: &TemplateExpr) -> bool {
    match expr.deparen() {
        TemplateExpr::Field(path) => path.as_slice() == ["Values"],
        TemplateExpr::Selector { operand, path } => {
            path.as_slice() == ["Values"]
                && matches!(operand.as_ref(), TemplateExpr::Variable(name) if name.is_empty())
        }
        _ => false,
    }
}

/// The merge operands as ordered layers, highest precedence first, when
/// every operand carries a distinct values-backed identity. Sprig's `merge`
/// keeps the FIRST occurrence of a key across its arguments while
/// `mergeOverwrite` keeps the LAST; any operand without a single identity
/// (a literal dict, a multi-path fallback) abstains to the unordered fold.
fn merge_layer_order(
    function: &str,
    operand_count: usize,
    values: &[AbstractValue],
) -> Option<Vec<AbstractValue>> {
    if values.len() < 2 || values.len() != operand_count {
        return None;
    }
    let identities = values
        .iter()
        .map(|value| value.unique_path().filter(|path| !path.is_empty()))
        .collect::<Option<Vec<_>>>()?;
    let distinct: BTreeSet<&String> = identities.iter().collect();
    if distinct.len() != identities.len() {
        return None;
    }
    let mut layers = values.to_vec();
    if function.contains("Overwrite") {
        layers.reverse();
    }
    Some(layers)
}
