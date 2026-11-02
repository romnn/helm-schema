use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use std::fmt::Write as _;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::tests::execute_document;

impl super::Arena {
    pub(in crate::value) fn created_runtime_nodes(&self) -> u64 {
        self.guards.borrow().next_node
    }
}

fn project_union(
    parts: Vec<helm_schema_core::Predicate>,
    _memo: &helm_schema_core::PredicateMemo,
) -> Option<helm_schema_core::Predicate> {
    let mut remaining = super::MAX_PROJECTION_WORK;
    super::project_union(parts, &mut remaining)
}

impl super::DecisionStore {
    fn project_union(
        &mut self,
        parts: Vec<helm_schema_core::Predicate>,
        _memo: &helm_schema_core::PredicateMemo,
    ) -> Option<helm_schema_core::Predicate> {
        let mut remaining = super::MAX_PROJECTION_WORK;
        self.project_union_with_work(parts, &mut remaining)
    }
}

#[test]
fn budget_cache_hits_account_for_retained_runtime_nodes() {
    use super::{Arena, Coverage, DecisionEdge, GuardWorkBudget, RuntimeGuard};
    use helm_schema_core::Predicate;
    let arena = Arena::default();
    let mut guard = RuntimeGuard::False;
    for index in 0..64 {
        let predicate = Predicate::truthy_path(format!("retained{index}"));
        let atom = arena.runtime_path(&[DecisionEdge {
            test: arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact),
            taken: true,
        }]);
        guard = arena.union_runtime_guards(&guard, &atom);
    }
    // A cached identity selection must not bypass the size of the returned graph.
    for _ in 0..2 {
        assert!(
            arena
                .select_runtime_guard_bounded(
                    &guard,
                    &RuntimeGuard::True,
                    &RuntimeGuard::False,
                    &mut GuardWorkBudget::new(4),
                )
                .is_err()
        );
    }
    assert!(guard.is_input_exact());
    assert!(
        arena
            .select_runtime_guard_bounded(
                &guard,
                &RuntimeGuard::True,
                &RuntimeGuard::False,
                &mut GuardWorkBudget::new(256),
            )
            .is_ok_and(|result| result == guard)
    );
    assert!(GuardWorkBudget::new(4).include([&guard]).is_err());
    assert!(
        GuardWorkBudget::new(64)
            .include([&guard, &guard])
            .is_ok_and(|work| work == 64)
    );
}

#[test]
fn exhausted_normalization_retains_a_certified_projection_proof() {
    use helm_schema_core::{Predicate, PredicateMemo};
    let proof = Predicate::Or(vec![
        Predicate::truthy_path("left"),
        Predicate::truthy_path("right"),
    ]);
    let mut remaining = 0;
    sim_assert_eq!(
        have: super::normalize_projection(proof.clone(), &PredicateMemo::new(), &mut remaining),
        want: Some(proof)
    );
    let oversized = Predicate::all(
        (0..super::MAX_PROJECTION_WORK)
            .map(|index| Predicate::truthy_path(format!("key{index}")))
            .collect(),
    );
    sim_assert_eq!(
        have: super::normalize_projection(oversized, &PredicateMemo::new(), &mut remaining),
        want: None
    );
}

#[test]
fn projection_allowance_covers_the_complete_runtime_dag_query() {
    use super::{Arena, Coverage, DecisionEdge, RuntimeGuard};
    use helm_schema_core::Predicate;
    let arena = Arena::default();
    let mut guard = RuntimeGuard::False;
    for index in 0..12 {
        let atom = Predicate::truthy_path(format!("branch{index}"));
        let atom = arena.runtime_path(&[DecisionEdge {
            test: arena.decision(atom.clone(), atom.negated(), Coverage::Exact),
            taken: true,
        }]);
        guard = arena.union_runtime_guards(&guard, &atom);
    }
    let mut remaining = 8;
    let result = arena
        .guards
        .borrow_mut()
        .project_with_work(&guard, &mut remaining);
    sim_assert_eq!(have: remaining, want: 0);
    sim_assert_eq!(have: result.necessary, want: Predicate::True);
    sim_assert_eq!(have: result.sufficient, want: Predicate::False);
    assert!(guard.is_input_exact());
}

#[test]
fn unary_projection_deduplicates_shared_conjuncts_before_reconvergence() -> eyre::Result<()> {
    use helm_schema_core::{Predicate, PredicateMemo};
    let memo = PredicateMemo::new();
    let atom = Predicate::truthy_path("shared");
    let mut predicate = atom.clone();
    // Repeated joins must not double the unfolded formula at each continuation.
    for _ in 0..80 {
        predicate = project_union(
            vec![Predicate::And(vec![predicate.clone(), predicate])],
            &memo,
        )
        .ok_or_eyre("unary projection unexpectedly exhausted")?;
        sim_assert_eq!(have: &predicate, want: &atom);
    }
    Ok(())
}

