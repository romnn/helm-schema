use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};
use helm_schema_core::{Guard, GuardValue, Predicate};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::scalar_value::{ScalarValueDispatch, TruthCondition};
use helm_schema_ast::{strict_operand_nil_aborts, type_is_schema_type};

use super::collections::direct_raw_identity_path;
use super::strict_operands::{record_comparable_kind_result, record_strict_kind_result};
use super::value_facts::identity_value_paths;

/// `ternary A B COND`: the first two arguments are the branch values, while
/// the trailing (or piped) condition must be a Go `bool`.
pub(super) fn eval_ternary(
    args: &[TemplateExpr],
    piped_condition: Option<(EvalResult, bool)>,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = Effects::default();
    let has_piped_condition = piped_condition.is_some();
    let mut condition_path = None;
    let mut condition_identity = BTreeSet::new();
    let condition_truth;
    if let Some((condition, _is_direct_values_path)) = piped_condition {
        // Derived Boolean values carry no raw identity, so this records a
        // contract only for direct selectors and aliases of direct selectors.
        record_strict_kind_result(
            &condition,
            "boolean",
            strict_operand_nil_aborts("ternary", false),
            &mut effects,
        );
        condition_path = condition.value.as_ref().and_then(raw_condition_path);
        condition_identity = identity_value_paths(condition.value.as_ref());
        condition_truth = condition.truth.clone();
        effects.merge(condition.effects);
    } else if let Some(condition_arg) = args.get(2) {
        let condition = eval_expr_with_helper_calls(condition_arg, env, resolver);
        record_strict_kind_result(
            &condition,
            "boolean",
            strict_operand_nil_aborts("ternary", false),
            &mut effects,
        );
        condition_path = condition.value.as_ref().and_then(raw_condition_path);
        condition_identity = identity_value_paths(condition.value.as_ref());
        condition_truth = condition.truth.clone();
        effects.merge(condition.effects);
    } else {
        condition_truth = TruthCondition::Unknown;
    }
    // The condition only SELECTS an arm — its value never renders into the
    // output slot, so its identity must not become a placed row there (a
    // Service port-name slot would stamp its provider string schema onto a
    // raw Boolean flag, as in harbor's `ternary "https-web" "http-web"
    // .Values.internalTLS.enabled`). The strict-kind capture above keeps
    // the Boolean operand contract.
    for path in &condition_identity {
        effects.output_paths.remove(path);
    }
    let mut values = Vec::new();
    let mut scalar_dispatches = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        if !has_piped_condition && index == 2 {
            continue;
        }
        let mut result = eval_expr_with_helper_calls(arg, env, resolver);
        if index < 2
            && let Some(path) = &condition_path
        {
            let predicate = if index == 0 {
                Predicate::truthy_path(path.clone())
            } else {
                Predicate::truthy_path(path.clone()).negated()
            };
            conjoin_result_selection(&mut result, predicate);
        }
        effects.merge(result.effects);
        if index < 2 {
            scalar_dispatches.push(result.scalar_dispatch);
            if let Some(value) = result.value {
                values.push(value);
            }
        }
    }
    effects.promote_tested_type_hints();
    let result = EvalResult::with_effects(AbstractValue::choice(values), effects);
    if let [Some(when_true), Some(when_false)] = scalar_dispatches.as_slice()
        && let Some(dispatch) =
            ScalarValueDispatch::select_ternary(&condition_truth, when_true, when_false)
    {
        return result.with_scalar_dispatch(dispatch);
    }
    result
}

fn raw_condition_path(value: &AbstractValue) -> Option<String> {
    match value {
        AbstractValue::ValuesPath(path)
        | AbstractValue::JsonDecodedPath(path)
        | AbstractValue::OutputPath(path, _) => Some(path.clone()),
        AbstractValue::Choice(choices) => {
            let mut paths = choices.iter().filter_map(raw_condition_path);
            let first = paths.next()?;
            paths.all(|path| path == first).then_some(first)
        }
        AbstractValue::FirstTruthy(candidates) => {
            let mut paths = candidates.iter().filter_map(raw_condition_path);
            let first = paths.next()?;
            paths.all(|path| path == first).then_some(first)
        }
        AbstractValue::Top
        | AbstractValue::Unknown
        | AbstractValue::RangeKey(_)
        | AbstractValue::KeysList(_)
        | AbstractValue::RootContext
        | AbstractValue::StringSet(_)
        | AbstractValue::DerivedBoolean(_)
        | AbstractValue::Dict(_)
        | AbstractValue::List(_)
        | AbstractValue::Overlay { .. }
        | AbstractValue::MergedLayers(_)
        | AbstractValue::SplitList { .. }
        | AbstractValue::SplitSegment { .. }
        | AbstractValue::Widened(_) => None,
    }
}

