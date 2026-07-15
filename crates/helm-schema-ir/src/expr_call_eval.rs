use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::{AbstractValue, path_is_encoded};
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{
    HelperCallValueResolver, direct_values_path, eval_expr, eval_expr_with_helper_calls,
};
use crate::helper_meta::HelperOutputMeta;
use helm_schema_ast::expression_schema_type;
use helm_schema_ast::type_is_schema_type;
use helm_schema_core::{Guard, GuardValue, Predicate};

use helm_schema_ast::{
    is_merge_function, is_provenance_preserving_function, is_string_predicate_function,
    is_string_splitting_function, is_string_transform_function, is_total_numeric_cast_function,
    is_total_stringification_function, string_operand_indices,
};
use helm_schema_ast::{literal_printf_format, render_printf_string_sets};

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

fn record_string_transform_effects(
    function: &str,
    value: &Option<AbstractValue>,
    string_paths: &BTreeSet<String>,
    raw_range_key_paths: &BTreeSet<String>,
    effects: &mut Effects,
) {
    let paths = identity_value_paths(value);
    if is_total_stringification_function(function) {
        // Sprig's `strval` fallback renders ANY input (maps, lists, nil), so
        // a total stringification constrains nothing about its input and the
        // sink observes only the rendered text, never the input shape.
        record_total_conversion_effects(paths, effects);
        effects
            .derived_range_key_paths
            .extend(identity_range_key_paths(value));
        return;
    }
    record_string_consumer_effects(string_paths, effects);
    record_raw_range_key_string_consumer_paths(raw_range_key_paths, effects);
    effects.derived_text_paths.extend(paths.iter().cloned());
    effects
        .derived_range_key_paths
        .extend(identity_range_key_paths(value));
    if function == "b64enc" {
        effects.add_encoded_paths(string_paths.clone());
    }
}

fn string_call_operand_facts(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut paths = BTreeSet::new();
    let mut range_key_paths = BTreeSet::new();
    for index in string_operand_indices(function, args.len()) {
        let Some(arg) = args.get(index) else {
            continue;
        };
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        paths.extend(identity_value_paths(&result.value));
        let keys = identity_range_key_paths(&result.value);
        range_key_paths.extend(
            keys.difference(&result.effects.derived_range_key_paths)
                .cloned(),
        );
    }
    (paths, range_key_paths)
}

fn pipeline_string_operand_facts(
    function: &str,
    args: &[TemplateExpr],
    piped_value: &Option<AbstractValue>,
    piped_effects: &Effects,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut paths = BTreeSet::new();
    let mut range_key_paths = BTreeSet::new();
    for index in string_operand_indices(function, args.len() + 1) {
        if index == args.len() {
            paths.extend(identity_value_paths(piped_value));
            let keys = identity_range_key_paths(piped_value);
            range_key_paths.extend(
                keys.difference(&piped_effects.derived_range_key_paths)
                    .cloned(),
            );
        } else if let Some(arg) = args.get(index) {
            let result = eval_expr_with_helper_calls(arg, env, resolver);
            paths.extend(identity_value_paths(&result.value));
            let keys = identity_range_key_paths(&result.value);
            range_key_paths.extend(
                keys.difference(&result.effects.derived_range_key_paths)
                    .cloned(),
            );
        }
    }
    (paths, range_key_paths)
}

fn record_string_call_consumers(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    let (paths, raw_range_key_paths) = string_call_operand_facts(function, args, env, resolver);
    record_string_consumer_effects(&paths, effects);
    record_raw_range_key_string_consumer_paths(&raw_range_key_paths, effects);
}

/// Record that an expression stage consumes the RAW value of `paths` as a
/// Go string, failing rendering otherwise. A path that already passed a
/// converting stage (`printf … | trunc`) or flows out of a shape-erasing
/// local binding reaches the consumer as derived text, so the earlier
/// conversion owns the contract. A path behind an ordered value selector is
/// consumed only on its selected arm, so its contract is captured as a
/// conditional fail-class implication instead of an unconditional row
/// contract.
fn record_string_consumer_effects(paths: &BTreeSet<String>, effects: &mut Effects) {
    for path in paths {
        if effects.derived_text_paths.contains(path)
            || effects
                .local_output_meta
                .get(path)
                .is_some_and(|meta| meta.shape_erased || meta.derived_text)
        {
            continue;
        }
        let has_selection_condition = effects.defaults.contains(path)
            || effects.local_default_paths.contains(path)
            || effects
                .local_output_meta
                .get(path)
                .is_some_and(|meta| !meta.predicates.is_empty());
        if has_selection_condition {
            for mut conjunction in operand_selection_conjunctions(effects, path) {
                conjunction.push(
                    Predicate::from(crate::Guard::TypeIs {
                        path: path.clone(),
                        schema_type: "string".to_string(),
                    })
                    .negated(),
                );
                let capture = crate::eval_effect::FailCapture {
                    conjunction,
                    approximate_condition_paths: BTreeSet::new(),
                    direct_ranged_paths: BTreeSet::new(),
                    json_decoded_ranged_paths: BTreeSet::new(),
                    destructured_ranged_paths: BTreeSet::new(),
                    member_access: false,
                    member_access_handled_kinds: BTreeSet::new(),
                    range_key_string_paths: BTreeSet::new(),
                };
                if !effects.helper_fails.contains(&capture) {
                    effects.helper_fails.push(capture);
                }
            }
        } else {
            effects.string_contract_paths.insert(path.clone());
            effects.direct_string_consumer_paths.insert(path.clone());
        }
    }
}

fn record_range_key_string_consumer_effects(value: &Option<AbstractValue>, effects: &mut Effects) {
    let paths = identity_range_key_paths(value);
    let raw_paths = paths
        .difference(&effects.derived_range_key_paths)
        .cloned()
        .collect::<BTreeSet<_>>();
    record_raw_range_key_string_consumer_paths(&raw_paths, effects);
    effects.derived_range_key_paths.extend(paths);
}

