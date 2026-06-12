use std::collections::HashMap;

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::binding::{FragmentBinding, HelperBinding};
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
pub(crate) fn fragment_binding_from_outer_expr(
    expr: &TemplateExpr,
    outer_locals: Option<&HashMap<String, FragmentBinding>>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> Option<FragmentBinding> {
    if matches!(expr, TemplateExpr::Variable(var) if var.is_empty())
        && let Some(bindings) = outer
    {
        return Some(FragmentBinding::Dict(
            bindings
                .iter()
                .map(|(key, binding)| {
                    (
                        key.clone(),
                        AbstractValue::from_helper_binding(binding)
                            .to_fragment_binding()
                            .unwrap_or(FragmentBinding::Unknown),
                    )
                })
                .collect(),
        ));
    }

    let env = EvalEnv::from_outer_fragment_expr_context(outer_locals, outer, current_dot);
    eval_expr(expr, &env)
        .value
        .as_ref()
        .and_then(AbstractValue::to_fragment_binding)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use helm_schema_ast::parse_action_expressions;

    use super::fragment_binding_from_outer_expr;
    use crate::binding::{FragmentBinding, HelperBinding};

    fn single_expr(action: &str) -> helm_schema_ast::TemplateExpr {
        let exprs = parse_action_expressions(&format!("{{{{ {action} }}}}"));
        assert_eq!(exprs.len(), 1, "expected exactly one parsed expression");
        exprs.into_iter().next().expect("expression exists")
    }

    #[test]
    fn outer_expr_bare_dot_uses_root_bindings_as_current_context() {
        let expr = single_expr(".");
        let root_bindings = HashMap::from([(
            "Values".to_string(),
            HelperBinding::ValuesPath(String::new()),
        )]);

        assert_eq!(
            fragment_binding_from_outer_expr(&expr, None, Some(&root_bindings), None),
            Some(FragmentBinding::Dict(BTreeMap::from([(
                "Values".to_string(),
                FragmentBinding::ValuesRoot,
            )])))
        );
    }

    #[test]
    fn outer_expr_root_variable_uses_root_bindings_as_current_context() {
        let expr = single_expr("$");
        let root_bindings = HashMap::from([(
            "Values".to_string(),
            HelperBinding::ValuesPath(String::new()),
        )]);

        assert_eq!(
            fragment_binding_from_outer_expr(&expr, None, Some(&root_bindings), None),
            Some(FragmentBinding::Dict(BTreeMap::from([(
                "Values".to_string(),
                FragmentBinding::ValuesRoot,
            )])))
        );
    }

    #[test]
    fn outer_expr_fragment_local_selector_uses_shared_abstract_eval() {
        let expr = single_expr(r#"dict "name" $ctx.config.name"#);
        let fragment_locals = HashMap::from([(
            "ctx".to_string(),
            FragmentBinding::Dict(BTreeMap::from([(
                "config".to_string(),
                FragmentBinding::ValuesPath("serviceAccount".to_string()),
            )])),
        )]);

        assert_eq!(
            fragment_binding_from_outer_expr(&expr, Some(&fragment_locals), None, None),
            Some(FragmentBinding::Dict(BTreeMap::from([(
                "name".to_string(),
                FragmentBinding::ValuesPath("serviceAccount.name".to_string()),
            )])))
        );
    }
}
