use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::{GetBinding, GetBindingPlan};
use crate::fragment_assignment::AssignmentKind;
use crate::helper_meta::HelperOutputMeta;
use crate::scalar_value::{
    ScalarValue, ScalarValueDispatch, TruthCondition, conjoin_predicates_with_memo,
};
use crate::symbolic_local_state::SymbolicLocalState;
use helm_schema_core::{GuardValue, Predicate, PredicateMemo, ValuesPath};
use test_util::prelude::sim_assert_eq;

fn state_with_scalar_arm_count(count: usize) -> SymbolicLocalState {
    let mut state = SymbolicLocalState::default();
    state.scalar_dispatches.insert(
        "value".to_string(),
        ScalarValueDispatch {
            arms: (0..count)
                .map(|index| {
                    (
                        Predicate::True,
                        ScalarValue::Literal(GuardValue::String(index.to_string())),
                    )
                })
                .collect(),
            complete: true,
        },
    );
    state
}

fn joined_scalar_dispatches_without_unchanged_shortcut(
    entry: &SymbolicLocalState,
    arms: &[(TruthCondition, SymbolicLocalState)],
    has_unconditional_else: bool,
    memo: &PredicateMemo,
) -> Option<HashMap<String, ScalarValueDispatch>> {
    if arms
        .iter()
        .any(|(condition, _)| matches!(condition, TruthCondition::Unknown))
    {
        return None;
    }
    let mut outcomes = arms.to_vec();
    if !has_unconditional_else {
        outcomes.push((
            TruthCondition::any_with_memo(
                arms.iter().map(|(condition, _)| condition.clone()),
                memo,
            )
            .negated_with_memo(memo),
            entry.clone(),
        ));
    }

    let variables: BTreeSet<&String> = arms
        .iter()
        .flat_map(|(_, state)| state.scalar_dispatches.keys())
        .chain(entry.scalar_dispatches.keys())
        .collect();
    let mut joined = HashMap::new();
    for variable in variables {
        let mut dispatch_arms = Vec::new();
        let mut complete = outcomes
            .iter()
            .all(|(condition, _)| condition.predicate().is_some());
        'outcomes: for (condition, state) in &outcomes {
            let outer_condition = condition.when_true();
            if outer_condition == Predicate::False {
                continue;
            }
            let Some(dispatch) = state.scalar_dispatches.get(variable) else {
                complete = false;
                continue;
            };
            complete &= dispatch.complete;
            for (inner_condition, value) in &dispatch.arms {
                if let Some(condition) = conjoin_predicates_with_memo(
                    outer_condition.clone(),
                    inner_condition.clone(),
                    memo,
                ) {
                    dispatch_arms.push((condition, value.clone()));
                    if dispatch_arms.len() > 128 {
                        break 'outcomes;
                    }
                }
            }
        }
        if dispatch_arms.is_empty() || dispatch_arms.len() > 128 {
            continue;
        }
        joined.insert(
            variable.clone(),
            ScalarValueDispatch {
                arms: dispatch_arms,
                complete,
            },
        );
    }
    Some(joined)
}

#[test]
fn scalar_dispatch_join_keeps_128_arms_and_discards_129() {
    let memo = PredicateMemo::default();
    let entry = SymbolicLocalState::default();
    let mut at_cap = SymbolicLocalState::default();
    at_cap.join_scalar_dispatch_arms(
        &entry,
        &[(
            TruthCondition::exact(Predicate::True),
            state_with_scalar_arm_count(128),
        )],
        true,
        &memo,
    );
    sim_assert_eq!(
        have: at_cap.scalar_dispatches.get("value").map(|dispatch| dispatch.arms.len()),
        want: Some(128)
    );

    let mut over_cap = SymbolicLocalState::default();
    over_cap.join_scalar_dispatch_arms(
        &entry,
        &[(
            TruthCondition::exact(Predicate::True),
            state_with_scalar_arm_count(129),
        )],
        true,
        &memo,
    );
    sim_assert_eq!(have: over_cap.scalar_dispatches.is_empty(), want: true);
}

