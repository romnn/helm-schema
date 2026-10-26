use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use std::fmt::Write as _;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::execute_document;

#[test]
fn independent_output_choices_keep_budget_guards_shared() -> eyre::Result<()> {
    helm_schema_test_support::init_perfetto_from_env()?;
    for branches in [8, 16, 24, 32] {
        let mut source = "{{\"\"}}".repeat(112);
        for index in 0..branches {
            write!(source, "{{{{if .Values.b{index}}}}}x{{{{end}}}}")?;
        }
        let db = IrAnalysisDb::new(&DefineIndex::new());
        let execution = execute_document(&source, None, &db);
        sim_assert_eq!(have: execution.exits.len(), want: 1);
        let exit = execution
            .exits
            .first()
            .ok_or_eyre("normal output exit missing")?;
        let mut store = exit.state.heap.arena.guards.borrow_mut();
        // Measure the execution graph independently of the bounded cache working set.
        store.working.clear();
        let live_nodes = store
            .nodes
            .values()
            .filter(|node| node.strong_count() != 0)
            .count();
        assert!(
            live_nodes < 4 * branches * branches,
            "branches={branches}, live_nodes={live_nodes}, nodes={}, applications={}, projections={}",
            store.nodes.len(),
            store.applications.len(),
            store.projections.len()
        );
        // Weak entries awaiting collection have a fixed allowance independent of branch count.
        assert!(store.nodes.len() < 4096 + 8 * branches * branches);
        assert!(store.applications.len() < 8 * branches * branches);
    }
    Ok(())
}

