use crate::value::{Remainder, State, Value};
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{Predicate, ValuesPath};
use std::collections::BTreeSet;
use std::rc::Rc;
use test_util::prelude::sim_assert_eq;

/// Preserves the widened-region and later-write law from `runtime_heap_coverage`.
#[test]
fn widened_coverage_keeps_both_inputs_and_later_known_write_provenance() -> eyre::Result<()> {
    use crate::value::{Coverage, Field};

    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let object = state.map_target(&map).ok_or_eyre("map missing")?;
    state
        .heap
        .register_input(object.clone(), ValuesPath::parse("source"));
    let mut yes = state.heap.fork();
    let mut no = state.heap.fork();
    yes.unknown_write(Rc::new(BTreeSet::from([ValuesPath::parse("left")])));
    no.unknown_write(Rc::new(BTreeSet::from([ValuesPath::parse("right")])));
    let predicate = Predicate::truthy_path("choose");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    state.heap.join(&decision, &yes, &no)?;
    let mut reads = crate::value::read_set::ReadSet::default();
    let root = &state.heap.version;
    super::project(&Predicate::True, &root.input_paths, &[root], &mut reads);
    sim_assert_eq!(have: reads.iter().map(|read| read.path.clone()).collect::<BTreeSet<_>>(), want: BTreeSet::from([
        ValuesPath::parse("source"), ValuesPath::parse("left"), ValuesPath::parse("right"),
    ]));
    state
        .heap
        .register_input(object.clone(), ValuesPath::parse("retargeted"));
    let root = &state.heap.version;
    let mut reads = crate::value::read_set::ReadSet::default();
    super::project(&Predicate::True, &root.input_paths, &[root], &mut reads);
    sim_assert_eq!(have: reads.iter().map(|read| read.path.clone()).collect::<BTreeSet<_>>(), want: BTreeSet::from([
        ValuesPath::parse("source"), ValuesPath::parse("retargeted"), ValuesPath::parse("left"), ValuesPath::parse("right"),
    ]));
    assert!(yes.write(
        &object,
        "known",
        Field::Present(Value::input(ValuesPath::parse("later")))
    ));
    state.heap.join(&decision, &yes, &no)?;
    let root = &state.heap.version;
    let mut reads = crate::value::read_set::ReadSet::default();
    super::project(&Predicate::True, &root.input_paths, &[root], &mut reads);
    sim_assert_eq!(have: reads.iter().map(|read| read.path.clone()).collect::<BTreeSet<_>>(), want: BTreeSet::from([
        ValuesPath::parse("retargeted"), ValuesPath::parse("left"), ValuesPath::parse("right"), ValuesPath::parse("later"),
    ]));
    Ok(())
}

/// Earlier observations survive storage replacement through their own frozen roots.
#[test]
fn region_observations_keep_earlier_roots_and_replace_current_contributions() -> eyre::Result<()> {
    use crate::value::{Multiplicity, ObjectData, ObjectVersion};
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Unknown {
        inputs: Rc::new(BTreeSet::from([ValuesPath::parse("dependency")])),
    });
    let object = state.map_target(&map).ok_or_eyre("map missing")?;
    state
        .heap
        .register_input(object.clone(), ValuesPath::parse("source"));
    let previous = Rc::clone(&state.heap.version);
    state.heap.install(
        object,
        ObjectVersion {
            revision: state.heap.arena.snapshot(),
            multiplicity: Multiplicity::Singleton,
            data: ObjectData::Map {
                fields: Rc::default(),
                remainder: Remainder::Closed,
            },
        },
    );
    let current = Rc::clone(&state.heap.version);
    drop(state);
    let expected = BTreeSet::from([ValuesPath::parse("dependency"), ValuesPath::parse("source")]);
    sim_assert_eq!(have: super::paths(&previous.input_paths, &[&previous]), want: expected.clone());
    sim_assert_eq!(have: super::paths(&current.input_paths, &[&current]), want: BTreeSet::new());
    sim_assert_eq!(have: super::paths(&current.input_paths, &[&previous, &current]), want: expected);
    Ok(())
}

#[test]
fn region_diff_matches_full_root_union_across_epoch_and_order_changes() -> eyre::Result<()> {
    use crate::value::Field;
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Unknown {
        inputs: Rc::new(BTreeSet::from([ValuesPath::parse("earlier")])),
    });
    let object = state.map_target(&map).ok_or_eyre("map missing")?;
    state
        .heap
        .register_input(object.clone(), ValuesPath::parse("source"));
    let earlier = Rc::clone(&state.heap.version);
    state
        .heap
        .unknown_write(Rc::new(BTreeSet::from([ValuesPath::parse("epoch")])));
    let epoch = Rc::clone(&state.heap.version);
    assert!(
        state
            .heap
            .write(&object, "known", Field::Present(Value::string("exact")))
    );
    let written = Rc::clone(&state.heap.version);
    for roots in [
        vec![earlier.as_ref(), epoch.as_ref(), written.as_ref()],
        vec![written.as_ref(), epoch.as_ref(), earlier.as_ref()],
        vec![
            epoch.as_ref(),
            written.as_ref(),
            epoch.as_ref(),
            written.as_ref(),
        ],
    ] {
        let reference = full_scan(&written.input_paths, &roots)?;
        sim_assert_eq!(have: super::paths(&written.input_paths, &roots), want: reference);
    }
    sim_assert_eq!(have: super::paths(&written.input_paths, &[&written, &epoch, &earlier]), want: BTreeSet::from([ValuesPath::parse("earlier"), ValuesPath::parse("epoch"), ValuesPath::parse("source")]));
    Ok(())
}

