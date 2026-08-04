use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::{
    Literal, TemplateExpr, literal_printf_format, render_printf_scalar_values,
    token_initial_printf_string_argument,
};
use helm_schema_core::{GuardDnf, GuardValue, Predicate};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, direct_values_path, eval_expr_with_helper_calls};
use crate::scalar_value::{ScalarValueDispatch, TruthCondition};

use helm_schema_ast::{
    is_checksum_function, is_coercing_arithmetic_function, is_merge_function,
    is_provenance_preserving_function, is_string_predicate_function, is_string_splitting_function,
    is_string_transform_function, is_total_numeric_cast_function, strict_operand_nil_aborts,
};

mod collections;
mod comparisons;
mod root_mutation;
mod serialization;
mod strict_operands;
mod traversal;
mod value_facts;

use collections::{
    eval_append, eval_coalesce, eval_concat, eval_default, eval_dict, eval_first,
    eval_first_result, eval_last, eval_last_result, eval_list, eval_merge, eval_nonempty_split,
    eval_nonempty_split_pipeline, eval_omit, eval_pick, eval_pluck, eval_prepend, eval_regex_split,
    eval_reverse, eval_reverse_result, eval_split_list, is_nonempty_string_literal,
};
use comparisons::{eval_comparison, eval_pipeline_comparison, eval_ternary, eval_type_is};
use root_mutation::eval_set_call;
use serialization::{
    conjoin_formatter_operand_selection, eval_cat, eval_from_json, eval_from_json_pipeline,
    eval_from_yaml, eval_from_yaml_pipeline, eval_join, eval_join_pipeline, eval_print,
    eval_printf, eval_regex_replace, eval_repeat, eval_replace, eval_replace_pipeline,
    eval_to_json, eval_to_json_result, eval_to_yaml, eval_to_yaml_result, eval_tpl,
    eval_trim_affix, eval_trim_affix_pipeline, record_printf_argument_effects,
    record_total_conversion_effects,
};
use strict_operands::{
    pipeline_string_operand_facts, push_fail_capture, record_collection_item_kind_result,
    record_length_bearing_operand, record_length_bearing_result, record_operand_presence_operands,
    record_operand_presence_result, record_raw_range_key_string_consumer_paths,
    record_strict_kind_operands, record_strict_kind_result, record_strict_parser_call,
    record_strict_parser_pipeline, record_string_call_consumers, record_string_consumer_effects,
    record_string_transform_effects, string_call_operand_facts,
};
use traversal::{eval_dig, eval_index};
use value_facts::{
    concrete_collection_len, concrete_integer, derive_value_text, identity_value_paths,
    mark_stringified_identities,
};

