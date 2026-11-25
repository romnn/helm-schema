use std::collections::BTreeSet;
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use super::{JoinObject, join_object};
use crate::value::work::WorkBudget;
use crate::value::{
    Coverage, Field, Multiplicity, ObjectData, ObjectVersion, Remainder, SnapshotId, State, Value,
};

#[test]
fn exhausted_join_of_widened_operands_keeps_unknown_provenance_local() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let object = state.map_target(&map).ok_or_eyre("map missing")?;
    let other = state.heap.map(Remainder::Closed);
    let other_id = state.map_target(&other).ok_or_eyre("other map missing")?;
    state
        .heap
        .write(&other_id, "saved", Field::Present(Value::string("exact")));
    let yes = state.heap.fork();
    let no = state.heap.fork();
    let predicate = Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let widened = |ordinal, path| {
        Some(Rc::new(ObjectVersion {
            revision: SnapshotId { ordinal },
            multiplicity: Multiplicity::Singleton,
            data: ObjectData::Widened {
                inputs: Rc::new(BTreeSet::from([ValuesPath::parse(path)])),
                reachable: crate::value::Reach::Bounded(std::rc::Rc::default()),
            },
        }))
    };
    let paths = state.heap.arena.input_paths.borrow();
    let joined = join_object(
        &JoinObject {
            id: SnapshotId { ordinal: 100 },
            arena: &state.heap.arena,
            paths: &paths,
            decision: &decision,
            yes: &yes.version,
            no: &no.version,
            object: &object,
        },
        widened(1, "left"),
        widened(2, "right"),
        &mut WorkBudget::new(0),
    )
    .ok_or_eyre("bounded join dropped the object")?;
    // Both operands are already widened, so exhaustion must not erase their dependencies.
    sim_assert_eq!(have: &joined.data, want: &ObjectData::Widened { inputs: Rc::new(BTreeSet::from([ValuesPath::default()])), reachable: crate::value::Reach::Bounded(std::rc::Rc::default()) });
    sim_assert_eq!(have: state.heap.version.field(&other_id, "saved"), want: Field::Present(Value::string("exact")));
    Ok(())
}

#[test]
fn widened_join_reuses_union_when_original_path_is_already_present() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let object = state.map_target(&map).ok_or_eyre("map missing")?;
    let source = ValuesPath::parse("source");
    state.heap.register_input(object.clone(), source.clone());
    let yes = state.heap.fork();
    let no = state.heap.fork();
    let predicate = Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let left = Rc::new(BTreeSet::from([source.clone(), ValuesPath::parse("left")]));
    let right = Rc::new(BTreeSet::from([source.clone(), ValuesPath::parse("right")]));
    let storage = |ordinal, inputs| {
        Some(Rc::new(ObjectVersion {
            revision: SnapshotId { ordinal },
            multiplicity: Multiplicity::Singleton,
            data: ObjectData::Widened {
                inputs,
                reachable: crate::value::Reach::Bounded(std::rc::Rc::default()),
            },
        }))
    };
    let paths = state.heap.arena.input_paths.borrow();
    let joined = join_object(
        &JoinObject {
            id: SnapshotId { ordinal: 100 },
            arena: &state.heap.arena,
            paths: &paths,
            decision: &decision,
            yes: &yes.version,
            no: &no.version,
            object: &object,
        },
        storage(1, Rc::clone(&left)),
        storage(2, Rc::clone(&right)),
        &mut WorkBudget::new(crate::value::work::OBJECT_WORK),
    )
    .ok_or_eyre("joined object missing")?;
    let ObjectData::Widened {
        inputs,
        reachable: _,
    } = &joined.data
    else {
        eyre::bail!("join changed its storage shape");
    };
    sim_assert_eq!(have: inputs.as_ref(), want: &BTreeSet::from([source, ValuesPath::parse("left"), ValuesPath::parse("right")]));
    let cached = crate::value::heap::join_input_sets(
        &left,
        &right,
        Some(&state.heap.arena.input_unions),
        &mut WorkBudget::new(64),
    );
    assert!(
        Rc::ptr_eq(inputs, &cached),
        "an existing original path must preserve the union's weak memo"
    );
    Ok(())
}