fn conjoin_result_selection(result: &mut EvalResult, predicate: Predicate) {
    let selection = BTreeSet::from([predicate]);
    for path in identity_value_paths(result.value.as_ref()) {
        result
            .effects
            .local_output_meta
            .entry(path)
            .or_default()
            .conjoin_branches(&selection);
    }
}

pub(super) fn eval_type_is(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = Effects::default();
    let schema_type = type_is_schema_type(args.first());
    let mut truth = TruthCondition::Unknown;
    let mut subject_paths = BTreeSet::new();
    for (index, arg) in args.iter().enumerate() {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        if index == 1 {
            subject_paths = identity_value_paths(result.value.as_ref());
            if let Some(schema_type) = &schema_type {
                truth = type_is_truth(&result, schema_type);
            }
        }
        effects.merge(result.effects);
    }
    if let Some(schema_type) = schema_type {
        effects.add_tested_type_hints(subject_paths, &schema_type);
    }
    let mut result = EvalResult::with_effects(None, effects);
    result.truth = truth;
    result
}

fn type_is_truth(result: &EvalResult, schema_type: &str) -> TruthCondition {
    if let Some(value) = result
        .scalar_dispatch
        .as_ref()
        .and_then(ScalarValueDispatch::constant_value)
    {
        return TruthCondition::exact(bool_predicate(
            guard_value_schema_type(&value) == schema_type,
        ));
    }
    result
        .value
        .as_ref()
        .map_or(TruthCondition::Unknown, |value| {
            abstract_value_type_is(value, schema_type)
        })
}

fn abstract_value_type_is(value: &AbstractValue, schema_type: &str) -> TruthCondition {
    match value {
        AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path) => {
            if path.is_empty() {
                TruthCondition::exact(bool_predicate(schema_type == "object"))
            } else {
                TruthCondition::exact(Predicate::from(Guard::TypeIs {
                    path: path.clone(),
                    schema_type: schema_type.to_string(),
                }))
            }
        }
        AbstractValue::OutputPath(path, meta) if meta.json_decoded => {
            TruthCondition::exact(Predicate::from(Guard::TypeIs {
                path: path.clone(),
                schema_type: schema_type.to_string(),
            }))
        }
        AbstractValue::Dict(_)
        | AbstractValue::Overlay { .. }
        | AbstractValue::MergedLayers(_)
        | AbstractValue::RootContext => {
            TruthCondition::exact(bool_predicate(schema_type == "object"))
        }
        AbstractValue::List(_) | AbstractValue::KeysList(_) | AbstractValue::SplitList { .. } => {
            TruthCondition::exact(bool_predicate(schema_type == "array"))
        }
        AbstractValue::StringSet(_) | AbstractValue::SplitSegment { .. } => {
            TruthCondition::exact(bool_predicate(schema_type == "string"))
        }
        AbstractValue::DerivedBoolean(_) => {
            TruthCondition::exact(bool_predicate(schema_type == "boolean"))
        }
        AbstractValue::Choice(choices) => type_is_for_alternatives(choices.iter(), schema_type),
        AbstractValue::FirstTruthy(candidates) => {
            type_is_for_alternatives(candidates.iter(), schema_type)
        }
        AbstractValue::Top
        | AbstractValue::Unknown
        | AbstractValue::RangeKey(_)
        | AbstractValue::OutputPath(_, _)
        | AbstractValue::Widened(_) => TruthCondition::Unknown,
    }
}

fn type_is_for_alternatives<'a>(
    alternatives: impl IntoIterator<Item = &'a AbstractValue>,
    schema_type: &str,
) -> TruthCondition {
    let conditions = alternatives
        .into_iter()
        .map(|value| abstract_value_type_is(value, schema_type))
        .collect::<Vec<_>>();
    if conditions.is_empty() {
        return TruthCondition::Unknown;
    }
    TruthCondition::from_subsets(
        Predicate::all(conditions.iter().map(TruthCondition::when_true).collect()),
        Predicate::all(conditions.iter().map(TruthCondition::when_false).collect()),
        false,
    )
}

fn guard_value_schema_type(value: &GuardValue) -> &'static str {
    match value {
        GuardValue::String(_) => "string",
        GuardValue::Bool(_) => "boolean",
        GuardValue::Int(_) => "integer",
        GuardValue::Float(_) => "number",
        GuardValue::Null => "null",
    }
}

fn bool_predicate(value: bool) -> Predicate {
    if value {
        Predicate::True
    } else {
        Predicate::False
    }
}

