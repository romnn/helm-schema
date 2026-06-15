use std::collections::{BTreeMap, BTreeSet};

use super::SymbolicLocalState;
use crate::binding::FragmentBinding;
use crate::bound_value_analysis::{GetBinding, GetBindingPlan};
use crate::fragment_scope_eval::AssignmentKind;
use crate::helper_analysis::HelperOutputMeta;

#[test]
fn snapshot_restore_replaces_all_local_state_maps() {
    let mut state = SymbolicLocalState::default();
    state.declare_fragment_binding(
        "image".to_string(),
        Some(FragmentBinding::ValuesPath("image".to_string())),
    );
    state
        .chart_value_defaults
        .insert("serviceAccount.name".to_string());
    let snapshot = state.snapshot();
    state.enter_local_scope();

    state.declare_fragment_binding(
        "image".to_string(),
        Some(FragmentBinding::ValuesPath("otherImage".to_string())),
    );
    state.insert_range_domain("key".to_string(), vec!["a".to_string()]);
    state
        .chart_value_defaults
        .insert("serviceAccount.labels".to_string());
    state.exit_local_scope();

    state.restore(snapshot);

    assert_eq!(
        state.fragment_bindings.get("image"),
        Some(&FragmentBinding::ValuesPath("image".to_string()))
    );
    assert!(state.range_domains.is_empty());
    assert_eq!(
        state.chart_value_defaults,
        ["serviceAccount.name".to_string()].into_iter().collect()
    );
}

#[test]
fn local_scope_restores_shadowed_fragment_binding() {
    let mut state = SymbolicLocalState::default();
    state.declare_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("outer".to_string())),
    );

    state.enter_local_scope();
    state.declare_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("inner".to_string())),
    );
    state.exit_local_scope();

    assert_eq!(
        state.fragment_bindings.get("name"),
        Some(&FragmentBinding::ValuesPath("outer".to_string()))
    );
}

#[test]
fn local_scope_keeps_assignment_to_outer_fragment_binding() {
    let mut state = SymbolicLocalState::default();
    state.declare_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("outer".to_string())),
    );

    state.enter_local_scope();
    state.assign_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("assigned".to_string())),
    );
    state.exit_local_scope();

    assert_eq!(
        state.fragment_bindings.get("name"),
        Some(&FragmentBinding::ValuesPath("assigned".to_string()))
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

    assert_eq!(
        state.get_bindings.get("value"),
        Some(&get_binding("outer", "key"))
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

    assert_eq!(
        state.get_bindings.get("value"),
        Some(&get_binding("assigned", "key"))
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
    state.assign_fragment_binding(
        "value".to_string(),
        Some(FragmentBinding::ValuesPath("assigned".to_string())),
    );
    state.exit_local_scope();

    assert!(!state.get_bindings.contains_key("value"));
    assert_eq!(
        state.fragment_bindings.get("value"),
        Some(&FragmentBinding::ValuesPath("assigned".to_string()))
    );
}

#[test]
fn local_scope_restores_range_domain_shadowing_outer_binding() {
    let mut state = SymbolicLocalState::default();
    state.declare_fragment_binding(
        "key".to_string(),
        Some(FragmentBinding::ValuesPath("outer".to_string())),
    );

    state.enter_local_scope();
    state.insert_range_domain("key".to_string(), vec!["inner".to_string()]);
    state.exit_local_scope();

    assert!(!state.range_domains.contains_key("key"));
    assert_eq!(
        state.fragment_bindings.get("key"),
        Some(&FragmentBinding::ValuesPath("outer".to_string()))
    );
}

#[test]
fn local_scope_restores_default_paths_for_shadowed_declaration() {
    let mut state = SymbolicLocalState::default();
    state.declare_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("outer".to_string())),
    );
    state.set_default_paths("name", BTreeSet::from(["outer.default".to_string()]));

    state.enter_local_scope();
    state.declare_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("inner".to_string())),
    );
    state.set_default_paths("name", BTreeSet::from(["inner.default".to_string()]));
    state.exit_local_scope();

    assert_eq!(
        state.default_paths.get("name"),
        Some(&BTreeSet::from(["outer.default".to_string()]))
    );
}

#[test]
fn local_scope_keeps_default_paths_for_outer_assignment() {
    let mut state = SymbolicLocalState::default();
    state.declare_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("outer".to_string())),
    );
    state.set_default_paths("name", BTreeSet::from(["outer.default".to_string()]));

    state.enter_local_scope();
    state.assign_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("assigned".to_string())),
    );
    state.set_default_paths("name", BTreeSet::from(["assigned.default".to_string()]));
    state.exit_local_scope();

    assert_eq!(
        state.default_paths.get("name"),
        Some(&BTreeSet::from(["assigned.default".to_string()]))
    );
}

#[test]
fn local_scope_restores_output_meta_for_shadowed_declaration() {
    let mut state = SymbolicLocalState::default();
    state.declare_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("outer".to_string())),
    );
    state.set_output_meta("name".to_string(), output_meta("outer.output"));

    state.enter_local_scope();
    state.declare_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("inner".to_string())),
    );
    state.set_output_meta("name".to_string(), output_meta("inner.output"));
    state.exit_local_scope();

    assert_eq!(
        state.output_meta.get("name"),
        Some(&output_meta("outer.output"))
    );
}

#[test]
fn local_scope_keeps_output_meta_for_outer_assignment() {
    let mut state = SymbolicLocalState::default();
    state.declare_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("outer".to_string())),
    );
    state.set_output_meta("name".to_string(), output_meta("outer.output"));

    state.enter_local_scope();
    state.assign_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("assigned".to_string())),
    );
    state.set_output_meta("name".to_string(), output_meta("assigned.output"));
    state.exit_local_scope();

    assert_eq!(
        state.output_meta.get("name"),
        Some(&output_meta("assigned.output"))
    );
}

#[test]
fn branch_join_keeps_bindings_present_in_all_outcomes() {
    let mut entry = SymbolicLocalState::default();
    entry.declare_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("entry".to_string())),
    );
    let entry_snapshot = entry.snapshot();

    let mut first = entry.clone();
    first.assign_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("first".to_string())),
    );
    let mut second = entry.clone();
    second.assign_fragment_binding(
        "name".to_string(),
        Some(FragmentBinding::ValuesPath("second".to_string())),
    );

    let mut joined = entry;
    joined.join_branch_outcomes(&entry_snapshot, vec![first.snapshot(), second.snapshot()]);

    assert_eq!(
        joined.fragment_bindings.get("name"),
        Some(&FragmentBinding::Choice(
            [
                FragmentBinding::ValuesPath("first".to_string()),
                FragmentBinding::ValuesPath("second".to_string())
            ]
            .into_iter()
            .collect()
        ))
    );
}

#[test]
fn branch_join_intersects_chart_value_defaults() {
    let mut entry = SymbolicLocalState::default();
    entry
        .chart_value_defaults
        .insert("already.defaulted".to_string());
    let entry_snapshot = entry.snapshot();

    let mut first = entry.clone();
    first.chart_value_defaults.insert("branch.only".to_string());
    let second = entry.clone();

    let mut joined = entry;
    joined.join_branch_outcomes(&entry_snapshot, vec![first.snapshot(), second.snapshot()]);

    assert_eq!(
        joined.chart_value_defaults,
        ["already.defaulted".to_string()].into_iter().collect()
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
