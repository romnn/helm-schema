use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{Predicate, PredicateKind};
use test_util::prelude::sim_assert_eq;

use super::{Admission, BranchCounts, RuntimeCondition};
use crate::value::guard::RuntimeGuard;
use crate::value::{Coverage, DecisionEdge, State, Value};

impl From<Predicate> for RuntimeCondition {
    fn from(predicate: Predicate) -> Self {
        let state = State::new(Value::nil());
        let arena = Rc::clone(&state.heap.arena);
        let decision = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        let ambient = Rc::new(vec![DecisionEdge {
            test: decision,
            taken: true,
        }]);
        let domain = arena.runtime_path(&ambient);
        Self {
            arena,
            admitted: domain,
            ambient,
            admission: Admission::All,
        }
    }
}

#[test]
fn reservation_carries_raw_threshold_without_ambient_support() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let arena = Rc::clone(&state.heap.arena);
    let predicate = Predicate::truthy_path("room");
    let choice = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let raw = arena.runtime_path(&[DecisionEdge {
        test: Rc::clone(&choice),
        taken: true,
    }]);
    let counts = BranchCounts::selected(
        &choice,
        &BranchCounts::constant_on(127, RuntimeGuard::True),
        &BranchCounts::constant_on(128, RuntimeGuard::True),
        &arena,
    );
    for index in 0..32 {
        let ambient = Predicate::truthy_path(format!("ambient{index}"));
        std::rc::Rc::make_mut(&mut state.active).push(DecisionEdge {
            test: arena.decision(ambient.clone(), ambient.negated(), Coverage::Exact),
            taken: true,
        });
        let domain = arena.runtime_path(&state.active);
        let restricted = counts.restrict(&domain, &arena);
        let (slots, rejected) = state
            .reserve_count(&restricted, index)
            .map_err(|reason| eyre::eyre!("reservation exhausted: {reason:?}"))?;
        sim_assert_eq!(have: rejected, want: true);
        let slot = slots.first().ok_or_eyre("reservation missing")?;
        sim_assert_eq!(have: slot.condition.admitted.clone(), want: arena.intersect_runtime_guards(&domain, &raw));
        sim_assert_eq!(have: slot.condition.admission.clone(), want: Admission::Restricted(raw.clone()));
    }
    Ok(())
}

#[test]
fn retained_ambient_edges_survive_later_state_mutation() {
    let mut state = State::new(Value::nil());
    let predicate = Predicate::truthy_path("before");
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    Rc::make_mut(&mut state.active).push(DecisionEdge {
        test: decision,
        taken: true,
    });
    let condition = RuntimeCondition {
        arena: Rc::clone(&state.heap.arena),
        admitted: state.heap.arena.runtime_path(&state.active),
        ambient: Rc::clone(&state.active),
        admission: Admission::All,
    };
    sim_assert_eq!(have: Rc::ptr_eq(&condition.ambient, &state.active), want: true);
    Rc::make_mut(&mut state.active).clear();
    sim_assert_eq!(have: condition.predicate(), want: predicate);
    sim_assert_eq!(have: state.path_predicate(), want: Predicate::True);

    let first = Predicate::truthy_path("cached-first");
    let second = Predicate::truthy_path("cached-second");
    state.active = Rc::new(vec![
        DecisionEdge {
            test: state
                .heap
                .arena
                .decision(first.clone(), first.negated(), Coverage::Exact),
            taken: true,
        },
        DecisionEdge {
            test: state
                .heap
                .arena
                .decision(second.clone(), second.negated(), Coverage::Exact),
            taken: true,
        },
    ]);
    let cached = state.path_predicate();
    let cached_condition = RuntimeCondition {
        arena: Rc::clone(&state.heap.arena),
        admitted: state.heap.arena.runtime_path(&state.active),
        ambient: Rc::clone(&state.active),
        admission: Admission::All,
    };
    let shared = cached_condition.predicate();
    sim_assert_eq!(have: shared.clone(), want: Predicate::all(vec![first.clone(), second]));
    assert!(
        matches!((cached.kind(), shared.kind()), (PredicateKind::And(left), PredicateKind::And(right)) if std::ptr::eq(left, right))
    );
    drop(cached_condition);
    let old_active = Rc::downgrade(&state.active);
    Rc::make_mut(&mut state.active).truncate(1);
    assert!(old_active.upgrade().is_none());
    sim_assert_eq!(have: state.path_predicate(), want: first);
}

