use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};
use helm_schema_core::{Guard, GuardValue, Predicate, PredicateMemo};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{
    Effects, EvalResult, ProvenOperand, ProvenOperands, SelectionPolarity, SelectionReachability,
    SelectionTruthSource,
};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::function_semantics::{ArgumentEvaluationMode, function_semantics, type_is_schema_type};
use crate::scalar_value::{
    ScalarValue, ScalarValueDispatch, TruthCondition, any_predicates_with_memo, bool_predicate,
    conjoin_predicates_with_memo,
};

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
    let (condition_truth, condition_reachability) =
        eval_ternary_condition(args, piped_condition, env, resolver, &mut effects);
    let mut values = Vec::new();
    let mut scalar_dispatches = Vec::new();
    let mut proven_operands = Vec::new();
    let exact_selector = condition_truth.predicate().is_some();
    for (index, arg) in args.iter().enumerate() {
        if !has_piped_condition && index == 2 {
            continue;
        }
        let mut result = eval_expr_with_helper_calls(arg, env, resolver);
        if index < 2 {
            let reachability = if index == 0 {
                condition_reachability.clone()
            } else {
                condition_reachability.complement_with_memo(env.predicate_memo.as_ref())
            };
            super::conjoin_result_reachability(
                &mut result,
                &reachability,
                "ternary output selection",
                condition_truth
                    .when_true()
                    .value_paths()
                    .union(
                        &condition_truth
                            .when_false_with_memo(env.predicate_memo.as_ref())
                            .value_paths(),
                    )
                    .map(helm_schema_core::ValuesPath::encode)
                    .collect(),
            );
            if exact_selector {
                proven_operands.push(ProvenOperand {
                    condition: if index == 0 {
                        condition_truth.when_true()
                    } else {
                        condition_truth.when_false_with_memo(env.predicate_memo.as_ref())
                    },
                    evaluation_mode: ArgumentEvaluationMode::Evaluated,
                    result: Box::new(result.clone()),
                });
            }
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
    let mut result = EvalResult::with_effects(AbstractValue::choice(values), effects);
    result.proven_operands = Some(ProvenOperands {
        known: proven_operands,
        has_unresolved: !exact_selector,
    });
    if let [Some(when_true), Some(when_false)] = scalar_dispatches.as_slice()
        && let Some(dispatch) = ScalarValueDispatch::select_ternary_with_memo(
            &condition_truth,
            when_true,
            when_false,
            env.predicate_memo.as_ref(),
        )
    {
        return result.with_scalar_dispatch_with_memo(dispatch, env.predicate_memo.as_ref());
    }
    result
}

/// Resolves the ternary's condition — piped, or the trailing third argument —
/// into the truth condition and the reachability its two branch slots select
/// on.
///
/// The strict-kind capture and predicate contracts recorded here describe
/// CONSUMING the condition, so the condition's effects merge into `effects`
/// while its returned identity never reaches the ternary's output slot.
fn eval_ternary_condition(
    args: &[TemplateExpr],
    piped_condition: Option<(EvalResult, bool)>,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) -> (TruthCondition, SelectionReachability) {
    let condition = match piped_condition {
        Some((condition, _is_direct_values_path)) => condition,
        None => match args.get(2) {
            Some(condition_arg) => eval_expr_with_helper_calls(condition_arg, env, resolver),
            None => {
                return (
                    TruthCondition::Unknown,
                    SelectionReachability::approximate(None, SelectionTruthSource::RawInput),
                );
            }
        },
    };
    // Derived Boolean values carry no raw identity, so this records a
    // contract only for direct selectors and aliases of direct selectors.
    record_strict_kind_result(
        &condition,
        "boolean",
        function_semantics("ternary").nil_aborts(ArgumentEvaluationMode::Evaluated),
        effects,
    );
    let truth = condition.truth.clone();
    let reachability = SelectionReachability::from_condition_with_memo(
        &truth,
        SelectionPolarity::Truthy,
        SelectionTruthSource::RawInput,
        env.predicate_memo.as_ref(),
    );
    effects.merge(condition.effects.consumed_as_predicate());
    (truth, reachability)
}

pub(super) fn eval_type_is(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = Effects::default();
    let type_name = args.first().and_then(literal_type_name);
    let schema_type = type_is_schema_type(args.first());
    let mut truth = TruthCondition::Unknown;
    let mut subject_paths = BTreeSet::new();
    for (index, arg) in args.iter().enumerate() {
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        if index == 1 {
            subject_paths = identity_value_paths(result.value.as_ref());
            if let Some(type_name) = type_name {
                truth = selected_type_test_truth(
                    &result,
                    function,
                    type_name,
                    schema_type.as_deref(),
                    env.predicate_memo.as_ref(),
                );
            }
        }
        effects.merge(result.effects);
    }
    if let Some(schema_type) = schema_type {
        let tested_paths = truth
            .when_true()
            .value_paths()
            .into_iter()
            .chain(
                truth
                    .when_false_with_memo(env.predicate_memo.as_ref())
                    .value_paths(),
            )
            .map(|path| path.encode())
            .filter(|path| subject_paths.contains(path))
            .collect();
        // A type test over a structurally derived value can be constant even
        // when that value retains source provenance. Only paths that control
        // a known test polarity inherit an input-type hint.
        effects.add_tested_type_hints(tested_paths, &schema_type);
    }
    let mut result = EvalResult::with_effects(None, effects);
    result.set_truth_condition_with_memo(
        truth,
        crate::eval_effect::SelectionTruthSource::RawInput,
        env.predicate_memo.as_ref(),
    );
    result
}

fn literal_type_name(expr: &TemplateExpr) -> Option<&str> {
    let TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) = expr.deparen()
    else {
        return None;
    };
    Some(value)
}

