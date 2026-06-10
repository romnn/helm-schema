use std::collections::HashMap;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::binding::HelperBinding;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::template_expr_analysis::is_merge_function;
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
    let Some(arg) = arg else {
        return HashMap::new();
    };

    match arg {
        TemplateExpr::Parenthesized(inner) => {
            bindings_for_helper_arg(Some(inner), outer, current_dot)
        }
        TemplateExpr::Field(path) if path.is_empty() => outer.cloned().unwrap_or_default(),
        TemplateExpr::Variable(var) if var.is_empty() => outer.cloned().unwrap_or_default(),
        TemplateExpr::Call { function, args } if function == "dict" => {
            let mut bindings = HashMap::new();
            let mut index = 0usize;
            while index + 1 < args.len() {
                let TemplateExpr::Literal(Literal::String(key)) = &args[index] else {
                    index += 1;
                    continue;
                };
                let binding = binding_from_expr(&args[index + 1], outer, current_dot)
                    .unwrap_or(HelperBinding::Unknown);
                bindings.insert(key.clone(), binding);
                index += 2;
            }
            bindings
        }
        TemplateExpr::Call { function, args } if is_merge_function(function) => {
            let mut merged = HashMap::new();
            for arg in args {
                match binding_from_expr(arg, outer, current_dot) {
                    Some(HelperBinding::Dict(map)) => {
                        for (key, value) in map {
                            merged.insert(key, value);
                        }
                    }
                    Some(HelperBinding::RootContext) => {
                        if let Some(outer) = outer {
                            for (key, value) in outer {
                                merged.insert(key.clone(), value.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            merged
        }
        _ => HashMap::new(),
    }
}