fn record_raw_range_key_string_consumer_paths(raw_paths: &BTreeSet<String>, effects: &mut Effects) {
    if !raw_paths.is_empty() {
        let capture = crate::eval_effect::FailCapture {
            conjunction: Vec::new(),
            approximate_condition_paths: BTreeSet::new(),
            direct_ranged_paths: BTreeSet::new(),
            json_decoded_ranged_paths: BTreeSet::new(),
            destructured_ranged_paths: BTreeSet::new(),
            member_access: false,
            member_access_handled_kinds: BTreeSet::new(),
            range_key_string_paths: raw_paths.clone(),
        };
        if !effects.helper_fails.contains(&capture) {
            effects.helper_fails.push(capture);
        }
    }
    effects
        .derived_range_key_paths
        .extend(raw_paths.iter().cloned());
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

fn eval_set_call(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = Effects::default();
    let target_result = args
        .first()
        .map(|expr| eval_expr_with_helper_calls(expr, env, resolver))
        .unwrap_or_else(EvalResult::none);
    let target_paths = value_paths(&target_result.value);
    let root_target = matches!(target_result.value, Some(AbstractValue::RootContext));
    effects.merge(target_result.effects);
    let target = match args.first().map(TemplateExpr::deparen) {
        Some(TemplateExpr::Variable(name)) => {
            let name = name.trim_start_matches('$');
            if !name.is_empty() && env.locals.contains_key(name) {
                Some(name.to_string())
            } else {
                None
            }
        }
        _ => None,
    };
    let mut keys = BTreeSet::new();
    if let Some(expr) = args.get(1) {
        let key = eval_expr_with_helper_calls(expr, env, resolver);
        keys = value_strings(&key.value);
        effects.merge(key.effects);
    }
    let assigned_predicate = args
        .get(2)
        .and_then(|expr| root_set_truthy_predicate(expr, env));
    let assigned_value = if let Some(expr) = args.get(2) {
        let result = eval_expr_with_helper_calls(expr, env, resolver);
        let value = result.value.clone().unwrap_or(AbstractValue::Unknown);
        effects.merge(result.effects);
        value
    } else {
        AbstractValue::Unknown
    };
    for target_path in &target_paths {
        for key in &keys {
            let defaulted_path = helm_schema_core::append_value_path(target_path, key);
            if effects.defaults.contains(&defaulted_path) {
                effects.chart_default_paths.insert(defaulted_path);
            }
        }
    }
    let value = target
        .as_ref()
        .and_then(|target| env.locals.get(target))
        .cloned()
        .map(|value| {
            let entries = keys
                .iter()
                .map(|key| (key.clone(), assigned_value.clone()))
                .collect();
            value.with_overlay_entries(entries)
        });
    if let Some(target) = target {
        effects.add_local_set_mutation(target, keys, assigned_value);
    } else if root_target {
        if keys.contains("Values")
            && let Some(assigned) = args.get(2)
            && let Some(source) = root_values_default_source(assigned, env)
        {
            effects.values_default_sources.insert(source);
        }
        for key in keys {
            if let Some(predicate) = &assigned_predicate {
                effects
                    .root_set_predicates
                    .insert(key.clone(), predicate.clone());
            }
            effects
                .root_set_mutations
                .insert(key, assigned_value.clone());
        }
    }
    EvalResult::with_effects(value, effects)
}

fn root_set_truthy_predicate(expr: &TemplateExpr, env: &EvalEnv) -> Option<Predicate> {
    match expr.deparen() {
        TemplateExpr::Literal(literal) => Some(if root_set_literal_is_truthy(literal) {
            Predicate::True
        } else {
            Predicate::False
        }),
        TemplateExpr::Field(path) if path.len() == 1 => {
            env.root_truthy_predicates.get(&path[0]).cloned()
        }
        TemplateExpr::Selector { operand, path }
            if path.len() == 1
                && matches!(operand.as_ref(), TemplateExpr::Variable(variable) if variable.is_empty()) =>
        {
            env.root_truthy_predicates.get(&path[0]).cloned()
        }
        TemplateExpr::Field(_) | TemplateExpr::Selector { .. } => {
            direct_values_path(expr).map(Predicate::truthy_path)
        }
        TemplateExpr::Call { function, args } => match function.as_str() {
            "and" => args
                .iter()
                .map(|arg| root_set_truthy_predicate(arg, env))
                .collect::<Option<Vec<_>>>()
                .map(Predicate::all),
            "or" => args
                .iter()
                .map(|arg| root_set_truthy_predicate(arg, env))
                .collect::<Option<Vec<_>>>()
                .map(root_set_predicate_any),
            "not" => {
                let [arg] = args.as_slice() else {
                    return None;
                };
                root_set_truthy_predicate(arg, env).map(|predicate| predicate.negated())
            }
            "eq" | "ne" => root_set_stringified_comparison(args, function == "ne"),
            _ => None,
        },
        _ => None,
    }
}

fn root_set_stringified_comparison(args: &[TemplateExpr], negated: bool) -> Option<Predicate> {
    let [left, right] = args else {
        return None;
    };
    let (subject, target) = match (
        stringified_values_path(left),
        root_set_string_literal(right),
        stringified_values_path(right),
        root_set_string_literal(left),
    ) {
        (Some(subject), Some(target), None, None) | (None, None, Some(subject), Some(target)) => {
            (subject, target)
        }
        _ => return None,
    };
    let predicate = match target {
        "true" => root_set_predicate_any(vec![
            Predicate::from(Guard::Eq {
                path: subject.clone(),
                value: GuardValue::string("true"),
            }),
            Predicate::from(Guard::Eq {
                path: subject,
                value: GuardValue::Bool(true),
            }),
        ]),
        "false" => root_set_predicate_any(vec![
            Predicate::from(Guard::Eq {
                path: subject.clone(),
                value: GuardValue::string("false"),
            }),
            Predicate::from(Guard::Eq {
                path: subject,
                value: GuardValue::Bool(false),
            }),
        ]),
        target => Predicate::from(Guard::Eq {
            path: subject,
            value: GuardValue::string(target),
        }),
    };
    Some(if negated {
        predicate.negated()
    } else {
        predicate
    })
}

fn stringified_values_path(expr: &TemplateExpr) -> Option<String> {
    match expr.deparen() {
        TemplateExpr::Call { function, args } if function == "toString" => {
            let [subject] = args.as_slice() else {
                return None;
            };
            direct_values_path(subject)
        }
        TemplateExpr::Pipeline(stages) => {
            let [subject, stage] = stages.as_slice() else {
                return None;
            };
            let TemplateExpr::Call { function, args } = stage.deparen() else {
                return None;
            };
            if function != "toString" || !args.is_empty() {
                return None;
            }
            direct_values_path(subject)
        }
        _ => None,
    }
}

fn root_set_string_literal(expr: &TemplateExpr) -> Option<&str> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => Some(value),
        _ => None,
    }
}

