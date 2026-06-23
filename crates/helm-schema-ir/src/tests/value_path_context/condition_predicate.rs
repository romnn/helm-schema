use super::*;
use crate::{Guard, GuardValue};
use test_util::prelude::sim_assert_eq;

#[test]
fn with_predicates_preserve_header_projection_semantics() {
    let predicate = Predicate::all(vec![
        Predicate::truthy_path("service.enabled"),
        Predicate::from(Guard::Eq {
            path: "service.type".to_string(),
            value: GuardValue::string("ClusterIP"),
        }),
        Predicate::Or(vec![
            Predicate::truthy_path("service.annotations"),
            Predicate::truthy_path("global.annotations"),
        ]),
        Predicate::truthy_path("service.disabled").negated(),
    ]);

    let with_predicate = Predicate::all(with_predicates_from_condition_predicate(predicate));

    sim_assert_eq!(
        have: with_predicate.contract_guards(),
        want: vec![
            Guard::With {
                path: "service.enabled".to_string(),
            },
            Guard::With {
                path: "service.type".to_string(),
            },
            Guard::Eq {
                path: "service.type".to_string(),
                value: GuardValue::string("ClusterIP"),
            },
            Guard::With {
                path: "service.annotations".to_string(),
            },
            Guard::With {
                path: "global.annotations".to_string(),
            },
            Guard::Or {
                paths: vec![
                    "service.annotations".to_string(),
                    "global.annotations".to_string(),
                ],
            },
            Guard::With {
                path: "service.disabled".to_string(),
            },
            Guard::Not {
                path: "service.disabled".to_string(),
            },
        ]
    );
    sim_assert_eq!(
        have: Predicate::all(with_predicates_from_condition_predicate(Predicate::False)),
        want: Predicate::False,
    );
}

#[test]
fn alias_or_predicate_subsumes_direct_truthy_predicates() {
    let alias_predicate = Predicate::Or(vec![
        Predicate::truthy_path("service.annotations"),
        Predicate::truthy_path("global.annotations"),
    ]);

    assert!(predicate_is_subsumed_by_alias_or_predicate(
        &Predicate::truthy_path("service.annotations"),
        std::slice::from_ref(&alias_predicate),
    ));
    assert!(predicate_is_subsumed_by_alias_or_predicate(
        &Predicate::Or(vec![
            Predicate::truthy_path("service.annotations"),
            Predicate::truthy_path("global.annotations"),
        ]),
        &[alias_predicate],
    ));
    assert!(!predicate_is_subsumed_by_alias_or_predicate(
        &Predicate::truthy_path("service.labels"),
        &[],
    ));
}

#[test]
fn precise_structural_predicate_suppresses_broader_truthy_fallback() {
    let structural = Predicate::Or(vec![
        Predicate::from(Guard::Eq {
            path: "service.type".to_string(),
            value: GuardValue::string("LoadBalancer"),
        }),
        Predicate::from(Guard::Eq {
            path: "service.type".to_string(),
            value: GuardValue::string("NodePort"),
        }),
    ]);
    let fallback = Predicate::Or(vec![Predicate::truthy_path("service.type")]);
    let structural_paths = predicate_value_paths(&structural);

    assert!(truthy_predicate_is_covered_by_structural_paths(
        &fallback,
        &structural_paths,
    ));
}
