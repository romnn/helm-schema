mod bound_helper_resolver;
mod context;

use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

pub(crate) use context::FragmentEvalContext;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{bindings_for_helper_arg_with, eval_expr};
use bound_helper_resolver::{
    BoundHelperValueResolverParams, HelperAnalysisProjection, eval_expr_result_with_bound_helpers,
};

pub(crate) fn context_value_from_outer_expr(
    expr: &TemplateExpr,
    outer_locals: Option<&HashMap<String, AbstractValue>>,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
) -> Option<AbstractValue> {
    if matches!(expr, TemplateExpr::Variable(var) if var.is_empty())
        && let Some(bindings) = outer
    {
        return Some(AbstractValue::Dict(
            bindings
                .iter()
                .map(|(key, binding)| (key.clone(), binding.to_context_value()))
                .collect(),
        ));
    }

    let env = EvalEnv::from_outer_fragment_expr_context(outer_locals, outer, current_dot);
    eval_expr(expr, &env)
        .value
        .map(|value| value.to_context_value())
}

pub(crate) fn helper_result_from_expr_with_fragment_locals(
    expr: &TemplateExpr,
    fragment_locals: &HashMap<String, AbstractValue>,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> crate::eval_effect::EvalResult {
    let env =
        EvalEnv::from_helper_context_with_fragment_locals(outer, current_dot, fragment_locals);
    let mut result = eval_expr_result_with_bound_helpers(
        expr,
        &env,
        BoundHelperValueResolverParams {
            fragment_locals,
            outer,
            current_dot,
            context,
            seen,
            projection: HelperAnalysisProjection::HelperValue,
        },
    );
    result.value = result.value.map(|value| value.to_context_value());
    result
}

pub(crate) fn helper_result_from_exprs_with_fragment_locals(
    exprs: &[TemplateExpr],
    fragment_locals: &HashMap<String, AbstractValue>,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen_seed: &HashSet<String>,
) -> EvalResult {
    let mut seen = seen_seed.clone();
    let mut values = Vec::new();
    let mut effects = Effects::default();
    for expr in exprs {
        let result = helper_result_from_expr_with_fragment_locals(
            expr,
            fragment_locals,
            outer,
            current_dot,
            context,
            &mut seen,
        );
        values.extend(result.value);
        effects.merge(result.effects);
    }
    EvalResult::with_effects(AbstractValue::choice(values), effects)
}

pub(crate) fn values_for_helper_arg_with_fragment_locals(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    fragment_locals: &HashMap<String, AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HashMap<String, AbstractValue> {
    bindings_for_helper_arg_with(arg, outer, |expr| {
        helper_result_from_expr_with_fragment_locals(
            expr,
            fragment_locals,
            outer,
            current_dot,
            context,
            seen,
        )
        .value
    })
}

pub(crate) fn fragment_value_from_expr(
    expr: &TemplateExpr,
    locals: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<AbstractValue> {
    let env = EvalEnv::from_fragment_context(locals, current_dot);
    let current_dot_helper = current_dot.map(AbstractValue::to_context_value);
    eval_expr_result_with_bound_helpers(
        expr,
        &env,
        BoundHelperValueResolverParams {
            fragment_locals: locals,
            outer: None,
            current_dot: current_dot_helper.as_ref(),
            context,
            seen,
            projection: HelperAnalysisProjection::FragmentValue,
        },
    )
    .value
    .map(|value| value.to_context_value())
}
