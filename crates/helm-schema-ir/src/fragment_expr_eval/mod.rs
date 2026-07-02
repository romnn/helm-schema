mod bound_helper_resolver;
mod context;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

pub(crate) use context::FragmentEvalContext;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{bindings_for_helper_arg_with, eval_expr};
use crate::helper_summary::HelperOutputMeta;
use bound_helper_resolver::{BoundHelperValueResolverParams, eval_expr_result_with_bound_helpers};

#[derive(Clone, Copy)]
pub(crate) struct FragmentLocalFacts<'a> {
    pub(crate) bindings: &'a HashMap<String, AbstractValue>,
    pub(crate) default_paths: Option<&'a HashMap<String, BTreeSet<String>>>,
    pub(crate) output_meta: Option<&'a HashMap<String, BTreeMap<String, HelperOutputMeta>>>,
}

impl<'a> FragmentLocalFacts<'a> {
    pub(crate) fn bindings_only(bindings: &'a HashMap<String, AbstractValue>) -> Self {
        Self {
            bindings,
            default_paths: None,
            output_meta: None,
        }
    }

    pub(crate) fn with_output_meta(
        bindings: &'a HashMap<String, AbstractValue>,
        default_paths: &'a HashMap<String, BTreeSet<String>>,
        output_meta: &'a HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    ) -> Self {
        Self {
            bindings,
            default_paths: Some(default_paths),
            output_meta: Some(output_meta),
        }
    }

    pub(crate) fn without_output_meta(
        bindings: &'a HashMap<String, AbstractValue>,
        default_paths: &'a HashMap<String, BTreeSet<String>>,
    ) -> Self {
        Self {
            bindings,
            default_paths: Some(default_paths),
            output_meta: None,
        }
    }
}

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
    fragment_locals: FragmentLocalFacts<'_>,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> crate::eval_effect::EvalResult {
    let mut env = EvalEnv::from_helper_context(outer, current_dot);
    env.locals = fragment_locals.bindings.clone();
    if let Some(default_paths) = fragment_locals.default_paths {
        env.local_default_paths = default_paths.clone();
    }
    if let Some(output_meta) = fragment_locals.output_meta {
        env.local_output_meta = output_meta.clone();
    }
    let mut result = eval_expr_result_with_bound_helpers(
        expr,
        &env,
        BoundHelperValueResolverParams {
            fragment_locals: fragment_locals.bindings,
            outer,
            current_dot,
            context,
            seen,
        },
    );
    result.value = result.value.map(|value| value.to_context_value());
    result
}

pub(crate) fn helper_result_from_exprs_with_fragment_locals(
    exprs: &[TemplateExpr],
    fragment_locals: FragmentLocalFacts<'_>,
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
            FragmentLocalFacts::bindings_only(fragment_locals),
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
        },
    )
    .value
    .and_then(AbstractValue::without_widened)
    .map(|value| value.to_context_value())
}
