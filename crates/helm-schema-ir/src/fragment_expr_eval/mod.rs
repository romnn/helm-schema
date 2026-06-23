mod bound_helper_resolver;
mod context;

use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

pub(crate) use context::FragmentEvalContext;

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::helper_arg_projection::bindings_for_helper_arg_with;
use crate::template_expr_analysis::expr_contains_helper_call;
use bound_helper_resolver::{
    BoundHelperValueResolverParams, HelperAnalysisProjection, eval_expr_with_bound_helpers,
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

pub(crate) fn helper_value_from_expr_with_fragment_locals(
    expr: &TemplateExpr,
    fragment_locals: &HashMap<String, AbstractValue>,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<AbstractValue> {
    let env =
        EvalEnv::from_helper_context_with_fragment_locals(outer, current_dot, fragment_locals);
    eval_expr_with_bound_helpers(
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
    )
    .map(|value| value.to_context_value())
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
        helper_value_from_expr_with_fragment_locals(
            expr,
            fragment_locals,
            outer,
            current_dot,
            context,
            seen,
        )
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
    eval_expr_with_bound_helpers(
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
    .map(|value| value.to_context_value())
}

pub(crate) fn fragment_or_helper_value_from_expr(
    expr: &TemplateExpr,
    fragment_locals: &HashMap<String, AbstractValue>,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<AbstractValue> {
    let current_dot_fragment = current_dot.map(AbstractValue::to_context_value);
    if !expr_contains_helper_call(expr)
        && let Some(binding) = fragment_value_from_expr(
            expr,
            fragment_locals,
            current_dot_fragment.as_ref(),
            context,
            seen,
        )
    {
        return Some(binding);
    }
    if let Some(binding) = helper_value_from_expr_with_fragment_locals(
        expr,
        fragment_locals,
        outer,
        current_dot,
        context,
        seen,
    ) {
        return Some(binding.to_context_value());
    }
    fragment_value_from_expr(
        expr,
        fragment_locals,
        current_dot_fragment.as_ref(),
        context,
        seen,
    )
}
