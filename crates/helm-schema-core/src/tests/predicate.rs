use super::{Conjunction, Predicate};
use crate::{Guard, GuardValue, ValuesPath};
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use test_util::prelude::sim_assert_eq;

fn path(value: &str) -> ValuesPath {
    ValuesPath::parse(value)
}

fn shares_allocation(left: &Predicate, right: &Predicate) -> bool {
    match (left, right) {
        (
            Predicate(super::PredicateStorage::Shared(left)),
            Predicate(super::PredicateStorage::Shared(right)),
        ) => std::sync::Arc::ptr_eq(left, right),
        _ => false,
    }
}

#[test]
fn identity_path_mapping_preserves_shared_predicate_storage() {
    // The syntax has 4,096 leaf occurrences but only one new node per level.
    let mut predicate = Predicate::truthy_path("leaf");
    for depth in 0..12 {
        predicate = if depth % 2 == 0 {
            Predicate::And(vec![predicate.clone(), predicate.clone()])
        } else {
            Predicate::Or(vec![predicate.clone(), predicate.clone()])
        };
    }
    let mut visits = 0;
    let mapped = predicate.clone().map_value_paths(&mut |path| {
        visits += 1;
        path
    });
    sim_assert_eq!(have: visits, want: 4096);
    sim_assert_eq!(have: &mapped, want: &predicate);
    sim_assert_eq!(have: shares_allocation(&mapped, &predicate), want: true);
}

#[test]
fn stateful_path_mapping_visits_each_shared_occurrence_in_order() {
    let leaf = Predicate::truthy_path("leaf");
    let predicate = Predicate::And(vec![leaf.clone(), leaf]);
    let mut visits = 0;
    let mapped = predicate.map_value_paths(&mut |_| {
        visits += 1;
        path(&format!("mapped{visits}"))
    });
    sim_assert_eq!(have: visits, want: 2);
    sim_assert_eq!(
        have: mapped,
        want: Predicate::And(vec![
            Predicate::truthy_path("mapped1"),
            Predicate::truthy_path("mapped2"),
        ])
    );
}

#[test]
fn path_mapping_keeps_unchanged_children_beside_a_changed_path() {
    let retained = Predicate::Not(Box::new(Predicate::truthy_path("retained")));
    let predicate = Predicate::Or(vec![retained.clone(), Predicate::truthy_path(" padded ")]);
    let mapped = predicate.map_value_paths(&mut |value| path(value.encode().trim()));
    sim_assert_eq!(
        have: &mapped,
        want: &Predicate::Or(vec![retained.clone(), Predicate::truthy_path("padded")])
    );
    let unchanged = match mapped.kind() {
        super::PredicateKind::Or(children) => children.first(),
        _ => None,
    };
    sim_assert_eq!(
        have: unchanged.is_some_and(|child| shares_allocation(child, &retained)),
        want: true
    );
}

fn recursive_contains_approximation(predicate: &Predicate) -> bool {
    match predicate.kind() {
        super::PredicateKind::Approximate { .. } => true,
        super::PredicateKind::Not(inner) => recursive_contains_approximation(inner),
        super::PredicateKind::And(predicates) | super::PredicateKind::Or(predicates) => {
            predicates.iter().any(recursive_contains_approximation)
        }
        super::PredicateKind::True
        | super::PredicateKind::False
        | super::PredicateKind::Guard(_) => false,
    }
}

#[test]
fn ordering_equal_shared_predicate_dag_does_not_unfold_it() {
    let mut predicate = Predicate::truthy_path("leaf");
    for depth in 0..30 {
        predicate = if depth % 2 == 0 {
            Predicate::And(vec![predicate.clone(), predicate.clone()])
        } else {
            Predicate::Or(vec![predicate.clone(), predicate.clone()])
        };
    }

    sim_assert_eq!(
        have: predicate.cmp(&predicate.clone()),
        want: Ordering::Equal
    );
}

