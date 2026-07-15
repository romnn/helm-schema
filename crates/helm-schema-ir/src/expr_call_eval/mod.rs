use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};

use helm_schema_ast::{
    is_merge_function, is_provenance_preserving_function, is_string_predicate_function,
    is_string_splitting_function, is_string_transform_function, is_total_numeric_cast_function,
};

mod collections;
mod comparisons;
mod root_mutation;
mod serialization;
mod strict_operands;
mod traversal;
mod value_facts;

use collections::{
    eval_append, eval_coalesce, eval_default, eval_dict, eval_first, eval_first_result, eval_list,
    eval_merge, eval_nonempty_split, eval_nonempty_split_pipeline, eval_omit, eval_pick,
    eval_reverse, eval_reverse_result, eval_split_list, is_nonempty_string_literal,
};
use comparisons::{eval_comparison, eval_pipeline_comparison, eval_ternary, eval_type_is};
use root_mutation::eval_set_call;
use serialization::{
    eval_cat, eval_from_json, eval_from_json_pipeline, eval_from_yaml, eval_from_yaml_pipeline,
    eval_join, eval_join_pipeline, eval_print, eval_printf, eval_to_json, eval_to_json_result,
    eval_to_yaml, eval_to_yaml_result, eval_tpl, record_printf_argument_effects,
    record_total_conversion_effects,
};
use strict_operands::{
    pipeline_string_operand_facts, record_length_bearing_operand, record_length_bearing_result,
    record_raw_range_key_string_consumer_paths, record_strict_kind_operands,
    record_strict_kind_result, record_string_call_consumers, record_string_consumer_effects,
    record_string_transform_effects, string_call_operand_facts,
};
use traversal::{eval_dig, eval_index};
use value_facts::identity_value_paths;

