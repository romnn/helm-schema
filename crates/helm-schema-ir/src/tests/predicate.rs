use super::Predicate;
use crate::{Guard, GuardValue};
use test_util::prelude::sim_assert_eq;

#[test]
fn or_truthy_predicate_projects_to_or_guard() {
    let predicate = Predicate::from(Guard::Or {
        paths: vec!["first".to_string(), "second".to_string()],
    });

    sim_assert_eq!(
        have: predicate.contract_guards(),
        want: vec![Guard::Or {
            paths: vec!["first".to_string(), "second".to_string()]
        }]
    );
}

#[test]
fn negated_truthy_predicate_projects_to_not_guard() {
    let predicate = Predicate::from(Guard::Truthy {
        path: "enabled".to_string(),
    })
    .negated();

    sim_assert_eq!(
        have: predicate.contract_guards(),
        want: vec![Guard::Not {
            path: "enabled".to_string()
        }]
    );
}

#[test]
fn double_negated_truthy_predicate_projects_to_truthy_guard() {
    let predicate = Predicate::from(Guard::Truthy {
        path: "enabled".to_string(),
    })
    .negated()
    .negated();

    sim_assert_eq!(
        have: predicate.contract_guards(),
        want: vec![Guard::Truthy {
            path: "enabled".to_string()
        }]
    );
}

#[test]
fn negated_eq_predicate_projects_to_not_eq_guard() {
    let predicate = Predicate::Not(Box::new(Predicate::from(Guard::Eq {
        path: "mode".to_string(),
        value: GuardValue::string("prod"),
    })));

    sim_assert_eq!(
        have: predicate.contract_guards(),
        want: vec![Guard::NotEq {
            path: "mode".to_string(),
            value: GuardValue::string("prod"),
        }]
    );
}

#[test]
fn not_eq_predicate_projects_to_not_eq_guard() {
    let predicate = Predicate::from(Guard::NotEq {
        path: "mode".to_string(),
        value: GuardValue::string("disabled"),
    });

    sim_assert_eq!(
        have: predicate.contract_guards(),
        want: vec![Guard::NotEq {
            path: "mode".to_string(),
            value: GuardValue::string("disabled"),
        }]
    );
}

#[test]
fn mixed_or_predicate_projects_to_structural_any_of_guard() {
    let predicate = Predicate::Or(vec![
        Predicate::from(Guard::Truthy {
            path: "first".to_string(),
        }),
        Predicate::from(Guard::Eq {
            path: "mode".to_string(),
            value: GuardValue::string("prod"),
        }),
    ]);

    sim_assert_eq!(
        have: predicate.contract_guards(),
        want: vec![Guard::AnyOf {
            alternatives: vec![
                vec![Guard::Truthy {
                    path: "first".to_string(),
                }],
                vec![Guard::Eq {
                    path: "mode".to_string(),
                    value: GuardValue::string("prod"),
                }],
            ],
        }]
    );
}

#[test]
fn contract_guard_stack_dedupes_projected_guards() {
    let predicate = Predicate::from(Guard::Truthy {
        path: "enabled".to_string(),
    });

    sim_assert_eq!(
        have: Predicate::contract_guard_stack(&[predicate.clone(), predicate]),
        want: vec![Guard::Truthy {
            path: "enabled".to_string()
        }]
    );
}
