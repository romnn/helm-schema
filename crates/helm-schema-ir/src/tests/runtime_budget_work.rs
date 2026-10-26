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

#[test]
fn partial_projection_skips_only_an_absorbed_sibling_for_both_purposes() -> eyre::Result<()> {
    use std::rc::Rc;

    use super::{Arena, Coverage, ProjectionKey, ProjectionPurpose, RuntimeGuard};
    use helm_schema_core::Predicate;

    for purpose in [ProjectionPurpose::Control, ProjectionPurpose::Budget] {
        for exact_outer in [false, true] {
            for constant_yes in [None, Some(true), Some(false)] {
                let arena = Arena::default();
                let a = Predicate::truthy_path("a");
                let input = arena.decision(a.clone(), a.negated(), Coverage::Exact);
                let sibling = arena.guards.borrow_mut().select(
                    input,
                    RuntimeGuard::True,
                    RuntimeGuard::False,
                );
                let unknown = arena.decision(Predicate::True, Predicate::True, Coverage::Partial);
                let yes = constant_yes.map_or_else(
                    || {
                        arena.guards.borrow_mut().select(
                            unknown,
                            RuntimeGuard::True,
                            RuntimeGuard::False,
                        )
                    },
                    |value| {
                        if value {
                            RuntimeGuard::True
                        } else {
                            RuntimeGuard::False
                        }
                    },
                );
                let outer_predicate = Predicate::truthy_path("outer");
                let outer = if exact_outer {
                    arena.decision(
                        outer_predicate.clone(),
                        outer_predicate.negated(),
                        Coverage::Exact,
                    )
                } else {
                    arena.decision(Predicate::True, Predicate::True, Coverage::Partial)
                };
                let guard =
                    arena
                        .guards
                        .borrow_mut()
                        .select(Rc::clone(&outer), yes, sibling.clone());
                let sibling_key = ProjectionKey {
                    guard: sibling.id(),
                    purpose,
                };
                assert!(!arena.guards.borrow().projections.contains_key(&sibling_key));
                let projection =
                    arena
                        .guards
                        .borrow_mut()
                        .project(&guard, purpose, &arena.predicate_memo);
                let absorbed = !exact_outer && constant_yes.is_none();
                sim_assert_eq!(
                    have: arena.guards.borrow().projections.contains_key(&sibling_key),
                    want: !absorbed,
                    "purpose={purpose:?}, exact_outer={exact_outer}, yes={constant_yes:?}"
                );
                if absorbed {
                    sim_assert_eq!(have: projection.necessary.clone(), want: Predicate::True);
                    sim_assert_eq!(have: projection.sufficient.clone(), want: Predicate::False);
                }
                verify_partial_projection_outcomes(&projection, exact_outer, constant_yes)?;
            }
        }
    }
    Ok(())
}

fn verify_partial_projection_outcomes(
    projection: &super::Projection,
    exact_outer: bool,
    constant_yes: Option<bool>,
) -> eyre::Result<()> {
    use helm_schema_core::ValuesPath;
    use std::collections::BTreeSet;
    // Enumerate every input assignment and every unconstrained runtime choice.
    // Existential/universal outcomes independently specify necessary/sufficient proofs.
    for input_a in [false, true] {
        for input_outer in [false, true] {
            let mut paths = BTreeSet::new();
            if input_a {
                paths.insert(ValuesPath::parse("a"));
            }
            if input_outer {
                paths.insert(ValuesPath::parse("outer"));
            }
            let mut outcomes = Vec::new();
            for runtime_outer in [false, true] {
                for runtime_yes in [false, true] {
                    let taken = if exact_outer {
                        input_outer
                    } else {
                        runtime_outer
                    };
                    outcomes.push(if taken {
                        constant_yes.unwrap_or(runtime_yes)
                    } else {
                        input_a
                    });
                }
            }
            let necessary = outcomes.iter().any(|value| *value);
            let sufficient = outcomes.iter().all(|value| *value);
            sim_assert_eq!(have: fixture_truth(&projection.necessary, &paths)?, want: necessary);
            sim_assert_eq!(have: fixture_truth(&projection.sufficient, &paths)?, want: sufficient);
        }
    }
    Ok(())
}