fn root_set_literal_is_truthy(literal: &Literal) -> bool {
    match literal {
        Literal::Bool(value) => *value,
        Literal::Int(value) => *value != 0,
        Literal::Float(value) => *value != 0.0,
        Literal::String(value) | Literal::RawString(value) => !value.is_empty(),
        Literal::Nil => false,
    }
}

fn root_set_predicate_any(predicates: Vec<Predicate>) -> Predicate {
    if predicates
        .iter()
        .any(|predicate| matches!(predicate, Predicate::True))
    {
        return Predicate::True;
    }
    let mut predicates = predicates
        .into_iter()
        .filter(|predicate| !matches!(predicate, Predicate::False))
        .collect::<Vec<_>>();
    match predicates.len() {
        0 => Predicate::False,
        1 => predicates.remove(0),
        _ => Predicate::Or(predicates),
    }
}

fn root_values_default_source(
    assigned: &TemplateExpr,
    env: &EvalEnv,
) -> Option<crate::ValuesDefaultSource> {
    let TemplateExpr::Call { function, args } = assigned.deparen() else {
        return None;
    };
    let [first, second] = args.as_slice() else {
        return None;
    };
    let (source, effective) = match function.as_str() {
        "merge" | "mustMerge" => (second, first),
        "mergeOverwrite" | "mustMergeOverwrite" => (first, second),
        _ => return None,
    };
    let effective_path = eval_expr(effective, env).value?.unique_path()?;
    if !effective_path.is_empty() {
        return None;
    }
    let source_path = eval_expr(source, env).value?.unique_path()?;
    if source_path.is_empty() {
        return None;
    }
    Some(crate::ValuesDefaultSource {
        target_path: String::new(),
        source_path,
    })
}

