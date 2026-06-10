use std::collections::{BTreeMap, HashMap};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::binding::HelperBinding;
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
    if let Some(path) = values_path_from_expr(expr) {
        return Some(HelperBinding::ValuesPath(path));
    }

    match expr {
        TemplateExpr::Parenthesized(inner) => binding_from_expr(inner, outer, current_dot),
        TemplateExpr::Field(path) if path.is_empty() => {
            current_dot.cloned().or(Some(HelperBinding::RootContext))
        }
        TemplateExpr::Variable(var) if var.is_empty() => Some(HelperBinding::RootContext),
        TemplateExpr::Variable(_) => None,
        TemplateExpr::Selector { operand, path } => {
            let binding = binding_from_expr(operand, outer, current_dot)?;
            binding.apply_to_binding(path)
        }
        TemplateExpr::Call { function, args } if matches!(function.as_str(), "list" | "tuple") => {
            let mut items = Vec::new();
            for arg in args {
                items.push(
                    binding_from_expr(arg, outer, current_dot).unwrap_or(HelperBinding::Unknown),
                );
            }
            Some(HelperBinding::List(items))
        }
        TemplateExpr::Call { function, args } if function == "dict" => {
            let mut map = BTreeMap::new();
            let mut index = 0usize;
            while index + 1 < args.len() {
                let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                    &args[index]
                else {
                    index += 1;
                    continue;
                };
                let binding = binding_from_expr(&args[index + 1], outer, current_dot)
                    .unwrap_or(HelperBinding::Unknown);
                map.insert(key.clone(), binding);
                index += 2;
            }
            Some(HelperBinding::Dict(map))
        }
        TemplateExpr::Call { function, args } if is_merge_function(function) => {
            let mut bindings = Vec::new();
            for arg in args {
                if let Some(binding) = binding_from_expr(arg, outer, current_dot) {
                    bindings.push(binding);
                }
            }
            HelperBinding::merge_all(bindings)
        }
        TemplateExpr::Call { function, args } if function == "coalesce" => {
            let mut choices = Vec::new();
            for arg in args {
                if let Some(binding) = binding_from_expr(arg, outer, current_dot) {
                    choices.push(binding);
                }
            }
            HelperBinding::choice(choices)
        }
        TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
            let mut choices = Vec::new();
            if let Some(primary) = binding_from_expr(&args[1], outer, current_dot) {
                choices.push(primary);
            }
            if let Some(fallback) = binding_from_expr(&args[0], outer, current_dot) {
                choices.push(fallback);
            }
            HelperBinding::choice(choices)
        }
        TemplateExpr::Call { function, args } if function == "ternary" => {
            let mut choices = Vec::new();
            for arg in args.iter().take(2) {
                if let Some(binding) = binding_from_expr(arg, outer, current_dot) {
                    choices.push(binding);
                }
            }
            HelperBinding::choice(choices)
        }
        TemplateExpr::Pipeline(stages) => {
            let mut current = binding_from_expr(&stages[0], outer, current_dot);
            for stage in &stages[1..] {
                let TemplateExpr::Call { function, args } = stage else {
                    continue;
                };
                current = match function.as_str() {
                    "default" => {
                        let mut choices = Vec::new();
                        if let Some(current) = current {
                            choices.push(current);
                        }
                        for arg in args {
                            if let Some(binding) = binding_from_expr(arg, outer, current_dot) {
                                choices.push(binding);
                            }
                        }
                        HelperBinding::choice(choices)
                    }
                    function if is_merge_function(function) => {
                        let mut bindings = Vec::new();
                        if let Some(current) = current {
                            bindings.push(current);
                        }
                        for arg in args {
                            if let Some(binding) = binding_from_expr(arg, outer, current_dot) {
                                bindings.push(binding);
                            }
                        }
                        HelperBinding::merge_all(bindings)
                    }
                    "toYaml" | "fromYaml" | "quote" | "toString" | "deepCopy" | "tpl"
                    | "nindent" | "indent" => current,
                    _ => None,
                };
            }
            current
        }
        TemplateExpr::Call { function, args } if function == "index" => {
            let mut binding = binding_from_expr(args.first()?, outer, current_dot)?;
            for arg in &args[1..] {
                let segment = match arg {
                    TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
                        value.clone()
                    }
                    TemplateExpr::Literal(Literal::Int(value)) => value.to_string(),
                    _ => return None,
                };
                binding = match &binding {
                    HelperBinding::ValuesPath(_)
                    | HelperBinding::RootContext
                    | HelperBinding::Unknown
                    | HelperBinding::OutputSet(_)
                    | HelperBinding::PathSet(_)
                    | HelperBinding::Dict(_)
                    | HelperBinding::List(_)
                    | HelperBinding::Overlay { .. }
                    | HelperBinding::Choice(_) => binding.apply_to_binding(&[segment])?,
                };
            }
            Some(binding)
        }
        TemplateExpr::Field(path) => {
            if let Some(bound) =
                outer.and_then(|bindings| binding_from_bound_segments(path, bindings))
            {
                return Some(bound);
            }
            if let Some(current_dot) = current_dot
                && let Some(bound) = current_dot.apply_to_binding(path)
            {
                return Some(bound);
            }
            None
        }
        _ => outer
            .and_then(|bindings| resolve_bound_path_expr(expr, bindings))
            .map(HelperBinding::ValuesPath),
    }
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
