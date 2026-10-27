use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

use super::{
    BranchCounts, CountExpression, CountExpressionKind, CountState, CountThreshold, TextBuffer,
    TextPart,
};
use crate::value::guard::RuntimeGuard;
use crate::value::{Arena, Coverage, DecisionEdge, FrozenTest, State, Value};

fn precise<T>(result: Result<T, crate::value::BoundReason>) -> eyre::Result<T> {
    result.map_err(|reason| eyre::eyre!("count query exhausted: {reason:?}"))
}

fn guard_holds(guard: &RuntimeGuard, assignment: &RuntimeGuard, arena: &Arena) -> bool {
    !arena
        .intersect_runtime_guards(guard, assignment)
        .is_impossible()
}

fn concrete_count(expression: &CountExpression, assignment: &RuntimeGuard, arena: &Arena) -> usize {
    match &expression.kind {
        CountExpressionKind::Constant(count) => *count,
        CountExpressionKind::Increment(source) => concrete_count(source, assignment, arena)
            .saturating_add(1)
            .min(129),
        CountExpressionKind::Select { condition, yes, no } => concrete_count(
            if guard_holds(condition, assignment, arena) {
                yes
            } else {
                no
            },
            assignment,
            arena,
        ),
    }
}

fn conditional_cost(
    source: &BranchCounts,
    decision: &Rc<FrozenTest>,
    yes_cost: usize,
    no_cost: usize,
    arena: &Arena,
) -> BranchCounts {
    let yes_domain = arena.runtime_path(&[DecisionEdge {
        test: Rc::clone(decision),
        taken: true,
    }]);
    let no_domain = arena.runtime_path(&[DecisionEdge {
        test: Rc::clone(decision),
        taken: false,
    }]);
    let mut yes = source.restrict(&yes_domain, arena);
    let mut no = source.restrict(&no_domain, arena);
    for _ in 0..yes_cost {
        yes.increment();
    }
    for _ in 0..no_cost {
        no.increment();
    }
    BranchCounts::selected(decision, &yes, &no, arena)
}

fn check_counts(
    counts: &BranchCounts,
    expected: &[usize; 8],
    domains: [bool; 8],
    tests: &[Rc<FrozenTest>; 3],
    arena: &Arena,
) -> eyre::Result<()> {
    let count_state = counts.state.borrow();
    let CountState::Exact(expression) = &*count_state else {
        return Err(eyre::eyre!("exact count expression missing"));
    };
    let expression = Rc::clone(expression);
    drop(count_state);
    for mask in 0..8 {
        let edges = tests
            .iter()
            .enumerate()
            .map(|(index, test)| DecisionEdge {
                test: Rc::clone(test),
                taken: mask & (1 << index) != 0,
            })
            .collect::<Vec<_>>();
        let assignment = arena.runtime_path(&edges);
        let want = expected.get(mask).ok_or_eyre("expected count missing")?;
        let active = *domains.get(mask).ok_or_eyre("expected domain missing")?;
        sim_assert_eq!(have: guard_holds(&counts.domain, &assignment, arena), want: active);
        if active {
            sim_assert_eq!(have: concrete_count(&expression, &assignment, arena), want: *want);
            assert!(expression.minimum <= *want && *want <= expression.maximum);
        }
        for threshold in [0, 1, 2, 127, 128, 129, 130] {
            sim_assert_eq!(have: guard_holds(&precise(counts.below(threshold, arena))?, &assignment, arena), want: active && *want < threshold, "mask={mask}, threshold={threshold}");
        }
        sim_assert_eq!(have: guard_holds(&precise(counts.overflow(arena))?, &assignment, arena), want: active && *want == 129);
    }
    Ok(())
}