fn full_scan(
    inputs: &crate::value::input_paths::InputPaths,
    roots: &[&crate::value::HeapVersion],
) -> eyre::Result<BTreeSet<ValuesPath>> {
    use crate::value::ObjectData;
    let mut paths = BTreeSet::new();
    for root in roots {
        for object in root.store.objects() {
            let storage = root
                .store
                .object(&object)
                .ok_or_eyre("allocated object missing")?;
            let original = match &storage.data {
                ObjectData::Widened { inputs }
                | ObjectData::Map {
                    fields: _,
                    remainder: Remainder::Unknown { inputs },
                } => {
                    paths.extend(inputs.iter().cloned());
                    true
                }
                ObjectData::Map {
                    fields: _,
                    remainder: Remainder::Open | Remainder::Layered(_),
                } => true,
                ObjectData::Map {
                    fields: _,
                    remainder: Remainder::Closed | Remainder::Input,
                }
                | ObjectData::Strings(_)
                | ObjectData::Opaque { reachable: _ } => false,
            };
            if original {
                paths.extend(inputs.get(&object).cloned());
            }
        }
    }
    Ok(paths)
}

#[test]
fn region_projection_matches_full_scan_with_overrides_and_unallocated_inputs() -> eyre::Result<()> {
    use crate::value::{Multiplicity, ObjectData, ObjectId, ObjectVersion};
    let mut state = State::new(Value::nil());
    let first = state.heap.map(Remainder::Unknown {
        inputs: Rc::new(BTreeSet::from([ValuesPath::parse("before")])),
    });
    let first_id = state.map_target(&first).ok_or_eyre("first map missing")?;
    let second = state.heap.map(Remainder::Closed);
    let second_id = state.map_target(&second).ok_or_eyre("second map missing")?;
    let later_id = ObjectId { ordinal: 256 };
    let absent_id = ObjectId { ordinal: 1 << 40 };
    for (object, path) in [
        (&first_id, "source-a"),
        (&second_id, "source-b"),
        (&later_id, "source-c"),
        (&absent_id, "unallocated"),
    ] {
        state
            .heap
            .register_input(object.clone(), ValuesPath::parse(path));
    }
    let earlier = Rc::clone(&state.heap.version);
    state
        .heap
        .unknown_write(Rc::new(BTreeSet::from([ValuesPath::parse("epoch")])));
    let epoch = Rc::clone(&state.heap.version);
    state.heap.install(
        first_id,
        ObjectVersion {
            revision: state.heap.arena.snapshot(),
            multiplicity: Multiplicity::Singleton,
            data: ObjectData::Map {
                fields: Rc::default(),
                remainder: Remainder::Closed,
            },
        },
    );
    let mixed = Rc::clone(&state.heap.version);
    state.heap.install(
        second_id,
        ObjectVersion {
            revision: state.heap.arena.snapshot(),
            multiplicity: Multiplicity::Summary,
            data: ObjectData::Map {
                fields: Rc::default(),
                remainder: Remainder::Unknown {
                    inputs: Rc::new(BTreeSet::from([ValuesPath::parse("explicit")])),
                },
            },
        },
    );
    let overridden = Rc::clone(&state.heap.version);
    state.heap.install(
        later_id,
        ObjectVersion {
            revision: state.heap.arena.snapshot(),
            multiplicity: Multiplicity::Summary,
            data: ObjectData::Map {
                fields: Rc::default(),
                remainder: Remainder::Open,
            },
        },
    );
    let allocated = Rc::clone(&state.heap.version);
    for roots in [
        vec![
            earlier.as_ref(),
            epoch.as_ref(),
            mixed.as_ref(),
            overridden.as_ref(),
            allocated.as_ref(),
        ],
        vec![
            allocated.as_ref(),
            overridden.as_ref(),
            mixed.as_ref(),
            epoch.as_ref(),
            earlier.as_ref(),
        ],
        vec![
            mixed.as_ref(),
            overridden.as_ref(),
            mixed.as_ref(),
            allocated.as_ref(),
        ],
        vec![overridden.as_ref()],
    ] {
        sim_assert_eq!(have: super::paths(&allocated.input_paths, &roots), want: full_scan(&allocated.input_paths, &roots)?);
    }
    sim_assert_eq!(have: super::paths(&mixed.input_paths, &[&mixed]), want: BTreeSet::from([ValuesPath::parse("epoch"), ValuesPath::parse("source-b")]));
    sim_assert_eq!(have: super::paths(&overridden.input_paths, &[&overridden]), want: BTreeSet::from([ValuesPath::parse("explicit"), ValuesPath::parse("source-b")]));
    Ok(())
}