/// Go template `eq`/`ne` terminate on incomparable operand kinds: any
/// composite (map/list) never compares, and a scalar literal fixes the
/// basic kind the other operands must share. The contract is bounded to
/// what a literal proves — nil/missing operands stay unmodeled (Helm
/// charts routinely compare optional values).
pub(super) fn eval_comparison(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let literal_kind = comparison_literal_kind(args);
    let operands = args
        .iter()
        .map(|arg| eval_expr_with_helper_calls(arg, env, resolver))
        .collect();
    let raw_identity_operands: Vec<bool> = args.iter().map(direct_comparison_identity).collect();
    eval_comparison_operands(function, operands, &raw_identity_operands, literal_kind)
}

pub(super) fn eval_pipeline_comparison(
    function: &str,
    current: EvalResult,
    current_is_direct_identity: bool,
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
    let mut raw_identity_operands = Vec::with_capacity(args.len() + 1);
    raw_identity_operands.push(current_is_direct_identity);
    raw_identity_operands.extend(args.iter().map(direct_comparison_identity));
    eval_comparison_operands(function, operands, &raw_identity_operands, literal_kind)
}

fn direct_comparison_identity(expr: &TemplateExpr) -> bool {
    matches!(
        expr.deparen(),
        TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
    )
}

pub(super) fn comparison_literal_kind(args: &[TemplateExpr]) -> Option<&'static str> {
    args.iter().find_map(|arg| match arg.deparen() {
        TemplateExpr::Literal(Literal::String(_) | Literal::RawString(_)) => Some("string"),
        TemplateExpr::Literal(Literal::Bool(_)) => Some("boolean"),
        TemplateExpr::Literal(Literal::Int(_)) => Some("integer"),
        TemplateExpr::Literal(Literal::Float(_)) => Some("number"),
        _ => None,
    })
}

pub(super) fn eval_comparison_operands(
    function: &str,
    operands: Vec<EvalResult>,
    raw_identity_operands: &[bool],
    literal_kind: Option<&str>,
) -> EvalResult {
    let mut comparison_effects = Effects::default();
    let equality = equality_condition(&operands, raw_identity_operands);
    let truth = if function == "ne" {
        equality.negated()
    } else {
        equality
    };
    let Some(literal_kind) = literal_kind else {
        let mut result = merge_operand_results(operands, comparison_effects);
        result.truth = truth;
        return result;
    };
    for operand in &operands {
        // Go templates compare only values of the same basic kind, with
        // relaxed exact types inside the integer family. JSON Schema cannot
        // distinguish a Go integer from an integral floating-point value, so
        // the `number` case stays conservatively broad rather than rejecting
        // a valid float such as `1.0`.
        record_comparable_kind_result(operand, literal_kind, &mut comparison_effects);
    }
    let mut result = merge_operand_results(operands, comparison_effects);
    result.truth = truth;
    result
}

fn equality_condition(operands: &[EvalResult], raw_identity_operands: &[bool]) -> TruthCondition {
    let [left, right] = operands else {
        return TruthCondition::Unknown;
    };
    let [left_is_raw, right_is_raw] = raw_identity_operands else {
        return TruthCondition::Unknown;
    };
    match (
        left.scalar_dispatch.as_ref(),
        right.scalar_dispatch.as_ref(),
    ) {
        (Some(left), Some(right)) => match (left.constant_value(), right.constant_value()) {
            (Some(left), Some(right)) => {
                return TruthCondition::exact(if left == right {
                    Predicate::True
                } else {
                    Predicate::False
                });
            }
            (Some(target), None) => {
                return right.condition_equals(&target);
            }
            (None, Some(target)) => {
                return left.condition_equals(&target);
            }
            (None, None) => {}
        },
        (Some(dispatch), None) => {
            if *right_is_raw
                && let Some(path) = direct_raw_identity_path(right.value.as_ref())
                && let Some(value) = dispatch.constant_value()
            {
                return TruthCondition::exact(Predicate::from(Guard::Eq { path, value }));
            }
        }
        (None, Some(dispatch)) => {
            if *left_is_raw
                && let Some(path) = direct_raw_identity_path(left.value.as_ref())
                && let Some(value) = dispatch.constant_value()
            {
                return TruthCondition::exact(Predicate::from(Guard::Eq { path, value }));
            }
        }
        (None, None) => {}
    }
    TruthCondition::Unknown
}

pub(super) fn merge_operand_results(operands: Vec<EvalResult>, mut effects: Effects) -> EvalResult {
    for operand in operands {
        effects.merge(operand.effects);
    }
    EvalResult::with_effects(
        Some(AbstractValue::DerivedBoolean(effects.output_paths.clone())),
        effects,
    )
}
