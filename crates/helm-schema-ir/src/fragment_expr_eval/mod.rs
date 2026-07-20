mod bound_helper_resolver;
mod context;

use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

pub(crate) use context::FragmentEvalContext;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::EvalResult;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use bound_helper_resolver::{BoundHelperValueResolverParams, eval_expr_result_with_bound_helpers};

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

    // An active with/range dot is the current context. Outside a nested
    // context, the caller's root bindings provide the helper-call dot; `$`
    // still resolves explicitly through the early root-variable arm above.
    let dot = current_dot.cloned().or_else(|| {
        outer.map(|bindings| {
            AbstractValue::Dict(
                bindings
                    .iter()
                    .map(|(name, binding)| (name.clone(), binding.clone()))
                    .collect(),
            )
        })
    });
    let env = EvalEnv {
        dot,
        root_fields: outer.cloned().unwrap_or_default(),
        locals: outer_locals.cloned().unwrap_or_default(),
        allow_field_root_lookup: true,
        ..EvalEnv::default()
    };
    eval_expr(expr, &env)
        .value
        .map(|value| value.to_context_value())
}

/// The fragment-context value of an expression: the plain outer-fragment
/// evaluation first, then the bound-helper-resolving fragment evaluation as
/// the fallback. This is the composition behind with-body dots and `hasKey`
/// probes.
///
/// The first arm uses the active helper dot when one exists, while `$` and
/// root fields keep resolving through `root_bindings`.
pub(crate) fn fragment_context_value(
    expr: &TemplateExpr,
    root_bindings: &HashMap<String, AbstractValue>,
    template_bindings: &HashMap<String, AbstractValue>,
    fragment_context: FragmentEvalContext<'_>,
    current_dot_fragment: Option<&AbstractValue>,
) -> Option<AbstractValue> {
    let locals = locals_with_roots(template_bindings, root_bindings);

    context_value_from_outer_expr(
        expr,
        Some(&locals),
        Some(root_bindings),
        current_dot_fragment,
    )
    .or_else(|| {
        fragment_context.fragment_value_from_expr(
            expr,
            template_bindings,
            current_dot_fragment,
            &mut HashSet::new(),
        )
    })
}

pub(crate) fn locals_with_roots(
    template_bindings: &HashMap<String, AbstractValue>,
    root_bindings: &HashMap<String, AbstractValue>,
) -> HashMap<String, AbstractValue> {
    let mut locals = template_bindings.clone();
    for (key, value) in root_bindings {
        locals.insert(key.clone(), value.to_context_value());
    }
    locals
}

/// Expression evaluation for the fragment interpreter: the caller supplies
/// the fully-populated environment (locals, bound values, local default/meta
/// facts) and this resolves bound helper calls through the memoized
/// in-domain summaries.
pub(crate) fn document_result_from_expr(
    expr: &TemplateExpr,
    env: &EvalEnv,
    fragment_locals: &HashMap<String, AbstractValue>,
    outer: Option<&HashMap<String, AbstractValue>>,
    outer_root_facts: crate::analysis_db::OuterRootFacts<'_>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> EvalResult {
    eval_expr_result_with_bound_helpers(
        expr,
        env,
        BoundHelperValueResolverParams {
            fragment_locals,
            outer,
            outer_root_facts,
            current_dot,
            context,
            seen,
        },
    )
}

pub(crate) fn helper_result_from_expr_with_fragment_locals(
    expr: &TemplateExpr,
    fragment_locals: &HashMap<String, AbstractValue>,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> EvalResult {
    let mut env = EvalEnv::from_helper_context(outer, current_dot);
    env.locals = fragment_locals.clone();
    let mut result = eval_expr_result_with_bound_helpers(
        expr,
        &env,
        BoundHelperValueResolverParams {
            fragment_locals,
            outer,
            outer_root_facts: crate::analysis_db::OuterRootFacts::default(),
            current_dot,
            context,
            seen,
        },
    );
    result.value = result.value.map(|value| value.to_context_value());
    result
}
