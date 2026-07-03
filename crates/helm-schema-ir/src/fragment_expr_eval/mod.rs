mod bound_helper_resolver;
mod context;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

pub(crate) use context::FragmentEvalContext;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::EvalResult;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
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

    // The outer-fragment dot is the dict of outer bindings when they exist
    // (so a bare `.` sees the caller's root context), else the ambient dot.
    let dot = outer
        .map(|bindings| {
            AbstractValue::Dict(
                bindings
                    .iter()
                    .map(|(name, binding)| (name.clone(), binding.clone()))
                    .collect(),
            )
        })
        .or_else(|| current_dot.cloned());
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
/// probes at both walkers.
///
/// The first arm binds `outer`, so its dot is always the dict of root
/// bindings; an ambient helper dot cannot reach it (hence no ambient-dot
/// parameter here).
pub(crate) fn fragment_context_value(
    expr: &TemplateExpr,
    root_bindings: &HashMap<String, AbstractValue>,
    template_bindings: &HashMap<String, AbstractValue>,
    fragment_context: FragmentEvalContext<'_>,
    current_dot_fragment: Option<&AbstractValue>,
) -> Option<AbstractValue> {
    let locals = locals_with_roots(template_bindings, root_bindings);

    context_value_from_outer_expr(expr, Some(&locals), Some(root_bindings), None).or_else(|| {
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

pub(crate) fn helper_result_from_expr_with_fragment_locals(
    expr: &TemplateExpr,
    fragment_locals: FragmentLocalFacts<'_>,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> EvalResult {
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
