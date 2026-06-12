use std::collections::HashMap;

use helm_schema_ast::TemplateExpr;

use crate::binding::HelperBinding;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::helper_arg_projection::bindings_for_helper_arg_with;
use crate::walker::values_path_from_expr;

pub(crate) fn resolve_bound_path_expr(
    expr: &TemplateExpr,
    bindings: &HashMap<String, HelperBinding>,
) -> Option<String> {
    if let Some(path) = values_path_from_expr(expr) {
        return Some(path);
    }

    match expr {
        TemplateExpr::Parenthesized(inner) => resolve_bound_path_expr(inner, bindings),
        TemplateExpr::Field(path) => resolve_bound_segments(path, bindings),
        TemplateExpr::Selector { operand, path } => {
            if let TemplateExpr::Variable(var) = operand.as_ref()
                && var.is_empty()
                && let Some((head, tail)) = path.split_first()
                && let Some(binding) = bindings.get(head)
            {
                return binding.apply_unique_path(tail);
            }
            if let Some(binding) = binding_from_expr(operand, Some(bindings), None) {
                return binding.apply_unique_path(path);
            }
            None
        }
        _ => None,
    }
}

fn resolve_bound_segments(
    segments: &[String],
    bindings: &HashMap<String, HelperBinding>,
) -> Option<String> {
    let binding = binding_from_bound_segments(segments, bindings)?;
    let paths = binding.paths();
    let mut paths = paths.into_iter();
    let first = paths.next()?;
    if paths.next().is_none() {
        Some(first)
    } else {
        None
    }
}

fn binding_from_bound_segments(
    segments: &[String],
    bindings: &HashMap<String, HelperBinding>,
) -> Option<HelperBinding> {
    let (first, rest) = segments.split_first()?;
    let binding = bindings.get(first)?;
    binding.apply_to_binding(rest)
}

pub(crate) fn binding_from_expr(
    expr: &TemplateExpr,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> Option<HelperBinding> {
    let env = EvalEnv::from_helper_context(outer, current_dot);
    eval_expr(expr, &env)
        .value
        .as_ref()
        .and_then(|value| value.to_helper_binding())
}

pub(crate) fn bindings_for_helper_arg(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> HashMap<String, HelperBinding> {
    bindings_for_helper_arg_with(arg, outer, |expr| {
        binding_from_expr(expr, outer, current_dot)
    })
}
