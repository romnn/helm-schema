use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::{TemplateExpr, parse_action_expressions};
use helm_schema_core::{Guard, GuardValue, Predicate};
use test_util::prelude::sim_assert_eq;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::EvalResult;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::scalar_value::{ScalarValue, ScalarValueDispatch};

struct StaticResolver;

impl HelperCallValueResolver for StaticResolver {
    fn resolve_helper_call(
        &mut self,
        name: &str,
        _arg: Option<&TemplateExpr>,
    ) -> Option<EvalResult> {
        match name {
            "common.name" => Some(EvalResult::from_value(values_path!("nameOverride"))),
            "common.labels" => Some(EvalResult::from_value(AbstractValue::Dict(BTreeMap::from(
                [("app".to_string(), values_path!("labels.app"))],
            )))),
            "partial.feature" => Some(EvalResult::none().with_scalar_dispatch(
                ScalarValueDispatch {
                    arms: vec![(
                        Predicate::truthy_path("feature.gate"),
                        ScalarValue::Literal(GuardValue::string("true")),
                    )],
                    complete: false,
                },
            )),
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
            values_path!("nameOverride"),
        )])))
    );
}

#[test]
fn printf_preserves_nested_helper_provenance_path() {
    sim_assert_eq!(
        have: eval(r#"printf "%s-sfx" (include "common.name" .)"#),
        want: Some(values_path!("nameOverride"))
    );
}

#[test]
fn pipeline_merge_can_consume_nested_helper_call() {
    sim_assert_eq!(
        have: eval(r#"dict "base" "static" | merge (include "common.labels" .)"#),
        want: Some(AbstractValue::Dict(BTreeMap::from([
            (
                "app".to_string(),
                values_path!("labels.app"),
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
                values_path!("nameOverride"),
            ),
            (
                "value".to_string(),
                values_path!("items.*"),
            ),
        ])))
    );
}

#[test]
fn partial_helper_truth_marks_later_short_circuit_member_access_incomplete() {
    let mut resolver = StaticResolver;
    let result = eval_expr_with_helper_calls(
        &single_expr(r#"and (eq (include "partial.feature" .) "true") .Values.host.member"#),
        &EvalEnv::default(),
        &mut resolver,
    );
    let member_capture = result
        .effects
        .observed_facts
        .captures
        .iter()
        .find(|capture| {
            matches!(
                capture.kind,
                crate::eval_effect::CaptureKind::MemberAccess { .. }
            )
        })
        .map(|capture| capture.conjunction.iter().cloned().collect::<BTreeSet<_>>());

    sim_assert_eq!(
        have: member_capture,
        want: Some(BTreeSet::from([
            Predicate::approximate_with_sound_subset(
                "and operand execution",
                BTreeSet::from(["feature.gate".to_string()]),
                vec![Guard::Truthy {
                    path: helm_schema_core::ValuesPath::parse("feature.gate"),
                }],
            ),
            Predicate::from(Guard::TypeIs {
                path: helm_schema_core::ValuesPath::parse("host"),
                schema_type: "object".to_string(),
            })
            .negated(),
        ])),
    );
    sim_assert_eq!(have: result.truth.predicate(), want: None);
    sim_assert_eq!(
        have: result.truth.when_true(),
        want: Predicate::all(vec![
            Predicate::truthy_path("feature.gate"),
            Predicate::truthy_path("host.member"),
        ]),
    );
}
