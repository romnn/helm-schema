use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::{AbstractValue, path_is_encoded};
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::helper_meta::HelperOutputMeta;
use helm_schema_ast::expression_schema_type;
use helm_schema_ast::type_is_schema_type;
use helm_schema_core::Predicate;

use helm_schema_ast::{
    is_merge_function, is_provenance_preserving_function, is_string_predicate_function,
    is_string_splitting_function, is_string_transform_function, is_total_numeric_cast_function,
    is_total_stringification_function,
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
        "first" if args.len() == 1 => eval_first(args, env, resolver),
        "reverse" if args.len() == 1 => eval_reverse(args, env, resolver),
        "splitList" if args.len() == 2 => eval_split_list(args, env, resolver),
        "append" => {
            let mut result = eval_append(args, env, resolver);
            if args.len() == 2 {
                record_truthy_kind_operands(
                    &args[..1],
                    "array",
                    env,
                    resolver,
                    &mut result.effects,
                );
            }
            result
        }
        "omit" if !args.is_empty() => eval_omit(args, env, resolver),
        function if is_merge_function(function) => {
            let mut result = eval_merge(args, EvalResult::none(), env, resolver);
            record_truthy_kind_operands(args, "object", env, resolver, &mut result.effects);
            result
        }
        "coalesce" => eval_all_args(args, env, resolver),
        "eq" | "ne" if args.len() >= 2 => eval_comparison(args, env, resolver),
        // These stay on eval_unknown_call's widened-value semantics: their
        // results (a count, a membership bool, a rebuilt list) are dataflow
        // through the call, not the operand's identity, so downstream string
        // consumers must not type the operand through them.
        "concat" => {
            let mut result = eval_unknown_call(args, Effects::default(), env, resolver);
            record_truthy_kind_operands(args, "array", env, resolver, &mut result.effects);
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
            record_truthy_kind_operands(&args[1..], "array", env, resolver, &mut result.effects);
            let subject = eval_expr_with_helper_calls(&args[1], env, resolver);
            record_total_conversion_effects(
                identity_value_paths(&subject.value),
                &mut result.effects,
            );
            result
        }
        "prepend" if args.len() == 2 => {
            let mut result = eval_unknown_call(args, Effects::default(), env, resolver);
            record_truthy_kind_operands(&args[..1], "array", env, resolver, &mut result.effects);
            result
        }
        "ternary" => eval_ternary(args, Effects::default(), env, resolver),
        "print" => eval_print(args, env, resolver),
        "printf" => eval_printf(args, env, resolver),
        "tpl" if args.len() == 2 => eval_tpl(args, env, resolver),
        "cat" => eval_cat(args, env, resolver),
        "index" => eval_index(args, env, resolver),
        "get" if args.len() == 2 => eval_index(args, env, resolver),
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
            record_string_transform_effects(function, &result.value, &mut effects);
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
            let subject = eval_expr_with_helper_calls(&args[args.len() - 1], env, resolver);
            let mut effects = result.effects;
            record_string_consumer_effects(&identity_value_paths(&subject.value), &mut effects);
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
            function if is_merge_function(function) => eval_merge(args, current, env, resolver),
            // The piped ternary operand is the condition: its effects flow,
            // its value does not.
            "ternary" => eval_ternary(args, current.effects, env, resolver),
            function if is_string_transform_function(function) => {
                let mut effects = current.effects;
                record_string_transform_effects(function, &current.value, &mut effects);
                for arg in args {
                    let arg_result = eval_expr_with_helper_calls(arg, env, resolver);
                    if function == "b64enc" {
                        effects.add_encoded_paths(identity_value_paths(&arg_result.value));
                    }
                    effects.merge(arg_result.effects);
                }
                EvalResult::with_effects(current.value, effects)
            }
            "fromYaml" => eval_from_yaml_pipeline(current, args, env, resolver),
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
                let mut effects = current.effects;
                record_string_consumer_effects(&identity_value_paths(&current.value), &mut effects);
                merge_arg_effects(args, env, resolver, &mut effects);
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
    effects: &mut Effects,
) {
    let paths = identity_value_paths(value);
    if is_total_stringification_function(function) {
        // Sprig's `strval` fallback renders ANY input (maps, lists, nil), so
        // a total stringification constrains nothing about its input and the
        // sink observes only the rendered text, never the input shape.
        record_total_conversion_effects(paths, effects);
        return;
    }
    record_string_consumer_effects(&paths, effects);
    effects.derived_text_paths.extend(paths.iter().cloned());
    if function == "b64enc" {
        effects.add_encoded_paths(paths);
    }
}