fn fixture_truth(
    predicate: &helm_schema_core::Predicate,
    true_paths: &std::collections::BTreeSet<helm_schema_core::ValuesPath>,
) -> eyre::Result<bool> {
    use helm_schema_core::{Guard, PredicateKind};

    match predicate.kind() {
        PredicateKind::True => Ok(true),
        PredicateKind::False => Ok(false),
        PredicateKind::Guard(Guard::Truthy { path }) => Ok(true_paths.contains(path)),
        PredicateKind::Not(inner) => Ok(!fixture_truth(inner, true_paths)?),
        PredicateKind::And(parts) => {
            for part in parts {
                if !fixture_truth(part, true_paths)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        PredicateKind::Or(parts) => {
            for part in parts {
                if fixture_truth(part, true_paths)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        PredicateKind::Guard(_) | PredicateKind::Approximate { .. } => {
            eyre::bail!("unexpected predicate in the Boolean projection fixture")
        }
    }
}

#[test]
fn budget_projection_cache_never_contaminates_exact_control_projection() -> eyre::Result<()> {
    use std::collections::BTreeSet;
    use std::rc::Rc;

    use helm_schema_core::{Guard, Predicate, ValuesPath};

    use super::{Arena, Coverage, DecisionEdge, ProjectionKey, ProjectionPurpose, RuntimeGuard};

    for first_purpose in [ProjectionPurpose::Budget, ProjectionPurpose::Control] {
        let arena = Arena::default();
        let paths = (0..300)
            .map(|index| ValuesPath::parse(&format!("b{index:03}")))
            .collect::<Vec<_>>();
        let mut guard = RuntimeGuard::False;
        for path in &paths {
            let predicate = Predicate::from(Guard::Truthy { path: path.clone() });
            let decision = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
            let atom = arena.runtime_path(&[DecisionEdge {
                test: decision,
                taken: true,
            }]);
            guard = arena.union_runtime_guards(&guard, &atom);
        }
        // Build the complete shared runtime guard before either projection purpose gets a cache entry.
        {
            let store = arena.guards.borrow();
            assert!(store.projections.is_empty());
            assert!(store.tests.is_empty());
        }
        let project =
            |purpose| arena.guard_test(&mut arena.guards.borrow_mut(), guard.clone(), purpose);
        let first = project(first_purpose);
        let other_purpose = match first_purpose {
            ProjectionPurpose::Budget => ProjectionPurpose::Control,
            ProjectionPurpose::Control => ProjectionPurpose::Budget,
        };
        let second = project(other_purpose);
        let (budget, control) = match first_purpose {
            ProjectionPurpose::Budget => (first, second),
            ProjectionPurpose::Control => (second, first),
        };
        sim_assert_eq!(have: budget.coverage, want: Coverage::Partial);
        sim_assert_eq!(have: control.coverage, want: Coverage::Exact);
        assert!(!Rc::ptr_eq(&budget, &control));
        let budget_again = project(ProjectionPurpose::Budget);
        let control_again = project(ProjectionPurpose::Control);
        assert!(Rc::ptr_eq(&budget, &budget_again));
        assert!(Rc::ptr_eq(&control, &control_again));
        sim_assert_eq!(have: budget.runtime.as_ref().ok_or_eyre("budget runtime missing")?.id(), want: guard.id());
        sim_assert_eq!(have: control.runtime.as_ref().ok_or_eyre("control runtime missing")?.id(), want: guard.id());
        sim_assert_eq!(have: control.when_true.value_paths(), want: paths.iter().cloned().collect::<BTreeSet<_>>());
        {
            let store = arena.guards.borrow();
            for purpose in [ProjectionPurpose::Budget, ProjectionPurpose::Control] {
                let key = ProjectionKey {
                    guard: guard.id(),
                    purpose,
                };
                assert!(store.projections.contains_key(&key));
                assert!(store.tests.contains_key(&key));
            }
        }
        let first_path = paths.first().ok_or_eyre("first fixture path missing")?;
        let last_path = paths.last().ok_or_eyre("last fixture path missing")?;
        let assignments = [
            BTreeSet::new(),
            BTreeSet::from([first_path.clone()]),
            BTreeSet::from([last_path.clone()]),
            paths.iter().step_by(2).cloned().collect(),
            paths.iter().cloned().collect(),
        ];
        // Every unlisted Boolean is false, so each fixture set specifies a complete assignment.
        for true_paths in assignments {
            let actual = !true_paths.is_empty();
            let budget_true = fixture_truth(&budget.when_true, &true_paths)?;
            let budget_false = fixture_truth(&budget.when_false, &true_paths)?;
            sim_assert_eq!(have: fixture_truth(&control.when_true, &true_paths)?, want: actual);
            sim_assert_eq!(have: fixture_truth(&control.when_false, &true_paths)?, want: !actual);
            // Exported true/false proofs must remain subsets of their actual runtime domains.
            sim_assert_eq!(have: !budget_true || actual, want: true);
            sim_assert_eq!(have: !budget_false || !actual, want: true);
        }
    }
    Ok(())
}

#[test]
fn guard_caches_release_dead_nodes_without_changing_live_identity() -> eyre::Result<()> {
    use std::rc::Rc;

    use helm_schema_core::{Guard, Predicate, ValuesPath};

    use super::{Arena, Coverage, DecisionEdge, ProjectionPurpose, RuntimeGuard};

    let arena = Arena::default();
    let decision = arena.decision(Predicate::True, Predicate::True, Coverage::Partial);
    let edge = DecisionEdge {
        test: decision,
        taken: true,
    };
    let live = arena.runtime_path(std::slice::from_ref(&edge));
    let projected = arena.guard_test(
        &mut arena.guards.borrow_mut(),
        live.clone(),
        ProjectionPurpose::Control,
    );
    let dead = arena.guards.borrow_mut().negate(&live);
    let old_id = dead.id();
    let RuntimeGuard::Branch(node) = &dead else {
        eyre::bail!("nonconstant complement missing");
    };
    let weak = Rc::downgrade(node);
    drop(dead);

    // Dead interner entries trigger collection while the original handles stay observable.
    for index in 0..4096 {
        let predicate = Predicate::from(Guard::Truthy {
            path: ValuesPath::parse(&format!("temporary{index}")),
        });
        let temporary = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        let _ = arena.runtime_path(&[DecisionEdge {
            test: temporary,
            taken: true,
        }]);
    }
    let again = arena.guard_test(
        &mut arena.guards.borrow_mut(),
        live.clone(),
        ProjectionPurpose::Control,
    );
    assert!(Rc::ptr_eq(&projected, &again));
    assert!(weak.upgrade().is_none());
    sim_assert_eq!(have: arena.guards.borrow().nodes.len(), want: 1);
    sim_assert_eq!(have: arena.runtime_path(&[edge]).id(), want: live.id());
    let recreated = arena.guards.borrow_mut().negate(&live);
    assert!(recreated.id() > old_id);
    sim_assert_eq!(have: arena.intersect_runtime_guards(&live, &recreated), want: RuntimeGuard::False);
    sim_assert_eq!(have: arena.union_runtime_guards(&live, &recreated), want: RuntimeGuard::True);
    Ok(())
}

#[test]
fn bounded_guard_construction_stops_recursion_without_poisoning_exact_queries() -> eyre::Result<()>
{
    use std::collections::BTreeSet;

    use helm_schema_core::{Guard, Predicate, ValuesPath};

    use super::{Arena, Coverage, DecisionEdge, GuardWorkBudget, ProjectionPurpose, RuntimeGuard};

    let arena = Arena::default();
    let paths = (0..16)
        .map(|index| ValuesPath::parse(&format!("b{index:02}")))
        .collect::<Vec<_>>();
    let mut condition = RuntimeGuard::True;
    let mut yes = RuntimeGuard::False;
    for (index, path) in paths.iter().enumerate() {
        let predicate = Predicate::from(Guard::Truthy { path: path.clone() });
        let test = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        let atom = arena.runtime_path(&[DecisionEdge { test, taken: true }]);
        if index < 8 {
            condition = arena.intersect_runtime_guards(&condition, &atom);
        } else {
            yes = arena.union_runtime_guards(&yes, &atom);
        }
    }
    let identity = arena
        .select_runtime_guard_bounded(
            &condition,
            &RuntimeGuard::True,
            &RuntimeGuard::False,
            &mut GuardWorkBudget::new(0),
        )
        .map_err(|reason| eyre::eyre!("identity selection exhausted: {reason:?}"))?;
    sim_assert_eq!(have: identity.id(), want: condition.id());
    let before = arena.guards.borrow().next_node;
    let result = arena.select_runtime_guard_bounded(
        &condition,
        &yes,
        &RuntimeGuard::False,
        &mut GuardWorkBudget::new(3),
    );
    sim_assert_eq!(have: matches!(result, Err(crate::value::BoundReason::BudgetNodes)), want: true);
    assert!(arena.guards.borrow().next_node - before <= 3);

    // A failed bounded query cannot install an inexact result in the shared operation cache.
    let exact = arena.intersect_runtime_guards(&condition, &yes);
    let cached = arena
        .select_runtime_guard_bounded(
            &condition,
            &yes,
            &RuntimeGuard::False,
            &mut GuardWorkBudget::new(0),
        )
        .map_err(|reason| eyre::eyre!("cached exact query exhausted: {reason:?}"))?;
    sim_assert_eq!(have: cached.id(), want: exact.id());
    let projected = arena.guard_test(
        &mut arena.guards.borrow_mut(),
        exact,
        ProjectionPurpose::Control,
    );
    let all: BTreeSet<_> = paths.iter().cloned().collect();
    let first: BTreeSet<_> = paths.iter().take(8).cloned().collect();
    let last: BTreeSet<_> = paths.iter().skip(8).cloned().collect();
    for assigned in [BTreeSet::new(), first, last, all] {
        let expected = paths.iter().take(8).all(|path| assigned.contains(path))
            && paths.iter().skip(8).any(|path| assigned.contains(path));
        sim_assert_eq!(have: fixture_truth(&projected.when_true, &assigned)?, want: expected);
    }
    Ok(())
}

#[test]
fn direct_guard_choice_matches_boolean_expansion_for_correlated_operands() -> eyre::Result<()> {
    use super::{Arena, Coverage, DecisionEdge, GuardWorkBudget, RuntimeGuard};
    use helm_schema_core::Predicate;

    let arena = Arena::default();
    let [a, b, c] = ["a", "b", "c"].map(|path| {
        let predicate = Predicate::truthy_path(path);
        let test = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        arena.runtime_path(&[DecisionEdge { test, taken: true }])
    });
    let operands = [
        RuntimeGuard::False,
        RuntimeGuard::True,
        a.clone(),
        b.clone(),
        c.clone(),
        arena.complement_runtime_guard(&a),
        arena.intersect_runtime_guards(&a, &b),
        arena.union_runtime_guards(&b, &c),
    ];
    // Canonical identity checks all assignments, including shared variables among operands.
    for condition in &operands {
        for yes in &operands {
            for no in &operands {
                let actual = arena
                    .select_runtime_guard_bounded(
                        condition,
                        yes,
                        no,
                        &mut GuardWorkBudget::new(32768),
                    )
                    .map_err(|reason| eyre::eyre!("choice exhausted: {reason:?}"))?;
                let expected = arena.union_runtime_guards(
                    &arena.intersect_runtime_guards(condition, yes),
                    &arena.intersect_runtime_guards(&arena.complement_runtime_guard(condition), no),
                );
                sim_assert_eq!(have: actual.id(), want: expected.id());
            }
        }
    }
    Ok(())
}

#[test]
fn newest_guard_choice_constructs_only_its_final_node() -> eyre::Result<()> {
    use super::{Arena, Coverage, DecisionEdge, GuardWorkBudget};
    use helm_schema_core::Predicate;

    let arena = Arena::default();
    let [yes, no, condition] = ["yes", "no", "condition"].map(|path| {
        let predicate = Predicate::truthy_path(path);
        let test = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        arena.runtime_path(&[DecisionEdge { test, taken: true }])
    });
    let before = arena.guards.borrow().next_node;
    assert!(
        arena
            .select_runtime_guard_bounded(&condition, &yes, &no, &mut GuardWorkBudget::new(0))
            .is_err()
    );
    sim_assert_eq!(have: arena.guards.borrow().next_node, want: before);
    let result = arena
        .select_runtime_guard_bounded(&condition, &yes, &no, &mut GuardWorkBudget::new(2))
        .map_err(|reason| eyre::eyre!("single-node choice exhausted: {reason:?}"))?;
    sim_assert_eq!(have: arena.guards.borrow().next_node - before, want: 1);
    let cached = arena
        .select_runtime_guard_bounded(&condition, &yes, &no, &mut GuardWorkBudget::new(0))
        .map_err(|reason| eyre::eyre!("cached choice exhausted: {reason:?}"))?;
    sim_assert_eq!(have: cached.id(), want: result.id());
    sim_assert_eq!(have: arena.intersect_runtime_guards(&result, &condition).id(), want: arena.intersect_runtime_guards(&yes, &condition).id());
    Ok(())
}
