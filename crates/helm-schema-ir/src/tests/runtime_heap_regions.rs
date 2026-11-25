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
                ObjectData::Widened {
                    inputs,
                    reachable: _,
                }
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

/// Cached root contributions equal uncached scans across writes, registrations and frozen roots.
#[test]
fn cached_region_union_matches_full_scans_over_randomized_histories() -> eyre::Result<()> {
    use crate::value::{Field, Multiplicity, ObjectData, ObjectId, ObjectVersion};

    let mut seed = 0x3141_5926_5358_9793_u64;
    let mut next = |bound: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        usize::try_from(seed % (bound as u64)).unwrap_or_default()
    };
    for _ in 0..8 {
        let mut state = State::new(Value::nil());
        let mut objects = Vec::new();
        let mut roots = Vec::new();
        // Unallocated registrations must not acquire an epoch's coverage.
        state
            .heap
            .register_input(ObjectId { ordinal: 1 << 40 }, ValuesPath::parse("absent"));
        for index in 0..48 {
            let known = Rc::new(BTreeSet::from([ValuesPath::parse(&format!(
                "known{}",
                next(8)
            ))]));
            if objects.is_empty() || next(4) == 0 {
                let remainder = match next(3) {
                    0 => Remainder::Closed,
                    1 => Remainder::Open,
                    _ => Remainder::Unknown {
                        inputs: Rc::clone(&known),
                    },
                };
                let map = state.heap.map(remainder);
                let object = state.map_target(&map).ok_or_eyre("map missing")?;
                state
                    .heap
                    .register_input(object.clone(), ValuesPath::parse(&format!("source{index}")));
                objects.push(object);
            } else {
                let object = &objects[next(objects.len())];
                match next(5) {
                    0 => state.heap.unknown_write(known),
                    1 => state.heap.register_input(
                        object.clone(),
                        ValuesPath::parse(&format!("retargeted{index}")),
                    ),
                    2 => {
                        state.heap.install(
                            object.clone(),
                            ObjectVersion {
                                revision: state.heap.arena.snapshot(),
                                multiplicity: Multiplicity::Singleton,
                                data: ObjectData::Map {
                                    fields: Rc::default(),
                                    remainder: if next(2) == 0 {
                                        Remainder::Closed
                                    } else {
                                        Remainder::Unknown { inputs: known }
                                    },
                                },
                            },
                        );
                    }
                    3 => {
                        assert!(state.heap.write(
                            object,
                            "leaf",
                            Field::Present(Value::input(ValuesPath::parse("written")))
                        ));
                    }
                    _ => {
                        let mut yes = state.heap.fork();
                        let no = state.heap.fork();
                        yes.unknown_write(known);
                        let predicate = Predicate::truthy_path("branch");
                        let decision = state.heap.arena.decision(
                            predicate.clone(),
                            predicate.negated(),
                            crate::value::Coverage::Exact,
                        );
                        state.heap.join(&decision, &yes, &no)?;
                    }
                }
            }
            let root = &state.heap.version;
            sim_assert_eq!(have: super::paths(&root.input_paths, &[root]), want: full_scan(&root.input_paths, &[root])?);
            roots.push(Rc::clone(&state.heap.version));
        }
        let inputs = Rc::clone(&state.heap.version.input_paths);
        // Repeated and reordered groups exercise both memo hits and store diffs across epochs.
        for _ in 0..32 {
            let selected = (0..next(12))
                .map(|_| roots[next(roots.len())].as_ref())
                .collect::<Vec<_>>();
            let expected = full_scan(&inputs, &selected)?;
            for _ in 0..2 {
                sim_assert_eq!(have: super::paths(&inputs, &selected), want: expected.clone());
            }
        }
    }
    Ok(())
}

#[test]
fn cached_epoch_support_counts_explicit_equal_sets_and_last_removal() -> eyre::Result<()> {
    use crate::value::{Multiplicity, ObjectData, ObjectVersion};

    let mut state = State::new(Value::nil());
    let mut objects = Vec::new();
    for path in ["first", "second"] {
        let map = state.heap.map(Remainder::Closed);
        let object = state.map_target(&map).ok_or_eyre("map missing")?;
        state
            .heap
            .register_input(object.clone(), ValuesPath::parse(path));
        objects.push(object);
    }
    let epoch = BTreeSet::from([ValuesPath::parse("epoch")]);
    state.heap.unknown_write(Rc::new(epoch.clone()));
    for (index, object) in objects.iter().enumerate() {
        state.heap.install(
            object.clone(),
            ObjectVersion {
                revision: state.heap.arena.snapshot(),
                multiplicity: Multiplicity::Singleton,
                data: ObjectData::Map {
                    fields: Rc::default(),
                    remainder: Remainder::Closed,
                },
            },
        );
        sim_assert_eq!(have: state.heap.version.store.region_projection().epoch_objects, want: 1 - index);
        let root = &state.heap.version;
        sim_assert_eq!(have: super::paths(&root.input_paths, &[root]), want: full_scan(&root.input_paths, &[root])?);
    }
    for object in &objects {
        state.heap.install(
            object.clone(),
            ObjectVersion {
                revision: state.heap.arena.snapshot(),
                multiplicity: Multiplicity::Singleton,
                data: ObjectData::Map {
                    fields: Rc::default(),
                    remainder: Remainder::Unknown {
                        inputs: Rc::new(epoch.clone()),
                    },
                },
            },
        );
    }
    sim_assert_eq!(have: state.heap.version.store.region_projection().epoch_objects, want: 2);
    assert!(state.heap.version.store.region_projection().known.is_none());
    state.heap.unknown_write(Rc::default());
    let root = &state.heap.version;
    sim_assert_eq!(have: root.store.region_projection().epoch_objects, want: 0);
    assert!(root.store.region_projection().epoch_inputs.is_none());
    sim_assert_eq!(have: super::paths(&root.input_paths, &[root]), want: full_scan(&root.input_paths, &[root])?);
    let mut empty = State::new(Value::nil());
    empty.heap.unknown_write(Rc::new(epoch));
    let map = empty.heap.map(Remainder::Open);
    let object = empty.map_target(&map).ok_or_eyre("new map missing")?;
    empty.heap.register_input(object, ValuesPath::parse("new"));
    let root = &empty.heap.version;
    sim_assert_eq!(have: super::paths(&root.input_paths, &[root]), want: BTreeSet::from([ValuesPath::parse("new")]));
    Ok(())
}