/// `default FALLBACK PRIMARY` and `PRIMARY | default FALLBACK` are one rule:
/// the primary's identity paths become defaulted (typed by a literal
/// fallback), and the value is the choice of primary and fallback values.
fn eval_default(
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
    // itself accepts whatever the render site accepts.
    if let Some(schema_type) = fallback_args
        .first()
        .map(TemplateExpr::deparen)
        .filter(|expr| matches!(expr, TemplateExpr::Literal(_)))
        .and_then(expression_schema_type)
    {
        effects.add_type_hints(primary_paths, schema_type);
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

fn direct_raw_identity_path(value: Option<&AbstractValue>) -> Option<String> {
    match value? {
        AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path) => {
            Some(path.clone())
        }
        _ => None,
    }
}

fn eval_coalesce(
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
    EvalResult::with_effects(AbstractValue::choice(values), effects)
}

fn eval_dict(
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

fn eval_pick(
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

fn eval_list(
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

fn eval_first(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    eval_first_result(eval_expr_with_helper_calls(&args[0], env, resolver))
}

fn eval_first_result(result: EvalResult) -> EvalResult {
    let value = match result.value {
        Some(AbstractValue::List(items)) => items.first().cloned(),
        other => other,
    };
    EvalResult::with_effects(value, result.effects)
}

fn eval_reverse(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    eval_reverse_result(eval_expr_with_helper_calls(&args[0], env, resolver))
}

fn eval_reverse_result(result: EvalResult) -> EvalResult {
    let value = match result.value {
        Some(AbstractValue::List(mut items)) => {
            items.reverse();
            Some(AbstractValue::List(items))
        }
        other => other,
    };
    EvalResult::with_effects(value, result.effects)
}

fn eval_split_list(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let separator = match args[0].deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => value,
        _ => return eval_all_args(args, env, resolver),
    };
    let mut result = eval_expr_with_helper_calls(&args[1], env, resolver);
    // The subject must be a Go string at runtime whatever the split
    // produces: the literal-split fast path below is value refinement on
    // top of that contract, not a replacement for it.
    record_string_consumer_effects(&identity_value_paths(&result.value), &mut result.effects);
    let value = result.value.clone();
    record_range_key_string_consumer_effects(&value, &mut result.effects);
    let Some(strings) = result.value.as_ref().map(AbstractValue::strings) else {
        return EvalResult::with_effects(None, result.effects);
    };
    if strings.is_empty() {
        return EvalResult::with_effects(None, result.effects);
    }

    let split_values = split_string_set(separator, strings);
    EvalResult::with_effects(split_values, result.effects)
}

fn eval_nonempty_split(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let result = eval_all_args(args, env, resolver);
    let mut effects = result.effects;
    record_string_call_consumers("split", args, env, resolver, &mut effects);
    EvalResult::with_effects(nonempty_split_map(result.value.as_ref()), effects)
}

fn eval_nonempty_split_pipeline(
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
    let value = nonempty_split_map(current.value.as_ref());
    let mut effects = current.effects;
    merge_arg_effects(args, env, resolver, &mut effects);
    record_string_consumer_effects(&string_paths, &mut effects);
    record_raw_range_key_string_consumer_paths(&raw_range_key_paths, &mut effects);
    EvalResult::with_effects(value, effects)
}

fn nonempty_split_map(source: Option<&AbstractValue>) -> Option<AbstractValue> {
    let paths = source.map(AbstractValue::paths).unwrap_or_default();
    let first = AbstractValue::widened(paths).unwrap_or(AbstractValue::Unknown);
    Some(AbstractValue::Overlay {
        entries: BTreeMap::from([("_0".to_string(), first)]),
        fallback: Box::new(AbstractValue::Unknown),
    })
}

fn is_nonempty_string_literal(expr: &TemplateExpr) -> bool {
    matches!(
        expr.deparen(),
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value))
            if !value.is_empty()
    )
}

fn split_string_set(separator: &str, strings: BTreeSet<String>) -> Option<AbstractValue> {
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

/// `dig "k1" … "kn" default subject`: walk literal keys through the
/// subject dict, falling back to `default` when a key is MISSING. A key
/// that is present but not a map aborts rendering (sprig type-asserts
/// every step), so the subject and every intermediate key carry a
/// truthy⇒object contract; the dug value itself may be any type.
fn eval_dig(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let (subject_expr, rest) = args.split_last().expect("dig arity checked at dispatch");
    let (default_expr, key_exprs) = rest.split_last().expect("dig arity checked at dispatch");
    let mut keys = Vec::new();
    for key in key_exprs {
        match key.deparen() {
            TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
                keys.push(value.clone());
            }
            _ => return eval_all_args(args, env, resolver),
        }
    }
    let subject = eval_expr_with_helper_calls(subject_expr, env, resolver);
    let default_result = eval_expr_with_helper_calls(default_expr, env, resolver);
    let mut effects = subject.effects;
    effects.merge(default_result.effects);
    for path in identity_value_paths(&subject.value) {
        let mut step = path;
        for prefix_len in 0..keys.len() {
            if prefix_len > 0 {
                step = format!("{step}.{}", keys[prefix_len - 1]);
            }
            let capture = crate::eval_effect::FailCapture {
                conjunction: vec![
                    Predicate::truthy_path(step.clone()),
                    Predicate::from(crate::Guard::TypeIs {
                        path: step.clone(),
                        schema_type: "object".to_string(),
                    })
                    .negated(),
                ],
                approximate_condition_paths: BTreeSet::new(),
                direct_ranged_paths: BTreeSet::new(),
                json_decoded_ranged_paths: BTreeSet::new(),
                destructured_ranged_paths: BTreeSet::new(),
                member_access: false,
                member_access_handled_kinds: BTreeSet::new(),
                range_key_string_paths: BTreeSet::new(),
            };
            if !effects.helper_fails.contains(&capture) {
                effects.helper_fails.push(capture);
            }
        }
    }
    let value = subject.value.and_then(|value| value.apply_to_path(&keys));
    // The dug leaf is a READ of that path whose absence falls back to the
    // literal default: an output path (so required-subject walking and
    // read rows see it) marked defaulted, exactly like `default`.
    for path in identity_value_paths(&value) {
        effects.output_paths.insert(path.clone());
        effects.defaults.insert(path);
    }
    EvalResult::with_effects(value, effects)
}

fn eval_index(
    args: &[TemplateExpr],
    object_host: bool,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let Some(base_expr) = args.first() else {
        return EvalResult::none();
    };
    let base = eval_expr_with_helper_calls(base_expr, env, resolver);
    let mut effects = Effects::default();
    if object_host {
        record_member_host_access(&base, &mut effects);
    }
    effects.merge(base.effects);
    let Some(value) = base.value else {
        return EvalResult::with_effects(None, effects);
    };

    let mut values = vec![value];
    for arg in &args[1..] {
        let arg_result = eval_expr_with_helper_calls(arg, env, resolver);
        effects.merge(arg_result.effects);
        let Some(options) = path_segment_options(arg, arg_result.value.as_ref()) else {
            return EvalResult::with_effects(None, effects);
        };
        let mut next_values = Vec::new();
        for value in &values {
            let base_paths = value.paths();
            for option in &options {
                if let Some(next) = apply_index_segment(value, option) {
                    for next_path in next.paths() {
                        for base_path in &base_paths {
                            if !base_path.is_empty()
                                && helm_schema_core::values_path_is_descendant(
                                    &next_path, base_path,
                                )
                            {
                                effects
                                    .local_output_meta
                                    .entry(next_path.clone())
                                    .or_insert_with(HelperOutputMeta::default)
                                    .suppress_predicate_path(base_path.clone());
                            }
                        }
                    }
                    next_values.push(next);
                }
            }
        }
        values = next_values;
    }

    let value = AbstractValue::choice(values);
    EvalResult::with_effects(value, effects)
}

fn record_member_host_access(operand: &EvalResult, effects: &mut Effects) {
    for path in strict_operand_identity_paths(operand) {
        for mut conjunction in strict_operand_selection_conjunctions(operand, &path) {
            conjunction.push(
                Predicate::from(crate::Guard::TypeIs {
                    path: path.clone(),
                    schema_type: "object".to_string(),
                })
                .negated(),
            );
            let capture = crate::eval_effect::FailCapture {
                conjunction,
                approximate_condition_paths: BTreeSet::new(),
                direct_ranged_paths: BTreeSet::new(),
                json_decoded_ranged_paths: BTreeSet::new(),
                destructured_ranged_paths: BTreeSet::new(),
                member_access: true,
                member_access_handled_kinds: BTreeSet::new(),
                range_key_string_paths: BTreeSet::new(),
            };
            if !effects.helper_fails.contains(&capture) {
                effects.helper_fails.push(capture);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PathSegmentOption {
    segments: Vec<String>,
    integer_index: bool,
}

fn apply_index_segment(value: &AbstractValue, option: &PathSegmentOption) -> Option<AbstractValue> {
    if !option.integer_index {
        return value.apply_to_path(&option.segments);
    }

    match value {
        AbstractValue::List(items) => {
            let index = option.segments.first()?.parse::<usize>().ok()?;
            items.get(index).cloned()
        }
        AbstractValue::Choice(choices) => AbstractValue::choice(
            choices
                .iter()
                .filter_map(|choice| apply_index_segment(choice, option))
                .collect(),
        ),
        _ => value.apply_to_path(&["*".to_string()]),
    }
}

fn eval_append(
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

fn eval_omit(
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
    EvalResult::with_effects(value, base.effects)
}

fn eval_printf(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = Effects::default();
    let mut provenance_paths = BTreeSet::new();
    let mut widened_paths = BTreeSet::new();
    let mut values = Vec::with_capacity(args.len());

    for (index, arg) in args.iter().enumerate() {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        let identity_paths = identity_value_paths(&result.value);
        widened_paths.extend(
            value_paths(&result.value)
                .difference(&identity_paths)
                .cloned(),
        );
        effects.merge(result.effects);
        record_printf_argument_effects(index == 0, &identity_paths, &mut effects);
        provenance_paths.extend(identity_paths);
        values.push(result.value);
    }

    let rendered = literal_printf_format(args).and_then(|format| {
        let arg_strings = values.iter().skip(1).map(value_strings).collect::<Vec<_>>();
        render_printf_string_sets(format, &arg_strings)
    });

    let mut values = Vec::new();
    if let Some(rendered) = rendered {
        values.push(AbstractValue::StringSet(rendered));
    }
    if let Some(paths) = AbstractValue::path_choices(provenance_paths) {
        values.push(paths);
    }
    // Influence stays widened: the format arguments flowed through an unknown
    // call, so they attribute the rendered text without becoming identities.
    if let Some(widened) = AbstractValue::widened(widened_paths) {
        values.push(widened);
    }
    EvalResult::with_effects(AbstractValue::choice(values), effects)
}

/// A total conversion (`quote`/`toString` via `strval`, the numeric casts
/// via `cast.ToXxx`) renders ANY input, so it constrains nothing about it
/// and the sink observes only derived output, never the input shape. Only
/// an earlier string-consuming stage (`b64enc | quote`) keeps its contract
/// on the raw path.
fn record_total_conversion_effects(paths: BTreeSet<String>, effects: &mut Effects) {
    let erasable = paths
        .iter()
        .filter(|path| !effects.string_contract_paths.contains(*path))
        .cloned()
        .collect();
    effects.add_shape_erased_paths(erasable);
    effects.derived_text_paths.extend(paths);
}

/// printf's parameters have different input contracts: the format parameter
/// is a real Go `string` (a non-string format fails template evaluation), so
/// a dynamic format binds a string contract on its raw paths; data parameters
/// render through any verb (Go's fmt embeds mismatches in the output instead
/// of failing), so like `quote` they erase input shape. Every argument
/// becomes derived text for later stages.
fn record_printf_argument_effects(
    is_format: bool,
    identity_paths: &BTreeSet<String>,
    effects: &mut Effects,
) {
    if is_format {
        let raw: BTreeSet<String> = identity_paths
            .iter()
            .filter(|path| !effects.derived_text_paths.contains(*path))
            .cloned()
            .collect();
        effects.string_contract_paths.extend(raw);
    } else {
        let erasable = identity_paths
            .iter()
            .filter(|path| !effects.string_contract_paths.contains(*path))
            .cloned()
            .collect();
        effects.add_shape_erased_paths(erasable);
    }
    effects
        .derived_text_paths
        .extend(identity_paths.iter().cloned());
}

fn eval_print(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = Effects::default();
    let mut rendered: BTreeSet<String> = [String::new()].into_iter().collect();
    for arg in args {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        effects.merge(result.effects);
        let strings = value_strings(&result.value);
        if strings.is_empty() {
            return EvalResult::with_effects(None, effects);
        }
        let mut next = BTreeSet::new();
        for prefix in &rendered {
            for value in &strings {
                next.insert(format!("{prefix}{value}"));
            }
        }
        rendered = next;
    }
    EvalResult::with_effects(Some(AbstractValue::StringSet(rendered)), effects)
}

/// `tpl` renders its first argument as a template against the given context.
/// Statically the rendered output is the template argument's content, so the
/// value transfers from the first argument (literal template text carries no
/// attributable content and is dropped).
fn eval_tpl(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let template = eval_expr_with_helper_calls(&args[0], env, resolver);
    let mut effects = template.effects;
    // The context argument's value AND effects are deliberately discarded:
    // a context like `$` reads the whole values tree, and letting that read
    // reach the call site stamps the context's map shape onto the rendered
    // scalar (grafana's `name: {{ tpl .name $ }}` items were typed as
    // objects this way).
    let _context = eval_expr_with_helper_calls(&args[1], env, resolver);
    let value = if expression_applies_to_yaml(&args[0]) {
        let paths = value_paths(&template.value);
        effects.add_encoded_paths(paths.clone());
        effects.add_shape_erased_paths(paths.clone());
        AbstractValue::widened(paths)
    } else {
        // `tpl` type-asserts its template to a Go string: a raw values
        // subject (`tpl .Values.extraEnv $`, also through a `with`-bound
        // dot) carries the same runtime string contract as any other
        // string-only consumer.
        record_string_consumer_effects(&identity_value_paths(&template.value), &mut effects);
        record_range_key_string_consumer_effects(&template.value, &mut effects);
        template.value
    }
    .and_then(rendered_content_value);
    EvalResult::with_effects(value, effects)
}

fn expression_applies_to_yaml(expr: &TemplateExpr) -> bool {
    match expr.deparen() {
        TemplateExpr::Call { function, .. } => function == "toYaml",
        TemplateExpr::Pipeline(stages) => stages.iter().any(|stage| {
            matches!(stage.deparen(), TemplateExpr::Call { function, .. } if function == "toYaml")
        }),
        _ => false,
    }
}

fn eval_from_yaml(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let result = eval_expr_with_helper_calls(&args[0], env, resolver);
    eval_from_yaml_result(result)
}

fn eval_to_yaml(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let result = eval_expr_with_helper_calls(&args[0], env, resolver);
    eval_to_yaml_result(result)
}

fn eval_to_yaml_result(result: EvalResult) -> EvalResult {
    let paths = serialization_payload_paths(&result.value);
    let mut effects = result.effects;
    effects.yaml_serialized_paths.extend(paths.iter().cloned());
    // The output is rendered YAML text: a later consuming transform
    // (`toYaml x | trim`) operates on that text and claims nothing about
    // the raw value, which serializes at any type.
    effects.derived_text_paths.extend(paths);
    EvalResult::with_effects(result.value, effects)
}

fn eval_from_json(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let result = eval_expr_with_helper_calls(&args[0], env, resolver);
    eval_from_json_result(result)
}

fn eval_to_json(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let result = eval_expr_with_helper_calls(&args[0], env, resolver);
    eval_to_json_result(result)
}

fn eval_to_json_result(result: EvalResult) -> EvalResult {
    let paths = serialization_payload_paths(&result.value);
    let mut effects = result.effects;
    effects.json_serialized_paths.extend(paths.iter().cloned());
    effects.derived_text_paths.extend(paths);
    EvalResult::with_effects(result.value, effects)
}

fn eval_from_json_pipeline(
    current: EvalResult,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut result = eval_from_json_result(current);
    merge_arg_effects(args, env, resolver, &mut result.effects);
    result
}

fn eval_from_json_result(result: EvalResult) -> EvalResult {
    let paths = serialization_payload_paths(&result.value);
    let round_trips_json = result
        .value
        .as_ref()
        .is_some_and(AbstractValue::is_definitely_json_serialized)
        || !paths.is_empty()
            && paths
                .iter()
                .all(|path| path_is_encoded(path, &result.effects.json_serialized_paths));
    let mut effects = result.effects;
    let value = if round_trips_json {
        result
            .value
            .as_ref()
            .and_then(AbstractValue::json_roundtrip_identity)
    } else {
        effects.add_type_hints(paths.clone(), "string");
        effects.string_contract_paths.extend(paths);
        None
    };
    EvalResult::with_effects(value, effects)
}

fn eval_from_yaml_pipeline(
    current: EvalResult,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut result = eval_from_yaml_result(current);
    merge_arg_effects(args, env, resolver, &mut result.effects);
    result
}

fn eval_from_yaml_result(result: EvalResult) -> EvalResult {
    let paths = serialization_payload_paths(&result.value);
    let round_trips_yaml = !paths.is_empty()
        && paths
            .iter()
            .all(|path| path_is_encoded(path, &result.effects.yaml_serialized_paths));
    let mut effects = result.effects;
    let string_input_paths = paths
        .iter()
        .filter(|path| !path_is_encoded(path, &effects.yaml_serialized_paths))
        .cloned()
        .collect::<BTreeSet<_>>();
    effects.add_type_hints(string_input_paths.clone(), "string");
    effects
        .string_contract_paths
        .extend(string_input_paths.iter().cloned());
    effects.parsed_yaml_input_paths.extend(string_input_paths);
    let value = if round_trips_yaml {
        result.value
    } else {
        AbstractValue::widened(paths)
    };
    EvalResult::with_effects(value, effects)
}

fn eval_join(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let separator = eval_expr_with_helper_calls(&args[0], env, resolver);
    let mut result = eval_expr_with_helper_calls(&args[1], env, resolver);
    result.effects.merge(separator.effects);
    erase_join_input_shape(&mut result);
    result
}

fn eval_join_pipeline(
    mut current: EvalResult,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    merge_arg_effects(args, env, resolver, &mut current.effects);
    erase_join_input_shape(&mut current);
    current
}

/// `join` is a total stringification like `toString`: Sprig's `strslice`
/// converts lists element-wise, wraps any other non-nil value as a singleton,
/// and turns nil into an empty slice, so any input type renders. As with
/// `quote`, a path that already passed a string-consuming transform keeps
/// its own contract.
fn erase_join_input_shape(result: &mut EvalResult) {
    let paths = identity_value_paths(&result.value);
    let erasable = paths
        .iter()
        .filter(|path| !result.effects.string_contract_paths.contains(*path))
        .cloned()
        .collect();
    result.effects.add_shape_erased_paths(erasable);
    result.effects.derived_text_paths.extend(paths);
}

/// `cat` joins its arguments into one string, so the output content is the
/// union of the arguments' contents. Literal strings are joined text, not
/// standalone alternatives, and carry no attributable content.
fn eval_cat(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = Effects::default();
    let mut values = Vec::new();
    for arg in args {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        effects.merge(result.effects);
        if let Some(value) = result.value.and_then(rendered_content_value) {
            values.push(value);
        }
    }
    EvalResult::with_effects(AbstractValue::choice(values), effects)
}

/// Content of a value that a string-rendering call (`tpl`, `cat`) passes
/// through: path-attributed and structured members survive, literal text and
/// contexts do not.
fn rendered_content_value(value: AbstractValue) -> Option<AbstractValue> {
    match value {
        AbstractValue::StringSet(_)
        | AbstractValue::Top
        | AbstractValue::Unknown
        | AbstractValue::RootContext => None,
        AbstractValue::Choice(choices) => AbstractValue::choice(
            choices
                .into_iter()
                .filter_map(rendered_content_value)
                .collect(),
        ),
        other => Some(other),
    }
}

fn eval_merge(
    args: &[TemplateExpr],
    piped: EvalResult,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = piped.effects;
    let mut values = piped.value.into_iter().collect::<Vec<_>>();
    merge_arg_values(args, env, resolver, &mut values, &mut effects);
    EvalResult::with_effects(AbstractValue::merge_all(values), effects)
}

/// `ternary A B COND`: the first two arguments are the branch values, the
/// trailing (or piped) condition contributes effects only.
fn eval_ternary(
    args: &[TemplateExpr],
    mut effects: Effects,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut values = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        effects.merge(result.effects);
        if index < 2
            && let Some(value) = result.value
        {
            values.push(value);
        }
    }
    effects.promote_tested_type_hints();
    EvalResult::with_effects(AbstractValue::choice(values), effects)
}

fn eval_type_is(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = Effects::default();
    let mut values = Vec::new();
    for arg in args {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        effects.merge(result.effects);
        values.push(result.value);
    }
    if let Some(schema_type) = type_is_schema_type(args.first()) {
        let paths = values.get(1).map(identity_value_paths).unwrap_or_default();
        effects.add_tested_type_hints(paths, &schema_type);
    }
    EvalResult::with_effects(None, effects)
}

/// Go template `eq`/`ne` terminate on incomparable operand kinds: any
/// composite (map/list) never compares, and a scalar literal fixes the
/// basic kind the other operands must share. The contract is bounded to
/// what a literal proves — nil/missing operands stay unmodeled (Helm
/// charts routinely compare optional values).
fn eval_comparison(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let literal_kind = comparison_literal_kind(args);
    let operands = args
        .iter()
        .map(|arg| eval_expr_with_helper_calls(arg, env, resolver))
        .collect();
    eval_comparison_operands(operands, literal_kind)
}

fn eval_pipeline_comparison(
    current: EvalResult,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let literal_kind = comparison_literal_kind(args);
    let mut operands = Vec::with_capacity(args.len() + 1);
    operands.push(current);
    operands.extend(
        args.iter()
            .map(|arg| eval_expr_with_helper_calls(arg, env, resolver)),
    );
    eval_comparison_operands(operands, literal_kind)
}

fn comparison_literal_kind(args: &[TemplateExpr]) -> Option<&'static str> {
    args.iter().find_map(|arg| match arg.deparen() {
        TemplateExpr::Literal(Literal::String(_) | Literal::RawString(_)) => Some("string"),
        TemplateExpr::Literal(Literal::Bool(_)) => Some("boolean"),
        TemplateExpr::Literal(Literal::Int(_)) => Some("integer"),
        TemplateExpr::Literal(Literal::Float(_)) => Some("number"),
        _ => None,
    })
}

fn eval_comparison_operands(operands: Vec<EvalResult>, literal_kind: Option<&str>) -> EvalResult {
    let mut comparison_effects = Effects::default();
    let Some(literal_kind) = literal_kind else {
        return merge_operand_results(operands, comparison_effects);
    };
    for operand in &operands {
        // Go templates compare only values of the same basic kind, with
        // relaxed exact types inside the integer family. JSON Schema cannot
        // distinguish a Go integer from an integral floating-point value, so
        // the `number` case stays conservatively broad rather than rejecting
        // a valid float such as `1.0`.
        record_strict_kind_result(operand, literal_kind, &mut comparison_effects);
    }
    merge_operand_results(operands, comparison_effects)
}

fn merge_operand_results(operands: Vec<EvalResult>, mut effects: Effects) -> EvalResult {
    let mut values = Vec::new();
    for operand in operands {
        if let Some(value) = operand.value {
            values.push(value);
        }
        effects.merge(operand.effects);
    }
    EvalResult::with_effects(AbstractValue::choice(values), effects)
}

/// Records the runtime operand contract of a strict collection function.
///
/// The call itself does not skip Helm-empty values. Only a `default` or `coalesce` selection
/// makes a raw source conditional on truthiness; structural `if`/`with` guards join later when
/// the effects are absorbed at the execution site.
fn record_strict_kind_operands(
    args: &[TemplateExpr],
    schema_type: &str,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    for arg in args {
        let operand = eval_expr_with_helper_calls(arg, env, resolver);
        record_strict_kind_result(&operand, schema_type, effects);
    }
}

fn record_strict_kind_result(operand: &EvalResult, schema_type: &str, effects: &mut Effects) {
    for path in strict_operand_identity_paths(operand) {
        for mut conjunction in strict_operand_selection_conjunctions(operand, &path) {
            conjunction.push(
                Predicate::from(crate::Guard::TypeIs {
                    path: path.clone(),
                    schema_type: schema_type.to_string(),
                })
                .negated(),
            );
            push_fail_capture(conjunction, effects);
        }
    }
}

fn record_forbidden_kind(
    path: &str,
    schema_type: &str,
    mut conjunction: Vec<Predicate>,
    effects: &mut Effects,
) {
    conjunction.push(Predicate::from(crate::Guard::TypeIs {
        path: path.to_string(),
        schema_type: schema_type.to_string(),
    }));
    push_fail_capture(conjunction, effects);
}

fn push_fail_capture(conjunction: Vec<Predicate>, effects: &mut Effects) {
    let capture = crate::eval_effect::FailCapture {
        conjunction,
        approximate_condition_paths: BTreeSet::new(),
        direct_ranged_paths: BTreeSet::new(),
        json_decoded_ranged_paths: BTreeSet::new(),
        destructured_ranged_paths: BTreeSet::new(),
        member_access: false,
        member_access_handled_kinds: BTreeSet::new(),
        range_key_string_paths: BTreeSet::new(),
    };
    if !effects.helper_fails.contains(&capture) {
        effects.helper_fails.push(capture);
    }
}

fn strict_operand_identity_paths(operand: &EvalResult) -> BTreeSet<String> {
    identity_value_paths(&operand.value)
        .into_iter()
        .filter(|path| {
            !operand.effects.shape_erased_paths.contains(path)
                && !operand.effects.derived_text_paths.contains(path)
                && !operand
                    .effects
                    .local_output_meta
                    .get(path)
                    .is_some_and(|meta| meta.shape_erased || meta.derived_text)
        })
        .collect()
}

fn strict_operand_selection_conjunctions(operand: &EvalResult, path: &str) -> Vec<Vec<Predicate>> {
    operand_selection_conjunctions(&operand.effects, path)
}

fn operand_selection_conjunctions(effects: &Effects, path: &str) -> Vec<Vec<Predicate>> {
    let mut shared = BTreeSet::new();
    if effects.defaults.contains(path) || effects.local_default_paths.contains(path) {
        shared.insert(Predicate::truthy_path(path));
    }
    let Some(meta) = effects.local_output_meta.get(path) else {
        return vec![shared.into_iter().collect()];
    };
    if meta.predicates.is_empty() {
        return vec![shared.into_iter().collect()];
    }
    meta.predicates
        .iter()
        .map(|branch| {
            let mut conjunction = shared.clone();
            conjunction.extend(branch.iter().cloned());
            conjunction.into_iter().collect()
        })
        .collect()
}

/// `len` requires a length-bearing value (string, list, or map): numeric
/// and boolean operands abort rendering outright.
fn record_length_bearing_operand(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    for arg in args {
        let operand = eval_expr_with_helper_calls(arg, env, resolver);
        record_length_bearing_result(&operand, effects);
    }
}

fn record_length_bearing_result(operand: &EvalResult, effects: &mut Effects) {
    for path in strict_operand_identity_paths(operand) {
        for kind in ["boolean", "integer", "number"] {
            for conjunction in strict_operand_selection_conjunctions(operand, &path) {
                record_forbidden_kind(&path, kind, conjunction, effects);
            }
        }
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

fn value_paths(value: &Option<AbstractValue>) -> BTreeSet<String> {
    value.as_ref().map(AbstractValue::paths).unwrap_or_default()
}

fn value_strings(value: &Option<AbstractValue>) -> BTreeSet<String> {
    value
        .as_ref()
        .map(AbstractValue::strings)
        .unwrap_or_default()
}

/// Paths whose value this abstract value may literally be. Widened influence
/// is dataflow through an unknown call, not value identity: defaulting or
/// type-hinting the call result (e.g. `required "..." .Values.x | quote`)
/// says nothing about the type or defaultedness of `.Values.x` itself.
fn identity_value_paths(value: &Option<AbstractValue>) -> BTreeSet<String> {
    fn collect(value: &AbstractValue, paths: &mut BTreeSet<String>) {
        match value {
            AbstractValue::ValuesPath(path)
            | AbstractValue::JsonDecodedPath(path)
            | AbstractValue::OutputPath(path, _) => {
                paths.insert(path.clone());
            }
            AbstractValue::Choice(choices) => {
                for choice in choices {
                    collect(choice, paths);
                }
            }
            AbstractValue::Top
            | AbstractValue::Unknown
            | AbstractValue::RangeKey(_)
            | AbstractValue::RootContext
            | AbstractValue::StringSet(_)
            | AbstractValue::Dict(_)
            | AbstractValue::List(_)
            | AbstractValue::Overlay { .. }
            | AbstractValue::Widened(_) => {}
        }
    }

    let mut paths = BTreeSet::new();
    if let Some(value) = value {
        collect(value, &mut paths);
    }
    paths
}

/// Values identities contained in a payload serialized by `toJson` or
/// `toYaml`.
/// Constructed containers are not themselves aliases of their leaves, so
/// strict consumers use [`identity_value_paths`] and stop at those container
/// boundaries. Serialization is different: every structurally retained leaf
/// is part of the encoded payload and must be marked so a matching decoder
/// can recover its runtime identity.
fn serialization_payload_paths(value: &Option<AbstractValue>) -> BTreeSet<String> {
    fn collect(value: &AbstractValue, paths: &mut BTreeSet<String>) {
        match value {
            AbstractValue::ValuesPath(path)
            | AbstractValue::JsonDecodedPath(path)
            | AbstractValue::OutputPath(path, _) => {
                paths.insert(path.clone());
            }
            AbstractValue::Dict(entries) => {
                for value in entries.values() {
                    collect(value, paths);
                }
            }
            AbstractValue::List(items) => {
                for value in items {
                    collect(value, paths);
                }
            }
            AbstractValue::Overlay { entries, fallback } => {
                for value in entries.values() {
                    collect(value, paths);
                }
                collect(fallback, paths);
            }
            AbstractValue::Choice(choices) => {
                for value in choices {
                    collect(value, paths);
                }
            }
            AbstractValue::Top
            | AbstractValue::Unknown
            | AbstractValue::RangeKey(_)
            | AbstractValue::RootContext
            | AbstractValue::StringSet(_)
            | AbstractValue::Widened(_) => {}
        }
    }

    let mut paths = BTreeSet::new();
    if let Some(value) = value {
        collect(value, &mut paths);
    }
    paths
}

fn identity_range_key_paths(value: &Option<AbstractValue>) -> BTreeSet<String> {
    value
        .as_ref()
        .map(AbstractValue::range_key_paths)
        .unwrap_or_default()
}

fn path_segment_options(
    expr: &TemplateExpr,
    evaluated_value: Option<&AbstractValue>,
) -> Option<Vec<PathSegmentOption>> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
            Some(vec![PathSegmentOption {
                segments: vec![value.clone()],
                integer_index: false,
            }])
        }
        TemplateExpr::Literal(Literal::Int(value)) => Some(vec![PathSegmentOption {
            segments: vec![value.to_string()],
            integer_index: true,
        }]),
        _ => {
            let strings = evaluated_value
                .map(AbstractValue::strings)
                .unwrap_or_default();
            if strings.is_empty() {
                None
            } else {
                let mut options = Vec::new();
                for value in strings {
                    options.push(PathSegmentOption {
                        segments: vec![value.clone()],
                        integer_index: false,
                    });
                    let structural_segments = helm_schema_core::split_value_path(&value);
                    if structural_segments.len() > 1 {
                        options.push(PathSegmentOption {
                            segments: structural_segments,
                            integer_index: false,
                        });
                    }
                }
                options.sort_by(|left, right| left.segments.cmp(&right.segments));
                options.dedup();
                Some(options)
            }
        }
    }
}
