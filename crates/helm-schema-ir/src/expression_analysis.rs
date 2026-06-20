use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, type_is_schema_type as eval_type_is_schema_type};
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

pub(crate) fn helper_binding_from_expr(
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

pub(crate) fn helper_bindings_for_arg(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
) -> HashMap<String, AbstractValue> {
    bindings_for_helper_arg_with(arg, outer, |expr| {
        helper_binding_from_expr(expr, outer, current_dot)
    })
}

pub(crate) fn resolved_default_fallback_paths_for_exprs(
    exprs: &[TemplateExpr],
    bindings: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let env = EvalEnv::from_helper_context(bindings, current_dot);

    for expr in exprs {
        out.extend(eval_expr(expr, &env).effects.defaults);
    }

    out
}

pub(crate) fn resolved_default_fallback_paths_for_expr(
    expr: &TemplateExpr,
    bindings: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
) -> BTreeSet<String> {
    let env = EvalEnv::from_helper_context(bindings, current_dot);
    eval_expr(expr, &env).effects.defaults
}

pub(crate) fn resolved_type_hint_paths_for_exprs_with_fragment_locals(
    exprs: &[TemplateExpr],
    bindings: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    fragment_locals: &HashMap<String, AbstractValue>,
) -> BTreeMap<String, BTreeSet<String>> {
    let env =
        EvalEnv::from_helper_context_with_fragment_locals(bindings, current_dot, fragment_locals);
    resolved_type_hint_paths_for_exprs_in_env(exprs, &env)
}

fn resolved_type_hint_paths_for_exprs_in_env(
    exprs: &[TemplateExpr],
    env: &EvalEnv,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for expr in exprs {
        for (path, hints) in eval_expr(expr, env).effects.type_hints {
            out.entry(path).or_default().extend(hints);
        }
    }
    out
}

pub(crate) fn resolved_string_transform_paths_for_exprs_with_fragment_locals(
    exprs: &[TemplateExpr],
    bindings: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    fragment_locals: &HashMap<String, AbstractValue>,
) -> BTreeMap<String, BTreeSet<String>> {
    let env =
        EvalEnv::from_helper_context_with_fragment_locals(bindings, current_dot, fragment_locals);
    resolved_string_transform_paths_for_exprs_in_env(exprs, &env)
}

fn resolved_string_transform_paths_for_exprs_in_env(
    exprs: &[TemplateExpr],
    env: &EvalEnv,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for expr in exprs {
        for path in eval_expr(expr, env).effects.string_hints {
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
pub(crate) fn set_default_chart_paths_for_exprs(
    exprs: &[TemplateExpr],
    bindings: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template_expr_cache::parse_expr_text;
    use test_util::prelude::sim_assert_eq;

    fn expr(text: &str) -> TemplateExpr {
        let exprs = parse_expr_text(text);
        sim_assert_eq!(have: exprs.len(), want: 1, "expected exactly one parsed expression");
        exprs.into_iter().next().expect("expression exists")
    }

    #[test]
    fn helper_value_expression_uses_shared_expression_eval() {
        let bindings = HashMap::from([(
            "ctx".to_string(),
            AbstractValue::Dict(
                [(
                    "config".to_string(),
                    AbstractValue::ValuesPath("serviceAccount".to_string()),
                )]
                .into_iter()
                .collect(),
            ),
        )]);

        sim_assert_eq!(
            have: helper_binding_from_expr(
                &expr(".ctx.config.name | default \"x\""),
                Some(&bindings),
                None
            ),
            want: Some(AbstractValue::Choice(
                [
                    AbstractValue::ValuesPath("serviceAccount.name".to_string()),
                    AbstractValue::StringSet(["x".to_string()].into_iter().collect()),
                ]
                .into_iter()
                .collect(),
            )),
        );
    }

    #[test]
    fn helper_argument_projection_uses_shared_expression_eval() {
        let bindings = helper_bindings_for_arg(
            Some(&expr(r#"dict "ctx" $ "config" .Values.serviceAccount"#)),
            None,
            None,
        );

        sim_assert_eq!(
            have: bindings,
            want: HashMap::from([
                ("ctx".to_string(), AbstractValue::RootContext),
                (
                    "config".to_string(),
                    AbstractValue::ValuesPath("serviceAccount".to_string()),
                ),
            ]),
        );
    }

    #[test]
    fn bound_path_resolution_uses_shared_expression_eval() {
        let bindings = HashMap::from([(
            "config".to_string(),
            AbstractValue::ValuesPath("serviceAccount".to_string()),
        )]);

        sim_assert_eq!(
            have: resolve_expr_to_values_path(&expr(".config.name"), Some(&bindings), None),
            want: Some("serviceAccount.name".to_string()),
        );
    }
}
