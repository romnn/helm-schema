use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{Predicate, ValuesPath};
use std::collections::BTreeSet;
use std::rc::Rc;
use test_util::prelude::sim_assert_eq;

use super::{CoverageAnswer, CoverageProjection};
use crate::value::{FrozenValue, Remainder, State, Value};

fn frozen_map() -> FrozenValue {
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    state.heap.freeze(&map)
}

#[test]
fn zero_budget_widens_without_replacing_a_complete_cached_answer() {
    let frozen = frozen_map();
    let mut projection = CoverageProjection::new();
    assert!(matches!(
        projection.query(&frozen.heap, 64),
        CoverageAnswer::Known(_)
    ));
    let entries = projection.views.len();

    // A warm cache cannot turn an exhausted query into a completeness proof.
    assert!(matches!(
        projection.query(&frozen.heap, 0),
        CoverageAnswer::Exhausted { work_limit: 0 }
    ));
    sim_assert_eq!(have: projection.views.len(), want: entries);
    assert!(matches!(
        projection.views.get(&frozen.heap.id.ordinal),
        Some(CoverageAnswer::Known(_))
    ));
    assert!(matches!(
        projection.query(&frozen.heap, 64),
        CoverageAnswer::Known(_)
    ));
}

#[test]
fn a_larger_budget_can_refine_a_cached_exhaustion_after_the_owner_is_dropped() {
    let frozen = frozen_map();
    let mut projection = CoverageProjection::new();
    sim_assert_eq!(have: frozen.heap.arena.upgrade().is_some(), want: false);

    // One visit cannot reconstruct even this empty map and its initial storage.
    assert!(matches!(
        projection.query(&frozen.heap, 1),
        CoverageAnswer::Exhausted { work_limit: 1 }
    ));
    assert!(matches!(
        projection.views.get(&frozen.heap.id.ordinal),
        Some(CoverageAnswer::Exhausted { work_limit: 1 })
    ));
    assert!(matches!(
        projection.query(&frozen.heap, 64),
        CoverageAnswer::Known(_)
    ));
}

#[test]
fn a_larger_allowance_is_compared_before_charging_the_cache_lookup() {
    let state = State::new(Value::nil());
    let frozen = state.heap.freeze(&Value::nil());
    let mut projection = CoverageProjection::new();

    // Empty history needs one visit and one completed-answer publication.
    assert!(matches!(
        projection.query(&frozen.heap, 1),
        CoverageAnswer::Exhausted { work_limit: 1 }
    ));
    assert!(matches!(
        projection.query(&frozen.heap, 2),
        CoverageAnswer::Known(_)
    ));
    sim_assert_eq!(have: projection.views.len(), want: 1);
}

#[test]
fn lack_of_remaining_work_alone_does_not_prove_work_exhaustion() {
    let frozen = frozen_map();
    let mut projection = CoverageProjection::new();
    let budget = super::QueryBudget::new(0);

    // Capacity or invariant failures must not become negative work-cache entries.
    projection.remember_exhaustion(
        super::PendingFrame {
            version: &frozen.heap,
            work_limit: 0,
        },
        &budget,
    );
    sim_assert_eq!(have: projection.views.len(), want: 0);
}

#[test]
fn completed_flattening_separates_input_generations_and_recomputes_after_eviction()
-> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let object = state.map_target(&map).ok_or_eyre("map missing")?;
    state.heap.unknown_write(Rc::default());
    let mut projection = CoverageProjection::new();
    let mut first = crate::value::read_set::ReadSet::default();
    let root = &state.heap.version;
    projection.project(&Predicate::True, &root.input_paths, &[root], &mut first);
    sim_assert_eq!(have: first.iter().count(), want: 0);
    sim_assert_eq!(have: projection.flat.len(), want: 1);
    let mut warm = crate::value::read_set::ReadSet::default();
    projection.project(&Predicate::True, &root.input_paths, &[root], &mut warm);
    sim_assert_eq!(have: warm, want: first);
    state
        .heap
        .register_input(object, ValuesPath::parse("source"));
    let root = &state.heap.version;
    let mut second = crate::value::read_set::ReadSet::default();
    projection.project(&Predicate::True, &root.input_paths, &[root], &mut second);
    sim_assert_eq!(have: second.iter().map(|read| read.path.clone()).collect::<BTreeSet<_>>(), want: BTreeSet::from([ValuesPath::parse("source")]));
    sim_assert_eq!(have: projection.flat.len(), want: 2);
    for region in 10_000..14_096 {
        projection.remember_paths(
            super::FlatKey {
                regions: vec![Some(region)],
                inputs: 0,
            },
            &BTreeSet::new(),
            1,
        );
    }
    let mut again = crate::value::read_set::ReadSet::default();
    projection.project(&Predicate::True, &root.input_paths, &[root], &mut again);
    sim_assert_eq!(have: again, want: second);
    Ok(())
}

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
    let mut projection = CoverageProjection::new();
    let mut reads = crate::value::read_set::ReadSet::default();
    let root = &state.heap.version;
    projection.project(&Predicate::True, &root.input_paths, &[root], &mut reads);
    sim_assert_eq!(have: reads.iter().map(|read| read.path.clone()).collect::<BTreeSet<_>>(), want: BTreeSet::from([
        ValuesPath::parse("source"), ValuesPath::parse("left"), ValuesPath::parse("right"),
    ]));
    state
        .heap
        .register_input(object.clone(), ValuesPath::parse("retargeted"));
    let root = &state.heap.version;
    let mut reads = crate::value::read_set::ReadSet::default();
    projection.project(&Predicate::True, &root.input_paths, &[root], &mut reads);
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
    projection.project(&Predicate::True, &root.input_paths, &[root], &mut reads);
    sim_assert_eq!(have: reads.iter().map(|read| read.path.clone()).collect::<BTreeSet<_>>(), want: BTreeSet::from([
        ValuesPath::parse("retargeted"), ValuesPath::parse("left"), ValuesPath::parse("right"), ValuesPath::parse("later"),
    ]));
    Ok(())
}

