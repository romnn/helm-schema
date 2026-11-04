use serde_json::json;
use test_util::prelude::sim_assert_eq;

use std::collections::BTreeSet;

use super::{GuardDnf, disjunction_is_tautology, predicates_are_contradictory};
use crate::{ConditionalGuard, Conjunction, Guard, Predicate, PredicateKind, ValuesPath};

fn path(value: &str) -> ValuesPath {
    ValuesPath::parse(value)
}

fn truthy(path: &str) -> Predicate {
    Predicate::truthy_path(path)
}

#[test]
fn shared_dnf_comparisons_keep_the_full_structural_order() {
    let values = [
        GuardDnf::never(),
        GuardDnf::unconditional(),
        GuardDnf::from_conjunction([truthy("a")]),
        GuardDnf::from_conjunction([truthy("b"), truthy("a")]),
        GuardDnf::from_conjunction([Predicate::Approximate(
            "opaque".to_owned(),
            BTreeSet::from([path("a")]),
            crate::ApproximationRole::Control,
            None,
        )]),
    ];
    let clones = values.clone();
    let copies = values.map(|value| GuardDnf::from_disjunction(value.disjuncts().iter().cloned()));
    for left in clones.iter().chain(&copies) {
        for right in clones.iter().chain(&copies) {
            sim_assert_eq!(have: left.cmp(right), want: left.disjuncts().cmp(right.disjuncts()));
        }
    }
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

    // Read-only copies share their formula; editing either copy preserves its sibling.
    let original = GuardDnf::from_conjunction([truthy("a")]);
    let mut fork = original.clone();
    assert!(std::ptr::eq(original.disjuncts(), fork.disjuncts()));
    fork.union_absorbing(GuardDnf::from_conjunction([truthy("b")]));
    sim_assert_eq!(have: fork.clone(), want: GuardDnf::from_disjunction([[truthy("a")], [truthy("b")]]));
    sim_assert_eq!(have: original.clone(), want: GuardDnf::from_conjunction([truthy("a")]));
    assert!(!std::ptr::eq(original.disjuncts(), fork.disjuncts()));
    let mut mapped = original.clone();
    mapped.map_value_paths(&mut |_| path("nested.a"));
    sim_assert_eq!(have: mapped, want: GuardDnf::from_conjunction([truthy("nested.a")]));
    sim_assert_eq!(have: original, want: GuardDnf::from_conjunction([truthy("a")]));
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

fn reference_key_is_subset<T: PartialEq>(subset: &[T], superset: &[T]) -> bool {
    subset.len() < superset.len() && subset.iter().all(|item| superset.contains(item))
}

fn reference_resolve_keys<T: Clone + PartialEq>(
    left: &[T],
    right: &[T],
    are_complementary: impl Fn(&T, &T) -> bool,
) -> Option<Vec<T>> {
    if left.len() != right.len() {
        return None;
    }

    let mut left_extra = None;
    let mut right_extra = None;
    for item in left {
        if !right.contains(item) {
            if left_extra.is_some() {
                return None;
            }
            left_extra = Some(item);
        }
    }
    for item in right {
        if !left.contains(item) {
            if right_extra.is_some() {
                return None;
            }
            right_extra = Some(item);
        }
    }

    let (Some(left_extra), Some(right_extra)) = (left_extra, right_extra) else {
        return None;
    };
    if !are_complementary(left_extra, right_extra) {
        return None;
    }
    Some(
        left.iter()
            .filter(|item| *item != left_extra)
            .cloned()
            .collect(),
    )
}

fn reference_minimize_by<T: Clone + Ord>(
    mut keys: Vec<Vec<T>>,
    are_complementary: impl Copy + Fn(&T, &T) -> bool,
) -> Vec<Vec<T>> {
    debug_assert!(keys.iter().all(|key| {
        key.windows(2)
            .all(|pair| matches!(pair, [left, right] if left < right))
    }));
    keys.sort();
    keys.dedup();
    loop {
        let mut resolved = None;
        'search: for (index, left) in keys.iter().enumerate() {
            for (other_index, right) in keys.iter().enumerate().skip(index + 1) {
                if let Some(common) = reference_resolve_keys(left, right, are_complementary) {
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
            let insert_at = keys.partition_point(|key| key < &common);
            keys.insert(insert_at, common);
        }
    }

    let retained = keys
        .iter()
        .enumerate()
        .map(|(candidate_index, candidate)| {
            !keys.iter().enumerate().any(|(other_index, other)| {
                candidate_index != other_index && reference_key_is_subset(other, candidate)
            })
        })
        .collect::<Vec<_>>();
    let mut retained = retained.into_iter();
    keys.retain(|_| retained.next().unwrap_or_default());
    keys
}

fn reference_minimize_disjunction(keys: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    reference_minimize_by(keys, |left, right| {
        matches!((*left, *right), (0, 1) | (1, 0) | (2, 3) | (3, 2))
    })
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
            ConditionalGuard::Not(std::sync::Arc::new(ConditionalGuard::Truthy {
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

fn reference_normalize_conjunction(
    predicates: impl IntoIterator<Item = Predicate>,
) -> Option<Vec<Predicate>> {
    let mut normalized = BTreeSet::new();
    for predicate in Conjunction::new(predicates).into_vec() {
        match predicate.kind() {
            PredicateKind::False => return None,
            PredicateKind::Or(predicates) if disjunction_is_tautology(predicates) => continue,
            _ => {}
        }
        if normalized
            .iter()
            .any(|other| predicates_are_contradictory(&predicate, other))
        {
            return None;
        }
        normalized.insert(predicate);
    }
    let absorbed_disjunctions = normalized
        .iter()
        .filter(|predicate| match predicate.kind() {
            PredicateKind::Or(alternatives) => alternatives
                .iter()
                .any(|alternative| normalized.contains(alternative)),
            _ => false,
        })
        .cloned()
        .collect::<Vec<_>>();
    for predicate in absorbed_disjunctions {
        normalized.remove(&predicate);
    }
    let exact = Predicate::all(
        normalized
            .iter()
            .filter(|predicate| !predicate.contains_approximation())
            .cloned()
            .collect(),
    );
    normalized.retain(|predicate| {
        let PredicateKind::Approximate {
            sound_subset: Some(sound_subset),
            ..
        } = predicate.kind()
        else {
            return true;
        };
        !crate::predicate_bdd::exact_implies(&exact, sound_subset)
    });
    Some(normalized.into_iter().collect())
}

#[test]
fn indexed_contradictions_match_full_scan_with_typed_predicates() {
    let mut candidates = vec![Predicate::True, Predicate::False];
    for name in ["input", "other", "input.child"] {
        let input = path(name);
        let mut guards = vec![
            Guard::Truthy {
                path: input.clone(),
            },
            Guard::Default {
                path: input.clone(),
            },
            Guard::MatchesPattern {
                path: input.clone(),
                pattern: "^a".to_owned(),
                templated: false,
            },
            Guard::MatchesPattern {
                path: input.clone(),
                pattern: "^b".to_owned(),
                templated: true,
            },
        ];
        for schema_type in ["object", "string", "integer", "number", "boolean", "null"] {
            guards.push(Guard::TypeIs {
                path: input.clone(),
                schema_type: schema_type.to_owned(),
            });
        }
        for value in [
            crate::GuardValue::Int(1),
            crate::GuardValue::Int(2),
            crate::GuardValue::Bool(true),
            crate::GuardValue::String("1".to_owned()),
            crate::GuardValue::Float("1.5".to_owned()),
            crate::GuardValue::Null,
        ] {
            guards.push(Guard::Eq {
                path: input.clone(),
                value: value.clone(),
            });
            guards.push(Guard::NotEq {
                path: input.clone(),
                value,
            });
        }
        candidates.extend(guards.into_iter().map(Predicate::from));
    }
    candidates.extend([
        Predicate::from(Guard::Or { paths: Vec::new() }),
        Predicate::from(Guard::Or {
            paths: vec![path("input"), path("other")],
        }),
        Predicate::from(Guard::AnyOf {
            alternatives: vec![
                vec![Guard::Truthy {
                    path: path("input"),
                }],
                vec![Guard::Default {
                    path: path("other"),
                }],
            ],
        }),
        Predicate::Approximate(
            "opaque".to_owned(),
            BTreeSet::from([path("input")]),
            crate::ApproximationRole::Control,
            Some(Box::new(truthy("input"))),
        ),
        Predicate::Approximate(
            "opaque".to_owned(),
            BTreeSet::from([path("other")]),
            crate::ApproximationRole::Control,
            Some(Box::new(truthy("input"))),
        ),
        Predicate::Approximate(
            "opaque".to_owned(),
            BTreeSet::from([path("input")]),
            crate::ApproximationRole::OutputSelection,
            None,
        ),
    ]);
    let negated = candidates
        .iter()
        .cloned()
        .map(Predicate::Not)
        .collect::<Vec<_>>();
    candidates.extend(negated.iter().cloned());
    candidates.extend(negated.into_iter().map(Predicate::Not));
    let filler = (0..32)
        .map(|index| truthy(&format!("unrelated.{index}")))
        .collect::<Vec<_>>();
    for left in &candidates {
        for right in &candidates {
            let input = filler
                .iter()
                .cloned()
                .chain([left.clone(), right.clone()])
                .collect::<Vec<_>>();
            sim_assert_eq!(have: super::normalize_conjunction(input.clone()), want: reference_normalize_conjunction(input));
        }
    }
}

#[test]
fn indexed_contradictions_preserve_raw_nested_negation_direction() {
    let a = truthy("a");
    let not_a = Predicate::Not(a);
    let not_not_a = Predicate::Not(not_a.clone());
    for pair in [[not_a.clone(), not_not_a.clone()], [not_not_a, not_a]] {
        let input = (0..32)
            .map(|index| truthy(&format!("filler.{index}")))
            .chain(pair)
            .collect::<Vec<_>>();
        let expected = reference_normalize_conjunction(input.clone());
        assert!(expected.is_some());
        sim_assert_eq!(have: super::normalize_conjunction(input), want: expected);
    }
}

#[test]
fn ranked_minimization_preserves_asymmetric_callback_order() {
    let choices = [
        vec![],
        vec![0],
        vec![1],
        vec![0, 2],
        vec![0, 3],
        vec![1, 2],
        vec![1, 3],
        vec![0, 2],
    ];
    for mask in 0_u16..(1 << choices.len()) {
        let input = choices
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, row)| row.clone())
            .collect::<Vec<_>>();
        let expected_calls = std::cell::RefCell::new(Vec::new());
        let expected = reference_minimize_by(input.clone(), |left, right| {
            expected_calls.borrow_mut().push((*left, *right));
            matches!((*left, *right), (0, 1) | (2, 3))
        });
        let actual_calls = std::cell::RefCell::new(Vec::new());
        let actual = crate::guard_algebra::minimize_disjunction_by(input, |left, right| {
            actual_calls.borrow_mut().push((*left, *right));
            matches!((*left, *right), (0, 1) | (2, 3))
        });
        sim_assert_eq!(have: actual, want: expected);
        sim_assert_eq!(have: actual_calls.into_inner(), want: expected_calls.into_inner());
    }
    let input = vec![vec![0, 2], vec![0, 3], vec![1, 2]];
    let actual = crate::guard_algebra::minimize_disjunction_by(input, |left, right| {
        matches!((*left, *right), (0, 1) | (1, 0) | (2, 3) | (3, 2))
    });
    sim_assert_eq!(have: actual, want: vec![vec![0], vec![1, 2]]);
}

#[test]
fn ranked_minimization_preserves_original_left_predicate_nodes() {
    let shared_leaf = truthy("shared");
    let shared = Predicate::Or(vec![shared_leaf.clone(), shared_leaf]);
    let unshared = Predicate::Or(vec![truthy("shared"), truthy("shared")]);
    sim_assert_eq!(have: &shared, want: &unshared);
    assert_ne!(shared.node_id(), unshared.node_id());
    let atom = truthy("a");
    let mut input = vec![
        vec![unshared, Predicate::Not(atom.clone())],
        vec![shared, atom],
    ];
    for row in &mut input {
        row.sort();
    }
    let expected = reference_minimize_by(input.clone(), super::predicates_are_complementary);
    let actual =
        crate::guard_algebra::minimize_disjunction_by(input, super::predicates_are_complementary);
    let node_ids = |rows: &[Vec<Predicate>]| {
        rows.iter()
            .map(|row| row.iter().map(Predicate::node_id).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    sim_assert_eq!(have: node_ids(&actual), want: node_ids(&expected));
    sim_assert_eq!(have: actual, want: expected);
}

#[test]
fn ranked_minimization_handles_long_prefixes_and_absorption() {
    let common = (100_u16..180).collect::<Vec<_>>();
    let mut input = vec![
        common.clone(),
        common.iter().copied().chain([0, 1]).collect(),
        common.iter().copied().chain([0, 2]).collect(),
        common.iter().copied().chain([1, 2]).collect(),
        vec![0, 1],
        vec![],
    ];
    for row in &mut input {
        row.sort_unstable();
        row.dedup();
    }
    let complementary = |left: &u16, right: &u16| matches!((*left, *right), (0, 1) | (1, 0 | 2));
    for count in 0..=input.len() {
        let rows = input.iter().take(count).cloned().collect::<Vec<_>>();
        sim_assert_eq!(have: crate::guard_algebra::minimize_disjunction_by(rows.clone(), complementary), want: reference_minimize_by(rows, complementary));
    }
}

#[test]
fn ranked_resolution_keeps_mask_boundaries_and_full_scan_callback_order() {
    use std::cell::RefCell;

    #[derive(Debug, PartialEq, Eq)]
    struct Comparison {
        left: u16,
        right: u16,
    }

    let common = (0_u16..130).collect::<Vec<_>>();
    let mut input = vec![
        common.iter().copied().chain([200]).collect::<Vec<_>>(),
        common.iter().copied().chain([201]).collect(),
        vec![0, 127],
        vec![0, 128],
        vec![1, 127],
        vec![1, 129],
        vec![0, 127],
        vec![126, 127],
    ];
    for rows in [input.clone(), {
        input.reverse();
        input
    }] {
        let expected_calls = RefCell::new(Vec::new());
        let actual_calls = RefCell::new(Vec::new());
        let complementary =
            |left: &u16, right: &u16| matches!((*left, *right), (200, 201) | (127, 128));
        let expected = reference_minimize_by(rows.clone(), |left, right| {
            expected_calls.borrow_mut().push(Comparison {
                left: *left,
                right: *right,
            });
            complementary(left, right)
        });
        let actual = crate::guard_algebra::minimize_disjunction_by(rows, |left, right| {
            actual_calls.borrow_mut().push(Comparison {
                left: *left,
                right: *right,
            });
            complementary(left, right)
        });
        sim_assert_eq!(have: actual, want: expected);
        sim_assert_eq!(have: actual_calls.into_inner(), want: expected_calls.into_inner());
    }
}