#[test]
fn control_projection_exhaustion_keeps_runtime_choices_and_widens_both_proofs() {
    use super::{Arena, Coverage, DecisionEdge};
    use helm_schema_core::Predicate;
    use std::rc::Rc;

    let arena = Arena::default();
    let predicate = Predicate::all(
        (0..5000)
            .map(|index| Predicate::truthy_path(format!("large{index}")))
            .collect(),
    );
    let decision = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let runtime = arena.runtime_path(&[DecisionEdge {
        test: decision,
        taken: true,
    }]);
    let test = arena.guard_test(&mut arena.guards.borrow_mut(), runtime.clone());
    sim_assert_eq!(have: test.coverage, want: Coverage::Partial);
    sim_assert_eq!(have: &test.runtime, want: &Some(runtime));
    sim_assert_eq!(have: &test.when_true, want: &Predicate::False);
    sim_assert_eq!(have: &test.when_false, want: &Predicate::False);

    // Equal exported proofs cannot identify independent runtime decisions.
    let first = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
    let second = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
    assert_ne!(first.id, second.id);
    for left in [false, true] {
        for right in [false, true] {
            assert!(
                !arena
                    .runtime_path(&[
                        DecisionEdge {
                            test: Rc::clone(&first),
                            taken: left
                        },
                        DecisionEdge {
                            test: Rc::clone(&second),
                            taken: right
                        },
                    ])
                    .is_impossible()
            );
        }
    }
}

#[test]
fn frozen_projection_unions_preserve_bounds_on_hits_and_after_eviction() {
    use helm_schema_core::{ApproximationRole, Guard, Predicate, PredicateMemo, ValuesPath};

    use super::{DecisionStore, MAX_PROJECTION_UNIONS, ProjectionUnionKey};

    let memo = PredicateMemo::new();
    let mut store = DecisionStore::default();
    let limit = super::MAX_PROJECTION_WORK;
    let mut fixtures = [0, 1, limit - 2, limit - 1, limit, limit + 1]
        .map(|count| {
            (0..count)
                .map(|index| {
                    Predicate::from(Guard::Truthy {
                        path: ValuesPath::parse(&format!("branch{index:03}")),
                    })
                })
                .collect::<Vec<_>>()
        })
        .to_vec();
    let atom = |name| {
        Predicate::from(Guard::Truthy {
            path: ValuesPath::parse(name),
        })
    };
    let prefix = atom("prefix");
    let left = atom("left");
    let right = atom("right");
    fixtures.push(vec![
        Predicate::all(vec![prefix.clone(), left.clone()]),
        Predicate::all(vec![prefix.clone(), right.clone()]),
    ]);
    let approximate = Predicate::Approximate(
        "unknown arm".to_owned(),
        std::collections::BTreeSet::from([ValuesPath::parse("opaque")]),
        ApproximationRole::Control,
        Some(Box::new(left)),
    );
    fixtures.push(vec![
        Predicate::all(vec![prefix.clone(), approximate]),
        Predicate::all(vec![prefix, right]),
    ]);
    for parts in &fixtures {
        let want = project_union(parts.clone(), &memo);
        sim_assert_eq!(have: store.project_union(parts.clone(), &memo), want: want.clone());
        let entries = store.projection_unions.len();
        sim_assert_eq!(have: store.project_union(parts.clone(), &memo), want: want);
        sim_assert_eq!(have: store.projection_unions.len(), want: entries);
        if parts.len() >= limit {
            sim_assert_eq!(
                have: store.project_union(parts.clone(), &memo).is_none(),
                want: true
            );
        }
    }
    // Fill with disjoint formulas so the original entries must actually be evicted.
    for index in 0..MAX_PROJECTION_UNIONS {
        let result = store.project_union(
            vec![Predicate::from(Guard::Truthy {
                path: ValuesPath::parse(&format!("eviction{index}")),
            })],
            &memo,
        );
        assert!(result.is_some());
    }
    for parts in &fixtures {
        assert!(!store.projection_unions.contains_key(&ProjectionUnionKey {
            parts: parts.clone()
        }));
        sim_assert_eq!(
            have: store.project_union(parts.clone(), &memo),
            want: project_union(parts.clone(), &memo)
        );
    }
    assert!(store.projection_unions.len() <= MAX_PROJECTION_UNIONS);
}

fn guard_cache_receipt(
    store: &super::DecisionStore,
) -> std::collections::BTreeMap<String, Option<u64>> {
    let mut receipt =
        std::collections::BTreeMap::from([("next_node".to_owned(), Some(store.next_node))]);
    for (key, node) in &store.nodes {
        receipt.insert(format!("node/{key:?}"), node.upgrade().map(|node| node.id));
    }
    for (key, guard) in &store.applications {
        receipt.insert(
            format!("application/{key:?}"),
            guard.upgrade().map(|guard| guard.id()),
        );
    }
    for (key, guard) in &store.negations {
        receipt.insert(
            format!("negation/{key}"),
            guard.upgrade().map(|guard| guard.id()),
        );
    }
    receipt
}

