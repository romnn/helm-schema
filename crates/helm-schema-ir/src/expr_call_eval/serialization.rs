use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::{AbstractValue, path_is_encoded};
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};

use super::strict_operands::{
    record_range_key_string_consumer_effects, record_string_consumer_effects,
};
use super::value_facts::{
    identity_value_paths, serialization_payload_paths, value_paths, value_strings,
};
use super::{eval_all_args, merge_arg_effects};
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
        let identity_paths = identity_value_paths(result.value.as_ref());
        widened_paths.extend(
            value_paths(result.value.as_ref())
                .difference(&identity_paths)
                .cloned(),
        );
        effects.merge(result.effects);
        record_printf_argument_effects(index == 0, &identity_paths, &mut effects);
        provenance_paths.extend(identity_paths);
        values.push(result.value);
    }

    let rendered = literal_printf_format(args).and_then(|format| {
        let arg_strings = values
            .iter()
            .skip(1)
            .map(|value| value_strings(value.as_ref()))
            .collect::<Vec<_>>();
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
    effects.add_shape_erased_paths(paths.clone());
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
        effects.add_shape_erased_paths(identity_paths.clone());
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
        let strings = value_strings(result.value.as_ref());
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

pub(super) fn eval_replace(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let [old, new, subject] = args else {
        return eval_all_args(args, env, resolver);
    };
    let old = eval_expr_with_helper_calls(old, env, resolver);
    let new = eval_expr_with_helper_calls(new, env, resolver);
    let mut subject = eval_expr_with_helper_calls(subject, env, resolver);
    let subject_effects = subject.effects.clone();
    subject.effects.merge(old.effects);
    subject.effects.merge(new.effects);
    let mut effects = subject.effects;
    let old_values = value_strings(old.value.as_ref());
    let new_values = value_strings(new.value.as_ref());
    let (string_paths, raw_range_key_paths) =
        super::strict_operands::string_call_operand_facts("replace", args, env, resolver);
    // A single nonempty literal OLD keeps a raw-identity subject's path
    // qualified by OLD as a lexical escape instead of degrading it
    // to derived text: the subject still must be a Go string, but its raw
    // value IS the output for strings not containing OLD.
    if let Some(old_token) = single_replace_token(&old_values)
        && let Some(value) = subject.value.as_ref().and_then(|value| {
            super::value_facts::replace_transformed_value(
                value,
                &subject_effects,
                old_token,
                &new_values,
            )
        })
    {
        super::strict_operands::record_string_consumer_effects(&string_paths, &mut effects);
        super::strict_operands::record_raw_range_key_string_consumer_paths(
            &raw_range_key_paths,
            &mut effects,
        );
        return EvalResult::with_effects(Some(value), effects);
    }
    let subject_values = value_strings(subject.value.as_ref());
    let value = if old_values.is_empty() || new_values.is_empty() || subject_values.is_empty() {
        super::value_facts::derive_value_text(subject.value)
    } else {
        let mut rendered = BTreeSet::new();
        for subject in subject_values {
            for old in &old_values {
                for new in &new_values {
                    rendered.insert(subject.replace(old, new));
                }
            }
        }
        Some(AbstractValue::StringSet(rendered))
    };
    super::strict_operands::record_string_transform_effects(
        "replace",
        value.as_ref(),
        &string_paths,
        &raw_range_key_paths,
        &mut effects,
    );
    EvalResult::with_effects(value, effects)
}

pub(super) fn eval_replace_pipeline(
    current: EvalResult,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let [old, new] = args else {
        let mut current = current;
        merge_arg_effects(args, env, resolver, &mut current.effects);
        return current;
    };
    let piped_value = current.value;
    let piped_effects = current.effects.clone();
    let old = eval_expr_with_helper_calls(old, env, resolver);
    let new = eval_expr_with_helper_calls(new, env, resolver);
    let mut effects = current.effects;
    effects.merge(old.effects);
    effects.merge(new.effects);
    let old_values = value_strings(old.value.as_ref());
    let new_values = value_strings(new.value.as_ref());
    let (string_paths, raw_range_key_paths) = super::strict_operands::pipeline_string_operand_facts(
        "replace",
        args,
        piped_value.as_ref(),
        &piped_effects,
        env,
        resolver,
    );
    // Same lexical-escape rule as the direct call.
    if let Some(old_token) = single_replace_token(&old_values)
        && let Some(value) = piped_value.as_ref().and_then(|value| {
            super::value_facts::replace_transformed_value(
                value,
                &piped_effects,
                old_token,
                &new_values,
            )
        })
    {
        super::strict_operands::record_string_consumer_effects(&string_paths, &mut effects);
        super::strict_operands::record_raw_range_key_string_consumer_paths(
            &raw_range_key_paths,
            &mut effects,
        );
        return EvalResult::with_effects(Some(value), effects);
    }
    let subject_values = piped_value
        .as_ref()
        .map(AbstractValue::strings)
        .unwrap_or_default();
    let value = if old_values.is_empty() || new_values.is_empty() || subject_values.is_empty() {
        super::value_facts::derive_value_text(piped_value)
    } else {
        let mut rendered = BTreeSet::new();
        for subject in subject_values {
            for old in &old_values {
                for new in &new_values {
                    rendered.insert(subject.replace(old, new));
                }
            }
        }
        Some(AbstractValue::StringSet(rendered))
    };
    super::strict_operands::record_string_transform_effects(
        "replace",
        value.as_ref(),
        &string_paths,
        &raw_range_key_paths,
        &mut effects,
    );
    EvalResult::with_effects(value, effects)
}

/// The single nonempty OLD literal of a `replace` call, when static.
fn single_replace_token(old_values: &BTreeSet<String>) -> Option<&String> {
    let mut old_values = old_values.iter();
    let (Some(token), None) = (old_values.next(), old_values.next()) else {
        return None;
    };
    (!token.is_empty()).then_some(token)
}

pub(super) fn eval_repeat(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let [count, subject] = args else {
        return eval_all_args(args, env, resolver);
    };
    let count = eval_expr_with_helper_calls(count, env, resolver);
    let mut subject = eval_expr_with_helper_calls(subject, env, resolver);
    subject.effects.merge(count.effects);
    let count = count
        .value
        .as_ref()
        .and_then(super::value_facts::concrete_integer);
    let subject_values = value_strings(subject.value.as_ref());
    let Some(count) = count.filter(|count| (0..=4096).contains(count)) else {
        return subject;
    };
    let Ok(count) = usize::try_from(count) else {
        return subject;
    };
    if subject_values.is_empty() {
        return subject;
    }
    let rendered = subject_values
        .into_iter()
        .map(|value| value.repeat(count))
        .collect();
    EvalResult::with_effects(Some(AbstractValue::StringSet(rendered)), subject.effects)
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
    let [template_expr, context_expr] = args else {
        return eval_all_args(args, env, resolver);
    };
    let template = eval_expr_with_helper_calls(template_expr, env, resolver);
    let mut effects = template.effects;
    // The context argument's value AND effects are deliberately discarded:
    // a context like `$` reads the whole values tree, and letting that read
    // reach the call site stamps the context's map shape onto the rendered
    // scalar (grafana's `name: {{ tpl .name $ }}` items were typed as
    // objects this way).
    let _context = eval_expr_with_helper_calls(context_expr, env, resolver);
    let value = if expression_applies_to_yaml(template_expr) {
        // `tpl` re-renders the serialized YAML text: template-free content
        // round-trips unchanged and templated scalar leaves stay scalars,
        // so the serialized placement identity carries through to the sink
        // instead of degrading to opaque text (cloudnative-pg's
        // `tpl (.Values.additionalEnv | toYaml) .` env fragment and
        // airflow's `tpl (toYaml .Values.scheduler.command) .`).
        template.value
    } else {
        // `tpl` type-asserts its template to a Go string: a raw values
        // subject (`tpl .Values.extraEnv $`, also through a `with`-bound
        // dot) carries the same runtime string contract as any other
        // string-only consumer.
        let subject_paths = identity_value_paths(template.value.as_ref());
        record_string_consumer_effects(&subject_paths, &mut effects);
        record_range_key_string_consumer_effects(template.value.as_ref(), &mut effects);
        // The rendered result is DERIVED TEXT: the raw argument is a Go
        // template PROGRAM, and constraints observed on the evaluated
        // output (a regex, an enum, a length) apply to the render, never
        // to the program source. redis-ha's `masterGroupName` is matched
        // against a regex only after `tpl`, so its raw value stays a
        // free template string.
        effects.derived_text_paths.extend(subject_paths);
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
    let Some(arg) = args.first() else {
        return eval_all_args(args, env, resolver);
    };
    eval_from_yaml_result(eval_expr_with_helper_calls(arg, env, resolver))
}

pub(super) fn eval_to_yaml(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let Some(arg) = args.first() else {
        return eval_all_args(args, env, resolver);
    };
    eval_to_yaml_result(eval_expr_with_helper_calls(arg, env, resolver))
}

pub(super) fn eval_to_yaml_result(result: EvalResult) -> EvalResult {
    let paths = serialization_payload_paths(result.value.as_ref());
    let mut effects = result.effects;
    if !result
        .value
        .as_ref()
        .is_some_and(is_structurally_rendered_yaml_value)
    {
        effects.yaml_serialized_paths.extend(paths.iter().cloned());
    }
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
    let Some(arg) = args.first() else {
        return eval_all_args(args, env, resolver);
    };
    eval_from_json_result(eval_expr_with_helper_calls(arg, env, resolver))
}

pub(super) fn eval_to_json(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let Some(arg) = args.first() else {
        return eval_all_args(args, env, resolver);
    };
    eval_to_json_result(eval_expr_with_helper_calls(arg, env, resolver))
}

pub(super) fn eval_to_json_result(result: EvalResult) -> EvalResult {
    let paths = serialization_payload_paths(result.value.as_ref());
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
    if let Some(folded) = literal_decoded_value(result.value.as_ref(), DecodeFormat::Json) {
        return EvalResult::with_effects(Some(folded), result.effects);
    }
    let paths = serialization_payload_paths(result.value.as_ref());
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

#[derive(Clone, Copy)]
enum DecodeFormat {
    Yaml,
    Json,
}

/// Constant-fold a literal serialized document into its typed abstract
/// value: chart-authored YAML/JSON tables (grafana's `fromYaml` over a
/// backtick literal of sensitive paths) become concrete dicts and lists
/// whose leaves are exact strings, so membership probes and comparisons
/// over them stay structural. Non-string scalars have no literal lattice
/// value and become `Unknown` members (present, untyped); undecodable
/// documents abstain to the widened result.
fn literal_decoded_value(
    value: Option<&AbstractValue>,
    format: DecodeFormat,
) -> Option<AbstractValue> {
    let AbstractValue::StringSet(strings) = value? else {
        return None;
    };
    let mut strings = strings.iter();
    let (Some(text), None) = (strings.next(), strings.next()) else {
        return None;
    };
    match format {
        DecodeFormat::Yaml => {
            abstract_value_from_yaml(&serde_yaml::from_str::<serde_yaml::Value>(text).ok()?)
        }
        DecodeFormat::Json => {
            abstract_value_from_json(&serde_json::from_str::<serde_json::Value>(text).ok()?)
        }
    }
}

fn abstract_value_from_yaml(node: &serde_yaml::Value) -> Option<AbstractValue> {
    match node {
        serde_yaml::Value::Mapping(entries) => {
            let mut members = std::collections::BTreeMap::new();
            for (key, value) in entries {
                let serde_yaml::Value::String(key) = key else {
                    return None;
                };
                members.insert(key.clone(), abstract_value_from_yaml(value)?);
            }
            Some(AbstractValue::Dict(members))
        }
        serde_yaml::Value::Sequence(items) => Some(AbstractValue::List(
            items
                .iter()
                .map(abstract_value_from_yaml)
                .collect::<Option<Vec<_>>>()?,
        )),
        serde_yaml::Value::String(text) => Some(AbstractValue::StringSet(
            [text.clone()].into_iter().collect(),
        )),
        serde_yaml::Value::Bool(_) | serde_yaml::Value::Number(_) | serde_yaml::Value::Null => {
            Some(AbstractValue::Unknown)
        }
        serde_yaml::Value::Tagged(_) => None,
    }
}

fn abstract_value_from_json(node: &serde_json::Value) -> Option<AbstractValue> {
    match node {
        serde_json::Value::Object(entries) => {
            let mut members = std::collections::BTreeMap::new();
            for (key, value) in entries {
                members.insert(key.clone(), abstract_value_from_json(value)?);
            }
            Some(AbstractValue::Dict(members))
        }
        serde_json::Value::Array(items) => Some(AbstractValue::List(
            items
                .iter()
                .map(abstract_value_from_json)
                .collect::<Option<Vec<_>>>()?,
        )),
        serde_json::Value::String(text) => Some(AbstractValue::StringSet(
            [text.clone()].into_iter().collect(),
        )),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) | serde_json::Value::Null => {
            Some(AbstractValue::Unknown)
        }
    }
}

pub(super) fn eval_from_yaml_result(result: EvalResult) -> EvalResult {
    if let Some(folded) = literal_decoded_value(result.value.as_ref(), DecodeFormat::Yaml) {
        return EvalResult::with_effects(Some(folded), result.effects);
    }
    let paths = serialization_payload_paths(result.value.as_ref());
    let structurally_rendered_yaml = result
        .value
        .as_ref()
        .is_some_and(is_structurally_rendered_yaml_value)
        && !paths.is_empty()
        && paths
            .iter()
            .all(|path| result.effects.derived_text_paths.contains(path));
    let round_trips_yaml = structurally_rendered_yaml
        || !paths.is_empty()
            && paths
                .iter()
                .all(|path| path_is_encoded(path, &result.effects.yaml_serialized_paths));
    let mut effects = result.effects;
    let string_input_paths = if round_trips_yaml {
        BTreeSet::new()
    } else {
        paths
            .iter()
            .filter(|path| !path_is_encoded(path, &effects.yaml_serialized_paths))
            .cloned()
            .collect::<BTreeSet<_>>()
    };
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

fn is_structurally_rendered_yaml_value(value: &AbstractValue) -> bool {
    match value {
        AbstractValue::Dict(_) | AbstractValue::List(_) | AbstractValue::Overlay { .. } => true,
        AbstractValue::Choice(choices) => {
            !choices.is_empty() && choices.iter().all(is_structurally_rendered_yaml_value)
        }
        AbstractValue::FirstTruthy(candidates) => {
            !candidates.is_empty() && candidates.iter().all(is_structurally_rendered_yaml_value)
        }
        _ => false,
    }
}

pub(super) fn eval_join(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let [separator, subject] = args else {
        return eval_all_args(args, env, resolver);
    };
    let separator = eval_expr_with_helper_calls(separator, env, resolver);
    let mut result = eval_expr_with_helper_calls(subject, env, resolver);
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
    let paths = identity_value_paths(result.value.as_ref());
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
        // A chain that loses a candidate can no longer state the selection
        // order and degrades to the unordered choice.
        AbstractValue::FirstTruthy(candidates) => {
            let mapped: Vec<Option<AbstractValue>> =
                candidates.into_iter().map(rendered_content_value).collect();
            let intact = mapped.iter().all(Option::is_some);
            let kept: Vec<AbstractValue> = mapped.into_iter().flatten().collect();
            if intact {
                AbstractValue::first_truthy(kept)
            } else {
                AbstractValue::choice(kept)
            }
        }
        other => Some(other),
    }
}

pub(super) fn eval_trim_affix(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let [affix, subject] = args else {
        return eval_all_args(args, env, resolver);
    };
    let affix = eval_expr_with_helper_calls(affix, env, resolver);
    let mut subject = eval_expr_with_helper_calls(subject, env, resolver);
    let subject_effects = subject.effects.clone();
    subject.effects.merge(affix.effects);
    let mut effects = subject.effects;
    let (string_paths, raw_range_key_paths) =
        super::strict_operands::string_call_operand_facts(function, args, env, resolver);
    // A single nonempty literal affix keeps a raw-identity subject's path
    // qualified by it as a lexical escape: trimming is the identity on
    // strings that do not contain the affix.
    if let Some(token) = single_replace_token(&value_strings(affix.value.as_ref()))
        && let Some(value) = subject.value.as_ref().and_then(|value| {
            super::value_facts::trim_affix_transformed_value(
                value,
                &subject_effects,
                token,
                function == "trimPrefix",
            )
        })
    {
        super::strict_operands::record_string_consumer_effects(&string_paths, &mut effects);
        super::strict_operands::record_raw_range_key_string_consumer_paths(
            &raw_range_key_paths,
            &mut effects,
        );
        return EvalResult::with_effects(Some(value), effects);
    }
    let value = super::value_facts::derive_value_text(subject.value);
    super::strict_operands::record_string_transform_effects(
        function,
        value.as_ref(),
        &string_paths,
        &raw_range_key_paths,
        &mut effects,
    );
    EvalResult::with_effects(value, effects)
}

pub(super) fn eval_trim_affix_pipeline(
    function: &str,
    current: EvalResult,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let [affix] = args else {
        let mut current = current;
        merge_arg_effects(args, env, resolver, &mut current.effects);
        return current;
    };
    let piped_value = current.value;
    let piped_effects = current.effects.clone();
    let affix = eval_expr_with_helper_calls(affix, env, resolver);
    let mut effects = current.effects;
    effects.merge(affix.effects);
    let (string_paths, raw_range_key_paths) = super::strict_operands::pipeline_string_operand_facts(
        function,
        args,
        piped_value.as_ref(),
        &piped_effects,
        env,
        resolver,
    );
    if let Some(token) = single_replace_token(&value_strings(affix.value.as_ref()))
        && let Some(value) = piped_value.as_ref().and_then(|value| {
            super::value_facts::trim_affix_transformed_value(
                value,
                &piped_effects,
                token,
                function == "trimPrefix",
            )
        })
    {
        super::strict_operands::record_string_consumer_effects(&string_paths, &mut effects);
        super::strict_operands::record_raw_range_key_string_consumer_paths(
            &raw_range_key_paths,
            &mut effects,
        );
        return EvalResult::with_effects(Some(value), effects);
    }
    let value = super::value_facts::derive_value_text(piped_value);
    super::strict_operands::record_string_transform_effects(
        function,
        value.as_ref(),
        &string_paths,
        &raw_range_key_paths,
        &mut effects,
    );
    EvalResult::with_effects(value, effects)
}

/// `regexReplaceAll REGEX SUBJECT REPLACEMENT` (and its literal/must
/// variants): when the pattern carries a mandatory literal, the call is the
/// identity on subjects not containing it, so a raw-identity subject keeps
/// its path qualified by that literal as a lexical escape. The
/// `TOKEN.*$`-with-empty-replacement shape is the exact cut-at-token
/// erasure (cilium's `regexReplaceAll "@.*$" tag ""` digest strip); other
/// patterns keep the contains-token exemption.
pub(super) fn eval_regex_replace(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let [pattern_expr, subject_expr, replacement_expr] = args else {
        return eval_all_args(args, env, resolver);
    };
    let pattern = eval_expr_with_helper_calls(pattern_expr, env, resolver);
    let mut subject = eval_expr_with_helper_calls(subject_expr, env, resolver);
    let replacement = eval_expr_with_helper_calls(replacement_expr, env, resolver);
    let subject_effects = subject.effects.clone();
    subject.effects.merge(pattern.effects);
    subject.effects.merge(replacement.effects);
    let mut effects = subject.effects;
    let (string_paths, raw_range_key_paths) =
        super::strict_operands::string_call_operand_facts(function, args, env, resolver);
    let pattern_strings = value_strings(pattern.value.as_ref());
    let escape = pattern_strings
        .iter()
        .next()
        .filter(|_| pattern_strings.len() == 1)
        .and_then(|pattern| {
            let token = super::value_facts::regex_mandatory_literal(pattern)?;
            let erases_to_empty = matches!(
                replacement_expr.deparen(),
                TemplateExpr::Literal(Literal::String(text) | Literal::RawString(text))
                    if text.is_empty()
            );
            if erases_to_empty && pattern.strip_prefix(token.as_str()) == Some(".*$") {
                Some(crate::helper_meta::LexicalEscape::CutAtToken(token))
            } else {
                Some(crate::helper_meta::LexicalEscape::Contains(token))
            }
        });
    if let Some(escape) = escape
        && let Some(value) = subject.value.as_ref().and_then(|value| {
            super::value_facts::regex_replace_transformed_value(value, &subject_effects, &escape)
        })
    {
        super::strict_operands::record_string_consumer_effects(&string_paths, &mut effects);
        super::strict_operands::record_raw_range_key_string_consumer_paths(
            &raw_range_key_paths,
            &mut effects,
        );
        return EvalResult::with_effects(Some(value), effects);
    }
    let value = super::value_facts::derive_value_text(subject.value);
    super::strict_operands::record_string_transform_effects(
        function,
        value.as_ref(),
        &string_paths,
        &raw_range_key_paths,
        &mut effects,
    );
    EvalResult::with_effects(value, effects)
}