#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic operation together makes its state transitions easier to audit"
)]
pub(crate) fn eval_call_with_helper_calls(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    match function {
        "include" | "template" => {
            let mut result = eval_helper_call(args, env, resolver);
            record_values_root_helper_include(function, args, &mut result.effects);
            result
        }
        "set" if args.len() == 3 => eval_set_call(args, env, resolver),
        "unset" if args.len() == 2 => {
            let [target, key] = args else {
                return EvalResult::none();
            };
            let operand = eval_expr_with_helper_calls(target, env, resolver);
            let mut effects = operand.effects.clone();
            effects.merge(eval_expr_with_helper_calls(key, env, resolver).effects);
            record_strict_kind_result(
                &operand,
                "object",
                strict_operand_nil_aborts(function, direct_values_path(target).is_some()),
                &mut effects,
            );
            EvalResult::with_effects(operand.value, effects)
        }
        "default" if matches!(args, [_, _]) => {
            let [fallback, primary] = args else {
                return EvalResult::none();
            };
            let primary = eval_expr_with_helper_calls(primary, env, resolver);
            eval_default(primary, std::slice::from_ref(fallback), env, resolver)
        }
        "and" => eval_short_circuit_args(args, true, env, resolver),
        "or" => eval_short_circuit_args(args, false, env, resolver),
        "not" | "empty" if matches!(args, [_]) => {
            let Some(arg) = args.first() else {
                return EvalResult::none();
            };
            let operand = eval_expr_with_helper_calls(arg, env, resolver);
            let truth = operand.truth.negated();
            let effects = operand.effects;
            let value = Some(AbstractValue::DerivedBoolean(effects.output_paths.clone()));
            let mut result = EvalResult::with_effects(value, effects);
            result.truth = truth;
            result
        }
        "dict" => eval_dict(args, env, resolver).with_truth(if args.is_empty() {
            Predicate::False
        } else {
            Predicate::True
        }),
        "list" | "tuple" => eval_list(args, env, resolver).with_truth(if args.is_empty() {
            Predicate::False
        } else {
            Predicate::True
        }),
        "deepCopy" | "mustDeepCopy" if matches!(args, [_]) => {
            let Some(arg) = args.first() else {
                return EvalResult::none();
            };
            // `copystructure` walks the operand with reflection and faults
            // on a zero value, but any non-nil KIND copies, so the operand
            // carries a presence claim and no kind claim at all.
            let mut result = eval_expr_with_helper_calls(arg, env, resolver);
            record_operand_presence_operands(args, env, resolver, &mut result.effects);
            result
        }
        "first" if args.len() == 1 => {
            let mut result = eval_first(args, env, resolver);
            record_strict_kind_operands(
                function,
                args,
                "array",
                env,
                resolver,
                &mut result.effects,
            );
            result
        }
        "last" if args.len() == 1 => {
            let mut result = eval_last(args, env, resolver);
            record_strict_kind_operands(
                function,
                args,
                "array",
                env,
                resolver,
                &mut result.effects,
            );
            result
        }
        "initial" | "rest" | "compact" if matches!(args, [_]) => {
            let Some(arg) = args.first() else {
                return EvalResult::none();
            };
            let mut result = eval_expr_with_helper_calls(arg, env, resolver);
            record_strict_kind_operands(
                function,
                args,
                "array",
                env,
                resolver,
                &mut result.effects,
            );
            result
        }
        "slice" | "mustSlice" if (2..=3).contains(&args.len()) => {
            let Some((subject, bounds)) = args.split_first() else {
                return EvalResult::none();
            };
            let mut result = eval_expr_with_helper_calls(subject, env, resolver);
            record_strict_kind_operands(
                function,
                std::slice::from_ref(subject),
                "array",
                env,
                resolver,
                &mut result.effects,
            );
            merge_arg_effects(bounds, env, resolver, &mut result.effects);
            result
        }
        "reverse" if args.len() == 1 => {
            let mut result = eval_reverse(args, env, resolver);
            record_strict_kind_operands(
                function,
                args,
                "array",
                env,
                resolver,
                &mut result.effects,
            );
            result
        }
        "splitList" if args.len() == 2 => {
            let mut result = eval_split_list(args, env, resolver);
            record_string_call_consumers("splitList", args, env, resolver, &mut result.effects);
            result
        }
        "split" if matches!(args.first(), Some(separator) if args.len() == 2 && is_nonempty_string_literal(separator)) => {
            eval_nonempty_split(args, env, resolver)
        }
        "append" => {
            let mut result = eval_append(args, env, resolver);
            if let [subject, _] = args {
                record_strict_kind_operands(
                    function,
                    std::slice::from_ref(subject),
                    "array",
                    env,
                    resolver,
                    &mut result.effects,
                );
            }
            result
        }
        "omit" if !args.is_empty() => {
            let mut result = eval_omit(args, env, resolver);
            if let Some(subject) = args.first() {
                record_strict_kind_operands(
                    function,
                    std::slice::from_ref(subject),
                    "object",
                    env,
                    resolver,
                    &mut result.effects,
                );
            }
            result
        }
        function if is_merge_function(function) => {
            let mut result = eval_merge(function, args, EvalResult::none(), env, resolver);
            record_strict_kind_operands(
                function,
                args,
                "object",
                env,
                resolver,
                &mut result.effects,
            );
            result
        }
        "coalesce" => eval_coalesce(args, env, resolver),
        "genSignedCert" | "genSelfSignedCert" if args.len() >= 4 => {
            let operands = args
                .iter()
                .map(|arg| eval_expr_with_helper_calls(arg, env, resolver))
                .collect::<Vec<_>>();
            let mut effects = Effects::default();
            for operand in &operands {
                effects.merge(operand.effects.clone());
            }
            if let Some(operand) = operands.first() {
                record_strict_kind_result(
                    operand,
                    "string",
                    strict_operand_nil_aborts(function, false),
                    &mut effects,
                );
            }
            for (index, operand) in operands.iter().enumerate().take(3).skip(1) {
                record_strict_kind_result(
                    operand,
                    "array",
                    strict_operand_nil_aborts(function, false),
                    &mut effects,
                );
                record_collection_item_kind_result(
                    operand,
                    "string",
                    helm_schema_ast::strict_collection_item_pattern(function, index),
                    &mut effects,
                );
            }
            if let Some(operand) = operands.get(3) {
                record_strict_kind_result(
                    operand,
                    "integer",
                    strict_operand_nil_aborts(function, false),
                    &mut effects,
                );
            }
            EvalResult::with_effects(None, effects)
        }
        "eq" | "ne" if args.len() >= 2 => eval_comparison(function, args, env, resolver),
        // These stay on eval_unknown_call's widened-value semantics: their
        // results (a count, a membership bool, a rebuilt list) are dataflow
        // through the call, not the operand's identity, so downstream string
        // consumers must not type the operand through them.
        "concat" => {
            let mut result = eval_concat(args, env, resolver);
            record_strict_kind_operands(
                function,
                args,
                "array",
                env,
                resolver,
                &mut result.effects,
            );
            result
        }
        // The checksum family consumes a typed Go string subject and emits a
        // digest. Unknown-call value semantics (not a string transform) keep
        // an `include … | sha256sum` annotation's serialized placement
        // intact while the subject gains its strict-string contract.
        function if is_checksum_function(function) && args.len() == 1 => {
            let mut result = eval_unknown_call(args, Effects::default(), env, resolver);
            record_string_call_consumers(function, args, env, resolver, &mut result.effects);
            // The digest shares no text or shape with its subject, so a
            // RAW identity operand must not project any slot language
            // backward through the call (datadog's `userValues |
            // sha256sum` checksum annotation beside the raw block-scalar
            // payload).
            if let Some(subject) = args.first() {
                let subject = eval_expr_with_helper_calls(subject, env, resolver);
                result
                    .effects
                    .add_shape_erased_paths(identity_value_paths(subject.value.as_ref()));
            }
            result
        }
        // len/has additionally erase operand shape: only a derived count or
        // membership bool reaches the sink, never the operand itself, so a
        // scalar sink position must not text-type the operand.
        "len" if args.len() == 1 => {
            let scalar_dispatch = args
                .first()
                .and_then(|subject| split_length_dispatch(subject, env, resolver));
            let mut result = eval_unknown_call(args, Effects::default(), env, resolver);
            record_length_bearing_operand(args, env, resolver, &mut result.effects);
            let Some(subject_expr) = args.first() else {
                return result;
            };
            let subject = eval_expr_with_helper_calls(subject_expr, env, resolver);
            record_total_conversion_effects(
                identity_value_paths(subject.value.as_ref()),
                &mut result.effects,
            );
            // A statically known collection has a constant length, which
            // unrolled traversals compare against iteration ordinals.
            if let Some(length) = subject.value.as_ref().and_then(concrete_collection_len) {
                result.value = Some(AbstractValue::StringSet(
                    [length.to_string()].into_iter().collect(),
                ));
            }
            match scalar_dispatch {
                Some(dispatch) => result.with_scalar_dispatch(dispatch),
                None => result,
            }
        }
        // Coercing Sprig arithmetic (`mulf`, `add`, `floor`, …): every
        // values-backed operand passes through `cast.ToInt64`/`ToFloat64`
        // before the computation, so the arithmetic constrains nothing
        // about the raw operand's kind (a numeric string or junk that
        // coerces to zero all render); the result is derived numeric
        // content. Traefik's `goMemLimitPercentage` reaches `mulf` this way.
        function if is_coercing_arithmetic_function(function) => {
            let mut result = eval_all_args(args, env, resolver);
            for arg in args {
                let operand = eval_expr_with_helper_calls(arg, env, resolver);
                record_total_conversion_effects(
                    identity_value_paths(operand.value.as_ref()),
                    &mut result.effects,
                );
            }
            // Constant-fold `add1` over a statically known integer so an
            // unrolled-iteration ordinal stays exact (last-element
            // arithmetic).
            if function == "add1"
                && let [arg] = args
                && let Some(value) = eval_expr_with_helper_calls(arg, env, resolver)
                    .value
                    .as_ref()
                    .and_then(concrete_integer)
            {
                result.value = Some(AbstractValue::StringSet(
                    [(value + 1).to_string()].into_iter().collect(),
                ));
            }
            result
        }
        "has" if matches!(args, [_, _]) => {
            let [_, subject_expr] = args else {
                return EvalResult::none();
            };
            let mut result = eval_unknown_call(args, Effects::default(), env, resolver);
            record_strict_kind_operands(
                function,
                std::slice::from_ref(subject_expr),
                "array",
                env,
                resolver,
                &mut result.effects,
            );
            let subject = eval_expr_with_helper_calls(subject_expr, env, resolver);
            record_total_conversion_effects(
                identity_value_paths(subject.value.as_ref()),
                &mut result.effects,
            );
            result
        }
        "prepend" if matches!(args, [_, _]) => {
            let mut result = eval_prepend(args, env, resolver);
            if let Some(subject) = args.first() {
                record_strict_kind_operands(
                    function,
                    std::slice::from_ref(subject),
                    "array",
                    env,
                    resolver,
                    &mut result.effects,
                );
            }
            result
        }
        "hasKey" if matches!(args, [_, _]) => {
            let [subject_expr, key_expr] = args else {
                return EvalResult::none();
            };
            let subject = eval_expr_with_helper_calls(subject_expr, env, resolver);
            let key = eval_expr_with_helper_calls(key_expr, env, resolver);
            let mut effects = subject.effects.clone();
            effects.merge(key.effects);
            let mut result = EvalResult::with_effects(
                AbstractValue::widened(effects.output_paths.clone()),
                effects,
            );
            record_strict_kind_result(
                &subject,
                "object",
                strict_operand_nil_aborts(function, direct_values_path(subject_expr).is_some()),
                &mut result.effects,
            );
            record_total_conversion_effects(
                identity_value_paths(subject.value.as_ref()),
                &mut result.effects,
            );
            let key = key.value.as_ref().and_then(|value| {
                let AbstractValue::StringSet(strings) = value else {
                    return None;
                };
                let mut strings = strings.iter();
                match (strings.next(), strings.next()) {
                    (Some(key), None) => Some(key.as_str()),
                    _ => None,
                }
            });
            if let Some(predicate) =
                subject.value.as_ref().zip(key).and_then(|(subject, key)| {
                    crate::value_path_context::value_has_key(subject, key)
                })
            {
                result.truth = TruthCondition::exact(predicate);
            }
            result
        }
        "pick" if !args.is_empty() => {
            let mut result = eval_pick(args, env, resolver);
            if let Some(subject) = args.first() {
                record_strict_kind_operands(
                    function,
                    std::slice::from_ref(subject),
                    "object",
                    env,
                    resolver,
                    &mut result.effects,
                );
            }
            result
        }
        "keys" | "values" if args.len() == 1 => {
            let Some(arg) = args.first() else {
                return EvalResult::none();
            };
            let operand = eval_expr_with_helper_calls(arg, env, resolver);
            let mut result = eval_unknown_call(args, Effects::default(), env, resolver);
            record_strict_kind_result(
                &operand,
                "object",
                strict_operand_nil_aborts(function, direct_values_path(arg).is_some()),
                &mut result.effects,
            );
            record_total_conversion_effects(
                identity_value_paths(operand.value.as_ref()),
                &mut result.effects,
            );
            // `keys m` over a single values-backed map keeps the map
            // identity: ranging the result binds the key domain, and
            // plucking a ranged key back out of the same map is a member
            // projection.
            if function == "keys"
                && let Some(AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path)) =
                    &operand.value
            {
                result.value = Some(AbstractValue::KeysList(path.clone()));
            }
            result
        }
        // `sortAlpha` stringifies and reorders list items. Over a keys list
        // the items are already strings, so the collection identity
        // survives; other operands keep the widened-call semantics (it
        // coerces non-lists to a singleton, so it imposes no operand kind).
        "sortAlpha" if args.len() == 1 => {
            let Some(arg) = args.first() else {
                return EvalResult::none();
            };
            let operand = eval_expr_with_helper_calls(arg, env, resolver);
            match &operand.value {
                Some(AbstractValue::KeysList(_)) => operand,
                _ => eval_unknown_call(args, Effects::default(), env, resolver),
            }
        }
        "pluck" if args.len() >= 2 => {
            let mut result = eval_pluck(args, env, resolver);
            if let Some((key, maps)) = args.split_first() {
                record_strict_kind_operands(
                    function,
                    std::slice::from_ref(key),
                    "string",
                    env,
                    resolver,
                    &mut result.effects,
                );
                record_strict_kind_operands(
                    function,
                    maps,
                    "object",
                    env,
                    resolver,
                    &mut result.effects,
                );
            }
            result
        }
        "uniq" | "mustUniq" if args.len() == 1 => {
            let mut result = eval_all_args(args, env, resolver);
            let operand = result.clone();
            record_strict_kind_result(
                &operand,
                "array",
                strict_operand_nil_aborts(function, false),
                &mut result.effects,
            );
            result
        }
        "ternary" => eval_ternary(args, None, env, resolver),
        "print" => eval_print(args, env, resolver),
        "printf" => eval_printf(args, env, resolver),
        "replace" if args.len() == 3 => eval_replace(args, env, resolver),
        "trimPrefix" | "trimSuffix" if args.len() == 2 => {
            eval_trim_affix(function, args, env, resolver)
        }
        "regexReplaceAll"
        | "mustRegexReplaceAll"
        | "regexReplaceAllLiteral"
        | "mustRegexReplaceAllLiteral"
            if args.len() == 3 =>
        {
            eval_regex_replace(function, args, env, resolver)
        }
        "repeat" if args.len() == 2 => {
            let mut result = eval_repeat(args, env, resolver);
            let (string_paths, raw_range_key_paths) =
                string_call_operand_facts("repeat", args, env, resolver);
            record_string_transform_effects(
                "repeat",
                result.value.as_ref(),
                &string_paths,
                &raw_range_key_paths,
                &mut result.effects,
            );
            result
        }
        "tpl" if args.len() == 2 => eval_tpl(args, env, resolver),
        "lookup" if args.len() == 4 => {
            let mut effects = Effects::default();
            merge_arg_effects(args, env, resolver, &mut effects);
            record_string_call_consumers("lookup", args, env, resolver, &mut effects);
            // `lookup` returns cluster state selected by its arguments, not
            // any argument's runtime value. Keep argument evaluation and
            // strict string contracts as dependencies while leaving the
            // external map's contents unknown to downstream sinks.
            EvalResult::with_effects(Some(AbstractValue::Unknown), effects)
        }
        "cat" => eval_cat(args, env, resolver),
        "index" => eval_index(args, false, env, resolver),
        "get" if args.len() == 2 => eval_index(args, true, env, resolver),
        "dig" if args.len() >= 3 => eval_dig(args, env, resolver),
        "required" if matches!(args, [_, _]) => {
            let [message, subject] = args else {
                return EvalResult::none();
            };
            let message = eval_expr_with_helper_calls(message, env, resolver);
            let mut subject = eval_expr_with_helper_calls(subject, env, resolver);
            // Direct input identities use the dedicated required-presence
            // lane. A derived subject can instead fail when its exact
            // reduction is falsy, so retain that negated reduction as the
            // call's terminal condition.
            if !matches!(
                subject.value.as_ref(),
                Some(AbstractValue::ValuesPath(_) | AbstractValue::JsonDecodedPath(_))
            ) && let Some(truth) = subject.truth.predicate()
            {
                push_fail_capture(vec![truth.clone().negated()], &mut subject.effects);
            }
            subject.effects.merge(message.effects);
            subject
        }
        "typeIs" | "kindIs" if args.len() >= 2 => eval_type_is(function, args, env, resolver),
        "fromYaml" if args.len() == 1 => eval_from_yaml(args, env, resolver),
        "toYaml" if args.len() == 1 => eval_to_yaml(args, env, resolver),
        "fromJson" | "fromJsonArray" if args.len() == 1 => eval_from_json(args, env, resolver),
        "toJson" | "mustToJson" | "toRawJson" | "mustToRawJson" if args.len() == 1 => {
            eval_to_json(args, env, resolver)
        }
        "join" if args.len() == 2 => eval_join(args, env, resolver),
        "regexSplit" if args.len() == 3 => eval_regex_split(args, env, resolver),
        function if is_total_numeric_cast_function(function) && args.len() == 1 => {
            let result = eval_all_args(args, env, resolver);
            let mut effects = result.effects;
            record_total_conversion_effects(
                identity_value_paths(result.value.as_ref()),
                &mut effects,
            );
            EvalResult::with_effects(derive_value_text(result.value), effects)
        }
        function if is_string_transform_function(function) => {
            let result = eval_all_args(args, env, resolver);
            let scalar_dispatch = if function == "toString" {
                result
                    .scalar_dispatch
                    .as_ref()
                    .map(ScalarValueDispatch::stringified)
            } else {
                None
            };
            let mut effects = result.effects;
            let (string_paths, raw_range_key_paths) =
                string_call_operand_facts(function, args, env, resolver);
            record_string_transform_effects(
                function,
                result.value.as_ref(),
                &string_paths,
                &raw_range_key_paths,
                &mut effects,
            );
            let value = if matches!(function, "quote" | "squote") {
                result
                    .value
                    .map(AbstractValue::clear_plain_slot_string_format)
            } else if function == "toString" {
                mark_stringified_identities(result.value)
            } else {
                result.value
            };
            let result = EvalResult::with_effects(derive_value_text(value), effects);
            match scalar_dispatch {
                Some(dispatch) => result.with_scalar_dispatch(dispatch),
                None => result,
            }
        }
        // Subject-last string consumers with non-string output (`splitList`,
        // `semverCompare`): the LAST argument must be a Go string; the
        // output carries the subject's influence without its identity.
        function
            if (is_string_splitting_function(function)
                || is_string_predicate_function(function))
                && !args.is_empty() =>
        {
            let scalar_truth = args.last().and_then(|subject_expr| {
                let subject = eval_expr_with_helper_calls(subject_expr, env, resolver);
                let dispatch = subject
                    .scalar_dispatch
                    .or_else(|| direct_stringified_dispatch(subject_expr, env, resolver));
                scalar_pattern_condition(function, args, dispatch.as_ref()).or_else(|| {
                    direct_quoted_falsy_pattern_condition(
                        function,
                        args,
                        subject_expr,
                        env,
                        resolver,
                    )
                })
            });
            let result = eval_all_args(args, env, resolver);
            let mut effects = result.effects;
            record_string_call_consumers(function, args, env, resolver, &mut effects);
            record_strict_parser_call(function, args, env, resolver, &mut effects);
            let widened = AbstractValue::widened(
                result
                    .value
                    .as_ref()
                    .map(AbstractValue::paths)
                    .unwrap_or_default(),
            );
            let result = EvalResult::with_effects(widened, effects);
            let mut result = result;
            if let Some(truth) = scalar_truth {
                result.truth = truth;
            }
            result
        }
        function if is_provenance_preserving_function(function) => {
            eval_all_args(args, env, resolver)
        }
        _ => eval_unknown_call(args, Effects::default(), env, resolver),
    }
}

