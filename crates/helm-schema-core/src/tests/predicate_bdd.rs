use std::collections::BTreeSet;

use test_util::prelude::sim_assert_eq;

use super::{
    FALSE, PredicateBdd, PredicateMemo, TRUE, exact_implies_uncached, memo_sizes,
    normalize_uncached, simplify_structure,
};
use crate::{ApproximationRole, Guard, Predicate, PredicateKind, ValuesPath};

fn guard(index: usize) -> Predicate {
    Predicate::from(Guard::Truthy {
        path: ValuesPath::from_segments([format!("value-{index}")]),
    })
}

fn generated_predicates() -> Vec<Predicate> {
    let first = guard(0);
    let second = guard(1);
    let approximation = Predicate::Approximate(
        "opaque".to_string(),
        BTreeSet::from([ValuesPath::from_segments(["opaque"])]),
        ApproximationRole::Control,
        Some(Box::new(first.clone())),
    );
    let normal_form_cap = Predicate::And(
        (0..9)
            .map(|index| Predicate::Or(vec![guard(index * 2), guard(index * 2 + 1)]))
            .collect(),
    );
    let bdd_node_cap = Predicate::And((0..=super::MAX_BDD_NODES).map(guard).collect());

    vec![
        Predicate::True,
        Predicate::False,
        first.clone(),
        first.negated(),
        Predicate::And(vec![first.clone(), second.clone(), approximation.clone()]),
        Predicate::Or(vec![first, second, approximation.clone()]),
        approximation,
        normal_form_cap,
        bdd_node_cap,
    ]
}

#[test]
fn memoized_predicate_operations_match_uncached_results() {
    let predicates = generated_predicates();

    for predicate in &predicates {
        let want = normalize_uncached(predicate.clone());

        let memo = PredicateMemo::new();
        let have = memo.normalize(predicate.clone());
        sim_assert_eq!(have: have.clone(), want: want);
        let populated = memo_sizes(&memo);
        sim_assert_eq!(have: memo.normalize(predicate.clone()), want: have);
        sim_assert_eq!(have: memo_sizes(&memo), want: populated);
    }

    for antecedent in &predicates {
        for consequent in &predicates {
            let want = exact_implies_uncached(antecedent, consequent);

            let memo = PredicateMemo::new();
            let have = memo.exactly_implies(antecedent, consequent);
            sim_assert_eq!(have: have, want: want);
            let populated = memo_sizes(&memo);
            sim_assert_eq!(
                have: memo.exactly_implies(antecedent, consequent),
                want: have
            );
            sim_assert_eq!(have: memo_sizes(&memo), want: populated);
        }
    }
}

#[test]
fn generated_predicates_reach_bounded_bdd_abstentions() {
    let normal_form_cap = Predicate::And(
        (0..9)
            .map(|index| Predicate::Or(vec![guard(index * 2), guard(index * 2 + 1)]))
            .collect(),
    );
    let true_paths = PredicateBdd::for_predicate(&normal_form_cap).and_then(|mut bdd| {
        let root = bdd.build(&normal_form_cap)?;
        Some(bdd.paths_to(root, TRUE))
    });
    assert!(matches!(true_paths, Some(None)));

    let bdd_node_cap = Predicate::And((0..=super::MAX_BDD_NODES).map(guard).collect());
    let built =
        PredicateBdd::for_predicate(&bdd_node_cap).and_then(|mut bdd| bdd.build(&bdd_node_cap));
    assert!(built.is_none());
}

#[test]
fn borrowed_atoms_canonicalize_negative_guards_without_cloning_keys() {
    let path = ValuesPath::from_segments(["selected"]);
    let positive = Predicate::from(Guard::Truthy { path: path.clone() });
    let negative = Predicate::from(Guard::Not { path });
    let predicate = Predicate::And(vec![positive.clone(), negative.clone()]);
    let bdd = PredicateBdd::for_predicate(&predicate);
    sim_assert_eq!(have: bdd.is_some(), want: true);
    let Some(mut bdd) = bdd else {
        return;
    };

    sim_assert_eq!(have: bdd.atoms.len(), want: 1);
    sim_assert_eq!(
        have: bdd.atoms[0].to_guard(),
        want: Guard::Truthy {
            path: ValuesPath::from_segments(["selected"]),
        }
    );
    sim_assert_eq!(have: bdd.build(&positive), want: Some(2));
    sim_assert_eq!(have: bdd.build(&negative), want: Some(3));
    sim_assert_eq!(have: bdd.build(&predicate), want: Some(FALSE));
}

fn same_compound_storage(left: &Predicate, right: &Predicate) -> bool {
    match (left.kind(), right.kind()) {
        (PredicateKind::And(left), PredicateKind::And(right))
        | (PredicateKind::Or(left), PredicateKind::Or(right)) => {
            std::ptr::eq(left.as_ptr(), right.as_ptr())
        }
        (PredicateKind::Not(left), PredicateKind::Not(right)) => std::ptr::eq(left, right),
        _ => false,
    }
}

#[test]
fn structural_simplification_retains_unchanged_boolean_dag_storage() {
    let shared = simplify_structure(Predicate::Or(vec![guard(1), guard(0).negated()]), None);
    let negated = Predicate::Not(Box::new(shared.clone()));
    let root = simplify_structure(Predicate::And(vec![shared.clone(), negated.clone()]), None);
    for predicate in [shared, negated, root] {
        let simplified = simplify_structure(predicate.clone(), None);
        sim_assert_eq!(have: simplified.clone(), want: predicate.clone());
        sim_assert_eq!(have: same_compound_storage(&predicate, &simplified), want: true);
    }
}

#[test]
fn structural_simplification_changes_only_the_required_branch() {
    let shared = simplify_structure(Predicate::Or(vec![guard(0), guard(1)]), None);
    let changed = Predicate::And(vec![
        Predicate::True,
        Predicate::Not(Box::new(Predicate::Not(Box::new(shared.clone())))),
    ]);
    let simplified = simplify_structure(changed, None);
    sim_assert_eq!(have: simplified.clone(), want: shared.clone());
    sim_assert_eq!(have: same_compound_storage(&shared, &simplified), want: true);
}
