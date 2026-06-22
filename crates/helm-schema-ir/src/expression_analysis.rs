use std::collections::{BTreeSet, HashMap};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::helper_arg_projection::bindings_for_helper_arg_with;
use crate::value_path_extraction::values_path_from_expr;

pub(crate) fn resolve_expr_to_values_path(
    expr: &TemplateExpr,
    bindings: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
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

pub(crate) fn helper_value_from_expr(
    expr: &TemplateExpr,
    bindings: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
) -> Option<AbstractValue> {
    let env = EvalEnv::from_helper_context(bindings, current_dot);
    eval_expr(expr, &env)
        .value
        .as_ref()
        .map(|value| value.to_context_value())
}

pub(crate) fn helper_values_for_arg(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
) -> HashMap<String, AbstractValue> {
    bindings_for_helper_arg_with(arg, outer, |expr| {
        helper_value_from_expr(expr, outer, current_dot)
    })
}

/// Extract Values-rooted paths that helper-body text declares as chart-level
/// defaults via the canonical `set OPERAND "KEY" (OPERAND.KEY | default V)`
/// pattern.
///
/// This models a render-time mutation: matching helper bodies default the
/// values object before later template reads. A bare `set X "K" V` without a
/// fallback does not prove that the original values input accepts null, so it
/// intentionally does not match.
pub(crate) fn set_default_chart_paths_for_exprs(
    exprs: &[TemplateExpr],
    bindings: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
) -> BTreeSet<String> {
    let env = EvalEnv::from_helper_context(bindings, current_dot);

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
    for expr in exprs {
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
            let defaulted_paths = eval_expr(&args[2], &env).effects.defaults;
            if !defaulted_paths.contains(&target_path) {
                return;
            }
            out.insert(target_path);
        });
    }
    out
}

#[cfg(test)]
#[path = "tests/expression_analysis.rs"]
mod tests;