/// Evaluates one type test without separating a selected value from its decision.
///
/// Proven leaves contribute independent true and false subsets under their own selection.
/// An unresolved remainder keeps both subsets partial instead of becoming a complement.
pub(crate) fn selected_type_test_truth(
    result: &EvalResult,
    function: &str,
    type_name: &str,
    schema_type: Option<&str>,
    memo: &PredicateMemo,
) -> TruthCondition {
    if let Some(proven) = &result.proven_operands
        && (proven.has_unresolved
            || !matches!(proven.known.as_slice(), [only] if only.condition == Predicate::True))
    {
        let mut when_true = Vec::new();
        let mut when_false = Vec::new();
        let mut complete = !proven.has_unresolved;
        for operand in &proven.known {
            let leaf_truth =
                selected_type_test_truth(&operand.result, function, type_name, schema_type, memo);
            complete &= leaf_truth.predicate().is_some();
            if let Some(condition) = conjoin_predicates_with_memo(
                operand.condition.clone(),
                leaf_truth.when_true(),
                memo,
            ) {
                when_true.push(condition);
            }
            if let Some(condition) = conjoin_predicates_with_memo(
                operand.condition.clone(),
                leaf_truth.when_false_with_memo(memo),
                memo,
            ) {
                when_false.push(condition);
            }
        }
        return TruthCondition::from_subsets_with_memo(
            any_predicates_with_memo(when_true, memo),
            any_predicates_with_memo(when_false, memo),
            complete,
            memo,
        );
    }
    if function == "kindIs" && type_name == "invalid" {
        return result
            .exact_input_identity()
            .map_or(TruthCondition::Unknown, |path| {
                TruthCondition::exact_with_memo(Predicate::invalid_kind_path(path), memo)
            });
    }
    let Some(schema_type) = schema_type else {
        return TruthCondition::Unknown;
    };
    if let Some(dispatch) = &result.scalar_dispatch {
        return scalar_dispatch_type_is(dispatch, schema_type, type_name, memo);
    }
    result
        .value
        .as_ref()
        .map_or(TruthCondition::Unknown, |value| {
            abstract_value_type_is(value, schema_type, type_name, memo)
        })
}