#[test]
fn unchanged_partial_dispatch_survives_the_join_cap() {
    let memo = PredicateMemo::default();
    let entry = state_with_scalar_arm_count(129);
    let mut partial = entry.clone();
    if let Some(dispatch) = partial.scalar_dispatches.get_mut("value") {
        dispatch.complete = false;
    }
    let entry = partial.clone();
    let mut joined = SymbolicLocalState::default();
    joined.join_scalar_dispatch_arms(
        &entry,
        &[(TruthCondition::exact(Predicate::True), partial.clone())],
        true,
        &memo,
    );
    sim_assert_eq!(
        have: joined.scalar_dispatches.get("value"),
        want: entry.scalar_dispatches.get("value")
    );

    let old = joined_scalar_dispatches_without_unchanged_shortcut(
        &entry,
        &[(TruthCondition::exact(Predicate::True), partial)],
        true,
        &memo,
    );
    sim_assert_eq!(
        have: old.and_then(|dispatches| dispatches.get("value").cloned()),
        want: None
    );
}

#[test]
fn changed_nested_and_missing_dispatches_match_the_old_join() {
    let memo = PredicateMemo::default();
    let entry = state_with_scalar_arm_count(1);
    let outer = Predicate::truthy_path("outer");
    let inner = Predicate::truthy_path("inner");
    let mut nested = SymbolicLocalState::default();
    nested.scalar_dispatches.insert(
        "value".to_string(),
        ScalarValueDispatch {
            arms: vec![
                (
                    inner.clone(),
                    ScalarValue::Literal(GuardValue::String("nested-true".to_string())),
                ),
                (
                    inner.negated(),
                    ScalarValue::Literal(GuardValue::String("nested-false".to_string())),
                ),
            ],
            complete: true,
        },
    );
    let changed_arms = vec![
        (TruthCondition::exact(outer.clone()), nested),
        (
            TruthCondition::exact(outer.negated()),
            state_with_scalar_arm_count(2),
        ),
    ];
    let mut changed = SymbolicLocalState::default();
    changed.join_scalar_dispatch_arms(&entry, &changed_arms, true, &memo);
    sim_assert_eq!(
        have: changed.scalar_dispatches,
        want: joined_scalar_dispatches_without_unchanged_shortcut(
            &entry,
            &changed_arms,
            true,
            &memo
        )
        .unwrap_or_default()
    );

    let missing_arms = vec![
        (
            TruthCondition::exact(Predicate::truthy_path("present")),
            entry.clone(),
        ),
        (
            TruthCondition::exact(Predicate::truthy_path("present").negated()),
            SymbolicLocalState::default(),
        ),
    ];
    let mut missing = SymbolicLocalState::default();
    missing.join_scalar_dispatch_arms(&entry, &missing_arms, true, &memo);
    sim_assert_eq!(
        have: missing.scalar_dispatches,
        want: joined_scalar_dispatches_without_unchanged_shortcut(
            &entry,
            &missing_arms,
            true,
            &memo
        )
        .unwrap_or_default()
    );
}

#[test]
fn snapshot_restore_replaces_all_local_state_maps() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "image".to_string(),
        Some(values_path!("image")),
    );
    state
        .chart_value_defaults
        .insert(ValuesPath::parse("serviceAccount.name"));
    let snapshot = state.clone();
    state.enter_local_scope();

    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "image".to_string(),
        Some(values_path!("otherImage")),
    );
    state.insert_range_domain("key".to_string(), vec!["a".to_string()]);
    state
        .chart_value_defaults
        .insert(ValuesPath::parse("serviceAccount.labels"));
    state.exit_local_scope();

    state = snapshot;

    sim_assert_eq!(
        have: state.fragment_values.get("image"),
        want: Some(&values_path!("image"))
    );
    assert!(state.range_domains.is_empty());
    sim_assert_eq!(
        have: state.chart_value_defaults,
        want: [ValuesPath::parse("serviceAccount.name")]
            .into_iter()
            .collect()
    );
}

#[test]
fn local_scope_restores_shadowed_fragment_value() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(values_path!("outer")),
    );

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(values_path!("inner")),
    );
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.fragment_values.get("name"),
        want: Some(&values_path!("outer"))
    );
}

#[test]
fn local_scope_keeps_assignment_to_outer_fragment_value() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(values_path!("outer")),
    );

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Assignment,
        "name".to_string(),
        Some(values_path!("assigned")),
    );
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.fragment_values.get("name"),
        want: Some(&values_path!("assigned"))
    );
}

#[test]
fn local_scope_restores_shadowed_get_binding() {
    let mut state = SymbolicLocalState::default();
    state.apply_get_binding(get_binding_plan(
        "value",
        AssignmentKind::Declaration,
        "outer",
        "key",
    ));

    state.enter_local_scope();
    state.apply_get_binding(get_binding_plan(
        "value",
        AssignmentKind::Declaration,
        "inner",
        "key",
    ));
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.get_bindings.get("value"),
        want: Some(&get_binding("outer", "key"))
    );
}

