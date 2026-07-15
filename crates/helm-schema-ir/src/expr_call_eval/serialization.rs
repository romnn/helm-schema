use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::{AbstractValue, path_is_encoded};
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};

use super::merge_arg_effects;
use super::strict_operands::{
    record_range_key_string_consumer_effects, record_string_consumer_effects,
};
use super::value_facts::{
    identity_value_paths, serialization_payload_paths, value_paths, value_strings,
};
use helm_schema_ast::{literal_printf_format, render_printf_string_sets};

pub(super) fn eval_printf(
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
pub(super) fn record_total_conversion_effects(paths: BTreeSet<String>, effects: &mut Effects) {
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
pub(super) fn record_printf_argument_effects(
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

pub(super) fn eval_print(
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
pub(super) fn eval_tpl(
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

pub(super) fn expression_applies_to_yaml(expr: &TemplateExpr) -> bool {
    match expr.deparen() {
        TemplateExpr::Call { function, .. } => function == "toYaml",
        TemplateExpr::Pipeline(stages) => stages.iter().any(|stage| {
            matches!(stage.deparen(), TemplateExpr::Call { function, .. } if function == "toYaml")
        }),
        _ => false,
    }
}

pub(super) fn eval_from_yaml(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let result = eval_expr_with_helper_calls(&args[0], env, resolver);
    eval_from_yaml_result(result)
}

pub(super) fn eval_to_yaml(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let result = eval_expr_with_helper_calls(&args[0], env, resolver);
    eval_to_yaml_result(result)
}

pub(super) fn eval_to_yaml_result(result: EvalResult) -> EvalResult {
    let paths = serialization_payload_paths(&result.value);
    let mut effects = result.effects;
    effects.yaml_serialized_paths.extend(paths.iter().cloned());
    // The output is rendered YAML text: a later consuming transform
    // (`toYaml x | trim`) operates on that text and claims nothing about
    // the raw value, which serializes at any type.
    effects.derived_text_paths.extend(paths);
    EvalResult::with_effects(result.value, effects)
}

pub(super) fn eval_from_json(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let result = eval_expr_with_helper_calls(&args[0], env, resolver);
    eval_from_json_result(result)
}

pub(super) fn eval_to_json(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let result = eval_expr_with_helper_calls(&args[0], env, resolver);
    eval_to_json_result(result)
}

pub(super) fn eval_to_json_result(result: EvalResult) -> EvalResult {
    let paths = serialization_payload_paths(&result.value);
    let mut effects = result.effects;
    effects.json_serialized_paths.extend(paths.iter().cloned());
    effects.derived_text_paths.extend(paths);
    EvalResult::with_effects(result.value, effects)
}

pub(super) fn eval_from_json_pipeline(
    current: EvalResult,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut result = eval_from_json_result(current);
    merge_arg_effects(args, env, resolver, &mut result.effects);
    result
}

pub(super) fn eval_from_json_result(result: EvalResult) -> EvalResult {
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

pub(super) fn eval_from_yaml_pipeline(
    current: EvalResult,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut result = eval_from_yaml_result(current);
    merge_arg_effects(args, env, resolver, &mut result.effects);
    result
}

pub(super) fn eval_from_yaml_result(result: EvalResult) -> EvalResult {
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

pub(super) fn eval_join(
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

pub(super) fn eval_join_pipeline(
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
pub(super) fn erase_join_input_shape(result: &mut EvalResult) {
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
pub(super) fn eval_cat(
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
pub(super) fn rendered_content_value(value: AbstractValue) -> Option<AbstractValue> {
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
