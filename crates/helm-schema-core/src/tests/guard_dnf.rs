use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::GuardDnf;
use crate::{ConditionalGuard, Guard, Predicate, ValuesPath};

fn path(value: &str) -> ValuesPath {
    ValuesPath::parse(value)
}

fn truthy(path: &str) -> Predicate {
    Predicate::truthy_path(path)
}

#[test]
fn single_conjunction_union_matches_full_normalization_exhaustively() {
    let a = truthy("a");
    let b = truthy("b");
    let c = truthy("c");
    let alternatives = [
        Vec::new(),
        vec![a.clone()],
        vec![a.clone().negated()],
        vec![b.clone()],
        vec![a.clone(), b.clone()],
        vec![a.clone().negated(), b.clone()],
        vec![a.clone(), c.clone()],
        vec![a.negated(), c],
    ];

    for mask in 0_u16..(1 << alternatives.len()) {
        let existing = GuardDnf::from_disjunction(alternatives.iter().enumerate().filter_map(
            |(index, conjunction)| (mask & (1 << index) != 0).then_some(conjunction.clone()),
        ));
        for incoming_conjunction in &alternatives {
            let incoming = GuardDnf::from_conjunction(incoming_conjunction.clone());
            let expected = GuardDnf::from_disjunction(
                existing
                    .disjuncts()
                    .iter()
                    .cloned()
                    .chain(incoming.disjuncts().iter().cloned()),
            );
            let mut actual = existing.clone();
            actual.union_absorbing(incoming);
            sim_assert_eq!(have: actual, want: expected);
        }
    }
}

#[test]
fn single_conjunction_union_keeps_large_antichain() {
    let mut condition = GuardDnf::never();
    for index in 0..1_000 {
        condition.union_absorbing(GuardDnf::from_conjunction([truthy(&format!(
            "choice.{index}"
        ))]));
    }

    sim_assert_eq!(have: condition.disjuncts().len(), want: 1_000);
}

fn reference_minimize_disjunction(mut keys: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    fn complementary(left: u8, right: u8) -> bool {
        matches!((left, right), (0, 1) | (1, 0) | (2, 3) | (3, 2))
    }

    fn resolve(left: &[u8], right: &[u8]) -> Option<Vec<u8>> {
        if left.len() != right.len() {
            return None;
        }
        let left_only = left
            .iter()
            .filter(|item| !right.contains(item))
            .collect::<Vec<_>>();
        let right_only = right
            .iter()
            .filter(|item| !left.contains(item))
            .collect::<Vec<_>>();
        let ([left_extra], [right_extra]) = (left_only.as_slice(), right_only.as_slice()) else {
            return None;
        };
        complementary(**left_extra, **right_extra).then(|| {
            left.iter()
                .filter(|item| *item != *left_extra)
                .copied()
                .collect()
        })
    }

    keys.sort();
    keys.dedup();
    loop {
        let mut resolved = None;
        'search: for (index, left) in keys.iter().enumerate() {
            for (other_index, right) in keys.iter().enumerate().skip(index + 1) {
                if let Some(common) = resolve(left, right) {
                    resolved = Some((index, other_index, common));
                    break 'search;
                }
            }
        }
        let Some((index, other_index, common)) = resolved else {
            break;
        };
        keys.remove(other_index);
        keys.remove(index);
        if !keys.contains(&common) {
            keys.push(common);
        }
        keys.sort();
    }
    let sets = keys.clone();
    keys.retain(|candidate| {
        !sets.iter().any(|other| {
            other.len() < candidate.len() && other.iter().all(|item| candidate.contains(item))
        })
    });
    keys
}

#[test]
fn complementary_conjunctions_resolve_to_their_shared_key() {
    let condition = GuardDnf::from_disjunction([
        vec![truthy("enabled"), truthy("shared")],
        vec![truthy("shared"), truthy("enabled").negated()],
    ]);

    sim_assert_eq!(
        have: condition,
        want: GuardDnf::from_conjunction([truthy("shared")])
    );
}

#[test]
fn optimized_minimizer_matches_the_former_fixed_point_order() {
    let candidates = [
        vec![],
        vec![0],
        vec![1],
        vec![2],
        vec![3],
        vec![0, 2],
        vec![0, 3],
        vec![1, 2],
        vec![1, 3],
    ];

    for selection in 0..(1_usize << candidates.len()) {
        let keys = candidates
            .iter()
            .enumerate()
            .filter(|(index, _)| selection & (1 << index) != 0)
            .map(|(_, key)| key.clone())
            .collect::<Vec<_>>();
        let have = crate::guard_algebra::minimize_disjunction_by(keys.clone(), |left, right| {
            matches!((*left, *right), (0, 1) | (1, 0) | (2, 3) | (3, 2))
        });
        let want = reference_minimize_disjunction(keys);

        sim_assert_eq!(have: have, want: want);
    }
}

#[test]
fn nested_boolean_predicates_normalize_before_dnf_projection() {
    let enabled = truthy("enabled");
    let condition = GuardDnf::from_conjunction([
        enabled.clone(),
        Predicate::Or(vec![enabled.clone(), enabled.negated()]),
    ]);

    sim_assert_eq!(
        have: condition,
        want: GuardDnf::from_conjunction([truthy("enabled")])
    );
}

