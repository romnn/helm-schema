mod bound_helper_resolver;
mod context;

use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

pub(crate) use context::FragmentEvalContext;

use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::fragment_binding::FragmentBinding;
use crate::fragment_binding_projection::fragment_to_helper_binding;
use crate::helper_arg_projection::bindings_for_helper_arg_with;
use crate::helper_binding::HelperBinding;
use crate::helper_binding_projection::helper_to_fragment_binding;
use crate::template_expr_analysis::expr_contains_helper_call;
use crate::template_expr_cache::parse_expr_text;
use bound_helper_resolver::{
    BoundHelperValueResolverParams, HelperAnalysisProjection, eval_expr_with_bound_helpers,
};

pub(crate) fn fragment_binding_from_outer_expr(
    expr: &TemplateExpr,
    outer_locals: Option<&HashMap<String, FragmentBinding>>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> Option<FragmentBinding> {
    if matches!(expr, TemplateExpr::Variable(var) if var.is_empty())
        && let Some(bindings) = outer
    {
        return Some(FragmentBinding::Dict(
            bindings
                .iter()
                .map(|(key, binding)| (key.clone(), helper_to_fragment_binding(binding)))
                .collect(),
        ));
    }

    let env = EvalEnv::from_outer_fragment_expr_context(outer_locals, outer, current_dot);
    eval_expr(expr, &env)
        .value
        .as_ref()
        .and_then(crate::abstract_value::AbstractValue::to_fragment_binding)
}

pub(crate) fn helper_binding_from_expr_with_fragment_locals(
    expr: &TemplateExpr,
    fragment_locals: &HashMap<String, FragmentBinding>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<HelperBinding> {
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
            projection: HelperAnalysisProjection::HelperBinding,
        },
    )
    .and_then(|value| value.to_helper_binding())
}

pub(crate) fn bindings_for_helper_arg_with_fragment_locals(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    fragment_locals: &HashMap<String, FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HashMap<String, HelperBinding> {
    bindings_for_helper_arg_with(arg, outer, |expr| {
        helper_binding_from_expr_with_fragment_locals(
            expr,
            fragment_locals,
            outer,
            current_dot,
            context,
            seen,
        )
    })
}

pub(crate) fn fragment_binding_from_expr(
    expr: &TemplateExpr,
    locals: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    let env = EvalEnv::from_fragment_context(locals, current_dot);
    let current_dot_helper = current_dot.and_then(fragment_to_helper_binding);
    eval_expr_with_bound_helpers(
        expr,
        &env,
        BoundHelperValueResolverParams {
            fragment_locals: locals,
            outer: None,
            current_dot: current_dot_helper.as_ref(),
            context,
            seen,
            projection: HelperAnalysisProjection::FragmentBinding,
        },
    )
    .and_then(|value| value.to_fragment_binding())
}

pub(crate) fn fragment_binding_from_text_with_helper_context(
    text: &str,
    fragment_locals: &HashMap<String, FragmentBinding>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    let current_dot_fragment = current_dot.map(helper_to_fragment_binding);
    let mut bindings = Vec::new();
    for expr in parse_expr_text(text) {
        if !expr_contains_helper_call(&expr)
            && let Some(binding) = fragment_binding_from_expr(
                &expr,
                fragment_locals,
                current_dot_fragment.as_ref(),
                context,
                seen,
            )
        {
            bindings.push(binding);
            continue;
        }
        if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
            &expr,
            fragment_locals,
            outer,
            current_dot,
            context,
            seen,
        ) {
            bindings.push(helper_to_fragment_binding(&binding));
            continue;
        }
        if let Some(binding) = fragment_binding_from_expr(
            &expr,
            fragment_locals,
            current_dot_fragment.as_ref(),
            context,
            seen,
        ) {
            bindings.push(binding);
        }
    }
    FragmentBinding::choice(bindings)
}
