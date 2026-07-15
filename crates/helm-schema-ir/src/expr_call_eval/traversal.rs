use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::helper_meta::HelperOutputMeta;
use helm_schema_core::Predicate;

use super::eval_all_args;
use super::strict_operands::{
    strict_operand_identity_paths, strict_operand_selection_conjunctions,
};
use super::value_facts::identity_value_paths;

/// `dig "k1" … "kn" default subject`: walk literal keys through the
/// subject dict, falling back to `default` when a key is MISSING. A key
/// that is present but not a map aborts rendering (sprig type-asserts
/// every step), so the subject and every intermediate key carry a
/// truthy⇒object contract; the dug value itself may be any type.
pub(super) fn eval_dig(
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
                ranged: crate::range_modes::RangeModes::default(),
                kind: crate::eval_effect::CaptureKind::Fail,
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

pub(super) fn eval_index(
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

pub(super) fn record_member_host_access(operand: &EvalResult, effects: &mut Effects) {
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
                ranged: crate::range_modes::RangeModes::default(),
                kind: crate::eval_effect::CaptureKind::MemberAccess {
                    handled_kinds: BTreeSet::new(),
                },
            };
            if !effects.helper_fails.contains(&capture) {
                effects.helper_fails.push(capture);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PathSegmentOption {
    segments: Vec<String>,
    integer_index: bool,
}

pub(super) fn apply_index_segment(
    value: &AbstractValue,
    option: &PathSegmentOption,
) -> Option<AbstractValue> {
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

pub(super) fn path_segment_options(
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
