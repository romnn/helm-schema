use super::*;
use test_util::prelude::sim_assert_eq;

#[test]
fn branch_join_restores_control_state_to_entry() {
    let mut state = SymbolicScopeState::default();
    state.push_predicate_if_absent(Predicate::truthy_path("enabled"));
    state.push_dot_binding(Some(AbstractValue::ValuesPath("root".to_string())));
    let entry = state.snapshot();

    state.push_predicate_if_absent(Predicate::truthy_path("branch"));
    state.push_dot_binding(Some(AbstractValue::ValuesPath("branch".to_string())));
    let branch = state.snapshot();

    state.restore(entry.clone());
    state.join_branch_outcomes(&entry, vec![branch]);

    sim_assert_eq!(
        have: state.contract_guards(),
        want: vec![Guard::Truthy {
            path: "enabled".to_string()
        }]
    );
    sim_assert_eq!(
        have: state.current_dot_fragment(),
        want: Some(AbstractValue::ValuesPath("root".to_string()))
    );
}

#[test]
fn branch_join_still_joins_local_state() {
    let mut state = SymbolicScopeState::default();
    let entry = state.snapshot();

    state.locals_mut().insert_range_domain(
        "scope".to_string(),
        vec!["frontend".to_string(), "backend".to_string()],
    );
    let branch = state.snapshot();

    state.restore(entry.clone());
    state
        .locals_mut()
        .insert_range_domain("scope".to_string(), vec!["frontend".to_string()]);
    let other_branch = state.snapshot();

    state.restore(entry.clone());
    state.join_branch_outcomes(&entry, vec![branch, other_branch]);

    sim_assert_eq!(
        have: state.locals().range_domains.get("scope"),
        want: Some(&vec!["backend".to_string(), "frontend".to_string()])
    );
}