#[test]
fn conjunction_memo_preserves_component_cache_work() {
    use super::{Arena, Coverage, DecisionEdge};
    use helm_schema_core::{Guard, Predicate, ValuesPath};
    use std::rc::Rc;

    let arena = Arena::default();
    let predicate = Predicate::from(Guard::Truthy {
        path: ValuesPath::parse("exact"),
    });
    let exact = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let partial = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
    let mut store = arena.guards.borrow_mut();
    for second in [&partial, &exact] {
        for taken in [true, false] {
            let edges = [
                DecisionEdge {
                    test: Rc::clone(&exact),
                    taken: true,
                },
                DecisionEdge {
                    test: Rc::clone(second),
                    taken,
                },
            ];
            let cold = store.conjunction_uncached(&edges, &arena.next_decision);
            let before = guard_cache_receipt(&store);
            let decisions = arena.next_decision.get();
            sim_assert_eq!(have: store.conjunction(&edges, &arena.next_decision), want: cold.clone());
            sim_assert_eq!(have: store.conjunction(&edges, &arena.next_decision), want: cold);
            sim_assert_eq!(have: guard_cache_receipt(&store), want: before);
            sim_assert_eq!(have: arena.next_decision.get(), want: decisions);
        }
    }
}

#[test]
fn conjunction_memo_rebuilds_expired_collection_survivors() {
    use super::{Arena, Coverage, DecisionEdge, GuardWorkBudget};
    use helm_schema_core::{Guard, Predicate, ValuesPath};
    use std::rc::Rc;

    let arena = Arena::default();
    let predicate = Predicate::from(Guard::Truthy {
        path: ValuesPath::parse("survivor"),
    });
    let test = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let edges = [
        DecisionEdge {
            test: Rc::clone(&test),
            taken: true,
        },
        DecisionEdge { test, taken: false },
    ];
    let mut store = arena.guards.borrow_mut();
    let [positive_edge, negative_edge] = &edges;
    let positive = store.edge(
        positive_edge,
        &arena.next_decision,
        &mut super::GuardWorkBudget::new(super::MAX_PROJECTION_WORK),
    );
    let negative = store.edge(
        negative_edge,
        &arena.next_decision,
        &mut super::GuardWorkBudget::new(super::MAX_PROJECTION_WORK),
    );
    for index in 0..4_096 {
        let predicate = Predicate::from(Guard::Truthy {
            path: ValuesPath::parse(&format!("temporary{index}")),
        });
        store.edge(
            &DecisionEdge {
                test: arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact),
                taken: true,
            },
            &arena.next_decision,
            &mut super::GuardWorkBudget::new(super::MAX_PROJECTION_WORK),
        );
    }
    store.collect_unused();
    sim_assert_eq!(have: store.working.as_slice(), want: &[] as &[_]);
    let before = store.next_node;
    assert!(
        store
            .conjunction(&edges, &arena.next_decision)
            .is_impossible()
    );
    sim_assert_eq!(have: store.next_node, want: before);
    sim_assert_eq!(have: store.conjunctions.len(), want: 1);
    // The cached constant owns no guards; surviving atoms can die after collection.
    drop(positive);
    drop(negative);
    assert!(store.queries.conjunctions_dirty.get());
    assert!(
        store
            .conjunction(&edges, &arena.next_decision)
            .is_impossible()
    );
    sim_assert_eq!(have: store.next_node, want: before + 2);
    let positive = store.edge(
        positive_edge,
        &arena.next_decision,
        &mut super::GuardWorkBudget::new(super::MAX_PROJECTION_WORK),
    );
    // Skipping reconstruction would leave this bounded query without its cold-run cache hit.
    assert!(
        store
            .negate_with_work(&positive, &mut GuardWorkBudget::new(0))
            .is_ok()
    );
}

#[test]
fn conjunction_memo_eviction_preserves_component_cache_work() {
    use super::{Arena, Coverage, DecisionEdge, GuardWorkBudget, MAX_CONJUNCTIONS};
    use helm_schema_core::{Guard, Predicate, ValuesPath};
    use std::rc::Rc;

    let arena = Arena::default();
    let predicate = Predicate::from(Guard::Truthy {
        path: ValuesPath::parse("retained"),
    });
    let test = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let edges = [
        DecisionEdge {
            test: Rc::clone(&test),
            taken: true,
        },
        DecisionEdge {
            test: Rc::clone(&test),
            taken: false,
        },
    ];
    let mut store = arena.guards.borrow_mut();
    assert!(
        store
            .conjunction(&edges, &arena.next_decision)
            .is_impossible()
    );
    for index in 0..MAX_CONJUNCTIONS {
        let predicate = Predicate::from(Guard::Truthy {
            path: ValuesPath::parse(&format!("other{index}")),
        });
        store.conjunction(
            &[
                DecisionEdge {
                    test: Rc::clone(&test),
                    taken: true,
                },
                DecisionEdge {
                    test: arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact),
                    taken: true,
                },
            ],
            &arena.next_decision,
        );
    }
    sim_assert_eq!(have: store.conjunctions.len(), want: 1);
    let before = guard_cache_receipt(&store);
    assert!(
        store
            .conjunction(&edges, &arena.next_decision)
            .is_impossible()
    );
    sim_assert_eq!(have: guard_cache_receipt(&store), want: before);
    let [positive_edge, _] = &edges;
    let positive = store.edge(
        positive_edge,
        &arena.next_decision,
        &mut super::GuardWorkBudget::new(super::MAX_PROJECTION_WORK),
    );
    assert!(
        store
            .negate_with_work(&positive, &mut GuardWorkBudget::new(0))
            .is_ok()
    );
}

