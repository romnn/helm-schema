use helm_schema_core::{Predicate, PredicateMemo};
use test_util::prelude::sim_assert_eq;

use crate::abstract_value::AbstractValue;
use crate::fragment_assignment::AssignmentKind;
use crate::value::{RootSlot, State};

fn root(value: &str) -> RootSlot {
    RootSlot::new(
        Some(AbstractValue::StringSet([value.to_owned()].into())),
        None,
        None,
    )
}

fn bind(state: &mut State, value: &str) {
    state.bind_fragment_value(
        AssignmentKind::Declaration,
        "local".to_owned(),
        Some(AbstractValue::StringSet([value.to_owned()].into())),
    );
}

#[test]
fn local_and_root_restores_preserve_the_other_namespace() {
    let mut entry = State::default();
    bind(&mut entry, "before");
    entry.write_root("field".to_owned(), root("before"));
    let mut changed = entry.fork();
    bind(&mut changed, "after");
    changed.write_root("field".to_owned(), root("after"));

    let mut expected = entry.fork();
    expected.write_root("field".to_owned(), root("after"));
    let mut local_restore = changed.fork();
    local_restore.restore_locals(entry.fork_locals());
    sim_assert_eq!(have: local_restore, want: expected);

    let mut expected = entry.fork();
    bind(&mut expected, "after");
    changed.restore_roots(&entry.fork_roots());
    sim_assert_eq!(have: changed, want: expected);
}

#[test]
fn local_join_and_scope_exit_preserve_root_writes() {
    let mut entry = State::default();
    bind(&mut entry, "before");
    let mut state = entry.fork();
    state.enter_local_scope();
    bind(&mut state, "shadow");
    state.write_root("field".to_owned(), root("after"));
    state.exit_local_scope();
    let mut expected = entry.fork();
    expected.write_root("field".to_owned(), root("after"));
    sim_assert_eq!(have: state.snapshot_id(), want: expected.snapshot_id());
    state.join(&entry, &[entry.fork_locals()]);
    sim_assert_eq!(have: state, want: expected);
}

#[test]
fn root_join_retains_ordered_replay_and_exact_scalar_arms() {
    let mut state = State::default();
    let entry = state.fork_roots();
    state.write_root("field".to_owned(), root("left"));
    let left = state.fork_roots();
    state.restore_roots(&entry);
    state.write_root("field".to_owned(), root("right"));
    let right = state.fork_roots();
    state.restore_roots(&entry);
    let condition = Predicate::truthy_path("enabled");
    state.join_root_set_arms(
        &entry,
        &[
            (condition.clone(), true, left),
            (condition.negated(), true, right),
        ],
        true,
    );
    let memo = PredicateMemo::default();
    let expected = crate::scalar_value::ScalarValueDispatch::bind_content(
        vec![
            (
                Predicate::truthy_path("enabled"),
                crate::scalar_value::ScalarContent::Literal(helm_schema_core::GuardValue::string(
                    "left",
                )),
            ),
            (
                Predicate::truthy_path("enabled").negated(),
                crate::scalar_value::ScalarContent::Literal(helm_schema_core::GuardValue::string(
                    "right",
                )),
            ),
        ],
        true,
    );
    sim_assert_eq!(
        have: state.read_root("field").and_then(|slot| slot.binding().dispatch.as_ref()),
        want: Some(&expected)
    );
    sim_assert_eq!(
        have: state.read_root("field").and_then(|slot| slot.binding().truth.clone()).map(|truth| memo.normalize(truth)),
        want: Some(Predicate::True)
    );
}
