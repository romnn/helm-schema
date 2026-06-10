use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::binding::HelperBinding;
use crate::helper_analysis::insert_type_hint;
use crate::helper_binding_eval::binding_from_expr;
use crate::template_expr_cache::parse_expr_text;
use crate::walker::values_path_from_expr;

pub(crate) fn resolve_expr_to_values_path(
    expr: &TemplateExpr,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> Option<String> {
    if let Some(path) = values_path_from_expr(expr) {
        return Some(path);
    }

    match binding_from_expr(expr, bindings, current_dot) {
        Some(HelperBinding::ValuesPath(path)) => Some(path),
        _ => None,
    }
}

pub(crate) fn resolve_expr_to_values_paths(
    expr: &TemplateExpr,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> BTreeSet<String> {
    if let Some(path) = values_path_from_expr(expr) {
        return [path].into_iter().collect();
    }

    binding_from_expr(expr, bindings, current_dot)
        .map(|binding| binding.paths())
        .unwrap_or_default()
}

pub(crate) fn resolved_default_fallback_paths_for_text(
    text: &str,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();

    for expr in parse_expr_text(text) {
        out.extend(resolved_default_fallback_paths_for_expr(
            &expr,
            bindings,
            current_dot,
        ));
    }

    out
}

pub(crate) fn resolved_default_fallback_paths_for_expr(
    expr: &TemplateExpr,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    expr.walk(|node| match node {
        TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
            out.extend(resolve_expr_to_values_paths(
                &args[1],
                bindings,
                current_dot,
            ));
        }
        TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
            for window in stages.windows(2) {
                let TemplateExpr::Call { function, .. } = &window[1] else {
                    continue;
                };
                if function != "default" {
                    continue;
                }
                out.extend(resolve_expr_to_values_paths(
                    &window[0],
                    bindings,
                    current_dot,
                ));
            }
        }
        _ => {}
    });
    out
}

pub(crate) fn resolved_type_is_paths_for_text(
    text: &str,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for expr in parse_expr_text(text) {
        expr.walk(|node| {
            let TemplateExpr::Call { function, args } = node else {
                return;
            };
            if function != "typeIs" || args.len() < 2 {
                return;
            }
            let Some(schema_type) = type_is_schema_type(args.first()) else {
                return;
            };
            let Some(binding) = binding_from_expr(&args[1], bindings, current_dot) else {
                return;
            };
            for path in binding.paths() {
                if !path.trim().is_empty() {
                    out.entry(path).or_default().insert(schema_type.clone());
                }
            }
        });
    }
    out
}

pub(crate) fn resolved_string_transform_paths_for_text(
    text: &str,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for expr in parse_expr_text(text) {
        resolved_string_transform_paths_for_expr(&expr, bindings, current_dot, &mut out);
    }
    out
}

fn resolved_string_transform_paths_for_expr(
    expr: &TemplateExpr,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    out: &mut BTreeMap<String, BTreeSet<String>>,
) {
    match expr {
        TemplateExpr::Parenthesized(inner) => {
            resolved_string_transform_paths_for_expr(inner, bindings, current_dot, out);
        }
        TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
            let mut current = stages.first();
            for stage in &stages[1..] {
                let TemplateExpr::Call { function, .. } = stage else {
                    continue;
                };
                if is_string_transform_function(function)
                    && let Some(current) = current
                {
                    for path in resolve_expr_to_values_paths(current, bindings, current_dot) {
                        insert_type_hint(out, path, "string");
                    }
                }
                current = Some(stage);
            }
        }
        TemplateExpr::Call { function, args } if is_string_transform_function(function) => {
            for arg in args {
                for path in resolve_expr_to_values_paths(arg, bindings, current_dot) {
                    insert_type_hint(out, path, "string");
                }
                resolved_string_transform_paths_for_expr(arg, bindings, current_dot, out);
            }
        }
        TemplateExpr::Call { args, .. } => {
            for arg in args {
                resolved_string_transform_paths_for_expr(arg, bindings, current_dot, out);
            }
        }
        TemplateExpr::Selector { operand, .. } => {
            resolved_string_transform_paths_for_expr(operand, bindings, current_dot, out);
        }
        _ => {}
    }
}

fn is_string_transform_function(function: &str) -> bool {
    matches!(
        function,
        "quote"
            | "squote"
            | "toString"
            | "trunc"
            | "trim"
            | "trimAll"
            | "trimPrefix"
            | "trimSuffix"
            | "replace"
    )
}

pub(crate) fn type_is_schema_type(expr: Option<&TemplateExpr>) -> Option<String> {
    let TemplateExpr::Literal(Literal::String(type_name) | Literal::RawString(type_name)) =
        expr?.deparen()
    else {
        return None;
    };
    let schema_type = match type_name.as_str() {
        "bool" | "boolean" => "boolean",
        "float64" | "number" => "number",
        "int" | "int64" | "integer" => "integer",
        "list" | "slice" | "array" => "array",
        "map" | "dict" | "object" => "object",
        "string" => "string",
        _ => return None,
    };
    Some(schema_type.to_string())
}

/// Extract Values-rooted paths that helper-body text declares as chart-level
/// defaults via the canonical `set OPERAND "KEY" (OPERAND.KEY | default V)`
/// pattern.
///
/// This models a render-time mutation: matching helper bodies default the
/// values object before later template reads. A bare `set X "K" V` without a
/// fallback does not prove that the original values input accepts null, so it
/// intentionally does not match.
pub(crate) fn set_default_chart_paths_for_text(
    text: &str,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> BTreeSet<String> {
    fn literal_string(expr: &TemplateExpr) -> Option<&str> {
        match expr {
            TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
                Some(value)
            }
            TemplateExpr::Parenthesized(inner) => literal_string(inner),
            _ => None,
        }
    }

    let mut out = BTreeSet::new();
    for expr in parse_expr_text(text) {
        expr.walk(|node| {
            let TemplateExpr::Call { function, args } = node else {
                return;
            };
            if function != "set" || args.len() != 3 {
                return;
            }
            let Some(operand_path) = resolve_expr_to_values_path(&args[0], bindings, current_dot)
            else {
                return;
            };
            let Some(key) = literal_string(&args[1]) else {
                return;
            };
            let target_path = if operand_path.is_empty() {
                key.to_string()
            } else {
                format!("{operand_path}.{key}")
            };
            let defaulted_paths =
                resolved_default_fallback_paths_for_expr(&args[2], bindings, current_dot);
            if !defaulted_paths.contains(&target_path) {
                return;
            }
            out.insert(target_path);
        });
    }
    out
}
