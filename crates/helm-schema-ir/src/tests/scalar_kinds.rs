use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::{TemplateExpr, parse_action_expressions};
use helm_schema_core::{GuardValue, Predicate, PredicateMemo};
use test_util::prelude::sim_assert_eq;

use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::scalar_value::{ScalarContent, ScalarValueDispatch};
use crate::value::kinds::GoType;

fn expression(source: &str) -> eyre::Result<TemplateExpr> {
    parse_action_expressions(&format!("{{{{ {source} }}}}"))
        .into_iter()
        .next()
        .ok_or_eyre("missing expression")
}

#[test]
fn runtime_types_are_stamped_at_literal_and_semver_production() -> eyre::Result<()> {
    for (source, expected) in [
        ("true", GoType::Bool),
        ("1", GoType::Int),
        ("1.0", GoType::Float64),
        ("\"one\"", GoType::String),
        ("nil", GoType::Nil),
        ("(semver \"1.2.3\").Major", GoType::Uint64),
    ] {
        let result = eval_expr(&expression(source)?, &EvalEnv::default());
        let dispatch = result
            .scalar_transport()
            .ok_or_eyre("missing typed transport")?;
        sim_assert_eq!(have: dispatch.arms.iter().map(|(_, leaf)| leaf.go_type).collect::<Vec<_>>(), want: vec![expected]);
    }
    Ok(())
}

#[test]
fn type_only_projection_is_absent_but_legacy_empty_projection_stays_present() {
    let dispatch = ScalarValueDispatch::typed(GoType::Float64);
    sim_assert_eq!(have: dispatch.content().is_none(), want: true);
    let mut empty = ScalarValueDispatch::bind_content(Vec::new(), false);
    empty.refine_type(GoType::String);
    sim_assert_eq!(have: empty.content().map(|content| (content.arms().count(), content.complete)), want: Some((0, false)));
}

#[test]
fn type_refinement_preserves_all_non_type_result_facts() {
    let memo = PredicateMemo::default();
    let mut result =
        crate::eval_effect::EvalResult::none().with_truth_with_memo(Predicate::True, &memo);
    let before = result.clone();
    result.refine_go_type(GoType::Float64);
    sim_assert_eq!(have: &result.value, want: &before.value);
    sim_assert_eq!(have: &result.effects, want: &before.effects);
    sim_assert_eq!(have: &result.truth, want: &before.truth);
    sim_assert_eq!(have: &result.selection_reachability, want: &before.selection_reachability);
    sim_assert_eq!(have: &result.json_payload_truth, want: &before.json_payload_truth);
    sim_assert_eq!(have: &result.field_scalar_dispatches, want: &before.field_scalar_dispatches);
    sim_assert_eq!(have: &result.proven_operands, want: &before.proven_operands);
    sim_assert_eq!(have: result.scalar_dispatch().is_none(), want: true);
    sim_assert_eq!(have: result.scalar_transport().is_some(), want: true);
}

#[test]
fn conflicting_types_join_without_changing_equal_content() -> eyre::Result<()> {
    let mut literal = ScalarValueDispatch::constant(GuardValue::Int(1));
    literal.refine_type(GoType::Int);
    let mut component = ScalarValueDispatch::constant(GuardValue::Int(1));
    component.refine_type(GoType::Uint64);
    sim_assert_eq!(have: literal.content_eq(&component), want: true);
    sim_assert_eq!(have: literal == component, want: false);
    let joined = literal
        .join_equal_content([&component])
        .ok_or_eyre("content join failed")?;
    sim_assert_eq!(have: joined.content_eq(&literal), want: true);
    let types = joined
        .arms
        .iter()
        .map(|(_, leaf)| leaf.go_type)
        .collect::<Vec<_>>();
    sim_assert_eq!(have: types, want: vec![GoType::Unknown]);
    sim_assert_eq!(have: joined.constant_value(), want: Some(GuardValue::Int(1)));
    Ok(())
}

#[test]
fn type_refinement_keeps_content_and_its_incomplete_coverage() {
    let mut dispatch = ScalarValueDispatch::bind_content(
        vec![(
            Predicate::truthy_path("enabled"),
            ScalarContent::Literal(GuardValue::Int(1)),
        )],
        false,
    );
    dispatch.refine_type(GoType::Int);
    sim_assert_eq!(have: dispatch.content().map(|content| (content.arms().count(), content.complete)), want: Some((1, false)));
    sim_assert_eq!(
        have: dispatch,
        want: ScalarValueDispatch {
            arms: vec![(
                Predicate::truthy_path("enabled"),
                super::ScalarValue {
                    go_type: GoType::Int,
                    content: Some(ScalarContent::Literal(GuardValue::Int(1))),
                },
            )],
            complete: false,
        }
    );
}