fn assert_guard_query_laws(guards: &[super::RuntimeGuard]) {
    for guard in guards {
        for decision in 0..4 {
            for taken in [true, false] {
                for outcome in [true, false] {
                    let want = guard.possible_atom_uncached(decision, taken, outcome);
                    sim_assert_eq!(have: guard.possible_atom(decision, taken, outcome), want: want);
                    sim_assert_eq!(have: guard.possible_atom(decision, taken, outcome), want: want);
                }
            }
        }
        for other in guards {
            for outcome in [true, false] {
                for other_outcome in [true, false] {
                    let want = guard.compatible_uncached(outcome, other, other_outcome);
                    sim_assert_eq!(have: guard.compatible(outcome, other, other_outcome), want: want);
                    sim_assert_eq!(have: guard.compatible(outcome, other, other_outcome), want: want);
                }
            }
        }
    }
}

#[test]
fn pure_guard_queries_preserve_results_budgets_and_lifetimes() -> eyre::Result<()> {
    use super::{
        Arena, AtomQuery, CompatibilityQuery, Coverage, DecisionEdge, GuardWorkBudget,
        MAX_GUARD_QUERIES, RuntimeGuard,
    };
    use helm_schema_core::{Guard, Predicate, ValuesPath};
    use std::rc::Rc;

    let arena = Arena::default();
    let predicate = Predicate::from(Guard::Truthy {
        path: ValuesPath::parse("query"),
    });
    let exact = arena.runtime_path(&[DecisionEdge {
        test: arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact),
        taken: true,
    }]);
    let partial = arena.runtime_path(&[DecisionEdge {
        test: arena.decision(Predicate::False, Predicate::False, Coverage::Partial),
        taken: true,
    }]);
    let independent = arena.runtime_path(&[DecisionEdge {
        test: arena.decision(Predicate::False, Predicate::False, Coverage::Partial),
        taken: true,
    }]);
    let joined = arena.intersect_runtime_guards(&exact, &partial);
    let weak = match &exact {
        RuntimeGuard::Branch(node) => Rc::downgrade(node),
        RuntimeGuard::False | RuntimeGuard::True => eyre::bail!("query fixture collapsed"),
    };
    let queries = Rc::clone(&arena.guards.borrow().queries);
    let atom_key = AtomQuery {
        guard: exact.id(),
        decision: 0,
        taken: true,
        outcome: true,
    };
    let compatibility_key = CompatibilityQuery {
        left: exact.id(),
        right: partial.id(),
        left_outcome: true,
        right_outcome: true,
    };
    let before = guard_cache_receipt(&arena.guards.borrow());
    assert!(
        arena
            .guards
            .borrow_mut()
            .negate_with_work(&exact, &mut GuardWorkBudget::new(0))
            .is_err()
    );
    let guards = [
        RuntimeGuard::False,
        RuntimeGuard::True,
        exact.clone(),
        partial,
        independent,
        joined,
    ];
    assert_guard_query_laws(&guards);
    assert!(queries.atoms.borrow().contains_key(&atom_key));
    assert!(
        queries
            .compatibilities
            .borrow()
            .contains_key(&compatibility_key)
    );
    sim_assert_eq!(have: guard_cache_receipt(&arena.guards.borrow()), want: before);
    assert!(
        arena
            .guards
            .borrow_mut()
            .negate_with_work(&exact, &mut GuardWorkBudget::new(0))
            .is_err()
    );
    // Disjoint keys force both bounded caches to evict the original queries.
    for index in 0..MAX_GUARD_QUERIES {
        sim_assert_eq!(have: exact.possible_atom(u64::try_from(index)? + 100_000, true, true), want: Some(true));
        let predicate = Predicate::from(Guard::Truthy {
            path: ValuesPath::parse(&format!("query_eviction{index}")),
        });
        let other = arena.runtime_path(&[DecisionEdge {
            test: arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact),
            taken: true,
        }]);
        sim_assert_eq!(have: exact.compatible(true, &other, true), want: Some(true));
    }
    assert!(!queries.atoms.borrow().contains_key(&atom_key));
    assert!(
        !queries
            .compatibilities
            .borrow()
            .contains_key(&compatibility_key)
    );
    let before = guard_cache_receipt(&arena.guards.borrow());
    assert_guard_query_laws(&guards);
    sim_assert_eq!(have: guard_cache_receipt(&arena.guards.borrow()), want: before);
    assert!(queries.atoms.borrow().len() <= MAX_GUARD_QUERIES);
    assert!(queries.compatibilities.borrow().len() <= MAX_GUARD_QUERIES);
    drop(guards);
    drop(exact);
    arena.guards.borrow_mut().working.clear();
    assert!(weak.upgrade().is_none());
    Ok(())
}

#[test]
fn guard_query_keys_do_not_cross_node_id_contexts() {
    use super::{Arena, Coverage, DecisionEdge};
    use helm_schema_core::{Guard, Predicate, ValuesPath};

    let first_arena = Arena::default();
    let second_arena = Arena::default();
    second_arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
    let guard = |arena: &Arena| {
        let predicate = Predicate::from(Guard::Truthy {
            path: ValuesPath::parse("context"),
        });
        arena.runtime_path(&[DecisionEdge {
            test: arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact),
            taken: true,
        }])
    };
    let first = guard(&first_arena);
    let second = guard(&second_arena);
    sim_assert_eq!(have: first.id(), want: second.id());
    sim_assert_eq!(have: first.compatible(true, &first, false), want: Some(false));
    sim_assert_eq!(have: first.compatible_uncached(true, &second, false), want: Some(true));
    sim_assert_eq!(have: first.compatible(true, &second, false), want: Some(true));
    sim_assert_eq!(have: second.compatible(false, &first, true), want: Some(true));
}

