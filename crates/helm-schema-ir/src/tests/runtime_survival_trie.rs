//! Survival re-kinding over a region's continuations (ADR 0072 §2): a branch link becomes a
//! survival only where every document that took its other outcome aborted.

use std::rc::Rc;

use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

use crate::value::path::{LinkKind, Path, PathLink, settle_survival};
use crate::value::{Coverage, DecisionEdge, FrozenTest, State, Value};

fn decision(state: &State) -> Rc<FrozenTest> {
    state
        .heap
        .arena
        .decision(Predicate::False, Predicate::False, Coverage::Partial)
}

fn path(edges: &[(&Rc<FrozenTest>, bool)]) -> Path {
    Path::from(
        edges
            .iter()
            .map(|(test, taken)| DecisionEdge {
                test: Rc::clone(test),
                taken: *taken,
            })
            .collect::<Vec<_>>(),
    )
}

fn kinds(path: &Path) -> Vec<LinkKind> {
    path.links().into_iter().map(PathLink::kind).collect()
}

/// `if d {if e {} else {break}} else {if h {} else {fail}}`: the break exit `[d, ¬e]` is not
/// joined, so it sits beside the joined continuation, whose reach link `j` (d∧e ∨ ¬d∧h)
/// overlaps it. Documents with d∧e continue, so ¬e is no survival.
#[test]
fn an_unjoined_exit_beside_a_joined_continuation_keeps_its_branches() {
    let state = State::new(Value::nil());
    let [d, e, j] = [decision(&state), decision(&state), decision(&state)];
    let mut exit = path(&[(&d, true), (&e, false)]);
    let mut joined = path(&[(&j, true)]);
    settle_survival(&Path::default(), &mut [&mut exit, &mut joined]);
    sim_assert_eq!(have: kinds(&exit), want: vec![LinkKind::Branch, LinkKind::Branch]);
    sim_assert_eq!(have: kinds(&joined), want: vec![LinkKind::Branch]);
}

/// Below a split on the two outcomes of one test, a link every continuation under that outcome
/// took is a survival: its other outcome only aborted.
#[test]
fn a_split_on_one_test_settles_the_links_below_it() {
    let state = State::new(Value::nil());
    let [d, e] = [decision(&state), decision(&state)];
    let mut taken = path(&[(&d, true), (&e, true)]);
    let mut other = path(&[(&d, false)]);
    settle_survival(&Path::default(), &mut [&mut taken, &mut other]);
    sim_assert_eq!(have: kinds(&taken), want: vec![LinkKind::Branch, LinkKind::Survival]);
    sim_assert_eq!(have: kinds(&other), want: vec![LinkKind::Branch]);
}

/// A continuation that ends above a link overlaps the documents that took its other outcome.
#[test]
fn a_continuation_ending_above_a_link_keeps_it() {
    let state = State::new(Value::nil());
    let [d, e] = [decision(&state), decision(&state)];
    let mut longer = path(&[(&d, true), (&e, true)]);
    let mut shorter = path(&[(&d, true)]);
    settle_survival(&Path::default(), &mut [&mut longer, &mut shorter]);
    sim_assert_eq!(have: kinds(&longer), want: vec![LinkKind::Survival, LinkKind::Branch]);
    sim_assert_eq!(have: kinds(&shorter), want: vec![LinkKind::Survival]);
}