#[test]
fn cached_regions_share_large_cold_groups_across_unrelated_writes() -> eyre::Result<()> {
    use crate::value::{Field, Multiplicity, ObjectData, ObjectId, ObjectVersion};

    let mut state = State::new(Value::nil());
    // Direct store slots keep the interpreter's allocation-class bound out of this fixture.
    for ordinal in 0..5_000_u64 {
        let object = ObjectId { ordinal };
        state.heap.install(
            object.clone(),
            ObjectVersion {
                revision: state.heap.arena.snapshot(),
                multiplicity: Multiplicity::Singleton,
                data: ObjectData::Map {
                    fields: Rc::default(),
                    remainder: Remainder::Closed,
                },
            },
        );
        state
            .heap
            .register_input(object, ValuesPath::parse(&format!("input{ordinal}")));
    }
    let object = ObjectId { ordinal: 5_000 };
    state.heap.install(
        object.clone(),
        ObjectVersion {
            revision: state.heap.arena.snapshot(),
            multiplicity: Multiplicity::Singleton,
            data: ObjectData::Map {
                fields: Rc::default(),
                remainder: Remainder::Closed,
            },
        },
    );
    state
        .heap
        .unknown_write(Rc::new(BTreeSet::from([ValuesPath::parse("epoch")])));
    state.heap.install(
        object.clone(),
        ObjectVersion {
            revision: state.heap.arena.snapshot(),
            multiplicity: Multiplicity::Singleton,
            data: ObjectData::Map {
                fields: Rc::default(),
                remainder: Remainder::Closed,
            },
        },
    );
    let projection = state.heap.version.store.region_projection().clone();
    let originals = projection
        .originals
        .as_ref()
        .ok_or_eyre("original selector missing")?;
    let epoch = projection
        .epoch_inputs
        .as_ref()
        .ok_or_eyre("epoch set missing")?;
    let mut roots = Vec::new();
    for index in 0..10_000 {
        assert!(state.heap.write(
            &object,
            "value",
            Field::Present(Value::string(&index.to_string()))
        ));
        let current = state.heap.version.store.region_projection();
        assert!(Rc::ptr_eq(
            originals,
            current
                .originals
                .as_ref()
                .ok_or_eyre("original selector lost")?
        ));
        assert!(Rc::ptr_eq(
            epoch,
            current.epoch_inputs.as_ref().ok_or_eyre("epoch set lost")?
        ));
        assert!(current.known.is_none());
        sim_assert_eq!(have: current.epoch_objects, want: 5_000);
        roots.push(Rc::clone(&state.heap.version));
    }
    let selected = roots.iter().map(Rc::as_ref).collect::<Vec<_>>();
    let current = &state.heap.version;
    sim_assert_eq!(have: super::paths(&current.input_paths, &selected), want: full_scan(&current.input_paths, &[current])?);
    Ok(())
}

#[test]
fn cached_projection_does_not_retain_execution_roots() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Open);
    let object = state.map_target(&map).ok_or_eyre("map missing")?;
    state
        .heap
        .register_input(object, ValuesPath::parse("source"));
    state
        .heap
        .unknown_write(Rc::new(BTreeSet::from([ValuesPath::parse("known")])));
    let root = Rc::clone(&state.heap.version);
    let weak_root = Rc::downgrade(&root);
    let weak_arena = Rc::downgrade(&state.heap.arena);
    let projection = root.store.region_projection().clone();
    drop(root);
    drop(state);
    sim_assert_eq!(have: weak_root.upgrade().is_some(), want: false);
    sim_assert_eq!(have: weak_arena.upgrade().is_some(), want: false);
    assert!(projection.originals.is_some());
    assert!(projection.epoch_inputs.is_some());
    Ok(())
}

#[test]
fn cached_regions_use_the_selected_registration_map_without_generation_aliasing() -> eyre::Result<()>
{
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Open);
    let object = state.map_target(&map).ok_or_eyre("map missing")?;
    state
        .heap
        .register_input(object.clone(), ValuesPath::parse("source"));
    let root = &state.heap.version;
    let mut left = root.input_paths.as_ref().clone();
    let mut right = left.clone();
    left.insert(object.clone(), ValuesPath::parse("left"));
    right.insert(object, ValuesPath::parse("right"));
    sim_assert_eq!(have: left.id(), want: right.id());
    for _ in 0..3 {
        sim_assert_eq!(have: super::paths(&left, &[root]), want: BTreeSet::from([ValuesPath::parse("left")]));
        sim_assert_eq!(have: super::paths(&right, &[root]), want: BTreeSet::from([ValuesPath::parse("right")]));
    }
    Ok(())
}