fn scalar_dispatch_type_is(
    dispatch: &ScalarValueDispatch,
    schema_type: &str,
    type_name: &str,
    memo: &PredicateMemo,
) -> TruthCondition {
    let mut when_true = Vec::new();
    let mut when_false = Vec::new();
    let mut complete = dispatch.complete;
    for (condition, value) in &dispatch.arms {
        let value_truth = scalar_value_type_is(value, schema_type, type_name, memo);
        complete &= value_truth.predicate().is_some();
        if let Some(predicate) =
            conjoin_predicates_with_memo(condition.clone(), value_truth.when_true(), memo)
        {
            when_true.push(predicate);
        }
        if let Some(predicate) = conjoin_predicates_with_memo(
            condition.clone(),
            value_truth.when_false_with_memo(memo),
            memo,
        ) {
            when_false.push(predicate);
        }
    }
    TruthCondition::from_subsets_with_memo(
        any_predicates_with_memo(when_true, memo),
        any_predicates_with_memo(when_false, memo),
        complete,
        memo,
    )
}

fn scalar_value_type_is(
    value: &ScalarValue,
    schema_type: &str,
    type_name: &str,
    memo: &PredicateMemo,
) -> TruthCondition {
    match value {
        ScalarValue::Literal(value) => TruthCondition::exact_with_memo(
            bool_predicate(guard_value_schema_type(value) == schema_type),
            memo,
        ),
        ScalarValue::Identity(path) => input_identity_type_is(path, schema_type, type_name, memo),
        ScalarValue::Rendered(_) | ScalarValue::PrintfStringIdentity(_) => {
            TruthCondition::exact_with_memo(bool_predicate(schema_type == "string"), memo)
        }
        ScalarValue::SplitLength { .. } => {
            TruthCondition::exact_with_memo(bool_predicate(schema_type == "integer"), memo)
        }
    }
}

fn input_identity_type_is(
    path: &helm_schema_core::ValuesPath,
    schema_type: &str,
    type_name: &str,
    memo: &PredicateMemo,
) -> TruthCondition {
    if path.segments().len() == 0 {
        TruthCondition::exact_with_memo(bool_predicate(schema_type == "object"), memo)
    } else if matches!(type_name, "int64" | "float64") {
        values_numeric_type_truth(&path.encode(), type_name, memo)
    } else {
        TruthCondition::exact_with_memo(
            Predicate::from(Guard::TypeIs {
                path: path.clone(),
                schema_type: schema_type.to_string(),
            }),
            memo,
        )
    }
}

fn abstract_value_type_is(
    value: &AbstractValue,
    schema_type: &str,
    type_name: &str,
    memo: &PredicateMemo,
) -> TruthCondition {
    match value {
        AbstractValue::ValuesPath(path) => {
            input_identity_type_is(path, schema_type, type_name, memo)
        }
        AbstractValue::OutputPath(path, meta) if meta.is_input_identity() => {
            input_identity_type_is(path, schema_type, type_name, memo)
        }
        AbstractValue::JsonDecodedPath(path) => {
            json_decoded_numeric_type_truth(&path.encode(), schema_type, type_name, memo)
        }
        AbstractValue::OutputPath(path, meta) if meta.json_decoded => {
            json_decoded_numeric_type_truth(&path.encode(), schema_type, type_name, memo)
        }
        AbstractValue::Dict(_)
        | AbstractValue::Overlay { .. }
        | AbstractValue::MergedLayers(_)
        | AbstractValue::RootContext => {
            TruthCondition::exact_with_memo(bool_predicate(schema_type == "object"), memo)
        }
        AbstractValue::List(_) | AbstractValue::KeysList(_) | AbstractValue::SplitList { .. } => {
            TruthCondition::exact_with_memo(bool_predicate(schema_type == "array"), memo)
        }
        AbstractValue::StringSet(_) | AbstractValue::SplitSegment { .. } => {
            TruthCondition::exact_with_memo(bool_predicate(schema_type == "string"), memo)
        }
        AbstractValue::DerivedBoolean(_) => {
            TruthCondition::exact_with_memo(bool_predicate(schema_type == "boolean"), memo)
        }
        AbstractValue::Choice(choices) => {
            type_is_for_alternatives(choices.iter(), schema_type, type_name, memo)
        }
        AbstractValue::FirstTruthy(candidates) => {
            type_is_for_first_truthy(candidates, schema_type, type_name, memo)
        }
        AbstractValue::Top
        | AbstractValue::Unknown
        | AbstractValue::RangeKey(_)
        | AbstractValue::OutputPath(_, _)
        | AbstractValue::Widened(_) => TruthCondition::Unknown,
    }
}

