use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, eval_expr_value};
use crate::expr_function_catalog::{
    is_provenance_preserving_function, is_string_transform_function, type_is_schema_type,
};
use crate::printf_eval::{literal_printf_format, render_printf_string_sets};
use crate::template_expr_analysis::is_merge_function;

pub(crate) fn eval_call(function: &str, args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    match function {
        "set" if args.len() == 3 => eval_set_call(args, env),
        "default" if args.len() == 2 => {
            let fallback = eval_expr(&args[0], env);
            let primary = eval_expr(&args[1], env);
            let primary_paths = primary
                .value
                .as_ref()
                .map(AbstractValue::paths)
                .unwrap_or_default();
            let mut effects = fallback.effects;
            effects.merge(primary.effects);
            effects.add_default_paths(primary_paths.clone());
            EvalResult::with_effects(
                AbstractValue::choice(
                    [primary.value, fallback.value]
                        .into_iter()
                        .flatten()
                        .collect(),
                ),
                effects,
            )
        }
        "dict" => {
            let mut map = BTreeMap::new();
            let mut effects = Effects::default();
            let mut index = 0usize;
            while index + 1 < args.len() {
                let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                    &args[index]
                else {
                    index += 1;
                    continue;
                };
                let value = eval_expr(&args[index + 1], env);
                effects.merge(value.effects);
                map.insert(key.clone(), value.value.unwrap_or(AbstractValue::Unknown));
                index += 2;
            }
            EvalResult::with_effects(Some(AbstractValue::Dict(map)), effects)
        }
        "list" | "tuple" => {
            let mut items = Vec::new();
            let mut effects = Effects::default();
            for arg in args {
                let item = eval_expr(arg, env);
                effects.merge(item.effects);
                items.push(item.value.unwrap_or(AbstractValue::Unknown));
            }
            EvalResult::with_effects(Some(AbstractValue::List(items)), effects)
        }
        "first" if args.len() == 1 => eval_first(args, env),
        "reverse" if args.len() == 1 => eval_reverse(args, env),
        "splitList" if args.len() == 2 => eval_split_list(args, env),
        "append" => eval_append(args, env),
        "omit" if !args.is_empty() => eval_omit(args, env),
        function if is_merge_function(function) => {
            let mut values = Vec::new();
            let mut effects = Effects::default();
            for arg in args {
                let value = eval_expr(arg, env);
                effects.merge(value.effects);
                if let Some(value) = value.value {
                    values.push(value);
                }
            }
            EvalResult::with_effects(AbstractValue::merge_all(values), effects)
        }
        "coalesce" => {
            let mut values = Vec::new();
            let mut effects = Effects::default();
            for arg in args {
                let value = eval_expr(arg, env);
                effects.merge(value.effects);
                if let Some(value) = value.value {
                    values.push(value);
                }
            }
            EvalResult::with_effects(AbstractValue::choice(values), effects)
        }
        "ternary" => {
            let mut values = Vec::new();
            let mut effects = Effects::default();
            for arg in args {
                let value = eval_expr(arg, env);
                effects.merge(value.effects);
            }
            for arg in args.iter().take(2) {
                if let Some(value) = eval_expr_value(arg, env) {
                    values.push(value);
                }
            }
            EvalResult::with_effects(AbstractValue::choice(values), effects)
        }
        "printf" => eval_printf(args, env),
        "index" => eval_index(args, env),
        "typeIs" if args.len() >= 2 => {
            let mut result = eval_all_args(args, env);
            if let Some(schema_type) = type_is_schema_type(args.first()) {
                let paths = eval_expr(&args[1], env)
                    .value
                    .as_ref()
                    .map(AbstractValue::paths)
                    .unwrap_or_default();
                result.effects.add_type_hints(paths, &schema_type);
            }
            EvalResult::with_effects(None, result.effects)
        }
        function if is_string_transform_function(function) => {
            let result = eval_all_args(args, env);
            let mut effects = result.effects;
            effects.add_string_hints(value_paths(&result.value));
            EvalResult::with_effects(result.value, effects)
        }
        function if is_provenance_preserving_function(function) => eval_all_args(args, env),
        _ => {
            let mut effects = Effects::default();
            for arg in args {
                effects.merge(eval_expr(arg, env).effects);
            }
            EvalResult::with_effects(None, effects)
        }
    }
}

fn eval_set_call(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let mut effects = Effects::default();
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
        let key = eval_expr(expr, env);
        keys = key
            .value
            .as_ref()
            .map(AbstractValue::strings)
            .unwrap_or_default();
        effects.merge(key.effects);
    }
    let assigned_value = if let Some(expr) = args.get(2) {
        let result = eval_expr(expr, env);
        let value = result.value.clone().unwrap_or(AbstractValue::Unknown);
        effects.merge(result.effects);
        value
    } else {
        AbstractValue::Unknown
    };
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

fn eval_first(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let result = eval_expr(&args[0], env);
    let value = match result.value {
        Some(AbstractValue::List(items)) => items.first().cloned(),
        other => other,
    };
    EvalResult::with_effects(value, result.effects)
}