#[test]
fn count_thresholds_preserve_nested_costs_saturation_reset_and_restriction() -> eyre::Result<()> {
    let state = State::new(Value::nil());
    let tests = ["a", "b", "c"].map(|path| {
        let condition = Predicate::truthy_path(path);
        state
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact)
    });
    let [a, b, c] = &tests;
    let mut counts = BranchCounts::default();
    for _ in 0..126 {
        counts.increment();
    }
    counts = conditional_cost(&counts, a, 2, 0, &state.heap.arena);
    counts = conditional_cost(&counts, b, 3, 0, &state.heap.arena);
    counts = conditional_cost(&counts, c, 5, 1, &state.heap.arena);
    let expected = std::array::from_fn(|mask| {
        (126usize
            + if mask & 1 != 0 { 2 } else { 0 }
            + if mask & 2 != 0 { 3 } else { 0 }
            + if mask & 4 != 0 { 5 } else { 1 })
        .min(129)
    });
    check_counts(&counts, &expected, [true; 8], &tests, &state.heap.arena)?;
    let overflow = precise(counts.overflow(&state.heap.arena))?;
    counts.reset_overflow(overflow);
    let reset = expected.map(|count| if count == 129 { 1 } else { count });
    check_counts(&counts, &reset, [true; 8], &tests, &state.heap.arena)?;
    assert!(counts.definitely_below(129));
    let selected = state.heap.arena.runtime_path(&[DecisionEdge {
        test: Rc::clone(a),
        taken: true,
    }]);
    let restricted = counts.restrict(&selected, &state.heap.arena);
    let domains = std::array::from_fn(|mask| mask & 1 != 0);
    check_counts(&restricted, &reset, domains, &tests, &state.heap.arena)?;
    let empty = restricted.restrict(&RuntimeGuard::False, &state.heap.arena);
    assert!(empty.is_empty());
    let zero = BranchCounts::default();
    assert!(!zero.is_empty());
    sim_assert_eq!(have: precise(zero.below(1, &state.heap.arena))?, want: RuntimeGuard::True);
    sim_assert_eq!(have: precise(empty.below(1, &state.heap.arena))?, want: RuntimeGuard::False);
    Ok(())
}

#[test]
fn count_work_exhaustion_is_sticky_without_an_exact_threshold() -> eyre::Result<()> {
    let mut state = State::new(Value::nil());
    let condition = Predicate::truthy_path("choice");
    let decision =
        state
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact);
    let low = BranchCounts::constant_on(127, RuntimeGuard::True);
    let high = BranchCounts::constant_on(129, RuntimeGuard::True);
    let mut counts = BranchCounts::selected(&decision, &low, &high, &state.heap.arena);
    let independent = counts.clone();
    let mut query = CountThreshold::new(0, 32768);
    assert!(
        counts
            .below_with(128, &state.heap.arena, &mut query)
            .is_err()
    );
    assert!(counts.is_exhausted());
    assert!(!independent.is_exhausted());
    assert!(counts.below(128, &state.heap.arena).is_err());
    counts.increment();
    assert!(counts.is_exhausted());
    let restricted = counts.restrict(&RuntimeGuard::True, &state.heap.arena);
    assert!(restricted.is_exhausted());
    let merged = BranchCounts::selected(&decision, &restricted, &low, &state.heap.arena);
    assert!(merged.is_exhausted());
    assert!(merged.overflow_under(&RuntimeGuard::True, &state.heap.arena));

    let mut output = TextBuffer::default();
    output.bytes("prior");
    output.counts = counts.clone();
    output.bytes("suffix");
    assert!(matches!(
        output.parts.as_slice(),
        [crate::value::OutputNode {
            occurrences: _,
            part: TextPart::Unknown(_)
        }]
    ));
    assert!(output.exact_text().is_none());
    assert!(!output.counts.is_exhausted());
    assert!(output.counts.definitely_below(2));

    state.observation_count = counts.clone();
    assert!(state.reserve_observation().is_empty());
    assert!(state.output_widened);
    assert!(state.observation_count.is_exhausted());
    state.invocation_count = counts;
    assert!(state.reserve_invocation().is_empty());
    assert!(!state.invocation_records_complete);
    assert!(!state.invocations_complete());
    sim_assert_eq!(have: precise(BranchCounts::default().below(128, &state.heap.arena))?, want: RuntimeGuard::True);
    Ok(())
}