/// Record an `include NAME` invocation whose argument carries the VALUES
/// ROOT: the callee may be a chart-authored program-wrapper engine
/// rewriting the whole values tree (`include "tplYaml" (dict "doc"
/// .Values …)`), which the symbolic context decides by inspecting the
/// callee's body.
fn record_values_root_helper_include(
    _function: &str,
    args: &[TemplateExpr],
    effects: &mut Effects,
) {
    let Some(TemplateExpr::Literal(helm_schema_ast::Literal::String(name))) =
        args.first().map(TemplateExpr::deparen)
    else {
        return;
    };
    let mut passes_values_root = false;
    for arg in args.iter().skip(1) {
        arg.walk(|inner| match inner {
            TemplateExpr::Field(path) if path.as_slice() == ["Values"] => {
                passes_values_root = true;
            }
            TemplateExpr::Selector { operand, path }
                if path.as_slice() == ["Values"]
                    && matches!(
                        operand.as_ref().deparen(),
                        TemplateExpr::Variable(variable) if variable.is_empty()
                    ) =>
            {
                passes_values_root = true;
            }
            _ => {}
        });
    }
    if passes_values_root {
        effects.values_root_helper_includes.insert(name.clone());
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic operation together makes its state transitions easier to audit"
)]
pub(crate) fn eval_pipeline_with_helper_calls(
    stages: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let Some((first_stage, remaining_stages)) = stages.split_first() else {
        return EvalResult::none();
    };
    let mut current = eval_expr_with_helper_calls(first_stage, env, resolver);
    let mut current_is_direct_values_path = direct_values_path(first_stage).is_some();

    for stage in remaining_stages {
        let TemplateExpr::Call { function, args } = stage else {
            current
                .effects
                .merge(eval_expr_with_helper_calls(stage, env, resolver).effects);
            current_is_direct_values_path = false;
            continue;
        };

        let piped_is_direct_values_path = current_is_direct_values_path;
        current = match function.as_str() {
            "default" => eval_default(current, args, env, resolver),
            function if is_merge_function(function) => {
                let piped_operand = current.clone();
                let mut result = eval_merge(function, args, current, env, resolver);
                record_strict_kind_result(
                    &piped_operand,
                    "object",
                    strict_operand_nil_aborts(function, false),
                    &mut result.effects,
                );
                record_strict_kind_operands(
                    function,
                    args,
                    "object",
                    env,
                    resolver,
                    &mut result.effects,
                );
                result
            }
            "first" if args.is_empty() => {
                let operand = current.clone();
                let mut result = eval_first_result(current);
                record_strict_kind_result(
                    &operand,
                    "array",
                    strict_operand_nil_aborts(function, false),
                    &mut result.effects,
                );
                result
            }
            "last" if args.is_empty() => {
                let operand = current.clone();
                let mut result = eval_last_result(current);
                record_strict_kind_result(
                    &operand,
                    "array",
                    strict_operand_nil_aborts(function, false),
                    &mut result.effects,
                );
                result
            }
            "initial" | "rest" | "compact" if args.is_empty() => {
                let operand = current.clone();
                let mut result = current;
                record_strict_kind_result(
                    &operand,
                    "array",
                    strict_operand_nil_aborts(function, false),
                    &mut result.effects,
                );
                result
            }
            "slice" | "mustSlice" if (1..=2).contains(&args.len()) => {
                let operand = current.clone();
                let mut result = current;
                record_strict_kind_result(
                    &operand,
                    "array",
                    strict_operand_nil_aborts(function, false),
                    &mut result.effects,
                );
                merge_arg_effects(args, env, resolver, &mut result.effects);
                result
            }
            "reverse" if args.is_empty() => {
                let operand = current.clone();
                let mut result = eval_reverse_result(current);
                record_strict_kind_result(
                    &operand,
                    "array",
                    strict_operand_nil_aborts(function, false),
                    &mut result.effects,
                );
                result
            }
            "len" if args.is_empty() => {
                let operand = current.clone();
                let mut result = eval_unknown_call(args, current.effects, env, resolver);
                record_length_bearing_result(&operand, &mut result.effects);
                record_total_conversion_effects(
                    identity_value_paths(operand.value.as_ref()),
                    &mut result.effects,
                );
                if let Some(length) = operand.value.as_ref().and_then(concrete_collection_len) {
                    result.value = Some(AbstractValue::StringSet(
                        [length.to_string()].into_iter().collect(),
                    ));
                }
                result
            }
            "eq" | "ne" if !args.is_empty() => eval_pipeline_comparison(
                function,
                current,
                piped_is_direct_values_path,
                args,
                env,
                resolver,
            ),
            // The piped ternary operand is the condition: its strict Boolean
            // contract and effects flow, but its value is not a result arm.
            "ternary" => eval_ternary(
                args,
                Some((current, piped_is_direct_values_path)),
                env,
                resolver,
            ),
            "replace" if args.len() == 2 => eval_replace_pipeline(current, args, env, resolver),
            "trimPrefix" | "trimSuffix" if args.len() == 1 => {
                eval_trim_affix_pipeline(function, current, args, env, resolver)
            }
            // The piped checksum subject keeps unknown-stage value
            // semantics (see the call form above) while gaining its
            // strict-string contract (redis' sentinel
            // `coalesce … | sha256sum` lane).
            function if is_checksum_function(function) && args.is_empty() => {
                let (string_paths, raw_range_key_paths) = pipeline_string_operand_facts(
                    function,
                    args,
                    current.value.as_ref(),
                    &current.effects,
                    env,
                    resolver,
                );
                // The digest shares no text or shape with its subject —
                // same erasure as the call form above.
                let subject_identities = identity_value_paths(current.value.as_ref());
                let mut result = eval_unknown_call(args, current.effects, env, resolver);
                result.effects.add_shape_erased_paths(subject_identities);
                record_string_consumer_effects(&string_paths, &mut result.effects);
                record_raw_range_key_string_consumer_paths(
                    &raw_range_key_paths,
                    &mut result.effects,
                );
                result
            }
            function if is_string_transform_function(function) => {
                let scalar_dispatch = if function == "toString" {
                    current
                        .scalar_dispatch
                        .as_ref()
                        .map(ScalarValueDispatch::stringified)
                } else {
                    None
                };
                let (string_paths, raw_range_key_paths) = pipeline_string_operand_facts(
                    function,
                    args,
                    current.value.as_ref(),
                    &current.effects,
                    env,
                    resolver,
                );
                let mut effects = current.effects;
                for arg in args {
                    let arg_result = eval_expr_with_helper_calls(arg, env, resolver);
                    if function == "b64enc" {
                        effects.add_encoded_paths(identity_value_paths(arg_result.value.as_ref()));
                    }
                    effects.merge(arg_result.effects);
                }
                record_string_transform_effects(
                    function,
                    current.value.as_ref(),
                    &string_paths,
                    &raw_range_key_paths,
                    &mut effects,
                );
                let value = if matches!(function, "quote" | "squote") {
                    current
                        .value
                        .map(AbstractValue::clear_plain_slot_string_format)
                } else if function == "toString" {
                    mark_stringified_identities(current.value)
                } else {
                    current.value
                };
                let result = EvalResult::with_effects(derive_value_text(value), effects);
                match scalar_dispatch {
                    Some(dispatch) => result.with_scalar_dispatch(dispatch),
                    None => result,
                }
            }
            "fromYaml" => eval_from_yaml_pipeline(current, args, env, resolver),
            "fromJson" | "fromJsonArray" => eval_from_json_pipeline(current, args, env, resolver),
            "printf" => {
                let piped_dispatch = current.scalar_dispatch.clone();
                let mut effects = current.effects;
                let piped_scalar = piped_dispatch
                    .as_ref()
                    .and_then(ScalarValueDispatch::constant_value);
                // The piped value is printf's FINAL data argument; `args`
                // hold the format plus any leading data arguments.
                let piped = identity_value_paths(current.value.as_ref());
                let token_initial_string_argument = token_initial_printf_string_argument(args);
                if token_initial_string_argument == Some(args.len()) {
                    conjoin_formatter_operand_selection(
                        &piped,
                        piped_dispatch.as_ref(),
                        &mut effects,
                    );
                }
                record_printf_argument_effects(false, &piped, &mut effects);
                let mut scalar_values = Vec::with_capacity(args.len());
                for (index, arg) in args.iter().enumerate() {
                    let mut result = eval_expr_with_helper_calls(arg, env, resolver);
                    let identity_paths = identity_value_paths(result.value.as_ref());
                    if token_initial_string_argument == Some(index) {
                        conjoin_formatter_operand_selection(
                            &identity_paths,
                            result.scalar_dispatch.as_ref(),
                            &mut result.effects,
                        );
                    }
                    if index > 0 {
                        scalar_values.push(
                            result
                                .scalar_dispatch
                                .as_ref()
                                .and_then(ScalarValueDispatch::constant_value),
                        );
                    }
                    effects.merge(result.effects);
                    record_printf_argument_effects(index == 0, &identity_paths, &mut effects);
                }
                scalar_values.push(piped_scalar);
                let dispatch = literal_printf_format(args)
                    .and_then(|format| {
                        scalar_values
                            .into_iter()
                            .collect::<Option<Vec<_>>>()
                            .and_then(|values| render_printf_scalar_values(format, &values))
                    })
                    .map(|value| ScalarValueDispatch::constant(GuardValue::string(value)))
                    .or_else(|| {
                        if args.len() != 1 {
                            return None;
                        }
                        let format = literal_printf_format(args)?;
                        piped_dispatch.as_ref()?.printf_string(format)
                    });
                let result = EvalResult::with_effects(current.value, effects);
                match dispatch {
                    Some(dispatch) => result.with_scalar_dispatch(dispatch),
                    None => result,
                }
            }
            "join" => eval_join_pipeline(current, args, env, resolver),
            "split" if matches!(args.as_slice(), [separator] if is_nonempty_string_literal(separator)) => {
                eval_nonempty_split_pipeline(current, args, env, resolver)
            }
            function if is_total_numeric_cast_function(function) => {
                let mut effects = current.effects;
                record_total_conversion_effects(
                    identity_value_paths(current.value.as_ref()),
                    &mut effects,
                );
                merge_arg_effects(args, env, resolver, &mut effects);
                EvalResult::with_effects(current.value, effects)
            }
            // The piped operand and every explicit operand of a coercing
            // arithmetic stage are coerced before the computation: their raw
            // kinds are unconstrained (`… | mulf $percentage`).
            function if is_coercing_arithmetic_function(function) => {
                let mut effects = current.effects;
                record_total_conversion_effects(
                    identity_value_paths(current.value.as_ref()),
                    &mut effects,
                );
                for arg in args {
                    let operand = eval_expr_with_helper_calls(arg, env, resolver);
                    record_total_conversion_effects(
                        identity_value_paths(operand.value.as_ref()),
                        &mut effects,
                    );
                    effects.merge(operand.effects);
                }
                let value = AbstractValue::widened(
                    current
                        .value
                        .as_ref()
                        .map(AbstractValue::paths)
                        .unwrap_or_default(),
                );
                EvalResult::with_effects(value, effects)
            }
            function
                if is_string_splitting_function(function)
                    || is_string_predicate_function(function) =>
            {
                let scalar_truth =
                    scalar_pattern_condition(function, args, current.scalar_dispatch.as_ref());
                let piped = current.clone();
                let (string_paths, raw_range_key_paths) = pipeline_string_operand_facts(
                    function,
                    args,
                    current.value.as_ref(),
                    &current.effects,
                    env,
                    resolver,
                );
                let mut effects = current.effects;
                merge_arg_effects(args, env, resolver, &mut effects);
                record_string_consumer_effects(&string_paths, &mut effects);
                record_raw_range_key_string_consumer_paths(&raw_range_key_paths, &mut effects);
                record_strict_parser_pipeline(
                    function,
                    args,
                    &piped,
                    piped_is_direct_values_path,
                    env,
                    resolver,
                    &mut effects,
                );
                let widened = AbstractValue::widened(
                    current
                        .value
                        .as_ref()
                        .map(AbstractValue::paths)
                        .unwrap_or_default(),
                );
                let mut result = EvalResult::with_effects(widened, effects);
                if let Some(truth) = scalar_truth {
                    result.truth = truth;
                }
                result
            }
            "toYaml" => {
                let mut result = eval_to_yaml_result(current);
                merge_arg_effects(args, env, resolver, &mut result.effects);
                result
            }
            "toJson" | "mustToJson" | "toRawJson" | "mustToRawJson" => {
                let mut result = eval_to_json_result(current);
                merge_arg_effects(args, env, resolver, &mut result.effects);
                result
            }
            "concat" => {
                let piped_operand = current.clone();
                let mut result = eval_unknown_call(args, current.effects, env, resolver);
                record_strict_kind_result(
                    &piped_operand,
                    "array",
                    strict_operand_nil_aborts(function, false),
                    &mut result.effects,
                );
                record_strict_kind_operands(
                    function,
                    args,
                    "array",
                    env,
                    resolver,
                    &mut result.effects,
                );
                result
            }
            "has" if args.len() == 1 => {
                let piped_operand = current.clone();
                let mut result = eval_unknown_call(args, current.effects, env, resolver);
                record_strict_kind_result(
                    &piped_operand,
                    "array",
                    strict_operand_nil_aborts(function, false),
                    &mut result.effects,
                );
                record_total_conversion_effects(
                    identity_value_paths(piped_operand.value.as_ref()),
                    &mut result.effects,
                );
                result
            }
            "keys" | "values" if args.is_empty() => {
                let operand = current.clone();
                let mut result = eval_unknown_call(args, current.effects, env, resolver);
                record_strict_kind_result(
                    &operand,
                    "object",
                    strict_operand_nil_aborts(function, false),
                    &mut result.effects,
                );
                record_total_conversion_effects(
                    identity_value_paths(operand.value.as_ref()),
                    &mut result.effects,
                );
                if function == "keys"
                    && let Some(
                        AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path),
                    ) = &operand.value
                {
                    result.value = Some(AbstractValue::KeysList(path.clone()));
                }
                result
            }
            // `sortAlpha` stringifies and reorders items; a keys list
            // survives (its items are already strings), other operands keep
            // the widened-stage semantics.
            "sortAlpha" if args.is_empty() => match &current.value {
                Some(AbstractValue::KeysList(_)) => current,
                _ => eval_unknown_call(args, current.effects, env, resolver),
            },
            "uniq" | "mustUniq" => {
                let piped_operand = current.clone();
                let mut effects = current.effects;
                merge_arg_effects(args, env, resolver, &mut effects);
                let mut result = EvalResult::with_effects(current.value, effects);
                record_strict_kind_result(
                    &piped_operand,
                    "array",
                    strict_operand_nil_aborts(function, false),
                    &mut result.effects,
                );
                result
            }
            "deepCopy" | "mustDeepCopy" if args.is_empty() => {
                let operand = current.clone();
                let mut result = current;
                record_operand_presence_result(&operand, &mut result.effects);
                result
            }
            function if is_provenance_preserving_function(function) => {
                let mut effects = current.effects;
                merge_arg_effects(args, env, resolver, &mut effects);
                EvalResult::with_effects(current.value, effects)
            }
            // An unknown stage widens the pipeline value, but everything
            // that flowed into the pipeline so far still influences it.
            _ => eval_unknown_call(args, current.effects, env, resolver),
        };
        current_is_direct_values_path = false;
    }

    current
}

