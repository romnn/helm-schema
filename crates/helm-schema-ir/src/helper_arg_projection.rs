use std::collections::HashMap;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::template_expr_analysis::is_merge_function;

pub(crate) fn bindings_for_helper_arg_with(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, AbstractValue>>,
    mut eval_binding: impl FnMut(&TemplateExpr) -> Option<AbstractValue>,
) -> HashMap<String, AbstractValue> {
    bindings_for_helper_arg_inner(arg, outer, &mut eval_binding)
}

fn bindings_for_helper_arg_inner(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, AbstractValue>>,
    eval_binding: &mut impl FnMut(&TemplateExpr) -> Option<AbstractValue>,
) -> HashMap<String, AbstractValue> {
    let Some(arg) = arg else {
        return HashMap::new();
    };

    match arg {
        TemplateExpr::Parenthesized(inner) => {
            bindings_for_helper_arg_inner(Some(inner), outer, eval_binding)
        }
        TemplateExpr::Field(path) if path.is_empty() => outer.cloned().unwrap_or_default(),
        TemplateExpr::Variable(var) if var.is_empty() => outer.cloned().unwrap_or_default(),
        TemplateExpr::Call { function, args } if function == "dict" => {
            bindings_from_dict_args(args, eval_binding)
        }
        TemplateExpr::Call { function, args } if is_merge_function(function) => {
            bindings_from_merge_args(args, outer, eval_binding)
        }
        _ => HashMap::new(),
    }
}

fn bindings_from_dict_args(
    args: &[TemplateExpr],
    eval_binding: &mut impl FnMut(&TemplateExpr) -> Option<AbstractValue>,
) -> HashMap<String, AbstractValue> {
    let mut bindings = HashMap::new();
    let mut index = 0usize;
    while index + 1 < args.len() {
        let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) = &args[index]
        else {
            index += 1;
            continue;
        };
        let binding = eval_binding(&args[index + 1]).unwrap_or(AbstractValue::Unknown);
        bindings.insert(key.clone(), binding);
        index += 2;
    }
    bindings
}

fn bindings_from_merge_args(
    args: &[TemplateExpr],
    outer: Option<&HashMap<String, AbstractValue>>,
    eval_binding: &mut impl FnMut(&TemplateExpr) -> Option<AbstractValue>,
) -> HashMap<String, AbstractValue> {
    let mut merged = HashMap::new();
    for arg in args {
        match eval_binding(arg) {
            Some(AbstractValue::Dict(map)) => {
                for (key, value) in map {
                    merged.insert(key, value);
                }
            }
            Some(AbstractValue::RootContext) => {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use helm_schema_ast::parse_action_expressions;

    use super::*;

    fn single_expr(action: &str) -> TemplateExpr {
        let exprs = parse_action_expressions(&format!("{{{{ {action} }}}}"));
        assert_eq!(exprs.len(), 1, "expected exactly one parsed expression");
        exprs.into_iter().next().expect("expression exists")
    }

    fn project(
        action: &str,
        outer: Option<&HashMap<String, AbstractValue>>,
    ) -> HashMap<String, AbstractValue> {
        let expr = single_expr(action);
        project_expr(&expr, outer)
    }

    fn project_expr(
        expr: &TemplateExpr,
        outer: Option<&HashMap<String, AbstractValue>>,
    ) -> HashMap<String, AbstractValue> {
        bindings_for_helper_arg_with(Some(expr), outer, |expr| match expr {
            TemplateExpr::Field(path) => Some(AbstractValue::ValuesPath(path.join("."))),
            TemplateExpr::Variable(var) if var.is_empty() => Some(AbstractValue::RootContext),
            TemplateExpr::Call { function, .. } if function == "fallback" => {
                Some(AbstractValue::Dict(BTreeMap::from([(
                    "fallback".to_string(),
                    AbstractValue::ValuesPath("fallback.value".to_string()),
                )])))
            }
            TemplateExpr::Call { function, .. } if function == "overrideMap" => {
                Some(AbstractValue::Dict(BTreeMap::from([(
                    "fallback".to_string(),
                    AbstractValue::ValuesPath("override".to_string()),
                )])))
            }
            _ => None,
        })
    }

    #[test]
    fn dict_argument_projects_string_and_raw_string_keys() {
        assert_eq!(
            project(r#"dict "name" .serviceAccount.name `raw` .raw"#, None),
            HashMap::from([
                (
                    "name".to_string(),
                    AbstractValue::ValuesPath("serviceAccount.name".to_string()),
                ),
                (
                    "raw".to_string(),
                    AbstractValue::ValuesPath("raw".to_string()),
                ),
            ])
        );
    }

    #[test]
    fn merge_argument_preserves_ordered_overwrite_and_root_context_expansion() {
        let outer = HashMap::from([(
            "root".to_string(),
            AbstractValue::ValuesPath("root.value".to_string()),
        )]);
        let expr = TemplateExpr::Call {
            function: "merge".to_string(),
            args: vec![
                TemplateExpr::Call {
                    function: "fallback".to_string(),
                    args: Vec::new(),
                },
                TemplateExpr::Variable(String::new()),
                TemplateExpr::Call {
                    function: "overrideMap".to_string(),
                    args: Vec::new(),
                },
            ],
        };

        assert_eq!(
            project_expr(&expr, Some(&outer)),
            HashMap::from([
                (
                    "fallback".to_string(),
                    AbstractValue::ValuesPath("override".to_string()),
                ),
                (
                    "root".to_string(),
                    AbstractValue::ValuesPath("root.value".to_string()),
                ),
            ])
        );
    }
}