#[test]
fn local_scope_keeps_assignment_to_outer_get_binding() {
    let mut state = SymbolicLocalState::default();
    state.apply_get_binding(get_binding_plan(
        "value",
        AssignmentKind::Declaration,
        "outer",
        "key",
    ));

    state.enter_local_scope();
    state.apply_get_binding(get_binding_plan(
        "value",
        AssignmentKind::Assignment,
        "assigned",
        "key",
    ));
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.get_bindings.get("value"),
        want: Some(&get_binding("assigned", "key"))
    );
}

#[test]
fn fragment_assignment_replaces_outer_get_binding() {
    let mut state = SymbolicLocalState::default();
    state.apply_get_binding(get_binding_plan(
        "value",
        AssignmentKind::Declaration,
        "outer",
        "key",
    ));

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Assignment,
        "value".to_string(),
        Some(values_path!("assigned")),
    );
    state.exit_local_scope();

    assert!(!state.get_bindings.contains_key("value"));
    sim_assert_eq!(
        have: state.fragment_values.get("value"),
        want: Some(&values_path!("assigned"))
    );
}

#[test]
fn local_scope_restores_range_domain_shadowing_outer_binding() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "key".to_string(),
        Some(values_path!("outer")),
    );

    state.enter_local_scope();
    state.insert_range_domain("key".to_string(), vec!["inner".to_string()]);
    state.exit_local_scope();

    assert!(!state.range_domains.contains_key("key"));
    sim_assert_eq!(
        have: state.fragment_values.get("key"),
        want: Some(&values_path!("outer"))
    );
}

#[test]
fn local_scope_restores_default_paths_for_shadowed_declaration() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(values_path!("outer")),
    );
    state.default_paths.insert(
        "name".to_string(),
        BTreeSet::from([ValuesPath::parse("outer.default")]),
    );

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(values_path!("inner")),
    );
    state.default_paths.insert(
        "name".to_string(),
        BTreeSet::from([ValuesPath::parse("inner.default")]),
    );
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.default_paths.get("name"),
        want: Some(&BTreeSet::from([ValuesPath::parse("outer.default")]))
    );
}

#[test]
fn local_scope_keeps_default_paths_for_outer_assignment() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(values_path!("outer")),
    );
    state.default_paths.insert(
        "name".to_string(),
        BTreeSet::from([ValuesPath::parse("outer.default")]),
    );

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Assignment,
        "name".to_string(),
        Some(values_path!("assigned")),
    );
    state.default_paths.insert(
        "name".to_string(),
        BTreeSet::from([ValuesPath::parse("assigned.default")]),
    );
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.default_paths.get("name"),
        want: Some(&BTreeSet::from([ValuesPath::parse("assigned.default")]))
    );
}

#[test]
fn local_scope_restores_output_meta_for_shadowed_declaration() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(values_path!("outer")),
    );
    state
        .output_meta
        .insert("name".to_string(), output_meta("outer.output"));

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(values_path!("inner")),
    );
    state
        .output_meta
        .insert("name".to_string(), output_meta("inner.output"));
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.output_meta.get("name"),
        want: Some(&output_meta("outer.output"))
    );
}

#[test]
fn local_scope_keeps_output_meta_for_outer_assignment() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(values_path!("outer")),
    );
    state
        .output_meta
        .insert("name".to_string(), output_meta("outer.output"));

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Assignment,
        "name".to_string(),
        Some(values_path!("assigned")),
    );
    state
        .output_meta
        .insert("name".to_string(), output_meta("assigned.output"));
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.output_meta.get("name"),
        want: Some(&output_meta("assigned.output"))
    );
}

#[test]
fn local_scope_restores_truthy_reduction_for_shadowed_declaration() {
    let mut state = SymbolicLocalState::default();
    state
        .truthy_reductions
        .insert("message".to_string(), Predicate::truthy_path("outer"));

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "message".to_string(),
        Some(AbstractValue::StringSet(
            [String::new()].into_iter().collect(),
        )),
    );
    state
        .truthy_reductions
        .insert("message".to_string(), Predicate::False);
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.truthy_reductions.get("message"),
        want: Some(&Predicate::truthy_path("outer"))
    );
}