fn predicate_hash(predicate: &Predicate) -> u64 {
    let mut hasher = DefaultHasher::new();
    predicate.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn cached_predicate_metadata_matches_structural_oracles() {
    let exact = Predicate::And(vec![
        Predicate::truthy_path("alpha"),
        Predicate::Not(Box::new(Predicate::truthy_path("beta"))),
    ]);
    let approximate = Predicate::Approximate(
        "opaque".to_string(),
        [path("gamma")].into_iter().collect(),
        crate::ApproximationRole::Control,
        Some(Box::new(Predicate::truthy_path("alpha"))),
    );
    let predicates = [
        Predicate::True,
        Predicate::False,
        exact.clone(),
        Predicate::Not(Box::new(approximate.clone())),
        Predicate::Or(vec![exact.clone(), approximate]),
    ];
    for predicate in predicates {
        sim_assert_eq!(
            have: predicate.contains_approximation(),
            want: recursive_contains_approximation(&predicate)
        );
    }

    let rebuilt = Predicate::And(vec![
        Predicate::truthy_path("alpha"),
        Predicate::Not(Box::new(Predicate::truthy_path("beta"))),
    ]);
    sim_assert_eq!(have: exact.clone(), want: rebuilt.clone());
    sim_assert_eq!(have: predicate_hash(&exact), want: predicate_hash(&rebuilt));
}

#[test]
fn predicate_order_preserves_the_former_variant_sequence() {
    let guard = Predicate::truthy_path("guard");
    let mut predicates = vec![
        Predicate::Or(vec![guard.clone()]),
        Predicate::And(vec![guard.clone()]),
        Predicate::Not(Box::new(guard.clone())),
        guard.clone(),
        Predicate::approximate("opaque", ["path".to_string()].into_iter().collect()),
        Predicate::False,
        Predicate::True,
    ];
    predicates.sort();

    sim_assert_eq!(
        have: predicates,
        want: vec![
            Predicate::True,
            Predicate::False,
            Predicate::approximate("opaque", ["path".to_string()].into_iter().collect()),
            guard.clone(),
            Predicate::Not(Box::new(guard.clone())),
            Predicate::And(vec![guard.clone()]),
            Predicate::Or(vec![guard]),
        ]
    );
}

#[test]
fn conjunction_canonicalizes_nested_and_identity_parts() {
    let first = Predicate::truthy_path("first");
    let second = Predicate::truthy_path("second");
    let conjunction = Conjunction::new([
        second.clone(),
        Predicate::True,
        Predicate::And(vec![first.clone(), second]),
    ]);

    sim_assert_eq!(have: conjunction.into_vec(), want: vec![first, Predicate::truthy_path("second")]);
}

#[test]
fn or_truthy_predicate_projects_to_or_guard() {
    let predicate = Predicate::from(Guard::Or {
        paths: vec![path("first"), path("second")],
    });

    sim_assert_eq!(
        have: predicate.contract_guards(),
        want: Some(vec![Guard::Or {
            paths: vec![path("first"), path("second")]
        }])
    );
}

#[test]
fn negated_truthy_predicate_projects_to_not_guard() {
    let predicate = Predicate::from(Guard::Truthy {
        path: path("enabled"),
    })
    .negated();

    sim_assert_eq!(
        have: predicate.contract_guards(),
        want: Some(vec![Guard::Not {
            path: path("enabled")
        }])
    );
}

#[test]
fn double_negated_truthy_predicate_projects_to_truthy_guard() {
    let predicate = Predicate::from(Guard::Truthy {
        path: path("enabled"),
    })
    .negated()
    .negated();

    sim_assert_eq!(
        have: predicate.contract_guards(),
        want: Some(vec![Guard::Truthy {
            path: path("enabled")
        }])
    );
}

#[test]
fn negated_eq_predicate_projects_to_not_eq_guard() {
    let predicate = Predicate::Not(Box::new(Predicate::from(Guard::Eq {
        path: path("mode"),
        value: GuardValue::string("prod"),
    })));

    sim_assert_eq!(
        have: predicate.contract_guards(),
        want: Some(vec![Guard::NotEq {
            path: path("mode"),
            value: GuardValue::string("prod"),
        }])
    );
}

#[test]
fn not_eq_predicate_projects_to_not_eq_guard() {
    let predicate = Predicate::from(Guard::NotEq {
        path: path("mode"),
        value: GuardValue::string("disabled"),
    });

    sim_assert_eq!(
        have: predicate.contract_guards(),
        want: Some(vec![Guard::NotEq {
            path: path("mode"),
            value: GuardValue::string("disabled"),
        }])
    );
}

#[test]
fn mixed_or_predicate_projects_to_structural_any_of_guard() {
    let predicate = Predicate::Or(vec![
        Predicate::from(Guard::Truthy {
            path: path("first"),
        }),
        Predicate::from(Guard::Eq {
            path: path("mode"),
            value: GuardValue::string("prod"),
        }),
    ]);

    sim_assert_eq!(
        have: predicate.contract_guards(),
        want: Some(vec![Guard::AnyOf {
            alternatives: vec![
                vec![Guard::Truthy {
                    path: path("first"),
                }],
                vec![Guard::Eq {
                    path: path("mode"),
                    value: GuardValue::string("prod"),
                }],
            ],
        }])
    );
}

#[test]
fn contract_guard_stack_dedupes_projected_guards() {
    let predicate = Predicate::from(Guard::Truthy {
        path: path("enabled"),
    });

    sim_assert_eq!(
        have: Predicate::contract_guard_stack(&[predicate.clone(), predicate]),
        want: vec![Guard::Truthy {
            path: path("enabled")
        }]
    );
}

#[test]
fn approximate_predicate_keeps_an_opaque_complement_without_becoming_a_guard() {
    let predicate =
        Predicate::approximate("condition-1", ["version".to_string()].into_iter().collect());

    sim_assert_eq!(
        have: predicate.negated(),
        want: Predicate::Not(Box::new(predicate.clone()))
    );
    sim_assert_eq!(have: predicate.contract_guards(), want: None);
    assert!(predicate.contains_approximation());
}

#[test]
fn inexact_conjunction_abstains_as_a_whole() {
    let predicate = Predicate::And(vec![
        Predicate::truthy_path("enabled"),
        Predicate::approximate("opaque", ["mode".to_string()].into_iter().collect()),
    ]);

    sim_assert_eq!(have: predicate.contract_guards(), want: None);
}

#[test]
fn boolean_normalization_absorbs_a_repeated_positive_arm() {
    let selected = Predicate::truthy_path("selected");
    let fallback = Predicate::truthy_path("fallback");
    let predicate = Predicate::And(vec![
        selected.clone(),
        Predicate::Or(vec![selected.clone().negated(), fallback.clone()]),
    ]);

    sim_assert_eq!(
        have: predicate.normalize_boolean(),
        want: Predicate::And(vec![fallback, selected])
    );
}

#[test]
fn boolean_normalization_absorbs_a_repeated_negative_arm() {
    let selected = Predicate::truthy_path("selected");
    let fallback = Predicate::truthy_path("fallback");
    let predicate = Predicate::Or(vec![
        selected.clone(),
        Predicate::And(vec![selected.negated(), fallback.clone()]),
    ]);

    sim_assert_eq!(
        have: predicate.normalize_boolean(),
        want: Predicate::Or(vec![fallback, Predicate::truthy_path("selected")])
    );
}

#[test]
fn boolean_normalization_collapses_complementary_branch_outputs() {
    let selected = Predicate::truthy_path("selected");
    let branch = Predicate::truthy_path("branch");
    let predicate = Predicate::Or(vec![
        Predicate::And(vec![selected.clone(), branch.clone()]),
        Predicate::And(vec![selected.clone(), branch.negated()]),
    ]);

    sim_assert_eq!(have: predicate.normalize_boolean(), want: selected);
}

#[test]
fn boolean_normalization_canonicalizes_atomic_negative_guards() {
    let selected = Guard::Eq {
        path: path("mode"),
        value: GuardValue::string("selected"),
    };
    let excluded = Guard::NotEq {
        path: path("mode"),
        value: GuardValue::string("selected"),
    };

    sim_assert_eq!(
        have: Predicate::And(vec![
            Predicate::from(selected),
            Predicate::from(excluded)
        ])
        .normalize_boolean(),
        want: Predicate::False
    );
}

#[test]
fn boolean_normalization_keeps_approximate_formulas_opaque() {
    let approximate = Predicate::approximate_with_sound_predicate(
        "partial",
        ["selected".to_string()].into_iter().collect(),
        Predicate::truthy_path("selected"),
    );
    let predicate = Predicate::Or(vec![approximate, Predicate::truthy_path("fallback")]);

    sim_assert_eq!(
        have: predicate.clone().normalize_boolean(),
        want: predicate
    );
}

#[test]
fn boolean_normalization_drops_an_approximation_implied_by_its_sound_subset() {
    let selected = Predicate::truthy_path("selected");
    let approximate = Predicate::approximate_with_sound_predicate(
        "partial",
        ["fallback".to_string(), "selected".to_string()]
            .into_iter()
            .collect(),
        Predicate::Or(vec![Predicate::truthy_path("fallback"), selected.clone()]),
    );

    sim_assert_eq!(
        have: Predicate::And(vec![approximate, selected.clone()]).normalize_boolean(),
        want: selected
    );
}

#[test]
fn boolean_normalization_keeps_an_unproven_approximate_conjunct() {
    let selected = Predicate::truthy_path("selected");
    let approximate = Predicate::approximate_with_sound_predicate(
        "partial",
        ["fallback".to_string()].into_iter().collect(),
        Predicate::truthy_path("fallback"),
    );
    let predicate = Predicate::And(vec![approximate, selected]);

    sim_assert_eq!(
        have: predicate.clone().normalize_boolean(),
        want: predicate
    );
}

#[test]
fn exact_implication_recognizes_a_selected_disjunction_arm() {
    let selected = Predicate::truthy_path("selected");
    let fallback = Predicate::truthy_path("fallback");

    assert!(selected.exactly_implies(&Predicate::Or(vec![selected.clone(), fallback])));
}

#[test]
fn exact_implication_abstains_for_opaque_approximations() {
    let approximate =
        Predicate::approximate("opaque", ["selected".to_string()].into_iter().collect());

    assert!(!approximate.exactly_implies(&approximate));
}

fn recursive_syntax_size(predicate: &Predicate) -> usize {
    match predicate.kind() {
        super::PredicateKind::Not(inner) => 1 + recursive_syntax_size(inner),
        super::PredicateKind::And(parts) | super::PredicateKind::Or(parts) => {
            1 + parts.iter().map(recursive_syntax_size).sum::<usize>()
        }
        super::PredicateKind::Approximate {
            marker: _,
            paths: _,
            role: _,
            sound_subset,
        } => 1 + sound_subset.map_or(0, recursive_syntax_size),
        super::PredicateKind::True
        | super::PredicateKind::False
        | super::PredicateKind::Guard(_) => 1,
    }
}

#[test]
fn predicate_syntax_size_counts_occurrences_and_approximation_subsets() {
    let shared = Predicate::Not(Box::new(Predicate::truthy_path("shared")));
    let approximate = Predicate::Approximate(
        "opaque".to_owned(),
        BTreeSet::new(),
        crate::ApproximationRole::Control,
        Some(Box::new(shared.clone())),
    );
    for predicate in [
        Predicate::True,
        Predicate::False,
        Predicate::And(Vec::new()),
        Predicate::Or(Vec::new()),
        Predicate::And(vec![shared.clone(), shared.clone()]),
        Predicate::Or(vec![shared, approximate]),
    ] {
        // A shared child occupies syntax in each arm; pointer uniqueness is not the bound.
        sim_assert_eq!(have: predicate.syntax_size(), want: recursive_syntax_size(&predicate));
    }
}

#[test]
fn predicate_syntax_size_saturates_without_unfolding_shared_children() {
    let mut predicate = Predicate::truthy_path("shared");
    for _ in 0..usize::BITS {
        predicate = Predicate::And(vec![predicate.clone(), predicate]);
    }
    sim_assert_eq!(have: predicate.syntax_size(), want: usize::MAX);
}

#[test]
fn path_collection_preserves_sets_across_deep_shared_boolean_graphs() {
    let mut predicate = Predicate::truthy_path("seed");
    let mut conditional = crate::ConditionalGuard::Truthy { path: path("seed") };
    let mut expected = BTreeSet::from([path("seed")]);
    for depth in 0..24 {
        let extra = format!("branch{depth}");
        expected.insert(path(&extra));
        predicate = Predicate::And(vec![
            Predicate::truthy_path(extra.clone()),
            Predicate::Or(vec![predicate.clone(), predicate.negated()]),
        ]);
        conditional = crate::ConditionalGuard::AllOf(
            vec![
                crate::ConditionalGuard::Truthy { path: path(&extra) },
                crate::ConditionalGuard::AnyOf(
                    vec![
                        conditional.clone(),
                        crate::ConditionalGuard::Not(std::sync::Arc::new(conditional)),
                    ]
                    .into(),
                ),
            ]
            .into(),
        );
    }
    sim_assert_eq!(have: predicate.value_paths(), want: expected.clone());
    sim_assert_eq!(have: conditional.value_paths(), want: expected);
    let approximate = Predicate::Approximate(
        "opaque".to_owned(),
        BTreeSet::from([path("visible")]),
        crate::ApproximationRole::Control,
        Some(Box::new(predicate)),
    );
    sim_assert_eq!(have: approximate.value_paths(), want: BTreeSet::from([path("visible")]));
}