pub(crate) fn eval_call_with_helper_calls(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    match function {
        "include" | "template" => eval_helper_call(args, env, resolver),
        "set" if args.len() == 3 => eval_set_call(args, env, resolver),
        "default" if args.len() == 2 => {
            let primary = eval_expr_with_helper_calls(&args[1], env, resolver);
            eval_default(primary, &args[..1], env, resolver)
        }
        "and" => eval_short_circuit_args(args, true, env, resolver),
        "or" => eval_short_circuit_args(args, false, env, resolver),
        "dict" => eval_dict(args, env, resolver),
        "list" | "tuple" => eval_list(args, env, resolver),
        "first" if args.len() == 1 => {
            let mut result = eval_first(args, env, resolver);
            record_strict_kind_operands(args, "array", env, resolver, &mut result.effects);
            result
        }
        "reverse" if args.len() == 1 => {
            let mut result = eval_reverse(args, env, resolver);
            record_strict_kind_operands(args, "array", env, resolver, &mut result.effects);
            result
        }
        "splitList" if args.len() == 2 => {
            let mut result = eval_split_list(args, env, resolver);
            record_string_call_consumers("splitList", args, env, resolver, &mut result.effects);
            result
        }
        "split" if args.len() == 2 && is_nonempty_string_literal(&args[0]) => {
            eval_nonempty_split(args, env, resolver)
        }
        "append" => {
            let mut result = eval_append(args, env, resolver);
            if args.len() == 2 {
                record_strict_kind_operands(
                    &args[..1],
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
            record_strict_kind_operands(&args[..1], "object", env, resolver, &mut result.effects);
            result
        }
        function if is_merge_function(function) => {
            let mut result = eval_merge(args, EvalResult::none(), env, resolver);
            record_strict_kind_operands(args, "object", env, resolver, &mut result.effects);
            result
        }
        "coalesce" => eval_coalesce(args, env, resolver),
        "eq" | "ne" if args.len() >= 2 => eval_comparison(args, env, resolver),
        // These stay on eval_unknown_call's widened-value semantics: their
        // results (a count, a membership bool, a rebuilt list) are dataflow
        // through the call, not the operand's identity, so downstream string
        // consumers must not type the operand through them.
        "concat" => {
            let mut result = eval_unknown_call(args, Effects::default(), env, resolver);
            record_strict_kind_operands(args, "array", env, resolver, &mut result.effects);
            result
        }
        // len/has additionally erase operand shape: only a derived count or
        // membership bool reaches the sink, never the operand itself, so a
        // scalar sink position must not text-type the operand.
        "len" if args.len() == 1 => {
            let mut result = eval_unknown_call(args, Effects::default(), env, resolver);
            record_length_bearing_operand(args, env, resolver, &mut result.effects);
            let subject = eval_expr_with_helper_calls(&args[0], env, resolver);
            record_total_conversion_effects(
                identity_value_paths(&subject.value),
                &mut result.effects,
            );
            result
        }
        "has" if args.len() == 2 => {
            let mut result = eval_unknown_call(args, Effects::default(), env, resolver);
            record_strict_kind_operands(&args[1..], "array", env, resolver, &mut result.effects);
            let subject = eval_expr_with_helper_calls(&args[1], env, resolver);
            record_total_conversion_effects(
                identity_value_paths(&subject.value),
                &mut result.effects,
            );
            result
        }
        "prepend" if args.len() == 2 => {
            let mut result = eval_unknown_call(args, Effects::default(), env, resolver);
            record_strict_kind_operands(&args[..1], "array", env, resolver, &mut result.effects);
            result
        }
        "hasKey" if args.len() == 2 => {
            let mut result = eval_unknown_call(args, Effects::default(), env, resolver);
            record_strict_kind_operands(&args[..1], "object", env, resolver, &mut result.effects);
            let subject = eval_expr_with_helper_calls(&args[0], env, resolver);
            record_total_conversion_effects(
                identity_value_paths(&subject.value),
                &mut result.effects,
            );
            result
        }
        "pick" if !args.is_empty() => {
            let mut result = eval_pick(args, env, resolver);
            record_strict_kind_operands(&args[..1], "object", env, resolver, &mut result.effects);
            result
        }
        "uniq" | "mustUniq" if args.len() == 1 => {
            let mut result = eval_all_args(args, env, resolver);
            let operand = result.clone();
            record_strict_kind_result(&operand, "array", &mut result.effects);
            result
        }
        "ternary" => eval_ternary(args, Effects::default(), env, resolver),
        "print" => eval_print(args, env, resolver),
        "printf" => eval_printf(args, env, resolver),
        "tpl" if args.len() == 2 => eval_tpl(args, env, resolver),
        "cat" => eval_cat(args, env, resolver),
        "index" => eval_index(args, false, env, resolver),
        "get" if args.len() == 2 => eval_index(args, true, env, resolver),
        "dig" if args.len() >= 3 => eval_dig(args, env, resolver),
        "required" if args.len() == 2 => {
            let message = eval_expr_with_helper_calls(&args[0], env, resolver);
            let mut subject = eval_expr_with_helper_calls(&args[1], env, resolver);
            subject.effects.merge(message.effects);
            subject
        }
        "typeIs" | "kindIs" if args.len() >= 2 => eval_type_is(args, env, resolver),
        "fromYaml" if args.len() == 1 => eval_from_yaml(args, env, resolver),
        "toYaml" if args.len() == 1 => eval_to_yaml(args, env, resolver),
        "fromJson" if args.len() == 1 => eval_from_json(args, env, resolver),
        "toJson" | "mustToJson" | "toRawJson" | "mustToRawJson" if args.len() == 1 => {
            eval_to_json(args, env, resolver)
        }
        "join" if args.len() == 2 => eval_join(args, env, resolver),
        function if is_total_numeric_cast_function(function) && args.len() == 1 => {
            let result = eval_all_args(args, env, resolver);
            let mut effects = result.effects;
            record_total_conversion_effects(identity_value_paths(&result.value), &mut effects);
            EvalResult::with_effects(result.value, effects)
        }
        function if is_string_transform_function(function) => {
            let result = eval_all_args(args, env, resolver);
            let mut effects = result.effects;
            let (string_paths, raw_range_key_paths) =
                string_call_operand_facts(function, args, env, resolver);
            record_string_transform_effects(
                function,
                &result.value,
                &string_paths,
                &raw_range_key_paths,
                &mut effects,
            );
            EvalResult::with_effects(result.value, effects)
        }
        // Subject-last string consumers with non-string output (`splitList`,
        // `semverCompare`): the LAST argument must be a Go string; the
        // output carries the subject's influence without its identity.
        function
            if (is_string_splitting_function(function)
                || is_string_predicate_function(function))
                && !args.is_empty() =>
        {
            let result = eval_all_args(args, env, resolver);
            let mut effects = result.effects;
            record_string_call_consumers(function, args, env, resolver, &mut effects);
            let widened = AbstractValue::widened(
                result
                    .value
                    .as_ref()
                    .map(AbstractValue::paths)
                    .unwrap_or_default(),
            );
            EvalResult::with_effects(widened, effects)
        }
        function if is_provenance_preserving_function(function) => {
            eval_all_args(args, env, resolver)
        }
        _ => eval_unknown_call(args, Effects::default(), env, resolver),
    }
}

pub(crate) fn eval_pipeline_with_helper_calls(
    stages: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let Some(first_stage) = stages.first() else {
        return EvalResult::none();
    };
    let mut current = eval_expr_with_helper_calls(first_stage, env, resolver);

    for stage in &stages[1..] {
        let TemplateExpr::Call { function, args } = stage else {
            current
                .effects
                .merge(eval_expr_with_helper_calls(stage, env, resolver).effects);
            continue;
        };

        current = match function.as_str() {
            "default" => eval_default(current, args, env, resolver),
            function if is_merge_function(function) => {
                let piped_operand = current.clone();
                let mut result = eval_merge(args, current, env, resolver);
                record_strict_kind_result(&piped_operand, "object", &mut result.effects);
                record_strict_kind_operands(args, "object", env, resolver, &mut result.effects);
                result
            }
            "first" if args.is_empty() => {
                let operand = current.clone();
                let mut result = eval_first_result(current);
                record_strict_kind_result(&operand, "array", &mut result.effects);
                result
            }
            "reverse" if args.is_empty() => {
                let operand = current.clone();
                let mut result = eval_reverse_result(current);
                record_strict_kind_result(&operand, "array", &mut result.effects);
                result
            }
            "len" if args.is_empty() => {
                let operand = current.clone();
                let mut result = eval_unknown_call(args, current.effects, env, resolver);
                record_length_bearing_result(&operand, &mut result.effects);
                record_total_conversion_effects(
                    identity_value_paths(&operand.value),
                    &mut result.effects,
                );
                result
            }
            "eq" | "ne" if !args.is_empty() => {
                eval_pipeline_comparison(current, args, env, resolver)
            }
            // The piped ternary operand is the condition: its effects flow,
            // its value does not.
            "ternary" => eval_ternary(args, current.effects, env, resolver),
            function if is_string_transform_function(function) => {
                let (string_paths, raw_range_key_paths) = pipeline_string_operand_facts(
                    function,
                    args,
                    &current.value,
                    &current.effects,
                    env,
                    resolver,
                );
                let mut effects = current.effects;
                for arg in args {
                    let arg_result = eval_expr_with_helper_calls(arg, env, resolver);
                    if function == "b64enc" {
                        effects.add_encoded_paths(identity_value_paths(&arg_result.value));
                    }
                    effects.merge(arg_result.effects);
                }
                record_string_transform_effects(
                    function,
                    &current.value,
                    &string_paths,
                    &raw_range_key_paths,
                    &mut effects,
                );
                EvalResult::with_effects(current.value, effects)
            }
            "fromYaml" => eval_from_yaml_pipeline(current, args, env, resolver),
            "fromJson" => eval_from_json_pipeline(current, args, env, resolver),
            "printf" => {
                let mut effects = current.effects;
                // The piped value is printf's FINAL data argument; `args`
                // hold the format plus any leading data arguments.
                let piped = identity_value_paths(&current.value);
                record_printf_argument_effects(false, &piped, &mut effects);
                for (index, arg) in args.iter().enumerate() {
                    let result = eval_expr_with_helper_calls(arg, env, resolver);
                    let identity_paths = identity_value_paths(&result.value);
                    effects.merge(result.effects);
                    record_printf_argument_effects(index == 0, &identity_paths, &mut effects);
                }
                EvalResult::with_effects(current.value, effects)
            }
            "join" => eval_join_pipeline(current, args, env, resolver),
            "split" if args.len() == 1 && is_nonempty_string_literal(&args[0]) => {
                eval_nonempty_split_pipeline(current, args, env, resolver)
            }
            function if is_total_numeric_cast_function(function) => {
                let mut effects = current.effects;
                record_total_conversion_effects(identity_value_paths(&current.value), &mut effects);
                merge_arg_effects(args, env, resolver, &mut effects);
                EvalResult::with_effects(current.value, effects)
            }
            function
                if is_string_splitting_function(function)
                    || is_string_predicate_function(function) =>
            {
                let (string_paths, raw_range_key_paths) = pipeline_string_operand_facts(
                    function,
                    args,
                    &current.value,
                    &current.effects,
                    env,
                    resolver,
                );
                let mut effects = current.effects;
                merge_arg_effects(args, env, resolver, &mut effects);
                record_string_consumer_effects(&string_paths, &mut effects);
                record_raw_range_key_string_consumer_paths(&raw_range_key_paths, &mut effects);
                let widened = AbstractValue::widened(
                    current
                        .value
                        .as_ref()
                        .map(AbstractValue::paths)
                        .unwrap_or_default(),
                );
                EvalResult::with_effects(widened, effects)
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
                record_strict_kind_result(&piped_operand, "array", &mut result.effects);
                record_strict_kind_operands(args, "array", env, resolver, &mut result.effects);
                result
            }
            "has" if args.len() == 1 => {
                let piped_operand = current.clone();
                let mut result = eval_unknown_call(args, current.effects, env, resolver);
                record_strict_kind_result(&piped_operand, "array", &mut result.effects);
                record_total_conversion_effects(
                    identity_value_paths(&piped_operand.value),
                    &mut result.effects,
                );
                result
            }
            "uniq" | "mustUniq" => {
                let piped_operand = current.clone();
                let mut effects = current.effects;
                merge_arg_effects(args, env, resolver, &mut effects);
                let mut result = EvalResult::with_effects(current.value, effects);
                record_strict_kind_result(&piped_operand, "array", &mut result.effects);
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
    let mut constrained_env = env.clone();
    for arg in args {
        effects.merge(eval_expr_with_helper_calls(arg, &constrained_env, resolver).effects);
        constrained_env.bound_values = constrained_env
            .bound_values
            .with_predicate_constraints(arg, previous_truthy);
    }
    EvalResult::with_effects(None, effects)
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
