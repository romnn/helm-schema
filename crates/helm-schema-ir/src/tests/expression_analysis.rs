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
        have: helper_value_from_expr(
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
    let bindings = helper_values_for_arg(
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

#[test]
fn set_default_chart_paths_ignores_unrelated_default_inside_set_rhs() {
    let exprs = parse_expr_text(
        r#"$_ := set .serviceAccount "name" (printf "%s" (.other | default "fallback"))"#,
    );

    sim_assert_eq!(
        have: set_default_chart_paths_for_exprs(
            &exprs,
            None,
            Some(&AbstractValue::ValuesPath(String::new()))
        ),
        want: BTreeSet::new(),
    );
}