/// Record that an expression stage consumes the RAW value of `paths` as a
/// Go string, failing rendering otherwise. A path that already passed a
/// converting stage (`printf … | trunc`) or flows out of a shape-erasing
/// local binding reaches the consumer as derived text, so the earlier
/// conversion owns the contract. A path behind a `default` fallback is
/// consumed only when TRUTHY (the fallback replaces it otherwise), so its
/// contract is the conditional `truthy => string` — captured as a
/// fail-class implication instead of an unconditional row contract.
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
        if effects.defaults.contains(path) || effects.local_default_paths.contains(path) {
            let capture = crate::eval_effect::FailCapture {
                conjunction: vec![
                    Predicate::truthy_path(path.clone()),
                    Predicate::from(crate::Guard::TypeIs {
                        path: path.clone(),
                        schema_type: "string".to_string(),
                    })
                    .negated(),
                ],
                approximate_condition_paths: BTreeSet::new(),
                direct_ranged_paths: BTreeSet::new(),
                member_access: false,
            };
            if !effects.helper_fails.contains(&capture) {
                effects.helper_fails.push(capture);
            }
        } else {
            effects.string_contract_paths.insert(path.clone());
            effects.direct_string_consumer_paths.insert(path.clone());
        }
    }
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
    let target_paths = args
        .first()
        .map(|expr| set_target_paths(expr, env, resolver))
        .unwrap_or_default();
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
    }
    EvalResult::with_effects(value, effects)
}