fn type_is_for_first_truthy(
    candidates: &[AbstractValue],
    schema_type: &str,
    type_name: &str,
    memo: &PredicateMemo,
) -> TruthCondition {
    let mut remaining = Predicate::True;
    let mut when_true = Vec::new();
    let mut when_false = Vec::new();
    let mut complete = !candidates.is_empty();
    for (index, candidate) in candidates.iter().enumerate() {
        if remaining == Predicate::False {
            break;
        }
        let selected = if index + 1 == candidates.len() {
            Some(remaining.clone())
        } else {
            let candidate_truth = abstract_value_truth(candidate, memo);
            complete &= candidate_truth.predicate().is_some();
            let selected =
                conjoin_predicates_with_memo(remaining.clone(), candidate_truth.when_true(), memo);
            remaining = conjoin_predicates_with_memo(
                remaining,
                candidate_truth.when_false_with_memo(memo),
                memo,
            )
            .unwrap_or(Predicate::False);
            selected
        };
        let Some(selected) = selected else {
            continue;
        };
        let value_truth = abstract_value_type_is(candidate, schema_type, type_name, memo);
        complete &= value_truth.predicate().is_some();
        if let Some(predicate) =
            conjoin_predicates_with_memo(selected.clone(), value_truth.when_true(), memo)
        {
            when_true.push(predicate);
        }
        if let Some(predicate) =
            conjoin_predicates_with_memo(selected, value_truth.when_false_with_memo(memo), memo)
        {
            when_false.push(predicate);
        }
    }
    TruthCondition::from_subsets_with_memo(
        any_predicates_with_memo(when_true, memo),
        any_predicates_with_memo(when_false, memo),
        complete,
        memo,
    )
}

fn abstract_value_truth(value: &AbstractValue, memo: &PredicateMemo) -> TruthCondition {
    if let Some(truthy) = value.static_truthiness() {
        return TruthCondition::exact_with_memo(bool_predicate(truthy), memo);
    }
    match value {
        AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path) => {
            TruthCondition::exact_with_memo(Predicate::truthy_path(path.encode()), memo)
        }
        _ => TruthCondition::Unknown,
    }
}

fn type_is_for_alternatives<'a>(
    alternatives: impl IntoIterator<Item = &'a AbstractValue>,
    schema_type: &str,
    type_name: &str,
    memo: &PredicateMemo,
) -> TruthCondition {
    let conditions = alternatives
        .into_iter()
        .map(|value| abstract_value_type_is(value, schema_type, type_name, memo))
        .collect::<Vec<_>>();
    if conditions.is_empty() {
        return TruthCondition::Unknown;
    }
    TruthCondition::from_subsets_with_memo(
        Predicate::all(conditions.iter().map(TruthCondition::when_true).collect()),
        Predicate::all(conditions.iter().map(TruthCondition::when_false).collect()),
        false,
        memo,
    )
}

fn values_numeric_type_truth(path: &str, type_name: &str, memo: &PredicateMemo) -> TruthCondition {
    let integer = Predicate::from(Guard::TypeIs {
        path: helm_schema_core::ValuesPath::parse(path),
        schema_type: "integer".to_string(),
    });
    let number = Predicate::from(Guard::TypeIs {
        path: helm_schema_core::ValuesPath::parse(path),
        schema_type: "number".to_string(),
    });
    match type_name {
        "int64" => {
            TruthCondition::from_subsets_with_memo(Predicate::False, integer.negated(), false, memo)
        }
        "float64" => TruthCondition::from_subsets_with_memo(
            Predicate::all(vec![number.clone(), integer.negated()]),
            number.negated(),
            false,
            memo,
        ),
        _ => TruthCondition::Unknown,
    }
}

