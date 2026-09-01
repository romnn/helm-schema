use super::{Conjunction, Predicate};
use crate::{Guard, GuardValue, ValuesPath};
use test_util::prelude::sim_assert_eq;

fn path(value: &str) -> ValuesPath {
    ValuesPath::parse(value)
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