fn eval_short_circuit_args(
    args: &[TemplateExpr],
    previous_truthy: bool,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = Effects::default();
    let mut values = Vec::new();
    let mut execution_predicates = BTreeSet::new();
    let mut operand_conditions = Vec::with_capacity(args.len());
    let mut constrained_env = env.clone();
    for (index, arg) in args.iter().enumerate() {
        let mut result = eval_expr_with_helper_calls(arg, &constrained_env, resolver);
        scope_execution_effects(&mut result.effects, &execution_predicates);

        // Each polarity is a sound subset of the operand's real domain.
        // Partial conditions keep an approximation marker around that
        // subset: positive-only consumers may use it, while member-domain
        // ownership and other complement-sensitive consumers abstain.
        let operand_truth = result.truth.clone();
        operand_conditions.push(operand_truth.clone());
        let mut selection = execution_predicates.clone();
        if index + 1 < args.len() {
            // The chain's VALUE is this operand's exactly when the chain
            // stops here, which is the operand's condition inverted for
            // `and` and held for `or`.
            let predicate = if previous_truthy {
                short_circuit_polarity(&operand_truth, false, "and operand selection")
            } else {
                short_circuit_polarity(&operand_truth, true, "or operand selection")
            };
            if predicate != Predicate::True {
                selection.insert(predicate);
            }
        }
        conjoin_result_selection(&mut result, &selection);
        if let Some(value) = result.value {
            values.push(value);
        }
        effects.merge(result.effects);

        if index + 1 == args.len() {
            break;
        }
        let next_condition = if previous_truthy {
            short_circuit_polarity(&operand_truth, true, "and operand execution")
        } else {
            short_circuit_polarity(&operand_truth, false, "or operand execution")
        };
        if next_condition != Predicate::True {
            execution_predicates.insert(next_condition);
        }
        constrained_env.bound_values = constrained_env
            .bound_values
            .with_predicate_constraints(arg, previous_truthy);
    }
    let mut result = EvalResult::with_effects(AbstractValue::choice(values), effects);
    result.truth = combined_short_circuit_truth(&operand_conditions, previous_truthy);
    result
}