#[test]
fn expired_region_metadata_is_reclaimed_before_completed_views() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Closed);
    let target = state.map_target(&map).ok_or_eyre("map missing")?;
    state
        .heap
        .register_input(target, ValuesPath::parse("source"));
    state.heap.unknown_write(Rc::default());
    let root = Rc::clone(&state.heap.version);
    let mut projection = CoverageProjection::new();
    let mut expected = crate::value::read_set::ReadSet::default();
    projection.project(&Predicate::True, &root.input_paths, &[&root], &mut expected);
    let retained = projection.retained.get();
    let views = projection.views.keys().copied().collect::<BTreeSet<_>>();
    assert!(!views.is_empty());
    let mut budget = super::QueryBudget::new(1000);
    let inputs = Rc::new(BTreeSet::from([ValuesPath::parse("live")]));
    let live = projection
        .known(&inputs, &mut budget)
        .flatten()
        .ok_or_eyre("live region missing")?;
    let object = crate::value::ObjectId { ordinal: u64::MAX };
    let object_region = projection
        .input_object(&object, &mut budget)
        .flatten()
        .ok_or_eyre("object region missing")?;
    let joined = projection
        .union(
            Some(Rc::clone(&live)),
            Some(Rc::clone(&object_region)),
            &mut budget,
        )
        .flatten()
        .ok_or_eyre("union region missing")?;
    let dead = std::rc::Weak::new();
    let live_objects = projection
        .input_objects
        .iter()
        .filter(|(_, region)| region.strong_count() != 0)
        .map(|(object, _)| object.clone())
        .collect::<BTreeSet<_>>();
    for ordinal in 0..super::MAX_COVERAGE_CACHE {
        projection.input_objects.insert(
            crate::value::ObjectId {
                ordinal: (1 << 40) + u64::try_from(ordinal)?,
            },
            dead.clone(),
        );
    }
    let before_work = budget.remaining;
    let before_retained = projection.retained.get();
    let before_region = projection.next_region;
    assert!(projection.reserve_cache(1).is_some());
    sim_assert_eq!(have: projection.views.keys().copied().collect::<BTreeSet<_>>(), want: views);
    sim_assert_eq!(have: projection.input_objects.keys().cloned().collect::<BTreeSet<_>>(), want: live_objects);
    assert!(projection.input_sets.values().any(|region| {
        region
            .upgrade()
            .is_some_and(|region| Rc::ptr_eq(&region, &live))
    }));
    assert!(projection.unions.values().any(|region| {
        region
            .upgrade()
            .is_some_and(|region| Rc::ptr_eq(&region, &joined))
    }));
    sim_assert_eq!(have: projection.retained.get(), want: before_retained);
    sim_assert_eq!(have: projection.next_region, want: before_region);
    sim_assert_eq!(have: budget.remaining, want: before_work);
    assert!(before_retained >= retained);
    let mut actual = crate::value::read_set::ReadSet::default();
    projection.project(&Predicate::True, &root.input_paths, &[&root], &mut actual);
    sim_assert_eq!(have: actual, want: expected);
    Ok(())
}

struct ReferenceProjection {
    reads: crate::value::read_set::ReadSet,
    work: usize,
}

fn reference_projection(
    projection: &mut CoverageProjection,
    condition: &Predicate,
    inputs: &crate::value::input_paths::InputPaths,
    roots: &[&crate::value::HeapVersion],
) -> ReferenceProjection {
    let mut seen = ahash::AHashSet::new();
    let mut paths = BTreeSet::new();
    let mut budget = super::QueryBudget::new(super::MAX_COVERAGE_WORK);
    let mut capped = false;
    'roots: for root in roots {
        let CoverageAnswer::Known(view) = projection.query(root, super::MAX_COVERAGE_WORK) else {
            capped = true;
            continue;
        };
        let root_region = view.root.as_ref().and_then(|node| node.regions.as_ref());
        let mut regions: Vec<_> = root_region.cloned().into_iter().collect();
        while let Some(region) = regions.pop() {
            if !seen.insert(region.id) {
                continue;
            }
            if !budget.charge(1) {
                capped = true;
                break 'roots;
            }
            match &region.kind {
                super::RegionKind::Paths(known) => {
                    if !budget.charge(known.len()) {
                        capped = true;
                        break 'roots;
                    }
                    paths.extend(known.iter().cloned());
                }
                super::RegionKind::InputObject(object) => {
                    if !budget.charge(inputs.lookup_work()) {
                        capped = true;
                        break 'roots;
                    }
                    paths.extend(inputs.get(object).cloned());
                }
                super::RegionKind::Union { yes, no } => {
                    regions.extend([Rc::clone(yes), Rc::clone(no)]);
                }
            }
        }
    }
    if capped {
        paths.insert(ValuesPath::default());
    }

    ReferenceProjection {
        reads: paths
            .into_iter()
            .map(|path| crate::value::RuntimeRead {
                path,
                condition: condition.clone(),
                coverage: crate::value::ReadCoverage::UnknownRegion,
            })
            .collect(),
        work: super::MAX_COVERAGE_WORK - budget.remaining,
    }
}