#[test]
fn refining_partial_content_does_not_change_an_exhausted_transform_projection() {
    let mut dispatch = ScalarValueDispatch::bind_content(
        vec![(
            Predicate::truthy_path("enabled"),
            ScalarContent::Length(Box::new(ScalarContent::Identity(
                helm_schema_core::ValuesPath::parse("input"),
            ))),
        )],
        false,
    );
    dispatch.refine_type(GoType::Int);
    sim_assert_eq!(have: dispatch.stringified().content().map(|content| (content.arms().count(), content.complete)), want: Some((0, false)));
    sim_assert_eq!(have: dispatch.printf_string("%s").is_none(), want: true);
}

#[test]
fn exhausting_type_only_coverage_keeps_legacy_absence() -> eyre::Result<()> {
    let memo = PredicateMemo::default();
    let outcomes = (0..129)
        .map(|index| {
            let mut state = crate::value::State::default();
            crate::tests::bind_test_dispatch(
                state.slot_mut("value"),
                Some(ScalarValueDispatch::typed(GoType::Int)),
            );
            let condition = Predicate::from(helm_schema_core::Guard::Eq {
                path: helm_schema_core::ValuesPath::parse("branch"),
                value: GuardValue::Int(index),
            });
            (crate::scalar_value::TruthCondition::exact(condition), state)
        })
        .collect::<Vec<_>>();
    let entry = crate::value::State::default();
    let mut joined = entry.clone();
    joined.join_scalar_dispatch_arms(&entry, &outcomes, true, &memo);
    let view = joined.read_local("value");
    let dispatch = view
        .scalar_dispatch
        .as_ref()
        .ok_or_eyre("missing widened type transport")?;
    sim_assert_eq!(have: dispatch.content().is_none(), want: true);
    sim_assert_eq!(have: dispatch.arms.iter().map(|(_, leaf)| leaf.go_type).collect::<Vec<_>>(), want: vec![GoType::Unknown]);
    Ok(())
}

#[test]
fn exhausting_type_only_coverage_retains_content_guards() -> eyre::Result<()> {
    let memo = PredicateMemo::default();
    let outcomes = (0..130)
        .map(|index| {
            let mut state = crate::value::State::default();
            let dispatch = if index == 0 {
                ScalarValueDispatch::constant(GuardValue::Int(1))
            } else {
                ScalarValueDispatch::typed(GoType::Float64)
            };
            crate::tests::bind_test_dispatch(state.slot_mut("value"), Some(dispatch));
            let condition = Predicate::from(helm_schema_core::Guard::Eq {
                path: helm_schema_core::ValuesPath::parse("branch"),
                value: GuardValue::Int(index),
            });
            (crate::scalar_value::TruthCondition::exact(condition), state)
        })
        .collect::<Vec<_>>();
    let entry = crate::value::State::default();
    let mut joined = entry.clone();
    joined.join_scalar_dispatch_arms(&entry, &outcomes, true, &memo);
    let view = joined.read_local("value");
    let dispatch = view
        .scalar_dispatch
        .as_ref()
        .ok_or_eyre("missing retained content")?;
    let content = dispatch
        .content()
        .ok_or_eyre("missing content projection")?;
    sim_assert_eq!(have: content.complete, want: false);
    sim_assert_eq!(
        have: content.arms().map(|(condition, content)| (condition.clone(), content.clone())).collect::<Vec<_>>(),
        want: vec![(Predicate::from(helm_schema_core::Guard::Eq { path: helm_schema_core::ValuesPath::parse("branch"), value: GuardValue::Int(0) }), ScalarContent::Literal(GuardValue::Int(1)))],
    );
    Ok(())
}

#[test]
fn a_type_only_helper_shadow_preserves_legacy_binding_and_scope_updates() {
    use crate::fragment_assignment::AssignmentKind;
    use crate::value::State;
    let mut entry = State::default();
    entry.bind_fragment_value(
        AssignmentKind::Declaration,
        "value".to_string(),
        Some(crate::abstract_value::AbstractValue::Dict(
            std::collections::BTreeMap::from([(
                "a".to_string(),
                crate::abstract_value::AbstractValue::Unknown,
            )]),
        )),
    );
    entry
        .slot_mut("value")
        .bind_truth(AssignmentKind::Declaration, Some(Predicate::True), false);
    let mut expected = entry.clone();
    let mut actual = entry;
    for (state, scalar_dispatch) in [
        (&mut expected, None),
        (
            &mut actual,
            Some(ScalarValueDispatch::typed(GoType::Float64)),
        ),
    ] {
        state.enter_local_scope();
        state.bind(
            AssignmentKind::Declaration,
            "value",
            crate::value::AssignmentBinding {
                binding: None,
                scalar_dispatch,
                truthy_reduction: Some(Predicate::False),
                helper_scope: true,
                self_preserving: false,
            },
        );
        state.exit_local_scope();
    }
    sim_assert_eq!(have: actual, want: expected);
}
