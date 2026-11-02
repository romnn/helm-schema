use std::collections::BTreeSet;
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use super::InputWrites;
use crate::value::{
    Coverage, Field, Multiplicity, ObjectData, ObjectId, ObjectVersion, Remainder, State, Value,
};

fn path(name: &str) -> ValuesPath {
    ValuesPath::parse(name)
}
fn prefixes(names: &[&str]) -> Rc<BTreeSet<ValuesPath>> {
    Rc::new(names.iter().map(|name| path(name)).collect())
}
fn input_object(state: &mut State) -> ObjectId {
    state.heap.allocate(
        ObjectData::Map {
            fields: Rc::default(),
            remainder: Remainder::Input,
        },
        Multiplicity::Singleton,
    )
}
fn baseline(state: &State) -> ObjectVersion {
    ObjectVersion {
        revision: state.heap.arena.snapshot(),
        multiplicity: Multiplicity::Singleton,
        data: ObjectData::Map {
            fields: Rc::default(),
            remainder: Remainder::Input,
        },
    }
}

#[test]
fn registration_and_baseline_storage_are_not_mutations() {
    let mut state = State::new(Value::nil());
    let object = input_object(&mut state);
    state.heap.register_input(object.clone(), path("config"));
    state
        .heap
        .register_storage(object.clone(), baseline(&state));
    sim_assert_eq!(have: &state.heap.version.index.input_writes.entries, want: &prefixes(&[]));
    state.heap.install(object, baseline(&state));
    sim_assert_eq!(have: &state.heap.version.index.input_writes.entries, want: &prefixes(&["config"]));
}

#[test]
fn writes_and_top_havocs_keep_their_input_prefixes() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let object = input_object(&mut state);
    state.heap.register_input(object.clone(), path("config"));
    let mut yes = state.heap.fork();
    let mut no = state.heap.fork();
    assert!(yes.write(&object, "nested", Field::Present(Value::nil())));
    sim_assert_eq!(have: &yes.version.index.input_writes.entries, want: &prefixes(&["config.nested"]));
    sim_assert_eq!(have: yes.version.index.changes_inputs(), want: true);
    no.unknown_write(Rc::default());
    sim_assert_eq!(have: &no.version.index.input_writes.entries, want: &prefixes(&[""]));
    let predicate = Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    state.heap.join(&decision, &yes, &no)?;
    sim_assert_eq!(have: &state.heap.version.index.input_writes.entries, want: &prefixes(&[""]));
    Ok(())
}

#[test]
fn original_aliases_registered_before_or_after_a_write_keep_both_paths() {
    for late in [false, true] {
        let mut state = State::new(Value::nil());
        let object = input_object(&mut state);
        state
            .heap
            .register_input(object.clone(), path("global.config"));
        if !late {
            state
                .heap
                .register_input(object.clone(), path("child.global.config"));
        }
        assert!(
            state
                .heap
                .write(&object, "port", Field::Present(Value::nil()))
        );
        if late {
            state
                .heap
                .register_input(object, path("child.global.config"));
        }
        sim_assert_eq!(have: &state.heap.version.index.input_writes.entries, want: &prefixes(&["global.config.port", "child.global.config.port"]));
    }
}

#[test]
fn joins_and_linear_events_synchronize_original_paths_from_other_branches() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let object = input_object(&mut state);
    state.heap.register_input(object.clone(), path("original"));
    let mut other = state.heap.fork();
    assert!(
        state
            .heap
            .write(&object, "first", Field::Present(Value::nil()))
    );
    other.register_input(object.clone(), path("alias"));
    let mut yes = state.heap.fork();
    assert!(yes.write(&object, "second", Field::Present(Value::nil())));
    sim_assert_eq!(have: &yes.version.index.input_writes.entries, want: &prefixes(&["original.first", "alias.first", "original.second", "alias.second"]));
    let predicate = Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let original = state.heap.fork();
    state.heap.join(&decision, &original, &other)?;
    sim_assert_eq!(have: &state.heap.version.index.input_writes.entries, want: &prefixes(&["original.first", "alias.first"]));
    Ok(())
}

#[test]
fn first_registration_recovers_writes_before_materialization() {
    let mut state = State::new(Value::nil());
    let object = input_object(&mut state);
    assert!(
        state
            .heap
            .write(&object, "port", Field::Present(Value::nil()))
    );
    state.heap.register_input(object, path("config"));
    sim_assert_eq!(have: &state.heap.version.index.input_writes.entries, want: &prefixes(&["config"]));
}

#[test]
fn root_rebinding_does_not_change_captured_original_objects() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let root = state.heap.map(Remainder::Closed);
    let target = state.map_target(&root).ok_or_eyre("root target")?;
    let old = input_object(&mut state);
    state
        .heap
        .register_input(old.clone(), ValuesPath::default());
    let captured = Value::leaf(crate::value::Datum::Map {
        view: crate::value::MapType::Values,
        target: Some(old.clone()),
    });
    let replacement = state.heap.map(Remainder::Closed);
    assert!(
        state
            .heap
            .write(&target, "Values", Field::Present(replacement))
    );
    sim_assert_eq!(have: state.map_target(&captured), want: Some(old));
    sim_assert_eq!(have: &state.heap.version.index.input_writes.entries, want: &prefixes(&[]));
    Ok(())
}

#[test]
fn prefix_subsumption_and_the_sixty_four_entry_bound_are_conservative() {
    let mut writes = InputWrites::default();
    writes.add(&path("config.child"));
    writes.add(&path("config"));
    sim_assert_eq!(have: &writes.entries, want: &prefixes(&["config"]));
    for index in 0..65 {
        writes.add(&path(&format!("key{index}")));
    }
    sim_assert_eq!(have: &writes.entries, want: &prefixes(&[""]));
}
