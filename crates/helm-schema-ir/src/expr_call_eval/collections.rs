use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use helm_schema_ast::expression_schema_type;
use helm_schema_core::{GuardValue, Predicate};

use super::strict_operands::{
    pipeline_string_operand_facts, record_range_key_string_consumer_effects,
    record_raw_range_key_string_consumer_paths, record_string_call_consumers,
    record_string_consumer_effects,
};
use super::value_facts::{identity_value_paths, split_transformed_value, value_strings};
use super::{eval_all_args, eval_unknown_call, merge_arg_effects, merge_arg_values};

/// `default FALLBACK PRIMARY` and `PRIMARY | default FALLBACK` are one rule:
/// the primary's identity paths become defaulted (typed by a literal
/// fallback), and the value is the choice of primary and fallback values.
pub(super) fn eval_default(
    primary: EvalResult,
    fallback_args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = primary.effects;
    let primary_paths = identity_value_paths(&primary.value);
    let primary_identity = direct_raw_identity_path(primary.value.as_ref());
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
        .and_then(expression_schema_type)
    {
        effects.add_fallback_type_hints(primary_paths, schema_type);
    }
    let mut values = primary.value.into_iter().collect::<Vec<_>>();
    let mut fallback_paths = BTreeSet::new();
    for fallback in fallback_args {
        let result = eval_expr_with_helper_calls(fallback, env, resolver);
        fallback_paths.extend(identity_value_paths(&result.value));
        effects.merge(result.effects);
        if let Some(value) = result.value {
            values.push(value);
        }
    }
    if let Some(primary_path) = primary_identity {
        let overlaps_fallback = fallback_paths.remove(&primary_path);
        if !overlaps_fallback {
            effects
                .local_output_meta
                .entry(primary_path.clone())
                .or_default()
                .conjoin_branches(&BTreeSet::from([Predicate::truthy_path(
                    primary_path.clone(),
                )]));
        }
        let fallback_condition = BTreeSet::from([Predicate::truthy_path(primary_path).negated()]);
        for path in fallback_paths {
            effects
                .local_output_meta
                .entry(path)
                .or_default()
                .conjoin_branches(&fallback_condition);
        }
    }
    EvalResult::with_effects(AbstractValue::choice(values), effects)
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
    let mut candidate_paths = Vec::with_capacity(args.len());
    for arg in args {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        default_paths.extend(identity_value_paths(&result.value));
        let candidate_path = matches!(
            arg.deparen(),
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
        )
        .then(|| direct_raw_identity_path(result.value.as_ref()))
        .flatten()
        .filter(|path| !path.is_empty());
        candidate_paths.push(candidate_path);
        effects.merge(result.effects);
        if let Some(value) = result.value {
            values.push(value);
        }
    }
    // `coalesce` selects only non-empty candidates. Downstream strict consumers therefore see
    // each source path only while it is truthy, just as they see a `default` primary.
    effects.add_default_paths(default_paths);
    // A computed candidate makes every later arm depend on a truthiness test
    // we cannot name, so only claim ordered selection when every arm has a
    // direct identity.
    if candidate_paths.iter().all(Option::is_some) {
        let mut previous_false = BTreeSet::new();
        let mut seen = BTreeSet::new();
        for path in candidate_paths.into_iter().flatten() {
            if seen.insert(path.clone()) {
                let mut selection = previous_false.clone();
                selection.insert(Predicate::truthy_path(path.clone()));
                effects
                    .local_output_meta
                    .entry(path.clone())
                    .or_default()
                    .conjoin_branches(&selection);
            }
            previous_false.insert(Predicate::truthy_path(path).negated());
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
    EvalResult::with_effects(AbstractValue::choice(values), effects)
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
        let stringified = meta.is_some_and(|meta| meta.stringified) || stringified_in_effects(path);
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
    let mut effects = Effects::default();
    let mut index = 0usize;
    while index + 1 < args.len() {
        let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) = &args[index]
        else {
            index += 1;
            continue;
        };
        let value = eval_expr_with_helper_calls(&args[index + 1], env, resolver);
        effects.merge(value.effects);
        map.insert(key.clone(), value.value.unwrap_or(AbstractValue::Unknown));
        index += 2;
    }
    EvalResult::with_effects(Some(AbstractValue::Dict(map)), effects)
}

pub(super) fn eval_pick(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut subject = eval_expr_with_helper_calls(&args[0], env, resolver);
    let mut keys = BTreeSet::new();
    for arg in &args[1..] {
        let key = eval_expr_with_helper_calls(arg, env, resolver);
        keys.extend(value_strings(&key.value));
        subject.effects.merge(key.effects);
    }
    let value = subject.value.map(|value| {
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
    EvalResult::with_effects(value, subject.effects)
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
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        effects.merge(result.effects);
        match result.value {
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
    let mut list = eval_expr_with_helper_calls(&args[0], env, resolver);
    let item = eval_expr_with_helper_calls(&args[1], env, resolver);
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
    if args.len() == 2 {
        let key = eval_expr_with_helper_calls(&args[0], env, resolver);
        if let Some(AbstractValue::RangeKey(key_source)) = &key.value {
            let map = eval_expr_with_helper_calls(&args[1], env, resolver);
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

pub(super) fn eval_first(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    eval_first_result(eval_expr_with_helper_calls(&args[0], env, resolver))
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

pub(super) fn eval_last(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    eval_last_result(eval_expr_with_helper_calls(&args[0], env, resolver))
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

pub(super) fn eval_reverse(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    eval_reverse_result(eval_expr_with_helper_calls(&args[0], env, resolver))
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
    let separator = match args[0].deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => value,
        _ => return eval_all_args(args, env, resolver),
    };
    let mut result = eval_expr_with_helper_calls(&args[1], env, resolver);
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
    record_string_consumer_effects(&identity_value_paths(&result.value), &mut result.effects);
    let value = result.value.clone();
    record_range_key_string_consumer_effects(&value, &mut result.effects);
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
    let mut subject = eval_expr_with_helper_calls(&args[1], env, resolver);
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
    for arg in [&args[0], &args[2]] {
        subject
            .effects
            .merge(eval_expr_with_helper_calls(arg, env, resolver).effects);
    }
    record_string_call_consumers("regexSplit", args, env, resolver, &mut subject.effects);

    let separator = match args[0].deparen() {
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
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let separator = eval_expr_with_helper_calls(&args[0], env, resolver);
    let mut subject = eval_expr_with_helper_calls(&args[1], env, resolver);
    subject.effects.merge(separator.effects);
    let mut effects = subject.effects;
    record_string_call_consumers("split", args, env, resolver, &mut effects);
    let separator = value_strings(&separator.value);
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

pub(super) fn eval_nonempty_split_pipeline(
    current: EvalResult,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let (string_paths, raw_range_key_paths) = pipeline_string_operand_facts(
        "split",
        args,
        &current.value,
        &current.effects,
        env,
        resolver,
    );
    let separator = args
        .first()
        .map(|arg| eval_expr_with_helper_calls(arg, env, resolver))
        .and_then(|result| single_string(value_strings(&result.value)));
    let value = separator.as_deref().and_then(|separator| {
        current
            .value
            .as_ref()
            .and_then(|value| split_transformed_value(value, &current.effects, separator))
            .or_else(|| nonempty_split_map(current.value.as_ref(), separator))
    });
    let mut effects = current.effects;
    merge_arg_effects(args, env, resolver, &mut effects);
    record_string_consumer_effects(&string_paths, &mut effects);
    record_raw_range_key_string_consumer_paths(&raw_range_key_paths, &mut effects);
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
                Some(value) => vec![value],
                None => Vec::new(),
            }
        }
        None => Vec::new(),
    };
    merge_arg_values(&args[1..], env, resolver, &mut items, &mut effects);
    EvalResult::with_effects(Some(AbstractValue::List(items)), effects)
}

pub(super) fn eval_omit(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut base = eval_expr_with_helper_calls(&args[0], env, resolver);
    let mut keys = BTreeSet::new();
    for arg in &args[1..] {
        let key = eval_expr_with_helper_calls(arg, env, resolver);
        keys.extend(value_strings(&key.value));
        base.effects.merge(key.effects);
    }
    let value = base.value.map(|value| value.omit_keys(&keys));
    // A values-backed base keeps its path identity (structural entries are
    // filtered in place), so the removal is recorded as an effect: sink
    // typing for the removed members must not bind through this render
    // (external-secrets' OpenShift `adaptSecurityContext` omit).
    if let Some(path) = value.as_ref().and_then(AbstractValue::unique_path) {
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
    let mut effects = piped.effects;
    let piped_values = piped.value.into_iter().collect::<Vec<_>>();
    let operand_count = args.len() + piped_values.len();
    let mut values = Vec::new();
    merge_arg_values(args, env, resolver, &mut values, &mut effects);
    // A Go pipeline passes the piped subject as the LAST argument.
    values.extend(piped_values);
    // Each identity-bearing operand's splice rows tolerate Helm-falsy
    // inputs: the strict map contract rides the operand's own fail
    // implication, not the merged value's render. Recorded even when the
    // ordered-layer form below abstains — a fold site's operands carry the
    // same contract split (airflow's worker-family labels merges).
    for value in &values {
        if let Some(path) = value.merge_layer_identity().filter(|path| !path.is_empty()) {
            effects.merge_operand_paths.insert(path);
        }
    }
    if let Some(layers) = merge_layer_order(function, operand_count, &values) {
        return EvalResult::with_effects(Some(AbstractValue::MergedLayers(layers)), effects);
    }
    EvalResult::with_effects(AbstractValue::merge_all(values), effects)
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