fn short_circuit_polarity(truth: &TruthCondition, truthy: bool, marker: &str) -> Predicate {
    let subset = if truthy {
        truth.when_true()
    } else {
        truth.when_false()
    };
    if truth.predicate().is_some() {
        return subset;
    }
    let paths = subset.value_paths();
    Predicate::approximate_with_sound_predicate(marker, paths, subset)
}

fn split_length_dispatch(
    expr: &TemplateExpr,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> Option<ScalarValueDispatch> {
    let TemplateExpr::Call { function, args } = expr.deparen() else {
        return None;
    };
    let [separator, subject] = args.as_slice() else {
        return None;
    };
    if function != "split" {
        return None;
    }
    let TemplateExpr::Literal(Literal::String(separator) | Literal::RawString(separator)) =
        separator.deparen()
    else {
        return None;
    };
    if separator.is_empty() {
        return None;
    }
    let subject = eval_expr_with_helper_calls(subject, env, resolver);
    Some(subject.scalar_dispatch?.split_length(separator))
}

fn scalar_pattern_condition(
    function: &str,
    args: &[TemplateExpr],
    dispatch: Option<&ScalarValueDispatch>,
) -> Option<TruthCondition> {
    let dispatch = dispatch?;
    match function {
        "regexMatch" | "mustRegexMatch" => {
            let pattern = literal_string(args.first()?)?;
            Some(dispatch.condition_matches_pattern(pattern))
        }
        "contains" => {
            let needle = literal_string(args.first()?)?;
            Some(dispatch.condition_matches_pattern(&escape_regex_literal(needle)))
        }
        "hasPrefix" => {
            let prefix = literal_string(args.first()?)?;
            Some(dispatch.condition_matches_pattern(&format!("^{}", escape_regex_literal(prefix))))
        }
        "hasSuffix" => {
            let suffix = literal_string(args.first()?)?;
            Some(dispatch.condition_matches_pattern(&format!("{}$", escape_regex_literal(suffix))))
        }
        "semverCompare" => {
            let constraint = literal_string(args.first()?)?;
            Some(dispatch.condition_matches_semver(constraint))
        }
        _ => None,
    }
}

/// Quotes literal text for every regular-expression dialect emitted by helm-schema.
#[doc(hidden)]
#[must_use]
pub fn escape_regex_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn direct_stringified_dispatch(
    expr: &TemplateExpr,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> Option<ScalarValueDispatch> {
    let TemplateExpr::Call { function, args } = expr.deparen() else {
        return None;
    };
    if function != "toString" {
        return None;
    }
    let [subject] = args.as_slice() else {
        return None;
    };
    eval_expr_with_helper_calls(subject, env, resolver)
        .scalar_dispatch
        .map(|dispatch| dispatch.stringified())
}

fn direct_quoted_falsy_pattern_condition(
    function: &str,
    args: &[TemplateExpr],
    subject_expr: &TemplateExpr,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> Option<TruthCondition> {
    let TemplateExpr::Call {
        function: conversion,
        args: conversion_args,
    } = subject_expr.deparen()
    else {
        return None;
    };
    let [subject] = conversion_args.as_slice() else {
        return None;
    };
    if conversion != "quote" {
        return None;
    }
    let needle = literal_string(args.first()?)?;
    let falsy_spellings = [
        "",
        r#""""#,
        r#""0""#,
        r#""-0""#,
        r#""false""#,
        r#""<nil>""#,
        r#""[]""#,
        r#""map[]""#,
    ];
    let matches_falsy = match function {
        "contains" => falsy_spellings.iter().any(|value| value.contains(needle)),
        "hasPrefix" => falsy_spellings
            .iter()
            .any(|value| value.starts_with(needle)),
        "hasSuffix" => falsy_spellings.iter().any(|value| value.ends_with(needle)),
        _ => return None,
    };
    if matches_falsy {
        return None;
    }
    let subject = eval_expr_with_helper_calls(subject, env, resolver);
    let AbstractValue::ValuesPath(path) = subject.value? else {
        return None;
    };
    Some(TruthCondition::from_subsets(
        Predicate::False,
        Predicate::truthy_path(path).negated(),
        false,
    ))
}

fn literal_string(expr: &TemplateExpr) -> Option<&str> {
    let TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) = expr.deparen()
    else {
        return None;
    };
    Some(value)
}

fn combined_short_circuit_truth(operands: &[TruthCondition], conjunction: bool) -> TruthCondition {
    if operands.is_empty() {
        return TruthCondition::Unknown;
    }
    if conjunction {
        TruthCondition::all(operands.iter().cloned())
    } else {
        TruthCondition::any(operands.iter().cloned())
    }
}

pub(super) fn conjoin_result_selection(result: &mut EvalResult, predicates: &BTreeSet<Predicate>) {
    if predicates.is_empty() {
        return;
    }
    let (embedded_paths, embedded_meta) = result.value.take().map_or_else(
        || (BTreeSet::new(), BTreeMap::new()),
        |value| {
            // The value and `local_output_meta` are parallel projections of
            // the same identities. Fuse them before selection, then publish
            // the selected metadata back from one owner; otherwise a fresh
            // predicate-only sibling later unions away the helper's inner
            // branch condition.
            let mut value = value.with_output_meta(&result.effects.local_output_meta);
            let embedded_paths = value.conjoin_output_path_branches(predicates);
            let embedded_meta = value.output_meta();
            result.value = Some(value);
            (embedded_paths, embedded_meta)
        },
    );
    for (path, meta) in embedded_meta {
        result.effects.local_output_meta.insert(path, meta);
    }
    for path in identity_value_paths(result.value.as_ref()) {
        if !embedded_paths.contains(&path) {
            let mut meta = crate::helper_meta::HelperOutputMeta::default();
            meta.conjoin_branches(predicates);
            result.effects.local_output_meta.insert(path, meta);
        }
    }
    for row in &mut result.effects.helper_rendered {
        row.meta.conjoin_branches(predicates);
    }
}

fn scope_execution_effects(effects: &mut Effects, predicates: &BTreeSet<Predicate>) {
    if predicates.is_empty() {
        return;
    }

    for meta in effects.local_output_meta.values_mut() {
        meta.conjoin_branches(predicates);
    }
    for row in effects
        .helper_rendered
        .iter_mut()
        .chain(&mut effects.helper_dependency_rendered)
    {
        row.meta.conjoin_branches(predicates);
    }
    for read in &mut effects.helper_reads {
        read.condition = read
            .condition
            .conjoined(&GuardDnf::from_conjunction(predicates.iter().cloned()));
    }
    for capture in &mut effects.helper_fails {
        for predicate in predicates {
            if !capture.conjunction.contains(predicate) {
                capture.conjunction.push(predicate.clone());
            }
        }
    }
    effects.member_host_conversions = std::mem::take(&mut effects.member_host_conversions)
        .into_iter()
        .map(|mut conversion| {
            for predicate in predicates {
                if !conversion.outer_predicates.contains(predicate) {
                    conversion.outer_predicates.push(predicate.clone());
                }
            }
            conversion
        })
        .collect();

    let direct_string_paths = std::mem::take(&mut effects.direct_string_consumer_paths);
    for path in direct_string_paths {
        effects.string_contract_paths.remove(&path);
        strict_operands::push_value_type_capture(
            predicates.iter().cloned().collect(),
            path,
            "string".to_string(),
            false,
            effects,
        );
    }

    // Conditional mutation channels cannot yet carry an execution guard.
    // Ignoring those mutations is conservative; applying them globally
    // would let a skipped short-circuit operand alter later analysis.
    effects.local_set_mutations.clear();
    effects.root_set_mutations.clear();
    effects.root_set_predicates.clear();
    effects.root_set_value_dispatches.clear();
}

fn eval_helper_call(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    if let Some(TemplateExpr::Literal(Literal::String(name) | Literal::RawString(name))) =
        args.first().map(TemplateExpr::deparen)
        && let Some(result) = resolver.resolve_helper_call(name, args.get(1))
    {
        return result;
    }
    if let Some(template_name) = args.first().and_then(template_base_path_suffix)
        && let Some(result) = resolver.resolve_implicit_template_call(&template_name, args.get(1))
    {
        return result;
    }
    if let Some(callee_expr) = args.first()
        && !matches!(callee_expr.deparen(), TemplateExpr::Literal(_))
    {
        let callee = eval_expr_with_helper_calls(callee_expr, env, resolver);
        if let Some(AbstractValue::StringSet(names)) = &callee.value
            && names.len() == 1
            && let Some(name) = names.first()
            && let Some(mut result) = resolver.resolve_helper_call(name, args.get(1))
        {
            result.effects.merge(callee.effects.execution_only());
            return result;
        }
    }

    if env.skip_helper_call_args {
        return EvalResult::none();
    }

    // Unresolved helper calls stay value-free: their output is attributed by
    // the bound-helper summary path, so carrying the call-site argument paths
    // as widened provenance would double-attribute the context argument.
    let mut effects = Effects::default();
    merge_arg_effects(args, env, resolver, &mut effects);
    EvalResult::with_effects(None, effects)
}

fn template_base_path_suffix(expr: &TemplateExpr) -> Option<String> {
    let TemplateExpr::Call { function, args } = expr.deparen() else {
        return None;
    };
    let (base, suffix_args) = args.split_first()?;
    if function != "print" || suffix_args.is_empty() || !is_template_base_path(base) {
        return None;
    }

    let mut suffix = String::new();
    for arg in suffix_args {
        let TemplateExpr::Literal(Literal::String(part) | Literal::RawString(part)) = arg.deparen()
        else {
            return None;
        };
        suffix.push_str(part);
    }
    (!suffix.is_empty()).then_some(suffix)
}

fn is_template_base_path(expr: &TemplateExpr) -> bool {
    match expr.deparen() {
        TemplateExpr::Field(path) => path.as_slice() == ["Template", "BasePath"],
        TemplateExpr::Selector { operand, path } => {
            path.as_slice() == ["Template", "BasePath"]
                && matches!(operand.deparen(), TemplateExpr::Variable(name) if name.is_empty())
        }
        _ => false,
    }
}

fn eval_all_args(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut values = Vec::new();
    let mut effects = Effects::default();
    merge_arg_values(args, env, resolver, &mut values, &mut effects);
    EvalResult::with_effects(AbstractValue::choice(values), effects)
}

fn merge_arg_values(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    values: &mut Vec<AbstractValue>,
    effects: &mut Effects,
) {
    for arg in args {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        effects.merge(result.effects);
        if let Some(value) = result.value {
            values.push(value);
        }
    }
}

fn merge_arg_effects(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    for arg in args {
        effects.merge(eval_expr_with_helper_calls(arg, env, resolver).effects);
    }
}

/// A call without a transfer function widens: the value is unknown, but every
/// path that flowed into the call (including a piped value's effects) still
/// influences the result.
fn eval_unknown_call(
    args: &[TemplateExpr],
    mut effects: Effects,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    merge_arg_effects(args, env, resolver, &mut effects);
    let value = AbstractValue::widened(effects.output_paths.clone());
    EvalResult::with_effects(value, effects)
}