#[test]
fn atom_query_keeps_a_foreign_arena_binding_independent() {
    use super::{Arena, Coverage, DecisionEdge, RuntimeGuard};
    use helm_schema_core::Predicate;

    let local = Arena::default();
    let foreign = Arena::default();
    let local_predicate = Predicate::truthy_path("local");
    let foreign_predicate = Predicate::truthy_path("foreign");
    let local_test = local.decision(
        local_predicate.clone(),
        local_predicate.negated(),
        Coverage::Exact,
    );
    let foreign_test = foreign.decision(
        foreign_predicate.clone(),
        foreign_predicate.negated(),
        Coverage::Exact,
    );
    sim_assert_eq!(have: local_test.id, want: foreign_test.id);
    let foreign_guard = foreign.runtime_path(&[DecisionEdge {
        test: foreign_test,
        taken: false,
    }]);
    let guard =
        local
            .guards
            .borrow_mut()
            .select(local_test.clone(), foreign_guard, RuntimeGuard::False);
    sim_assert_eq!(have: guard.possible_atom(local_test.id, true, true), want: Some(true));
    sim_assert_eq!(have: guard.possible_atom(local_test.id, true, false), want: Some(true));
    sim_assert_eq!(have: guard.possible_atom(local_test.id, false, true), want: Some(false));
    sim_assert_eq!(have: guard.possible_atom(local_test.id, false, false), want: Some(true));
}

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
fn bounded_projection_caches_preserve_both_proof_directions() -> eyre::Result<()> {
    use super::{Arena, Coverage, DecisionEdge, RuntimeGuard};
    use helm_schema_core::{Guard, Predicate, ValuesPath};
    use std::collections::BTreeSet;
    use std::rc::Rc;

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
    let first = arena.guard_test(&mut arena.guards.borrow_mut(), guard.clone());
    let second = arena.budget_test(guard.clone());
    sim_assert_eq!(have: first.coverage, want: Coverage::Partial);
    assert!(Rc::ptr_eq(&first, &second));
    sim_assert_eq!(have: first.runtime.as_ref().ok_or_eyre("runtime missing")?.id(), want: guard.id());
    let first_path = paths.first().ok_or_eyre("first fixture path missing")?;
    let last_path = paths.last().ok_or_eyre("last fixture path missing")?;
    // Every unlisted Boolean is false, so each set specifies a complete assignment.
    for true_paths in [
        BTreeSet::new(),
        BTreeSet::from([first_path.clone()]),
        BTreeSet::from([last_path.clone()]),
        paths.iter().step_by(2).cloned().collect(),
        paths.iter().cloned().collect(),
    ] {
        let actual = !true_paths.is_empty();
        // Both exported proofs remain subsets of their runtime outcome domains.
        assert!(!fixture_truth(&first.when_true, &true_paths)? || actual);
        assert!(!fixture_truth(&first.when_false, &true_paths)? || !actual);
    }
    Ok(())
}

#[test]
fn guard_caches_release_dead_nodes_without_changing_live_identity() -> eyre::Result<()> {
    use std::rc::Rc;

    use helm_schema_core::{Guard, Predicate, ValuesPath};

    use super::{Arena, Coverage, DecisionEdge, RuntimeGuard};

    let arena = Arena::default();
    let decision = arena.decision(Predicate::True, Predicate::True, Coverage::Partial);
    let edge = DecisionEdge {
        test: decision,
        taken: true,
    };
    let live = arena.runtime_path(std::slice::from_ref(&edge));
    let projected = arena.guard_test(&mut arena.guards.borrow_mut(), live.clone());
    let dead = arena.guards.borrow_mut().negate(
        &live,
        &arena.next_decision,
        &mut super::GuardWorkBudget::new(super::MAX_PROJECTION_WORK),
    );
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
    let again = arena.guard_test(&mut arena.guards.borrow_mut(), live.clone());
    assert!(Rc::ptr_eq(&projected, &again));
    assert!(weak.upgrade().is_none());
    sim_assert_eq!(have: arena.guards.borrow().nodes.len(), want: 1);
    sim_assert_eq!(have: arena.runtime_path(&[edge]).id(), want: live.id());
    let recreated = arena.guards.borrow_mut().negate(
        &live,
        &arena.next_decision,
        &mut super::GuardWorkBudget::new(super::MAX_PROJECTION_WORK),
    );
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

    use super::{Arena, Coverage, DecisionEdge, GuardWorkBudget, RuntimeGuard};

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
            &mut GuardWorkBudget::new(8),
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
    assert!(
        arena
            .select_runtime_guard_bounded(
                &condition,
                &yes,
                &RuntimeGuard::False,
                &mut GuardWorkBudget::new(0),
            )
            .is_err()
    );
    let cached = arena
        .select_runtime_guard_bounded(
            &condition,
            &yes,
            &RuntimeGuard::False,
            &mut GuardWorkBudget::new(16),
        )
        .map_err(|reason| eyre::eyre!("cached exact query exhausted: {reason:?}"))?;
    sim_assert_eq!(have: cached.id(), want: exact.id());
    let projected = arena.guard_test(&mut arena.guards.borrow_mut(), exact);
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
        .select_runtime_guard_bounded(&condition, &yes, &no, &mut GuardWorkBudget::new(5))
        .map_err(|reason| eyre::eyre!("single-node choice exhausted: {reason:?}"))?;
    sim_assert_eq!(have: arena.guards.borrow().next_node - before, want: 1);
    assert!(
        arena
            .select_runtime_guard_bounded(&condition, &yes, &no, &mut GuardWorkBudget::new(0),)
            .is_err()
    );
    let cached = arena
        .select_runtime_guard_bounded(&condition, &yes, &no, &mut GuardWorkBudget::new(3))
        .map_err(|reason| eyre::eyre!("cached choice exhausted: {reason:?}"))?;
    sim_assert_eq!(have: cached.id(), want: result.id());
    sim_assert_eq!(have: arena.intersect_runtime_guards(&result, &condition).id(), want: arena.intersect_runtime_guards(&yes, &condition).id());
    Ok(())
}

