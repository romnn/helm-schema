use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::GuardDnf;
use crate::{ConditionalGuard, Guard, Predicate};

fn truthy(path: &str) -> Predicate {
    Predicate::truthy_path(path)
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
        path: "mode".to_string(),
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
            path: "mode".to_string(),
            value: value.clone(),
        },
        Guard::NotEq {
            path: "mode".to_string(),
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
            path: "second".to_string(),
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
fn conditional_guard_disjunction_uses_the_same_normalization() {
    let condition = GuardDnf::normalize_conditional_guard_disjunction([
        vec![
            ConditionalGuard::Truthy {
                path: "enabled".to_string(),
            },
            ConditionalGuard::Truthy {
                path: "shared".to_string(),
            },
        ],
        vec![
            ConditionalGuard::Not(Box::new(ConditionalGuard::Truthy {
                path: "enabled".to_string(),
            })),
            ConditionalGuard::Truthy {
                path: "shared".to_string(),
            },
        ],
    ]);

    sim_assert_eq!(
        have: condition,
        want: vec![vec![ConditionalGuard::Truthy {
            path: "shared".to_string(),
        }]]
    );
}