#[test]
fn grouped_flattening_replays_original_paths_and_work() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let map = state.heap.map(Remainder::Unknown {
        inputs: Rc::new(BTreeSet::from([ValuesPath::parse("known")])),
    });
    let object = state.map_target(&map).ok_or_eyre("map missing")?;
    state
        .heap
        .register_input(object.clone(), ValuesPath::parse("source"));
    let first = Rc::clone(&state.heap.version);
    let left = state.heap.map(Remainder::Open);
    let left_object = state.map_target(&left).ok_or_eyre("left map missing")?;
    state
        .heap
        .register_input(left_object, ValuesPath::parse("left"));
    let second = Rc::clone(&state.heap.version);
    let right = state.heap.map(Remainder::Unknown {
        inputs: Rc::new(BTreeSet::from([ValuesPath::parse("right-input")])),
    });
    let right_object = state.map_target(&right).ok_or_eyre("right map missing")?;
    state
        .heap
        .register_input(right_object, ValuesPath::parse("right"));
    let third = Rc::clone(&state.heap.version);
    state
        .heap
        .register_input(object, ValuesPath::parse("retargeted"));
    let fourth = Rc::clone(&state.heap.version);
    let mut actual_projection = CoverageProjection::new();
    let mut reference = CoverageProjection::new();
    let groups = [
        vec![first.as_ref(), second.as_ref(), third.as_ref()],
        vec![third.as_ref(), second.as_ref(), first.as_ref()],
        vec![second.as_ref(), second.as_ref(), third.as_ref()],
        vec![],
    ];
    for _ in 0..3 {
        for inputs in [&first.input_paths, &fourth.input_paths] {
            for roots in &groups {
                for condition in [Predicate::True, Predicate::truthy_path("enabled")] {
                    let expected = reference_projection(&mut reference, &condition, inputs, roots);
                    let mut actual = crate::value::read_set::ReadSet::default();
                    actual_projection.project(&condition, inputs, roots, &mut actual);
                    sim_assert_eq!(have: actual, want: expected.reads);
                    let key = actual_projection
                        .known_flat_key(inputs, roots)
                        .ok_or_eyre("known views missing")?;
                    let flat = actual_projection
                        .flat
                        .get(&key)
                        .ok_or_eyre("complete flattening missing")?;
                    sim_assert_eq!(have: flat.work, want: expected.work);
                }
            }
        }
    }
    // Rebuilt views have fresh region identities even for the same heap snapshots.
    let roots = &groups[0];
    let before = actual_projection
        .known_flat_key(&first.input_paths, roots)
        .ok_or_eyre("known views missing")?;
    actual_projection.views.clear();
    reference.views.clear();
    let expected =
        reference_projection(&mut reference, &Predicate::True, &first.input_paths, roots);
    let mut actual = crate::value::read_set::ReadSet::default();
    actual_projection.project(&Predicate::True, &first.input_paths, roots, &mut actual);
    sim_assert_eq!(have: actual, want: expected.reads);
    let after = actual_projection
        .known_flat_key(&first.input_paths, roots)
        .ok_or_eyre("rebuilt views missing")?;
    assert_ne!(before, after);
    // A cached complete flattening cannot replace an exhausted view query.
    let mut complete = crate::value::read_set::ReadSet::default();
    actual_projection.project(
        &Predicate::True,
        &first.input_paths,
        &[&first],
        &mut complete,
    );
    assert!(complete.iter().count() > 0);
    let key = actual_projection
        .known_flat_key(&first.input_paths, &[&first])
        .ok_or_eyre("known root missing")?;
    assert!(actual_projection.flat.contains_key(&key));
    actual_projection.views.insert(
        first.id.ordinal,
        CoverageAnswer::Exhausted {
            work_limit: super::MAX_COVERAGE_WORK,
        },
    );
    let mut exhausted = crate::value::read_set::ReadSet::default();
    actual_projection.project(
        &Predicate::True,
        &first.input_paths,
        &[&first],
        &mut exhausted,
    );
    sim_assert_eq!(have: exhausted.iter().map(|read| read.path.clone()).collect::<Vec<_>>(), want: vec![ValuesPath::default()]);
    Ok(())
}
