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
        projection.remember_paths(super::FlatKey { region, inputs: 0 }, &BTreeSet::new(), 1);
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