fn eval_reverse(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let result = eval_expr(&args[0], env);
    let value = match result.value {
        Some(AbstractValue::List(mut items)) => {
            items.reverse();
            Some(AbstractValue::List(items))
        }
        other => other,
    };
    EvalResult::with_effects(value, result.effects)
}

fn eval_split_list(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let separator = match args[0].deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => value,
        _ => return eval_all_args(args, env),
    };
    let result = eval_expr(&args[1], env);
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

fn eval_index(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let Some(base_expr) = args.first() else {
        return EvalResult::none();
    };
    let base = eval_expr(base_expr, env);
    let mut effects = base.effects;
    let Some(value) = base.value else {
        return EvalResult::with_effects(None, effects);
    };

    let mut values = vec![value];
    for arg in &args[1..] {
        let arg_result = eval_expr(arg, env);
        effects.merge(arg_result.effects);
        let Some(options) = path_segment_options(arg, arg_result.value.as_ref()) else {
            return EvalResult::with_effects(None, effects);
        };
        let mut next_values = Vec::new();
        for value in &values {
            for option in &options {
                if let Some(next) = apply_index_segment(value, option) {
                    next_values.push(next);
                }
            }
        }
        values = next_values;
    }

    let value = AbstractValue::choice(values);
    if let Some(value) = &value {
        effects.reads.extend(value.paths());
    }
    EvalResult::with_effects(value, effects)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PathSegmentOption {
    segment: String,
    integer_index: bool,
}

pub(crate) fn apply_index_segment(
    value: &AbstractValue,
    option: &PathSegmentOption,
) -> Option<AbstractValue> {
    if !option.integer_index {
        return value.apply_to_path(std::slice::from_ref(&option.segment));
    }

    match value {
        AbstractValue::List(items) => {
            let index = option.segment.parse::<usize>().ok()?;
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

fn eval_append(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let mut effects = Effects::default();
    let mut items = match args.first().map(|expr| eval_expr(expr, env)) {
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
    for arg in &args[1..] {
        let result = eval_expr(arg, env);
        effects.merge(result.effects);
        if let Some(value) = result.value {
            items.push(value);
        }
    }
    EvalResult::with_effects(Some(AbstractValue::List(items)), effects)
}

fn eval_omit(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let mut base = eval_expr(&args[0], env);
    let mut keys = BTreeSet::new();
    for arg in &args[1..] {
        let key = eval_expr(arg, env);
        keys.extend(
            key.value
                .as_ref()
                .map(AbstractValue::strings)
                .unwrap_or_default(),
        );
        base.effects.merge(key.effects);
    }
    let value = base.value.map(|value| value.omit_keys(&keys));
    EvalResult::with_effects(value, base.effects)
}

fn eval_printf(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let mut effects = Effects::default();
    let mut provenance_paths = BTreeSet::new();
    let mut values = Vec::with_capacity(args.len());

    for arg in args {
        let result = eval_expr(arg, env);
        provenance_paths.extend(value_paths(&result.value));
        effects.merge(result.effects);
        values.push(result.value);
    }

    let rendered = literal_printf_format(args).and_then(|format| {
        let arg_strings = values
            .iter()
            .skip(1)
            .map(|value| {
                value
                    .as_ref()
                    .map(AbstractValue::strings)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        render_printf_string_sets(format, &arg_strings)
    });

    effects.add_string_hints(provenance_paths.clone());
    let mut values = Vec::new();
    if let Some(rendered) = rendered {
        values.push(AbstractValue::StringSet(rendered));
    }
    if !provenance_paths.is_empty() {
        values.push(AbstractValue::PathSet(provenance_paths));
    }
    EvalResult::with_effects(AbstractValue::choice(values), effects)
}

fn eval_all_args(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let mut values = Vec::new();
    let mut effects = Effects::default();
    for arg in args {
        let result = eval_expr(arg, env);
        effects.merge(result.effects);
        if let Some(value) = result.value {
            values.push(value);
        }
    }
    EvalResult::with_effects(AbstractValue::choice(values), effects)
}

pub(crate) fn value_paths(value: &Option<AbstractValue>) -> BTreeSet<String> {
    value.as_ref().map(AbstractValue::paths).unwrap_or_default()
}

pub(crate) fn path_segment_options(
    expr: &TemplateExpr,
    evaluated_value: Option<&AbstractValue>,
) -> Option<Vec<PathSegmentOption>> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
            Some(vec![PathSegmentOption {
                segment: value.clone(),
                integer_index: false,
            }])
        }
        TemplateExpr::Literal(Literal::Int(value)) => Some(vec![PathSegmentOption {
            segment: value.to_string(),
            integer_index: true,
        }]),
        _ => {
            let strings = evaluated_value
                .map(AbstractValue::strings)
                .unwrap_or_default();
            if strings.is_empty() {
                None
            } else {
                Some(
                    strings
                        .into_iter()
                        .map(|segment| PathSegmentOption {
                            segment,
                            integer_index: false,
                        })
                        .collect(),
                )
            }
        }
    }
}