#[test]
fn partial_projection_skips_only_an_absorbed_sibling_() -> eyre::Result<()> {
    use std::rc::Rc;

    use super::{Arena, Coverage, RuntimeGuard};
    use helm_schema_core::Predicate;

    for exact_outer in [false, true] {
        for constant_yes in [None, Some(true), Some(false)] {
            let arena = Arena::default();
            let a = Predicate::truthy_path("a");
            let input = arena.decision(a.clone(), a.negated(), Coverage::Exact);
            let sibling =
                arena
                    .guards
                    .borrow_mut()
                    .select(input, RuntimeGuard::True, RuntimeGuard::False);
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
            let guard = arena
                .guards
                .borrow_mut()
                .select(Rc::clone(&outer), yes, sibling.clone());
            let sibling_key = sibling.id();
            assert!(!arena.guards.borrow().projections.contains_key(&sibling_key));
            let projection = arena
                .guards
                .borrow_mut()
                .project(&guard, &arena.predicate_memo);
            let absorbed = !exact_outer && constant_yes.is_none();
            sim_assert_eq!(
                have: arena.guards.borrow().projections.contains_key(&sibling_key),
                want: !absorbed,
                "exact_outer={exact_outer}, yes={constant_yes:?}"
            );
            if absorbed {
                sim_assert_eq!(have: projection.necessary.clone(), want: Predicate::True);
                sim_assert_eq!(have: projection.sufficient.clone(), want: Predicate::False);
            }
            verify_partial_projection_outcomes(&projection, exact_outer, constant_yes)?;
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
        let yes = store.conjunction(yes, &self.next_decision);
        let no = store.conjunction(no, &self.next_decision);
        let runtime = store.apply(
            super::Operation::Or,
            &yes,
            &no,
            &self.next_decision,
            &mut super::GuardWorkBudget::new(super::MAX_PROJECTION_WORK),
        );
        self.guard_test(&mut store, runtime)
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
                    if let Some(runtime) = &node.test.runtime {
                        pending.push(runtime);
                    } else {
                        atoms.insert(node.test.id);
                    }
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
        RuntimeGuard::Branch(node) => {
            let outcomes = node.test.runtime.as_ref().map_or_else(
                || {
                    assigned.get(&node.test.id).copied().map_or_else(
                        || std::collections::BTreeSet::from([false, true]),
                        |value| std::collections::BTreeSet::from([value]),
                    )
                },
                |runtime| truth_query_results(runtime, assigned),
            );
            outcomes
                .into_iter()
                .flat_map(|taken| {
                    truth_query_results(if taken { &node.yes } else { &node.no }, assigned)
                })
                .collect()
        }
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
        std::rc::Rc::make_mut(&mut state.active).push(DecisionEdge {
            test: Rc::clone(&selector),
            taken: true,
        });
        let yes = state.test_with_unknown(&yes, &mut fallback);
        std::rc::Rc::make_mut(&mut state.active).pop();
        std::rc::Rc::make_mut(&mut state.active).push(DecisionEdge {
            test: Rc::clone(&selector),
            taken: false,
        });
        let no = state.test_with_unknown(&no, &mut fallback);
        std::rc::Rc::make_mut(&mut state.active).pop();
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

#[test]
fn dead_guard_arms_do_not_spend_the_live_projection_budget() {
    use helm_schema_core::{Predicate, PredicateMemo};
    let memo = PredicateMemo::new();
    let live = Predicate::truthy_path("live");
    let large = Predicate::all(
        (0..300)
            .map(|index| Predicate::truthy_path(format!("dead{index}")))
            .collect(),
    );
    let dead = Predicate::all(vec![large.clone(), Predicate::False]);
    sim_assert_eq!(have: project_union(vec![live.clone(), dead.clone()], &memo), want: Some(live.clone()));
    assert!(project_union(vec![large], &memo).is_some());
}

#[test]
fn deferred_guards_preserve_their_primitive_assignment() {
    use super::{Arena, Coverage, DecisionEdge, RuntimeGuard};
    use helm_schema_core::Predicate;
    let arena = Arena::default();
    let predicate = Predicate::truthy_path("primitive");
    let atom = arena.runtime_path(&[DecisionEdge {
        test: arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact),
        taken: true,
    }]);
    let mut store = arena.guards.borrow_mut();
    let first = store.deferred_choice(
        atom.clone(),
        RuntimeGuard::True,
        RuntimeGuard::False,
        &arena.next_decision,
    );
    let second = store.deferred_choice(
        atom.clone(),
        RuntimeGuard::True,
        RuntimeGuard::False,
        &arena.next_decision,
    );
    drop(store);
    // Separate deferred selectors expand the same condition under one assignment.
    for left in [false, true] {
        for right in [false, true] {
            sim_assert_eq!(have: first.compatible(left, &atom, right), want: Some(left == right));
            sim_assert_eq!(have: first.compatible(left, &second, right), want: Some(left == right));
        }
    }
    let negated = arena.guards.borrow_mut().negate(
        &atom,
        &arena.next_decision,
        &mut super::GuardWorkBudget::new(super::MAX_PROJECTION_WORK),
    );
    assert!(
        arena
            .intersect_runtime_guards(&first, &negated)
            .is_impossible()
    );
    assert!(first.is_input_exact());
    let exported = arena.guard_test(&mut arena.guards.borrow_mut(), first);
    sim_assert_eq!(have: exported.coverage, want: Coverage::Partial);
}

#[test]
fn deferred_query_exhaustion_is_uncached_and_deep_guards_drop_iteratively() {
    use super::{Arena, Coverage, DecisionEdge, RuntimeGuard};
    use helm_schema_core::Predicate;
    let arena = Arena::default();
    let predicate = Predicate::truthy_path("deep");
    let mut guard = arena.runtime_path(&[DecisionEdge {
        test: arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact),
        taken: true,
    }]);
    {
        let mut store = arena.guards.borrow_mut();
        for _ in 0..20_000 {
            guard = store.deferred_choice(
                guard,
                RuntimeGuard::True,
                RuntimeGuard::False,
                &arena.next_decision,
            );
        }
    }
    let before = arena.guards.borrow().queries.compatibilities.borrow().len();
    for _ in 0..2 {
        sim_assert_eq!(have: guard.compatible(true, &RuntimeGuard::True, true), want: None);
        sim_assert_eq!(have: arena.guards.borrow().queries.compatibilities.borrow().len(), want: before);
        assert!(!guard.is_impossible());
    }
    arena.guards.borrow_mut().working.clear();
    drop(guard);
    arena.guards.borrow_mut().collect_unused();
    sim_assert_eq!(have: arena.guards.borrow().nodes.len(), want: 0);
}

#[test]
fn deferred_nonconstant_choices_and_negation_match_every_assignment() -> eyre::Result<()> {
    use super::{Arena, Coverage, DecisionEdge};
    use helm_schema_core::Predicate;
    use std::collections::{BTreeMap, BTreeSet};
    let arena = Arena::default();
    let tests: Vec<_> = ["a", "b", "c"]
        .into_iter()
        .map(|name| {
            let predicate = Predicate::truthy_path(name);
            arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact)
        })
        .collect();
    let mut atoms = tests.iter().map(|test| {
        arena.runtime_path(&[DecisionEdge {
            test: test.clone(),
            taken: true,
        }])
    });
    let a = atoms.next().ok_or_eyre("missing a")?;
    let b = atoms.next().ok_or_eyre("missing b")?;
    let c = atoms.next().ok_or_eyre("missing c")?;
    let mut store = arena.guards.borrow_mut();
    let first = store.deferred_choice(a, b.clone(), c.clone(), &arena.next_decision);
    let second = store.deferred_choice(first.clone(), c, b, &arena.next_decision);
    let negated = store.negate(
        &second,
        &arena.next_decision,
        &mut super::GuardWorkBudget::new(super::MAX_PROJECTION_WORK),
    );
    drop(store);
    for bits in 0_u8..8 {
        let assigned: BTreeMap<_, _> = tests
            .iter()
            .enumerate()
            .map(|(index, test)| (test.id, bits & (1 << index) != 0))
            .collect();
        let active: Vec<_> = tests
            .iter()
            .map(|test| DecisionEdge {
                test: test.clone(),
                taken: assigned[&test.id],
            })
            .collect();
        let active = arena.runtime_path(&active);
        for guard in [&first, &second, &negated] {
            let expected = truth_query_results(guard, &assigned);
            for outcome in [false, true] {
                sim_assert_eq!(have: guard.compatible(outcome, &active, true), want: Some(expected.contains(&outcome)));
            }
        }
        sim_assert_eq!(have: truth_query_atoms(&second), want: tests.iter().map(|test|test.id).collect::<BTreeSet<_>>());
    }
    Ok(())
}