#[test]
fn guard_work_exhaustion_does_not_publish_a_threshold_or_poison_a_clone() {
    let state = State::new(Value::nil());
    let tests = ["a", "b"].map(|path| {
        let condition = Predicate::truthy_path(path);
        state
            .heap
            .arena
            .decision(condition.clone(), condition.negated(), Coverage::Exact)
    });
    let [a, b] = &tests;
    let counts = BranchCounts::default();
    let counts = conditional_cost(&counts, a, 127, 0, &state.heap.arena);
    let counts = conditional_cost(&counts, b, 2, 0, &state.heap.arena);
    let independent = counts.clone();
    let mut query = CountThreshold::new(4096, 0);
    assert!(
        counts
            .below_with(128, &state.heap.arena, &mut query)
            .is_err()
    );
    assert!(counts.is_exhausted());
    assert!(!independent.is_exhausted());
    assert!(independent.below(128, &state.heap.arena).is_ok());
}

#[test]
fn fully_admitted_domain_preserves_the_exact_ambient_guard() -> eyre::Result<()> {
    use helm_schema_core::{Guard, ValuesPath};

    let mut state = State::new(Value::nil());
    let arena = Rc::clone(&state.heap.arena);
    let predicate = Predicate::all(
        (0..300)
            .map(|index| {
                Predicate::from(Guard::Truthy {
                    path: ValuesPath::parse(&format!("choice{index}")),
                })
            })
            .collect(),
    );
    let decision = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let counts = conditional_cost(&BranchCounts::default(), &decision, 1, 128, &arena);
    state.active.push(DecisionEdge {
        test: Rc::clone(&decision),
        taken: true,
    });
    let active = arena.runtime_path(&state.active);
    let admitted = counts.restrict(&active, &arena);
    let (slots, rejected) = precise(state.reserve_count(&admitted, 17))?;

    // The live arm has room even though the global count bounds include a full sibling.
    // Its existing control guard does not need a bounded budget projection.
    sim_assert_eq!(have: rejected, want: false);
    sim_assert_eq!(have: slots.len(), want: 1);
    let slot = slots.first().ok_or_eyre("missing admitted observation")?;
    sim_assert_eq!(have: slot.coverage_exact, want: true);
    sim_assert_eq!(have: slot.condition.clone().normalize_boolean(), want: state.path_predicate().normalize_boolean());
    sim_assert_eq!(have: slot.ordinal, want: 17);

    state.active = vec![DecisionEdge {
        test: decision,
        taken: false,
    }];
    let active = arena.runtime_path(&state.active);
    let full = counts.restrict(&active, &arena);
    let (slots, rejected) = precise(state.reserve_count(&full, 18))?;
    sim_assert_eq!(have: slots.len(), want: 0);
    sim_assert_eq!(have: rejected, want: true);
    Ok(())
}

#[test]
fn every_counter_carry_preserves_both_live_branches() -> eyre::Result<()> {
    let arena = Arena::default();
    let tests = ["choice", "unused_b", "unused_c"].map(|path| {
        let condition = Predicate::truthy_path(path);
        arena.decision(condition.clone(), condition.negated(), Coverage::Exact)
    });
    let [choice, _, _] = &tests;
    for initial in 0..=129 {
        let yes = BranchCounts::constant_on(initial, RuntimeGuard::True);
        let no = BranchCounts::constant_on(129 - initial, RuntimeGuard::True);
        let mut counts = BranchCounts::selected(choice, &yes, &no, &arena);
        for increments in 0..3 {
            let expected = std::array::from_fn(|mask| {
                let count = if mask & 1 != 0 {
                    initial
                } else {
                    129 - initial
                };
                (count + increments).min(129)
            });
            check_counts(&counts, &expected, [true; 8], &tests, &arena)?;
            counts.increment();
        }
    }
    Ok(())
}