#[test]
fn native_choice_matches_composed_runtime_and_exact_partial_input_proofs() -> eyre::Result<()> {
    use super::{Arena, Coverage, DecisionEdge};
    use helm_schema_core::{Predicate, ValuesPath};
    use std::collections::BTreeSet;
    use std::rc::Rc;

    for exact_condition in [false, true] {
        for exact_yes in [false, true] {
            for exact_no in [false, true] {
                let arena = Arena::default();
                let tests = [
                    ("condition", exact_condition),
                    ("yes", exact_yes),
                    ("no", exact_no),
                ]
                .map(|(name, exact)| {
                    if exact {
                        let predicate = Predicate::truthy_path(name);
                        arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact)
                    } else {
                        arena.decision(Predicate::False, Predicate::False, Coverage::Partial)
                    }
                });
                let [condition, yes, no] = tests;
                let selected =
                    arena.choose_test(Rc::clone(&condition), Rc::clone(&yes), Rc::clone(&no));
                let composed = arena.surviving_decision(
                    &[
                        DecisionEdge {
                            test: Rc::clone(&condition),
                            taken: true,
                        },
                        DecisionEdge {
                            test: yes,
                            taken: true,
                        },
                    ],
                    &[
                        DecisionEdge {
                            test: condition,
                            taken: false,
                        },
                        DecisionEdge {
                            test: no,
                            taken: true,
                        },
                    ],
                );
                sim_assert_eq!(have: selected.runtime.clone(), want: composed.runtime.clone());
                sim_assert_eq!(have: selected.when_true.clone(), want: composed.when_true.clone());
                sim_assert_eq!(have: selected.when_false.clone(), want: composed.when_false.clone());
                for input_bits in 0..8 {
                    let inputs = [
                        input_bits & 1 != 0,
                        input_bits & 2 != 0,
                        input_bits & 4 != 0,
                    ];
                    let paths: BTreeSet<_> = ["condition", "yes", "no"]
                        .into_iter()
                        .zip(inputs)
                        .filter(|(_, value)| *value)
                        .map(|(name, _)| ValuesPath::parse(name))
                        .collect();
                    let outcomes: Vec<_> = (0..8)
                        .map(|runtime_bits| {
                            let [input_condition, input_yes, input_no] = inputs;
                            let condition = if exact_condition {
                                input_condition
                            } else {
                                runtime_bits & 1 != 0
                            };
                            let yes = if exact_yes {
                                input_yes
                            } else {
                                runtime_bits & 2 != 0
                            };
                            let no = if exact_no {
                                input_no
                            } else {
                                runtime_bits & 4 != 0
                            };
                            if condition { yes } else { no }
                        })
                        .collect();
                    sim_assert_eq!(have: fixture_truth(&selected.when_true, &paths)?, want: outcomes.iter().all(|value| *value));
                    sim_assert_eq!(have: fixture_truth(&selected.when_false, &paths)?, want: !outcomes.iter().any(|value| *value));
                }
            }
        }
    }
    Ok(())
}

impl super::Arena {
    pub(in crate::value) fn surviving_decision(
        &self,
        yes: &[crate::value::DecisionEdge],
        no: &[crate::value::DecisionEdge],
    ) -> std::rc::Rc<crate::value::FrozenTest> {
        let mut store = self.guards.borrow_mut();
        let yes = store.conjunction(yes);
        let no = store.conjunction(no);
        let runtime = store.apply(super::Operation::Or, &yes, &no);
        self.guard_test(&mut store, runtime, super::ProjectionPurpose::Control)
    }
}

fn truth_query_atoms(guard: &super::RuntimeGuard) -> std::collections::BTreeSet<u64> {
    use super::RuntimeGuard;
    use std::collections::BTreeSet;
    let mut pending = vec![guard];
    let mut visited = BTreeSet::new();
    let mut atoms = BTreeSet::new();
    while let Some(guard) = pending.pop() {
        match guard {
            RuntimeGuard::False | RuntimeGuard::True => {}
            RuntimeGuard::Branch(node) => {
                if visited.insert(node.id) {
                    atoms.insert(node.test.id);
                    pending.extend([&node.yes, &node.no]);
                }
            }
        }
    }
    atoms
}