#[test]
fn complete_path_uses_one_canonicalization_allowance() -> eyre::Result<()> {
    use super::{Arena, Coverage, DecisionEdge, RuntimeGuard};
    use helm_schema_core::Predicate;
    let arena = Arena::default();
    let edges: Vec<_> = (0..2500)
        .map(|index| {
            let predicate = Predicate::truthy_path(format!("path{index}"));
            DecisionEdge {
                test: arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact),
                taken: true,
            }
        })
        .collect();
    let guard = arena.runtime_path(&edges);
    let RuntimeGuard::Branch(node) = guard else {
        eyre::bail!("expected a nonconstant path")
    };
    // A fresh allowance per edge would canonicalize the entire unbounded path.
    assert!(!node.alias_free);
    assert!(node.input_exact);
    Ok(())
}

#[test]
fn application_memo_is_bounded_even_when_every_operand_stays_live() {
    use super::{Arena, Coverage, DecisionEdge, RuntimeGuard};
    use helm_schema_core::Predicate;
    let arena = Arena::default();
    let limit = 32_768;
    let mut operands = Vec::new();
    // Constant results retain no nodes, but their live operand pairs survive ordinary GC.
    for index in 0..=limit {
        let predicate = Predicate::truthy_path(format!("memo{index}"));
        let test = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        let positive = arena.runtime_path(&[DecisionEdge {
            test: test.clone(),
            taken: true,
        }]);
        let negative = arena.runtime_path(&[DecisionEdge { test, taken: false }]);
        sim_assert_eq!(have: arena.intersect_runtime_guards(&positive, &negative), want: RuntimeGuard::False);
        operands.extend([positive, negative]);
    }
    arena.guards.borrow_mut().collect_unused();
    assert!(arena.guards.borrow().applications.len() <= limit);
    // Eviction cannot change the retained inputs or their Boolean relation.
    let (pairs, remainder) = operands.as_chunks::<2>();
    sim_assert_eq!(have: remainder, want: &[] as &[RuntimeGuard]);
    for [positive, negative] in pairs {
        sim_assert_eq!(have: arena.intersect_runtime_guards(positive, negative), want: RuntimeGuard::False);
    }
    assert!(arena.guards.borrow().applications.len() <= limit);
}

