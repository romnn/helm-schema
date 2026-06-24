use std::collections::BTreeMap;

use helm_schema_ast::{TemplateExpr, parse_action_expressions};
use test_util::prelude::sim_assert_eq;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::EvalResult;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};

struct StaticResolver;

impl HelperCallValueResolver for StaticResolver {
    fn resolve_helper_call(
        &mut self,
        name: &str,
        _arg: Option<&TemplateExpr>,
    ) -> Option<EvalResult> {
        match name {
            "common.name" => Some(EvalResult::from_value(AbstractValue::ValuesPath(
                "nameOverride".to_string(),
            ))),
            "common.labels" => Some(EvalResult::from_value(AbstractValue::Dict(BTreeMap::from(
                [(
                    "app".to_string(),
                    AbstractValue::ValuesPath("labels.app".to_string()),
                )],
            )))),
            _ => None,
        }
    }
}

fn single_expr(action: &str) -> TemplateExpr {
    let exprs = parse_action_expressions(&format!("{{{{ {action} }}}}"));
    sim_assert_eq!(have: exprs.len(), want: 1, "expected exactly one parsed expression");
    exprs.into_iter().next().expect("expression exists")
}

fn eval(action: &str) -> Option<AbstractValue> {
    let mut resolver = StaticResolver;
    eval_expr_with_helper_calls(&single_expr(action), &EvalEnv::default(), &mut resolver).value
}

#[test]
fn dict_value_can_be_nested_helper_call() {
    sim_assert_eq!(
        have: eval(r#"dict "name" (include "common.name" .)"#),
        want: Some(AbstractValue::Dict(BTreeMap::from([(
            "name".to_string(),
            AbstractValue::ValuesPath("nameOverride".to_string()),
        )])))
    );
}

#[test]
fn printf_preserves_nested_helper_provenance_path() {
    sim_assert_eq!(
        have: eval(r#"printf "%s-sfx" (include "common.name" .)"#),
        want: Some(AbstractValue::PathSet(
            ["nameOverride".to_string()].into_iter().collect()
        ))
    );
}

#[test]
fn pipeline_merge_can_consume_nested_helper_call() {
    sim_assert_eq!(
        have: eval(r#"dict "base" "static" | merge (include "common.labels" .)"#),
        want: Some(AbstractValue::Dict(BTreeMap::from([
            (
                "app".to_string(),
                AbstractValue::ValuesPath("labels.app".to_string()),
            ),
            (
                "base".to_string(),
                AbstractValue::StringSet(["static".to_string()].into_iter().collect()),
            ),
        ])))
    );
}

#[test]
fn integer_index_on_values_path_uses_array_item_wildcard_with_helper_context() {
    sim_assert_eq!(
        have: eval(r#"dict "value" (index .Values.items 0) "name" (include "common.name" .)"#),
        want: Some(AbstractValue::Dict(BTreeMap::from([
            (
                "name".to_string(),
                AbstractValue::ValuesPath("nameOverride".to_string()),
            ),
            (
                "value".to_string(),
                AbstractValue::ValuesPath("items.*".to_string()),
            ),
        ])))
    );
}