fn truth_query_results(
    guard: &super::RuntimeGuard,
    assigned: &std::collections::BTreeMap<u64, bool>,
) -> std::collections::BTreeSet<bool> {
    use super::RuntimeGuard;
    match guard {
        RuntimeGuard::False => std::collections::BTreeSet::from([false]),
        RuntimeGuard::True => std::collections::BTreeSet::from([true]),
        RuntimeGuard::Branch(node) => match assigned.get(&node.test.id) {
            Some(true) => truth_query_results(&node.yes, assigned),
            Some(false) => truth_query_results(&node.no, assigned),
            None => truth_query_results(&node.yes, assigned)
                .union(&truth_query_results(&node.no, assigned))
                .copied()
                .collect(),
        },
    }
}

#[test]
fn query_local_unknown_preserves_known_and_unknown_leaf_relations() {
    use super::{Coverage, DecisionEdge};
    use crate::value::{State, Value};
    use helm_schema_core::Predicate;
    use std::collections::{BTreeMap, BTreeSet};
    use std::rc::Rc;

    let state = State::new(Value::nil());
    let arena = &state.heap.arena;
    let outer = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
    let inner = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
    let value = Value::select(
        Rc::clone(&outer),
        Value::select(
            Rc::clone(&inner),
            Value::literal(helm_schema_core::GuardValue::Bool(true)),
            Value::unknown(),
        ),
        Value::select(
            Rc::clone(&inner),
            Value::literal(helm_schema_core::GuardValue::Bool(false)),
            Value::influenced(BTreeSet::from([helm_schema_core::ValuesPath::parse(
                "other",
            )])),
        ),
    );
    let queried = state.test(&value);
    let runtime = arena.runtime_path(&[DecisionEdge {
        test: queried,
        taken: true,
    }]);
    for selected_outer in [false, true] {
        for selected_inner in [false, true] {
            let expected = if selected_inner {
                BTreeSet::from([selected_outer])
            } else {
                BTreeSet::from([false, true])
            };
            sim_assert_eq!(
                have: truth_query_results(&runtime, &BTreeMap::from([(outer.id, selected_outer), (inner.id, selected_inner)])),
                want: expected,
            );
        }
    }
    let fresh: BTreeSet<_> = truth_query_atoms(&runtime)
        .difference(&BTreeSet::from([outer.id, inner.id]))
        .copied()
        .collect();
    sim_assert_eq!(have: fresh.len(), want: 1);
}

#[test]
fn separate_truth_queries_keep_independent_fallback_choices() {
    use super::DecisionEdge;
    use crate::value::{State, Value};
    use std::rc::Rc;
    let state = State::new(Value::nil());
    let first = state.test(&Value::unknown());
    let second = state.test(&Value::unknown());
    assert_ne!(first.id, second.id);
    for yes in [false, true] {
        for no in [false, true] {
            let guard = state.heap.arena.runtime_path(&[
                DecisionEdge {
                    test: Rc::clone(&first),
                    taken: yes,
                },
                DecisionEdge {
                    test: Rc::clone(&second),
                    taken: no,
                },
            ]);
            assert!(!guard.is_impossible());
        }
    }
}

