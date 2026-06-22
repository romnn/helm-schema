use std::collections::BTreeMap;
use test_util::prelude::sim_assert_eq;

use helm_schema_ast::parse_action_expressions;

use super::*;

fn single_expr(action: &str) -> TemplateExpr {
    let exprs = parse_action_expressions(&format!("{{{{ {action} }}}}"));
    sim_assert_eq!(have: exprs.len(), want: 1, "expected exactly one parsed expression");
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
    sim_assert_eq!(
        have: project(r#"dict "name" .serviceAccount.name `raw` .raw"#, None),
        want: HashMap::from([
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

    sim_assert_eq!(
        have: project_expr(&expr, Some(&outer)),
        want: HashMap::from([
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
