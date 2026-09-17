mod bound_helper_resolver;
mod context;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use helm_schema_ast::TemplateExpr;
use helm_schema_core::PredicateMemo;

pub(crate) use context::FragmentEvalContext;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::EvalResult;
use crate::eval_env::{EvalEnv, LocalBinding};
use crate::expr_eval::eval_expr;
use crate::helper_meta::HelperOutputMeta;
use bound_helper_resolver::{BoundHelperValueResolverParams, eval_expr_result_with_bound_helpers};

pub(crate) fn context_value_from_outer_expr(
    expr: &TemplateExpr,
    outer_locals: Option<&HashMap<String, LocalBinding>>,
    outer_output_meta: Option<
        &HashMap<String, BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    >,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    predicate_memo: &Rc<PredicateMemo>,
) -> Option<AbstractValue> {
    if matches!(expr, TemplateExpr::Variable(var) if var.is_empty()) {
        if let Some(binding) = outer_locals.and_then(|locals| locals.get("")) {
            return binding.value(predicate_memo);
        }
        return Some(AbstractValue::RootContext);
    }
    if matches!(expr, TemplateExpr::Field(path) if path.is_empty()) && current_dot.is_none() {
        return Some(AbstractValue::RootContext);
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
        dot_binding_mode: crate::eval_env::BindingEvaluationMode::Direct,
        root_fields: outer.cloned().unwrap_or_default(),
        locals: outer_locals.cloned().unwrap_or_default(),
        local_output_meta: outer_output_meta.cloned().unwrap_or_default(),
        allow_field_root_lookup: true,
        predicate_memo: Rc::clone(predicate_memo),
        ..EvalEnv::default()
    };
    let result = eval_expr(expr, &env);
    let mut output_meta = result.effects.local_output_meta.clone();
    for path in &result.effects.yaml_serialized_paths {
        output_meta.entry(path.clone()).or_default().yaml_serialized = true;
    }
    result
        .value
        .map(|value| value.with_output_meta(&output_meta))
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
    template_bindings: &HashMap<String, LocalBinding>,
    template_output_meta: &HashMap<
        String,
        BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>,
    >,
    fragment_context: FragmentEvalContext<'_>,
    current_dot_fragment: Option<&AbstractValue>,
) -> Option<AbstractValue> {
    let locals = locals_with_roots(template_bindings, root_bindings);

    context_value_from_outer_expr(
        expr,
        Some(&locals),
        Some(template_output_meta),
        Some(root_bindings),
        current_dot_fragment,
        fragment_context.analysis_db.predicate_memo(),
    )
    .or_else(|| {
        fragment_context.fragment_value_from_expr_with_meta(
            expr,
            template_bindings,
            template_output_meta,
            current_dot_fragment,
            &mut HashSet::new(),
        )
    })
}

pub(crate) fn locals_with_roots(
    template_bindings: &HashMap<String, LocalBinding>,
    root_bindings: &HashMap<String, AbstractValue>,
) -> HashMap<String, LocalBinding> {
    let mut locals = template_bindings.clone();
    for (key, value) in root_bindings {
        // A template binding keeps its name: `$patch` is the range member
        // even when the helper's call dict carries a same-named `patch`
        // field (nats' jsonpatch engine) — variable lookups never resolve
        // to dot fields, which ride the dot/root-fields channels instead.
        locals
            .entry(key.clone())
            .or_insert_with(|| LocalBinding::direct(value.to_context_value()));
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
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> EvalResult {
    eval_expr_result_with_bound_helpers(
        expr,
        env,
        BoundHelperValueResolverParams {
            outer,
            current_dot,
            context,
            seen,
        },
    )
}