#[test]
fn selected_unknown_truth_queries_allocate_one_fresh_atom_at_every_size() -> eyre::Result<()> {
    use super::{Coverage, DecisionEdge};
    use crate::value::{State, Value};
    use helm_schema_core::{Predicate, ValuesPath};
    use std::collections::BTreeSet;
    use std::rc::Rc;

    for leaves in [8, 16, 32, 64, 128] {
        let state = State::new(Value::nil());
        let arena = &state.heap.arena;
        let mut old_atoms = BTreeSet::new();
        let mut values: Vec<_> = (0..leaves)
            .map(|index| {
                Value::influenced(BTreeSet::from([ValuesPath::parse(&format!("leaf{index}"))]))
            })
            .collect();
        while values.len() > 1 {
            let decision = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
            old_atoms.insert(decision.id);
            let (pairs, remainder) = values.as_chunks::<2>();
            assert!(remainder.is_empty(), "balanced fixture pair missing");
            values = pairs
                .iter()
                .map(|[yes, no]| Value::select(Rc::clone(&decision), yes.clone(), no.clone()))
                .collect();
        }
        let value = values.pop().ok_or_eyre("balanced fixture root missing")?;
        let before = arena.guards.borrow().next_node;
        let query = state.test(&value);
        let guard = arena.runtime_path(&[DecisionEdge {
            test: query,
            taken: true,
        }]);
        let fresh: BTreeSet<_> = truth_query_atoms(&guard)
            .difference(&old_atoms)
            .copied()
            .collect();
        sim_assert_eq!(have: fresh.len(), want: 1);
        assert!(arena.guards.borrow().next_node - before <= 2 * leaves + 4);
        sim_assert_eq!(have: truth_query_results(&guard, &std::collections::BTreeMap::new()), want: BTreeSet::from([false, true]));
    }
    Ok(())
}

#[test]
fn truth_query_exhaustion_remains_a_free_result_only_under_its_hole() -> eyre::Result<()> {
    use super::{Coverage, DecisionEdge};
    use crate::value::{State, Value};
    use helm_schema_core::{GuardValue, Predicate, ValuesPath};
    use std::collections::{BTreeMap, BTreeSet};
    use std::rc::Rc;

    let state = State::new(Value::nil());
    let arena = &state.heap.arena;
    let mut decisions = Vec::new();
    let mut value = Value::literal(GuardValue::Bool(false));
    // Each link visits its true leaf before the next link; the tail exceeds 255 visits.
    for index in 0..130 {
        let decision = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
        let yes = Value::with_influences(
            Value::literal(GuardValue::Bool(true)),
            BTreeSet::from([ValuesPath::parse(&format!("known{index}"))]),
        );
        value = Value::select(Rc::clone(&decision), yes, value);
        decisions.push(decision);
    }
    decisions.reverse();
    let outer = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
    value = Value::select(
        Rc::clone(&outer),
        Value::literal(GuardValue::Bool(false)),
        value,
    );
    let query = state.test(&value);
    let runtime = arena.runtime_path(&[DecisionEdge {
        test: query,
        taken: true,
    }]);
    let first = decisions.first().ok_or_eyre("first selector missing")?;
    let second = decisions.get(1).ok_or_eyre("second selector missing")?;
    for choose_outer in [false, true] {
        for choose_first in [false, true] {
            for choose_second in [false, true] {
                let mut assigned: BTreeMap<_, _> =
                    decisions.iter().map(|test| (test.id, false)).collect();
                assigned.extend([
                    (outer.id, choose_outer),
                    (first.id, choose_first),
                    (second.id, choose_second),
                ]);
                let expected = if choose_outer {
                    BTreeSet::from([false])
                } else if choose_first || choose_second {
                    BTreeSet::from([true])
                } else {
                    BTreeSet::from([false, true])
                };
                sim_assert_eq!(have: truth_query_results(&runtime, &assigned), want: expected);
            }
        }
    }
    Ok(())
}

#[test]
fn invocation_operand_fallback_sharing_preserves_selected_leaf_relations() {
    use super::{Coverage, DecisionEdge};
    use crate::value::{State, Value};
    use helm_schema_core::{GuardValue, Predicate, ValuesPath};
    use std::collections::{BTreeMap, BTreeSet};
    use std::rc::Rc;

    for known_yes in [None, Some(false), Some(true)] {
        let mut state = State::new(Value::nil());
        let selector =
            state
                .heap
                .arena
                .decision(Predicate::False, Predicate::False, Coverage::Partial);
        let yes = known_yes.map_or_else(
            || Value::influenced(BTreeSet::from([ValuesPath::parse("yes")])),
            |value| Value::literal(GuardValue::Bool(value)),
        );
        let no = Value::influenced(BTreeSet::from([ValuesPath::parse("no")]));
        let mut fallback = None;
        state.active.push(DecisionEdge {
            test: Rc::clone(&selector),
            taken: true,
        });
        let yes = state.test_with_unknown(&yes, &mut fallback);
        state.active.pop();
        state.active.push(DecisionEdge {
            test: Rc::clone(&selector),
            taken: false,
        });
        let no = state.test_with_unknown(&no, &mut fallback);
        state.active.pop();
        let query = state.heap.arena.choose_test(Rc::clone(&selector), yes, no);
        let guard = state.heap.arena.runtime_path(&[DecisionEdge {
            test: query,
            taken: true,
        }]);
        let fresh: BTreeSet<_> = truth_query_atoms(&guard)
            .difference(&BTreeSet::from([selector.id]))
            .copied()
            .collect();
        sim_assert_eq!(have: fresh.len(), want: 1);
        for taken in [false, true] {
            let expected = if taken {
                known_yes.map_or_else(
                    || BTreeSet::from([false, true]),
                    |value| BTreeSet::from([value]),
                )
            } else {
                BTreeSet::from([false, true])
            };
            sim_assert_eq!(have: truth_query_results(&guard, &BTreeMap::from([(selector.id, taken)])), want: expected);
        }
    }
}

