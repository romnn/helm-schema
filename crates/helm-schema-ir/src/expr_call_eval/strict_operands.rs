use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, direct_values_path, eval_expr_with_helper_calls};
use helm_schema_core::Predicate;

use super::serialization::record_total_conversion_effects;
use super::value_facts::{identity_range_key_paths, identity_value_paths};
use helm_schema_ast::{
    is_total_stringification_function, strict_parser_operand_pattern, string_operand_indices,
};

pub(super) fn record_string_transform_effects(
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

pub(super) fn string_call_operand_facts(
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

pub(super) fn pipeline_string_operand_facts(
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

pub(super) fn record_string_call_consumers(
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

pub(super) fn record_strict_parser_call(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    let Some((index, pattern)) = strict_parser_operand_pattern(function, args.len()) else {
        return;
    };
    let Some(arg) = args.get(index) else {
        return;
    };
    // Locals and helper returns can denote several guarded or transformed
    // alternatives. Until those return alternatives remain disjunctive at
    // the call site, only a parser-proven direct values selector can safely
    // inherit the parser's lexical domain.
    if direct_values_path(arg).is_none() {
        return;
    }
    let operand = eval_expr_with_helper_calls(arg, env, resolver);
    record_strict_parser_result(&operand, pattern, effects);
}

pub(super) fn record_strict_parser_pipeline(
    function: &str,
    args: &[TemplateExpr],
    piped: &EvalResult,
    piped_is_direct_values_path: bool,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    let Some((index, pattern)) = strict_parser_operand_pattern(function, args.len() + 1) else {
        return;
    };
    if index == args.len() {
        // The pipeline flag is syntax-derived at the first stage; abstract
        // path identity alone cannot distinguish a raw selector from a local
        // or helper result assembled from several source paths.
        if piped_is_direct_values_path {
            record_strict_parser_result(piped, pattern, effects);
        }
    } else if let Some(arg) = args.get(index) {
        if direct_values_path(arg).is_none() {
            return;
        }
        let operand = eval_expr_with_helper_calls(arg, env, resolver);
        record_strict_parser_result(&operand, pattern, effects);
    }
}

fn record_strict_parser_result(operand: &EvalResult, pattern: &str, effects: &mut Effects) {
    for path in strict_operand_identity_paths(operand) {
        for mut conjunction in strict_operand_selection_conjunctions(operand, &path) {
            conjunction.push(
                Predicate::from(crate::Guard::MatchesPattern {
                    path: path.clone(),
                    pattern: pattern.to_string(),
                    templated: false,
                })
                .negated(),
            );
            push_fail_capture(conjunction, effects);
        }
    }
}

/// Record that an expression stage consumes the RAW value of `paths` as a
/// Go string, failing rendering otherwise. A path that already passed a
/// converting stage (`printf … | trunc`) or flows out of a shape-erasing
/// local binding reaches the consumer as derived text, so the earlier
/// conversion owns the contract. A path behind an ordered value selector is
/// consumed only on its selected arm, so its contract is captured as a
/// conditional fail-class implication instead of an unconditional row
/// contract.
pub(super) fn record_string_consumer_effects(paths: &BTreeSet<String>, effects: &mut Effects) {
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
                    ranged: crate::range_modes::RangeModes::default(),
                    kind: crate::eval_effect::CaptureKind::Fail,
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

pub(super) fn record_range_key_string_consumer_effects(
    value: &Option<AbstractValue>,
    effects: &mut Effects,
) {
    let paths = identity_range_key_paths(value);
    let raw_paths = paths
        .difference(&effects.derived_range_key_paths)
        .cloned()
        .collect::<BTreeSet<_>>();
    record_raw_range_key_string_consumer_paths(&raw_paths, effects);
    effects.derived_range_key_paths.extend(paths);
}

pub(super) fn record_raw_range_key_string_consumer_paths(
    raw_paths: &BTreeSet<String>,
    effects: &mut Effects,
) {
    if !raw_paths.is_empty() {
        let capture = crate::eval_effect::FailCapture {
            conjunction: Vec::new(),
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::RangeKeyStrings {
                paths: raw_paths.clone(),
            },
        };
        if !effects.helper_fails.contains(&capture) {
            effects.helper_fails.push(capture);
        }
    }
    effects
        .derived_range_key_paths
        .extend(raw_paths.iter().cloned());
}

/// Records the runtime operand contract of a strict collection function.
///
/// The call itself does not skip Helm-empty values. Only a `default` or `coalesce` selection
/// makes a raw source conditional on truthiness; structural `if`/`with` guards join later when
/// the effects are absorbed at the execution site.
pub(super) fn record_strict_kind_operands(
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

pub(super) fn record_strict_kind_result(
    operand: &EvalResult,
    schema_type: &str,
    effects: &mut Effects,
) {
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

pub(super) fn record_forbidden_kind(
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

pub(super) fn push_fail_capture(conjunction: Vec<Predicate>, effects: &mut Effects) {
    let capture = crate::eval_effect::FailCapture {
        conjunction,
        ranged: crate::range_modes::RangeModes::default(),
        kind: crate::eval_effect::CaptureKind::Fail,
    };
    if !effects.helper_fails.contains(&capture) {
        effects.helper_fails.push(capture);
    }
}

pub(super) fn strict_operand_identity_paths(operand: &EvalResult) -> BTreeSet<String> {
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

pub(super) fn strict_operand_selection_conjunctions(
    operand: &EvalResult,
    path: &str,
) -> Vec<Vec<Predicate>> {
    operand_selection_conjunctions(&operand.effects, path)
}

pub(super) fn operand_selection_conjunctions(effects: &Effects, path: &str) -> Vec<Vec<Predicate>> {
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
pub(super) fn record_length_bearing_operand(
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

pub(super) fn record_length_bearing_result(operand: &EvalResult, effects: &mut Effects) {
    for path in strict_operand_identity_paths(operand) {
        for kind in ["boolean", "integer", "number"] {
            for conjunction in strict_operand_selection_conjunctions(operand, &path) {
                record_forbidden_kind(&path, kind, conjunction, effects);
            }
        }
    }
}
