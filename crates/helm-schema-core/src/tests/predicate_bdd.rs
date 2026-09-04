use std::collections::BTreeSet;

use test_util::prelude::sim_assert_eq;

use super::{
    PredicateBdd, PredicateMemo, TRUE, exact_implies_uncached, memo_sizes, normalize_uncached,
};
use crate::{ApproximationRole, Guard, Predicate, ValuesPath};

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
