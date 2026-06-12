use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::binding::HelperBinding;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, type_is_schema_type as eval_type_is_schema_type};
use crate::helper_arg_projection::bindings_for_helper_arg_with;
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

pub(crate) fn helper_binding_from_expr(
    expr: &TemplateExpr,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> Option<HelperBinding> {
    let env = EvalEnv::from_helper_context(bindings, current_dot);
    eval_expr(expr, &env)
        .value
        .as_ref()
        .and_then(|value| value.to_helper_binding())
}

pub(crate) fn helper_bindings_for_arg(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> HashMap<String, HelperBinding> {
    bindings_for_helper_arg_with(arg, outer, |expr| {
        helper_binding_from_expr(expr, outer, current_dot)
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn expr(text: &str) -> TemplateExpr {
        let exprs = parse_expr_text(text);
        assert_eq!(exprs.len(), 1, "expected exactly one parsed expression");
        exprs.into_iter().next().expect("expression exists")
    }

    #[test]
    fn helper_binding_projection_uses_shared_abstract_eval() {
        let bindings = HashMap::from([(
            "ctx".to_string(),
            HelperBinding::Dict(
                [(
                    "config".to_string(),
                    HelperBinding::ValuesPath("serviceAccount".to_string()),
                )]
                .into_iter()
                .collect(),
            ),
        )]);

        assert_eq!(
            helper_binding_from_expr(
                &expr(".ctx.config.name | default \"x\""),
                Some(&bindings),
                None
            ),
            Some(HelperBinding::Choice(
                [
                    HelperBinding::ValuesPath("serviceAccount.name".to_string()),
                    HelperBinding::StringSet(["x".to_string()].into_iter().collect()),
                ]
                .into_iter()
                .collect(),
            )),
        );
    }

    #[test]
    fn helper_argument_projection_uses_shared_abstract_eval() {
        let bindings = helper_bindings_for_arg(
            Some(&expr(r#"dict "ctx" $ "config" .Values.serviceAccount"#)),
            None,
            None,
        );

        assert_eq!(
            bindings,
            HashMap::from([
                ("ctx".to_string(), HelperBinding::RootContext),
                (
                    "config".to_string(),
                    HelperBinding::ValuesPath("serviceAccount".to_string()),
                ),
            ]),
        );
    }

    #[test]
    fn bound_path_resolution_uses_shared_abstract_eval() {
        let bindings = HashMap::from([(
            "config".to_string(),
            HelperBinding::ValuesPath("serviceAccount".to_string()),
        )]);

        assert_eq!(
            resolve_expr_to_values_path(&expr(".config.name"), Some(&bindings), None),
            Some("serviceAccount.name".to_string()),
        );
    }
}
