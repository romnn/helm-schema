use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::{GetBinding, GetBindingPlan};
use crate::fragment_assignment::AssignmentKind;
use crate::helper_meta::HelperOutputMeta;
use crate::symbolic_local_state::SymbolicLocalState;
use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

#[test]
fn snapshot_restore_replaces_all_local_state_maps() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "image".to_string(),
        Some(AbstractValue::ValuesPath("image".to_string())),
    );
    state
        .chart_value_defaults
        .insert("serviceAccount.name".to_string());
    let snapshot = state.clone();
    state.enter_local_scope();

    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "image".to_string(),
        Some(AbstractValue::ValuesPath("otherImage".to_string())),
    );
    state.insert_range_domain("key".to_string(), vec!["a".to_string()]);
    state
        .chart_value_defaults
        .insert("serviceAccount.labels".to_string());
    state.exit_local_scope();

    state = snapshot;

    sim_assert_eq!(
        have: state.fragment_values.get("image"),
        want: Some(&AbstractValue::ValuesPath("image".to_string()))
    );
    assert!(state.range_domains.is_empty());
    sim_assert_eq!(
        have: state.chart_value_defaults,
        want: ["serviceAccount.name".to_string()].into_iter().collect()
    );
}

#[test]
fn local_scope_restores_shadowed_fragment_value() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("outer".to_string())),
    );

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("inner".to_string())),
    );
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.fragment_values.get("name"),
        want: Some(&AbstractValue::ValuesPath("outer".to_string()))
    );
}

#[test]
fn local_scope_keeps_assignment_to_outer_fragment_value() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("outer".to_string())),
    );

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Assignment,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("assigned".to_string())),
    );
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.fragment_values.get("name"),
        want: Some(&AbstractValue::ValuesPath("assigned".to_string()))
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
        Some(AbstractValue::ValuesPath("assigned".to_string())),
    );
    state.exit_local_scope();

    assert!(!state.get_bindings.contains_key("value"));
    sim_assert_eq!(
        have: state.fragment_values.get("value"),
        want: Some(&AbstractValue::ValuesPath("assigned".to_string()))
    );
}

#[test]
fn local_scope_restores_range_domain_shadowing_outer_binding() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "key".to_string(),
        Some(AbstractValue::ValuesPath("outer".to_string())),
    );

    state.enter_local_scope();
    state.insert_range_domain("key".to_string(), vec!["inner".to_string()]);
    state.exit_local_scope();

    assert!(!state.range_domains.contains_key("key"));
    sim_assert_eq!(
        have: state.fragment_values.get("key"),
        want: Some(&AbstractValue::ValuesPath("outer".to_string()))
    );
}

#[test]
fn local_scope_restores_default_paths_for_shadowed_declaration() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("outer".to_string())),
    );
    state.default_paths.insert(
        "name".to_string(),
        BTreeSet::from(["outer.default".to_string()]),
    );

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("inner".to_string())),
    );
    state.default_paths.insert(
        "name".to_string(),
        BTreeSet::from(["inner.default".to_string()]),
    );
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.default_paths.get("name"),
        want: Some(&BTreeSet::from(["outer.default".to_string()]))
    );
}

#[test]
fn local_scope_keeps_default_paths_for_outer_assignment() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("outer".to_string())),
    );
    state.default_paths.insert(
        "name".to_string(),
        BTreeSet::from(["outer.default".to_string()]),
    );

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Assignment,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("assigned".to_string())),
    );
    state.default_paths.insert(
        "name".to_string(),
        BTreeSet::from(["assigned.default".to_string()]),
    );
    state.exit_local_scope();

    sim_assert_eq!(
        have: state.default_paths.get("name"),
        want: Some(&BTreeSet::from(["assigned.default".to_string()]))
    );
}

#[test]
fn local_scope_restores_output_meta_for_shadowed_declaration() {
    let mut state = SymbolicLocalState::default();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("outer".to_string())),
    );
    state
        .output_meta
        .insert("name".to_string(), output_meta("outer.output"));

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("inner".to_string())),
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
        Some(AbstractValue::ValuesPath("outer".to_string())),
    );
    state
        .output_meta
        .insert("name".to_string(), output_meta("outer.output"));

    state.enter_local_scope();
    state.bind_fragment_value(
        AssignmentKind::Assignment,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("assigned".to_string())),
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
        Some(AbstractValue::ValuesPath("entry".to_string())),
    );
    let entry_snapshot = entry.clone();

    let mut first = entry.clone();
    first.bind_fragment_value(
        AssignmentKind::Assignment,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("first".to_string())),
    );
    let mut second = entry.clone();
    second.bind_fragment_value(
        AssignmentKind::Assignment,
        "name".to_string(),
        Some(AbstractValue::ValuesPath("second".to_string())),
    );

    let mut joined = entry;
    joined.join_branch_outcomes(&entry_snapshot, &[first, second]);

    sim_assert_eq!(
        have: joined.fragment_values.get("name"),
        want: Some(&AbstractValue::Choice(
            [
                AbstractValue::ValuesPath("first".to_string()),
                AbstractValue::ValuesPath("second".to_string())
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
fn branch_join_intersects_chart_value_defaults() {
    let mut entry = SymbolicLocalState::default();
    entry
        .chart_value_defaults
        .insert("already.defaulted".to_string());
    let entry_snapshot = entry.clone();

    let mut first = entry.clone();
    first.chart_value_defaults.insert("branch.only".to_string());
    let second = entry.clone();

    let mut joined = entry;
    joined.join_branch_outcomes(&entry_snapshot, &[first, second]);

    sim_assert_eq!(
        have: joined.chart_value_defaults,
        want: ["already.defaulted".to_string()].into_iter().collect()
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
        base: base.to_string(),
        key_var: key_var.to_string(),
    }
}

fn output_meta(path: &str) -> BTreeMap<String, HelperOutputMeta> {
    BTreeMap::from([(path.to_string(), HelperOutputMeta::default())])
}