#[test]
fn weaker_conjunction_absorbs_its_strict_superset() {
    let condition = GuardDnf::from_disjunction([
        vec![truthy("shared")],
        vec![truthy("enabled"), truthy("shared")],
    ]);

    sim_assert_eq!(
        have: condition,
        want: GuardDnf::from_conjunction([truthy("shared")])
    );
}

#[test]
fn contradictory_conjunction_is_never_live() {
    let condition = GuardDnf::from_conjunction([truthy("enabled"), truthy("enabled").negated()]);

    sim_assert_eq!(have: condition, want: GuardDnf::never());
}

#[test]
fn negated_equality_makes_its_equality_branch_never_live() {
    let equality = Predicate::from(Guard::Eq {
        path: path("mode"),
        value: crate::GuardValue::string("prod"),
    });
    let condition = GuardDnf::from_conjunction([equality.clone(), equality.negated()]);

    sim_assert_eq!(have: condition, want: GuardDnf::never());
}

#[test]
fn lowered_equality_and_inequality_are_never_live() {
    let value = crate::GuardValue::string("prod");
    let condition = GuardDnf::from_guards([
        Guard::Eq {
            path: path("mode"),
            value: value.clone(),
        },
        Guard::NotEq {
            path: path("mode"),
            value,
        },
    ]);

    sim_assert_eq!(have: condition, want: GuardDnf::never());
}

#[test]
fn serialized_condition_uses_guard_conjunctions() {
    let condition = GuardDnf::from_disjunction([
        vec![truthy("first")],
        vec![Predicate::from(Guard::Default {
            path: path("second"),
        })],
    ]);
    let serialized = serde_json::to_value(&condition).expect("serialize guard DNF");

    sim_assert_eq!(
        have: serialized,
        want: json!([
            [{"type": "truthy", "path": "first"}],
            [{"type": "default", "path": "second"}]
        ])
    );
    sim_assert_eq!(
        have: serde_json::from_value::<GuardDnf>(serialized).expect("deserialize guard DNF"),
        want: condition
    );
}

#[test]
fn serialized_condition_retains_exact_guards_from_approximate_conjunctions() {
    let condition = GuardDnf::from_disjunction([
        vec![truthy("exact")],
        vec![
            truthy("shared"),
            Predicate::approximate("condition-1", ["version".to_string()].into_iter().collect()),
        ],
    ]);

    sim_assert_eq!(
        have: serde_json::to_value(condition).expect("serialize guard DNF"),
        want: json!([
            [{"type": "truthy", "path": "shared"}],
            [{"type": "truthy", "path": "exact"}]
        ])
    );
}

#[test]
fn serialized_condition_deduplicates_approximate_branches_with_equal_exact_guards() {
    let condition = GuardDnf::from_disjunction([
        vec![
            truthy("shared"),
            Predicate::approximate("condition-1", ["first".to_string()].into_iter().collect()),
        ],
        vec![
            truthy("shared"),
            Predicate::approximate("condition-2", ["second".to_string()].into_iter().collect()),
        ],
    ]);

    sim_assert_eq!(
        have: serde_json::to_value(condition).expect("serialize guard DNF"),
        want: json!([[{"type": "truthy", "path": "shared"}]])
    );
}

#[test]
fn equal_evidence_across_opaque_branch_complements_is_unconditional() {
    let approximate =
        Predicate::approximate("condition-1", ["version".to_string()].into_iter().collect());
    let mut condition = GuardDnf::from_conjunction([approximate.clone()]);

    condition.union_absorbing(GuardDnf::from_conjunction([approximate.negated()]));

    sim_assert_eq!(have: condition, want: GuardDnf::unconditional());
}

#[test]
fn conjunction_drops_an_approximation_proven_by_its_exact_sibling() {
    let enabled = truthy("enabled");
    let approximate = Predicate::approximate_with_sound_predicate(
        "partial",
        ["enabled".to_string(), "fallback".to_string()]
            .into_iter()
            .collect(),
        Predicate::Or(vec![enabled.clone(), truthy("fallback")]),
    );

    sim_assert_eq!(
        have: GuardDnf::from_conjunction([approximate, enabled.clone()]),
        want: GuardDnf::from_conjunction([enabled])
    );
}

#[test]
fn conditional_guard_disjunction_uses_the_same_normalization() {
    let condition = GuardDnf::normalize_conditional_guard_disjunction([
        vec![
            ConditionalGuard::Truthy {
                path: path("enabled"),
            },
            ConditionalGuard::Truthy {
                path: path("shared"),
            },
        ],
        vec![
            ConditionalGuard::Not(Box::new(ConditionalGuard::Truthy {
                path: path("enabled"),
            })),
            ConditionalGuard::Truthy {
                path: path("shared"),
            },
        ],
    ]);

    sim_assert_eq!(
        have: condition,
        want: vec![vec![ConditionalGuard::Truthy {
            path: path("shared"),
        }]]
    );
}
