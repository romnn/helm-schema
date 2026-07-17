use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};
use helm_schema_core::Predicate;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use helm_schema_ast::type_is_schema_type;

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
    if let Some((condition, _is_direct_values_path)) = piped_condition {
        // Derived Boolean values carry no raw identity, so this records a
        // contract only for direct selectors and aliases of direct selectors.
        record_strict_kind_result(&condition, "boolean", &mut effects);
        condition_path = condition.value.as_ref().and_then(raw_condition_path);
        condition_identity = identity_value_paths(&condition.value);
        effects.merge(condition.effects);
    } else if let Some(condition_arg) = args.get(2) {
        let condition = eval_expr_with_helper_calls(condition_arg, env, resolver);
        record_strict_kind_result(&condition, "boolean", &mut effects);
        condition_path = condition.value.as_ref().and_then(raw_condition_path);
        condition_identity = identity_value_paths(&condition.value);
        effects.merge(condition.effects);
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
        if index < 2
            && let Some(value) = result.value
        {
            values.push(value);
        }
    }
    effects.promote_tested_type_hints();
    EvalResult::with_effects(AbstractValue::choice(values), effects)
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
    for path in identity_value_paths(&result.value) {
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
pub(super) fn eval_comparison(
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

pub(super) fn eval_pipeline_comparison(
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
    operands: Vec<EvalResult>,
    literal_kind: Option<&str>,
) -> EvalResult {
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
        record_comparable_kind_result(operand, literal_kind, &mut comparison_effects);
    }
    merge_operand_results(operands, comparison_effects)
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