#[test]
fn application_eviction_rebuilds_skipped_conjunction_work() -> eyre::Result<()> {
    use super::{Arena, Coverage, DecisionEdge, GuardWorkBudget, Operation};
    use helm_schema_core::Predicate;
    let arena = Arena::default();
    let edges: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|name| {
            let predicate = Predicate::truthy_path(name);
            DecisionEdge {
                test: arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact),
                taken: true,
            }
        })
        .collect();
    let first = arena.runtime_path(edges.get(..1).ok_or_eyre("missing first edge")?);
    let second = arena.runtime_path(edges.get(1..).ok_or_eyre("missing second edge")?);
    let retained = arena.runtime_path(&edges);
    // Evict on the final unrelated insertion, while the conjunction result remains live.
    for index in 0..32_768 {
        let predicate = Predicate::truthy_path(format!("unrelated{index}"));
        let test = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
        let positive = arena.runtime_path(&[DecisionEdge {
            test: test.clone(),
            taken: true,
        }]);
        let negative = arena.runtime_path(&[DecisionEdge { test, taken: false }]);
        assert!(
            arena
                .intersect_runtime_guards(&positive, &negative)
                .is_impossible()
        );
    }
    sim_assert_eq!(have: arena.runtime_path(&edges), want: retained.clone());
    let rebuilt = arena.guards.borrow_mut().apply_with_work(
        Operation::And,
        &first,
        &second,
        &mut GuardWorkBudget::new(0),
    );
    let rebuilt =
        rebuilt.map_err(|reason| eyre::eyre!("component cache was not rebuilt: {reason:?}"))?;
    sim_assert_eq!(have: rebuilt, want: retained);
    Ok(())
}

#[test]
fn local_guard_facts_are_proved_before_expanding_ambient_history() {
    use super::{Arena, Coverage, DecisionEdge, RuntimeGuard};
    use helm_schema_core::Predicate;
    let arena = Arena::default();
    let predicate = Predicate::truthy_path("ambient");
    let mut ambient = arena.runtime_path(&[DecisionEdge {
        test: arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact),
        taken: true,
    }]);
    let predicate = Predicate::truthy_path("local");
    let local = arena.decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    let atom = arena.runtime_path(&[DecisionEdge {
        test: local.clone(),
        taken: true,
    }]);
    let opposite = arena.complement_runtime_guard(&atom);
    let mut store = arena.guards.borrow_mut();
    for _ in 0..20_000 {
        ambient = store.deferred_choice(
            ambient,
            RuntimeGuard::True,
            RuntimeGuard::False,
            &arena.next_decision,
        );
    }
    let live = store.deferred_choice(
        ambient.clone(),
        atom,
        RuntimeGuard::False,
        &arena.next_decision,
    );
    let dead = store.deferred_choice(
        live.clone(),
        opposite,
        RuntimeGuard::False,
        &arena.next_decision,
    );
    drop(store);
    // The ambient condition alone exceeds the allowance, but local facts need no traversal of it.
    sim_assert_eq!(have: ambient.compatible(true, &RuntimeGuard::True, true), want: None);
    let implied = live.implies_test(&local);
    let impossible = dead.is_impossible();
    sim_assert_eq!(have: implied, want: Some(true));
    sim_assert_eq!(have: impossible, want: true);
}