fn json_decoded_numeric_type_truth(
    path: &str,
    schema_type: &str,
    type_name: &str,
    memo: &PredicateMemo,
) -> TruthCondition {
    match type_name {
        "int64" => TruthCondition::exact_with_memo(Predicate::False, memo),
        "float64" => TruthCondition::exact_with_memo(
            Predicate::from(Guard::TypeIs {
                path: helm_schema_core::ValuesPath::parse(path),
                schema_type: "number".to_string(),
            }),
            memo,
        ),
        _ => TruthCondition::exact_with_memo(
            Predicate::from(Guard::TypeIs {
                path: helm_schema_core::ValuesPath::parse(path),
                schema_type: schema_type.to_string(),
            }),
            memo,
        ),
    }
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

/// Go template `eq`/`ne` terminate on incomparable operand kinds: any
/// composite (map/list) never compares, and a scalar literal fixes the
/// basic kind the other operands must share. The contract is bounded to
/// what a literal proves — nil/missing operands stay unmodeled (Helm
/// charts routinely compare optional values).
pub(super) fn eval_comparison(
    function: &str,
    args: &[TemplateExpr],
    piped: Option<(EvalResult, bool)>,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let literal_kind = comparison_literal_kind(args);
    let mut operands = Vec::with_capacity(args.len() + usize::from(piped.is_some()));
    let mut raw_identity_operands = Vec::with_capacity(args.len() + usize::from(piped.is_some()));
    if let Some((piped, is_direct_identity)) = piped {
        operands.push(piped);
        raw_identity_operands.push(is_direct_identity);
    }
    operands.extend(
        args.iter()
            .map(|arg| eval_expr_with_helper_calls(arg, env, resolver)),
    );
    raw_identity_operands.extend(args.iter().map(direct_comparison_identity));
    eval_comparison_operands_with_memo(
        function,
        operands,
        &raw_identity_operands,
        literal_kind,
        env.predicate_memo.as_ref(),
    )
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

fn eval_comparison_operands_with_memo(
    function: &str,
    operands: Vec<EvalResult>,
    raw_identity_operands: &[bool],
    literal_kind: Option<&str>,
    memo: &PredicateMemo,
) -> EvalResult {
    let mut comparison_effects = Effects::default();
    let equality = equality_condition(&operands, raw_identity_operands, memo);
    let truth = if function == "ne" {
        equality.negated_with_memo(memo)
    } else {
        equality
    };
    let Some(literal_kind) = literal_kind else {
        let mut result = merge_operand_results(operands, comparison_effects);
        result.set_truth_condition_with_memo(
            truth,
            crate::eval_effect::SelectionTruthSource::RawInput,
            memo,
        );
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
    result.set_truth_condition_with_memo(
        truth,
        crate::eval_effect::SelectionTruthSource::RawInput,
        memo,
    );
    result
}

fn equality_condition(
    operands: &[EvalResult],
    raw_identity_operands: &[bool],
    memo: &PredicateMemo,
) -> TruthCondition {
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
                return TruthCondition::exact_with_memo(
                    if left == right {
                        Predicate::True
                    } else {
                        Predicate::False
                    },
                    memo,
                );
            }
            (Some(target), None) => {
                return right.condition_equals_with_memo(&target, memo);
            }
            (None, Some(target)) => {
                return left.condition_equals_with_memo(&target, memo);
            }
            (None, None) => {}
        },
        (Some(dispatch), None) => {
            if *right_is_raw
                && let Some(path) = direct_raw_identity_path(right.value.as_ref())
                && let Some(value) = dispatch.constant_value()
            {
                return TruthCondition::exact_with_memo(
                    Predicate::from(Guard::Eq {
                        path: helm_schema_core::ValuesPath::parse(&path),
                        value,
                    }),
                    memo,
                );
            }
        }
        (None, Some(dispatch)) => {
            if *left_is_raw
                && let Some(path) = direct_raw_identity_path(left.value.as_ref())
                && let Some(value) = dispatch.constant_value()
            {
                return TruthCondition::exact_with_memo(
                    Predicate::from(Guard::Eq {
                        path: helm_schema_core::ValuesPath::parse(&path),
                        value,
                    }),
                    memo,
                );
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