#[test]
fn equal_joint_domains_preserve_distinct_partial_ambient_envelopes() {
    let mut state = State::new(Value::nil());
    let arena = Rc::clone(&state.heap.arena);
    let opaque = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
    let input = Predicate::truthy_path("input");
    let exact = arena.decision(input.clone(), input.negated(), Coverage::Exact);
    let edge = |test, taken| DecisionEdge { test, taken };
    let positive = arena.runtime_path(&[edge(Rc::clone(&opaque), true)]);
    let negative = arena.runtime_path(&[edge(opaque, false)]);
    let input_domain = arena.runtime_path(&[edge(Rc::clone(&exact), true)]);
    let first = arena.union_runtime_guards(&positive, &input_domain);
    let second = arena.union_runtime_guards(&negative, &input_domain);
    state.active = Rc::new(vec![
        edge(arena.budget_test(first.clone()), true),
        edge(arena.budget_test(second.clone()), true),
    ]);
    let envelope = state.path_predicate();
    let partial = RuntimeCondition {
        arena: Rc::clone(&arena),
        admitted: arena.runtime_path(&state.active),
        ambient: Rc::clone(&state.active),
        admission: Admission::All,
    };
    let exact_condition = RuntimeCondition {
        arena: Rc::clone(&arena),
        admitted: input_domain,
        ambient: Rc::new(vec![edge(exact, true)]),
        admission: Admission::All,
    };
    sim_assert_eq!(have: partial.admitted.clone(), want: exact_condition.admitted.clone());
    sim_assert_eq!(have: partial.predicate(), want: envelope);
    sim_assert_eq!(have: partial.predicate().normalize_boolean(), want: Predicate::True);
    sim_assert_eq!(have: exact_condition.predicate(), want: input.clone());
    sim_assert_eq!(have: partial == exact_condition, want: false);

    let restricted = RuntimeCondition {
        arena: Rc::clone(&arena),
        admitted: arena.intersect_runtime_guards(&first, &second),
        ambient: Rc::new(vec![edge(arena.budget_test(first), true)]),
        admission: Admission::Restricted(second),
    };
    // Joint admission resolves the shared opaque choice before exporting either polarity.
    sim_assert_eq!(have: restricted.predicate().normalize_boolean(), want: input);
    sim_assert_eq!(have: restricted.exact(), want: true);
}

#[test]
fn reservation_retains_joint_admission_across_repeated_materialization() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let arena = Rc::clone(&state.heap.arena);
    let room = Predicate::truthy_path("room");
    let choice = arena.decision(room.clone(), room.negated(), Coverage::Exact);
    let raw = arena.runtime_path(&[DecisionEdge {
        test: Rc::clone(&choice),
        taken: true,
    }]);
    let ambient = Predicate::truthy_path("ambient");
    Rc::make_mut(&mut state.active).push(DecisionEdge {
        test: arena.decision(ambient.clone(), ambient.negated(), Coverage::Exact),
        taken: true,
    });
    let domain = arena.runtime_path(&state.active);
    let counts = BranchCounts::selected(
        &choice,
        &BranchCounts::constant_on(127, RuntimeGuard::True),
        &BranchCounts::constant_on(128, RuntimeGuard::True),
        &arena,
    )
    .restrict(&domain, &arena);
    let (slots, rejected) = state
        .reserve_count(&counts, 0)
        .map_err(|reason| eyre::eyre!("reservation exhausted: {reason:?}"))?;
    sim_assert_eq!(have: rejected, want: true);
    let condition = &slots.first().ok_or_eyre("reservation missing")?.condition;
    let joint_id = arena.intersect_runtime_guards(&domain, &raw).id();
    let expected = Predicate::all(vec![ambient, room]).normalize_boolean();
    for _ in 0..4 {
        sim_assert_eq!(have: condition.predicate().normalize_boolean(), want: expected.clone());
        sim_assert_eq!(have: condition.exact(), want: true);
    }
    sim_assert_eq!(have: arena.intersect_runtime_guards(&domain, &raw).id(), want: joint_id);
    Ok(())
}