#[test]
fn operand_and_invocation_fallback_scopes_remain_independent() {
    use super::DecisionEdge;
    use crate::value::{State, Value};
    use std::rc::Rc;
    let state = State::new(Value::nil());
    let mut first_operand = None;
    let mut second_operand = None;
    let mut next_invocation = None;
    let first = state.test_with_unknown(&Value::unknown(), &mut first_operand);
    let repeated = state.test_with_unknown(&Value::unknown(), &mut first_operand);
    sim_assert_eq!(have: first.id, want: repeated.id);
    for other in [
        state.test_with_unknown(&Value::unknown(), &mut second_operand),
        state.test_with_unknown(&Value::unknown(), &mut next_invocation),
    ] {
        assert_ne!(first.id, other.id);
        for first_taken in [false, true] {
            for other_taken in [false, true] {
                let guard = state.heap.arena.runtime_path(&[
                    DecisionEdge {
                        test: Rc::clone(&first),
                        taken: first_taken,
                    },
                    DecisionEdge {
                        test: Rc::clone(&other),
                        taken: other_taken,
                    },
                ]);
                assert!(!guard.is_impossible());
            }
        }
    }
}

#[test]
fn not_invocation_dispatch_keeps_one_unknown_atom_for_its_selected_operand() -> eyre::Result<()> {
    use super::{Coverage, DecisionEdge};
    use crate::value::{
        ArgumentForm, Callee, CompletionInput, Flow, Invocation, Operand, State, Value,
    };
    use helm_schema_core::{Predicate, ValuesPath};
    use std::collections::BTreeSet;
    use std::rc::Rc;

    let state = State::new(Value::nil());
    let arena = Rc::clone(&state.heap.arena);
    let selector = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
    let operand = Value::select(
        Rc::clone(&selector),
        Value::influenced(BTreeSet::from([ValuesPath::parse("a")])),
        Value::influenced(BTreeSet::from([ValuesPath::parse("b")])),
    );
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let exits = crate::value::eval::Executor { db: &db }
        .complete(CompletionInput::Invocation {
            invocation: Invocation {
                callee: Callee::Function("not".to_owned()),
                operands: vec![Operand {
                    value: operand,
                    site: 0,
                    form: ArgumentForm::Evaluated,
                }],
                site: 0,
            },
            state,
        })
        .into_exits();
    sim_assert_eq!(have: exits.len(), want: 1);
    let exit = exits.first().ok_or_eyre("not invocation exit missing")?;
    let Flow::Normal(normal) = &exit.flow else {
        eyre::bail!("not invocation aborted")
    };
    let query = exit.state.test(&normal.value);
    let guard = arena.runtime_path(&[DecisionEdge {
        test: query,
        taken: true,
    }]);
    let fresh: BTreeSet<_> = truth_query_atoms(&guard)
        .difference(&BTreeSet::from([selector.id]))
        .copied()
        .collect();
    sim_assert_eq!(have: fresh.len(), want: 1);
    for taken in [false, true] {
        sim_assert_eq!(have: truth_query_results(&guard, &std::collections::BTreeMap::from([(selector.id, taken)])), want: BTreeSet::from([false, true]));
    }
    Ok(())
}
