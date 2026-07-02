use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::helper_summary::HelperOutputMeta;
use helm_schema_ast::expression_schema_type;
use helm_schema_ast::{
    is_merge_function, is_provenance_preserving_function, is_string_transform_function,
    type_is_schema_type,
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
        "append" => eval_append(args, env, resolver),
        "omit" if !args.is_empty() => eval_omit(args, env, resolver),
        function if is_merge_function(function) => {
            eval_merge(args, EvalResult::none(), env, resolver)
        }
        "coalesce" => eval_all_args(args, env, resolver),
        "ternary" => eval_ternary(args, Effects::default(), env, resolver),
        "print" => eval_print(args, env, resolver),
        "printf" => eval_printf(args, env, resolver),
        "tpl" if args.len() == 2 => eval_tpl(args, env, resolver),
        "cat" => eval_cat(args, env, resolver),
        "index" => eval_index(args, env, resolver),
        "typeIs" if args.len() >= 2 => eval_type_is(args, env, resolver),
        function if is_string_transform_function(function) => {
            let result = eval_all_args(args, env, resolver);
            let mut effects = result.effects;
            record_string_transform_effects(function, &result.value, &mut effects);
            EvalResult::with_effects(result.value, effects)
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
    effects.add_string_hints(paths.clone());
    if function == "b64enc" {
        effects.add_encoded_paths(paths);
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
            let defaulted_path = if target_path.is_empty() {
                key.clone()
            } else {
                format!("{target_path}.{key}")
            };
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
    if let Some(schema_type) = fallback_args.first().and_then(expression_schema_type) {
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
    let result = eval_expr_with_helper_calls(&args[1], env, resolver);
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
    segment: String,
    integer_index: bool,
}

fn apply_index_segment(value: &AbstractValue, option: &PathSegmentOption) -> Option<AbstractValue> {
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

    for arg in args {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        let identity_paths = identity_value_paths(&result.value);
        widened_paths.extend(
            value_paths(&result.value)
                .difference(&identity_paths)
                .cloned(),
        );
        provenance_paths.extend(identity_paths);
        effects.merge(result.effects);
        values.push(result.value);
    }

    let rendered = literal_printf_format(args).and_then(|format| {
        let arg_strings = values.iter().skip(1).map(value_strings).collect::<Vec<_>>();
        render_printf_string_sets(format, &arg_strings)
    });

    effects.add_string_hints(provenance_paths.clone());
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
    effects.merge(eval_expr_with_helper_calls(&args[1], env, resolver).effects);
    let value = template.value.and_then(rendered_content_value);
    EvalResult::with_effects(value, effects)
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
    EvalResult::with_effects(AbstractValue::choice(values), effects)
}

fn eval_type_is(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut values = Vec::new();
    let mut effects = Effects::default();
    for arg in args {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        effects.merge(result.effects);
        values.push(result.value);
    }
    if let Some(schema_type) = type_is_schema_type(args.first()) {
        let paths = values.get(1).map(identity_value_paths).unwrap_or_default();
        effects.add_type_hints(paths, &schema_type);
    }
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
