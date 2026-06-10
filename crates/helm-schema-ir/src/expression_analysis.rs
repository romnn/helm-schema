use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::binding::HelperBinding;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, type_is_schema_type as eval_type_is_schema_type};
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

    let env = EvalEnv::from_helper_context(bindings, current_dot);
    eval_expr(expr, &env)
        .value
        .as_ref()
        .and_then(|value| value.unique_path())
}

pub(crate) fn resolved_default_fallback_paths_for_text(
    text: &str,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let env = EvalEnv::from_helper_context(bindings, current_dot);

    for expr in parse_expr_text(text) {
        out.extend(eval_expr(&expr, &env).effects.defaults);
    }

    out
}

pub(crate) fn resolved_default_fallback_paths_for_expr(
    expr: &TemplateExpr,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> BTreeSet<String> {
    let env = EvalEnv::from_helper_context(bindings, current_dot);
    eval_expr(expr, &env).effects.defaults
}

pub(crate) fn resolved_type_is_paths_for_text(
    text: &str,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let env = EvalEnv::from_helper_context(bindings, current_dot);
    for expr in parse_expr_text(text) {
        for (path, hints) in eval_expr(&expr, &env).effects.type_hints {
            out.entry(path).or_default().extend(hints);
        }
    }
    out
}

pub(crate) fn resolved_string_transform_paths_for_text(
    text: &str,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let env = EvalEnv::from_helper_context(bindings, current_dot);
    for expr in parse_expr_text(text) {
        for path in eval_expr(&expr, &env).effects.string_hints {
            out.entry(path).or_default().insert("string".to_string());
        }
    }
    out
}

pub(crate) fn type_is_schema_type(expr: Option<&TemplateExpr>) -> Option<String> {
    eval_type_is_schema_type(expr)
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