#[test]
fn branch_join_keeps_bindings_present_in_all_outcomes() {
    let mut entry = SymbolicLocalState::default();
    entry.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(values_path!("entry")),
    );
    let entry_snapshot = entry.clone();

    let mut first = entry.clone();
    first.bind_fragment_value(
        AssignmentKind::Assignment,
        "name".to_string(),
        Some(values_path!("first")),
    );
    let mut second = entry.clone();
    second.bind_fragment_value(
        AssignmentKind::Assignment,
        "name".to_string(),
        Some(values_path!("second")),
    );

    let mut joined = entry;
    joined.join_branch_outcomes(&entry_snapshot, &[first, second]);

    sim_assert_eq!(
        have: joined.fragment_values.get("name"),
        want: Some(&AbstractValue::Choice(
            [
                values_path!("first"),
                values_path!("second")
            ]
            .into_iter()
            .collect()
        ))
    );
}

#[test]
fn branch_join_unions_truthy_reductions_across_outcomes() {
    let mut entry = SymbolicLocalState::default();
    entry
        .truthy_reductions
        .insert("message".to_string(), Predicate::False);
    let entry_snapshot = entry.clone();

    let mut populated = entry.clone();
    populated
        .truthy_reductions
        .insert("message".to_string(), Predicate::truthy_path("legacy"));

    let mut joined = entry;
    joined.join_branch_outcomes(&entry_snapshot, &[populated, entry_snapshot.clone()]);

    sim_assert_eq!(
        have: joined.truthy_reductions.get("message"),
        want: Some(&Predicate::truthy_path("legacy"))
    );
}

#[test]
fn over_cap_branch_stamp_removes_the_changed_truthy_reduction() {
    let mut entry = SymbolicLocalState::default();
    entry
        .truthy_reductions
        .insert("message".to_string(), Predicate::False);
    let mut branch = entry.clone();
    branch
        .truthy_reductions
        .insert("message".to_string(), Predicate::truthy_path("result"));
    let condition = Predicate::all(
        (0..6)
            .map(|index| Predicate::truthy_path(format!("guard.{index}")))
            .collect(),
    );

    branch.conjoin_changed_truthy_reductions(
        &entry,
        &condition,
        &helm_schema_core::PredicateMemo::new(),
    );
    sim_assert_eq!(have: branch.truthy_reductions.get("message"), want: None);
    assert!(branch.truthiness_abstentions.contains("message"));

    let mut joined = entry.clone();
    joined.join_branch_outcomes(&entry, &[branch, entry.clone()]);
    sim_assert_eq!(have: joined.truthy_reductions.get("message"), want: None);
    assert!(joined.truthiness_abstentions.contains("message"));
}

#[test]
fn exact_if_join_conditions_an_untouched_entry_truthy_reduction() {
    let mut entry = SymbolicLocalState::default();
    entry
        .truthy_reductions
        .insert("continue".to_string(), Predicate::truthy_path("gate"));
    let mut stopped = entry.clone();
    stopped
        .truthy_reductions
        .insert("continue".to_string(), Predicate::False);
    let mut joined = entry.clone();

    joined.join_truthy_reduction_arms(
        &entry,
        &[(
            TruthCondition::exact(Predicate::truthy_path("stop")),
            stopped,
        )],
        false,
        &helm_schema_core::PredicateMemo::new(),
    );

    sim_assert_eq!(
        have: joined.truthy_reductions.get("continue"),
        want: Some(
            &Predicate::all(vec![
                Predicate::truthy_path("gate"),
                Predicate::truthy_path("stop").negated(),
            ])
            .normalize_boolean()
        )
    );
}

#[test]
fn branch_join_intersects_chart_value_defaults() {
    let mut entry = SymbolicLocalState::default();
    entry
        .chart_value_defaults
        .insert(ValuesPath::parse("already.defaulted"));
    let entry_snapshot = entry.clone();

    let mut first = entry.clone();
    first
        .chart_value_defaults
        .insert(ValuesPath::parse("branch.only"));
    let second = entry.clone();

    let mut joined = entry;
    joined.join_branch_outcomes(&entry_snapshot, &[first, second]);

    sim_assert_eq!(
        have: joined.chart_value_defaults,
        want: [ValuesPath::parse("already.defaulted")]
            .into_iter()
            .collect()
    );
}

fn get_binding_plan(
    variable: &str,
    kind: AssignmentKind,
    base: &str,
    key_var: &str,
) -> GetBindingPlan {
    GetBindingPlan {
        variable: variable.to_string(),
        kind,
        binding: get_binding(base, key_var),
    }
}

fn get_binding(base: &str, key_var: &str) -> GetBinding {
    GetBinding {
        base: ValuesPath::parse(base),
        key_var: key_var.to_string(),
    }
}

fn output_meta(path: &str) -> BTreeMap<ValuesPath, HelperOutputMeta> {
    BTreeMap::from([(ValuesPath::parse(path), HelperOutputMeta::default())])
}