fn set_target_paths(
    expr: &TemplateExpr,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> BTreeSet<String> {
    let deparened = expr.deparen();
    if let TemplateExpr::Variable(name) = deparened
        && !name.is_empty()
    {
        return env
            .locals
            .get(name)
            .or_else(|| env.root_fields.get(name))
            .map(AbstractValue::paths)
            .unwrap_or_default();
    }
    if let TemplateExpr::Selector { operand, path } = deparened
        && let TemplateExpr::Variable(name) = operand.deparen()
        && !name.is_empty()
    {
        return env
            .locals
            .get(name)
            .or_else(|| env.root_fields.get(name))
            .and_then(|value| value.apply_to_path(path))
            .map(|value| value.paths())
            .unwrap_or_default();
    }
    value_paths(&eval_expr_with_helper_calls(expr, env, resolver).value)
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
    merge_arg_values(fallback_args, env, resolver, &mut values, &mut effects);
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
    let result = eval_expr_with_helper_calls(&args[0], env, resolver);
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
    let result = eval_expr_with_helper_calls(&args[0], env, resolver);
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
    let Some(strings) = result.value.as_ref().map(AbstractValue::strings) else {
        return EvalResult::with_effects(None, result.effects);
    };
    if strings.is_empty() {
        return EvalResult::with_effects(None, result.effects);
    }

    let split_values = split_string_set(separator, strings);
    EvalResult::with_effects(split_values.map(AbstractValue::List), result.effects)
}

fn split_string_set(separator: &str, strings: BTreeSet<String>) -> Option<Vec<AbstractValue>> {
    if separator.is_empty() {
        return None;
    }

    let split: Vec<Vec<String>> = strings
        .iter()
        .map(|value| value.split(separator).map(str::to_string).collect())
        .collect();
    let first_len = split.first()?.len();
    if split.iter().all(|parts| parts.len() == first_len) {
        let mut items = Vec::with_capacity(first_len);
        for index in 0..first_len {
            let options = split
                .iter()
                .filter_map(|parts| parts.get(index).cloned())
                .collect();
            items.push(AbstractValue::StringSet(options));
        }
        return Some(items);
    }

    Some(vec![AbstractValue::StringSet(strings)])
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
                member_access: false,
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
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let Some(base_expr) = args.first() else {
        return EvalResult::none();
    };
    let base = eval_expr_with_helper_calls(base_expr, env, resolver);
    let mut effects = base.effects;
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
    let paths = identity_value_paths(&result.value);
    let mut effects = result.effects;
    effects.yaml_serialized_paths.extend(paths.iter().cloned());
    // The output is rendered YAML text: a later consuming transform
    // (`toYaml x | trim`) operates on that text and claims nothing about
    // the raw value, which serializes at any type.
    effects.derived_text_paths.extend(paths);
    EvalResult::with_effects(result.value, effects)
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
    let paths = identity_value_paths(&result.value);
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
    let literal_kinds: Vec<&str> = args
        .iter()
        .filter_map(|arg| match arg.deparen() {
            TemplateExpr::Literal(Literal::String(_) | Literal::RawString(_)) => Some("string"),
            TemplateExpr::Literal(Literal::Bool(_)) => Some("boolean"),
            TemplateExpr::Literal(Literal::Int(_)) => Some("integer"),
            TemplateExpr::Literal(Literal::Float(_)) => Some("number"),
            _ => None,
        })
        .collect();
    let mut result = eval_all_args(args, env, resolver);
    let Some(literal_kind) = literal_kinds.first() else {
        return result;
    };
    // Composites never compare; a mismatched scalar kind fails Go's
    // basicKind check (numeric literals stay permissive across
    // integer/number to keep int-or-string style values unharmed).
    let mut failing_kinds = vec!["object", "array"];
    for scalar in ["string", "boolean", "integer", "number"] {
        let numeric_pair =
            matches!(*literal_kind, "integer" | "number") && matches!(scalar, "integer" | "number");
        if scalar != *literal_kind && !numeric_pair {
            failing_kinds.push(scalar);
        }
    }
    for arg in args {
        if !matches!(
            arg.deparen(),
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
        ) {
            continue;
        }
        let operand = eval_expr_with_helper_calls(arg, env, resolver);
        for path in identity_value_paths(&operand.value) {
            for kind in &failing_kinds {
                let capture = crate::eval_effect::FailCapture {
                    conjunction: vec![Predicate::from(crate::Guard::TypeIs {
                        path: path.clone(),
                        schema_type: (*kind).to_string(),
                    })],
                    approximate_condition_paths: BTreeSet::new(),
                    direct_ranged_paths: BTreeSet::new(),
                    member_access: false,
                };
                if !result.effects.helper_fails.contains(&capture) {
                    result.effects.helper_fails.push(capture);
                }
            }
        }
    }
    result
}

/// Truthy⇒kind operand contract of a strict collection function: the call
/// type-asserts its operand (sprig `merge` subjects are maps, `concat`
/// operands are lists), while falsy values only reach it through guards
/// that skip the call, so they stay accepted.
fn record_truthy_kind_operands(
    args: &[TemplateExpr],
    schema_type: &str,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    for arg in args {
        if !matches!(
            arg.deparen(),
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
        ) {
            continue;
        }
        let operand = eval_expr_with_helper_calls(arg, env, resolver);
        for path in identity_value_paths(&operand.value) {
            let capture = crate::eval_effect::FailCapture {
                conjunction: vec![
                    Predicate::truthy_path(path.clone()),
                    Predicate::from(crate::Guard::TypeIs {
                        path,
                        schema_type: schema_type.to_string(),
                    })
                    .negated(),
                ],
                approximate_condition_paths: BTreeSet::new(),
                direct_ranged_paths: BTreeSet::new(),
                member_access: false,
            };
            if !effects.helper_fails.contains(&capture) {
                effects.helper_fails.push(capture);
            }
        }
    }
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
        if !matches!(
            arg.deparen(),
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
        ) {
            continue;
        }
        let operand = eval_expr_with_helper_calls(arg, env, resolver);
        for path in identity_value_paths(&operand.value) {
            for kind in ["boolean", "integer", "number"] {
                let capture = crate::eval_effect::FailCapture {
                    conjunction: vec![Predicate::from(crate::Guard::TypeIs {
                        path: path.clone(),
                        schema_type: kind.to_string(),
                    })],
                    approximate_condition_paths: BTreeSet::new(),
                    direct_ranged_paths: BTreeSet::new(),
                    member_access: false,
                };
                if !effects.helper_fails.contains(&capture) {
                    effects.helper_fails.push(capture);
                }
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
    value
        .clone()
        .and_then(AbstractValue::without_widened)
        .map(|value| value.paths())
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