fn collect_guard_scratch(arena: &Arena, phase: &str) {
    for index in 0..4096 {
        let predicate = Predicate::truthy_path(format!("{phase}{index}"));
        let test = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        let _ = arena.runtime_path(&[DecisionEdge { test, taken: true }]);
    }
    let _ = arena.budget_test(RuntimeGuard::True);
}

#[test]
fn threshold_memo_releases_replaced_results_and_dropped_count_graphs() -> eyre::Result<()> {
    let arena = Arena::default();
    let [a, b] = ["a", "b"].map(|path| {
        let condition = Predicate::truthy_path(path);
        let test = arena.decision(condition.clone(), condition.negated(), Coverage::Exact);
        arena.runtime_path(&[DecisionEdge { test, taken: true }])
    });
    let inner = CountExpression::selected(
        b,
        &CountExpression::constant(0),
        &CountExpression::constant(1),
    );
    let outer = CountExpression::selected(a.clone(), &inner, &CountExpression::constant(2));
    let first = precise(CountThreshold::default().below(&outer, 1, &arena))?;
    let RuntimeGuard::Branch(node) = &first else {
        eyre::bail!("compound threshold missing");
    };
    let replaced = Rc::downgrade(node);
    drop(first);
    // The next threshold is just `a`; its graph does not need the old `a and b` result.
    let second = precise(CountThreshold::default().below(&outer, 2, &arena))?;
    sim_assert_eq!(have: second.id(), want: a.id());
    collect_guard_scratch(&arena, "replace");
    assert!(replaced.upgrade().is_none());

    let result = precise(CountThreshold::default().below(&outer, 1, &arena))?;
    let RuntimeGuard::Branch(node) = &result else {
        eyre::bail!("recreated compound threshold missing");
    };
    let cached = Rc::downgrade(node);
    drop(result);
    collect_guard_scratch(&arena, "retain");
    assert!(cached.upgrade().is_some());
    let increment = CountExpression::increment(&outer);
    precise(CountThreshold::default().below(&increment, 2, &arena))?;
    assert!(increment.threshold.borrow().is_none());
    drop(increment);
    drop(outer);
    drop(inner);
    collect_guard_scratch(&arena, "drop");
    assert!(cached.upgrade().is_none());
    Ok(())
}

#[test]
fn cancelling_thresholds_remain_exact_and_release_internal_memo_results() -> eyre::Result<()> {
    for branches in [8, 16, 32] {
        let arena = Arena::default();
        let mut condition = RuntimeGuard::False;
        for index in 0..branches {
            let predicate = Predicate::truthy_path(format!("branch{index}"));
            let test = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
            let atom = arena.runtime_path(&[DecisionEdge { test, taken: true }]);
            condition = arena.union_runtime_guards(&condition, &atom);
        }
        let child = CountExpression::selected(
            condition.clone(),
            &CountExpression::constant(1),
            &CountExpression::constant(0),
        );
        let root = CountExpression::selected(condition, &child, &CountExpression::constant(1));
        // The inner threshold is large, but the outer correlated choice cancels it completely.
        sim_assert_eq!(have: precise(CountThreshold::default().below(&root, 1, &arena))?, want: RuntimeGuard::False);
        let retained = child.threshold.borrow();
        let result = retained
            .as_ref()
            .ok_or_eyre("inner threshold memo missing")?;
        let RuntimeGuard::Branch(node) = &result.guard else {
            eyre::bail!("inner threshold graph missing");
        };
        let weak = Rc::downgrade(node);
        drop(retained);
        drop(root);
        drop(child);
        collect_guard_scratch(&arena, "cancel");
        assert!(weak.upgrade().is_none());
    }
    Ok(())
}
