use std::collections::BTreeSet;

use test_util::prelude::sim_assert_eq;

use super::{
    FALSE, PredicateBdd, PredicateMemo, TRUE, exact_implies_uncached, memo_sizes,
    normalize_literal_conjunction, normalize_uncached, simplify_structure,
};
use crate::{
    ApproximationRole, CollectionDomain, CollectionQuantifier, Guard, GuardValue, Predicate,
    PredicateKind, ValuePattern, ValuesPath,
};

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

fn comparison_guards() -> Vec<Guard> {
    let path = ValuesPath::parse("subject");
    vec![
        Guard::Truthy { path: path.clone() },
        Guard::Not { path: path.clone() },
        Guard::Eq {
            path: path.clone(),
            value: GuardValue::Bool(true),
        },
        Guard::NotEq {
            path: path.clone(),
            value: GuardValue::Bool(true),
        },
        Guard::Absent { path: path.clone() },
        Guard::MatchesPattern {
            path: path.clone(),
            pattern: "^a$".into(),
            templated: false,
        },
        Guard::NotMatchesPattern {
            path: path.clone(),
            pattern: "^a$".into(),
        },
        Guard::MatchesValue {
            path: path.clone(),
            pattern: ValuePattern::Any,
        },
        Guard::TypeIs {
            path: path.clone(),
            schema_type: "string".into(),
        },
        Guard::NotTypeIs {
            path: path.clone(),
            schema_type: "string".into(),
        },
        Guard::IntGt {
            path: path.clone(),
            bound: 0,
        },
        Guard::IntLt {
            path: path.clone(),
            bound: 3,
        },
        Guard::HasKey {
            path: path.clone(),
            key: "key".into(),
        },
        Guard::NotHasKey {
            path,
            key: "key".into(),
        },
    ]
}

fn collection_guards() -> Vec<Guard> {
    let path = ValuesPath::parse("subject");
    vec![
        Guard::CollectionPattern {
            path: path.clone(),
            domain: CollectionDomain::ObjectValues,
            quantifier: CollectionQuantifier::All,
            pattern: "^a$".into(),
        },
        Guard::RangeKeyPrefix {
            path: path.clone(),
            prefix: "a".into(),
        },
        Guard::RangeKeyEquals {
            path: path.clone(),
            key: "key".into(),
        },
        Guard::RangeKeyMatches {
            path: path.clone(),
            pattern: "^a$".into(),
        },
        Guard::Range { path: path.clone() },
        Guard::With { path: path.clone() },
        Guard::Default { path: path.clone() },
        Guard::AtMostOneMember { path: path.clone() },
        Guard::MinMembers {
            path: path.clone(),
            bound: 1,
        },
        Guard::ContainsEquals {
            path: path.clone(),
            value: GuardValue::Null,
        },
        Guard::ContainsMemberEquals {
            path: path.clone(),
            member: "key".into(),
            value: GuardValue::Bool(true),
        },
        Guard::ContainsTruthyMember {
            path,
            member: "key".into(),
        },
    ]
}

fn coupled_guard_predicates() -> Vec<Predicate> {
    let path = ValuesPath::parse("subject");
    let first = Guard::Eq {
        path: path.clone(),
        value: GuardValue::Int(1),
    };
    let second = Guard::Eq {
        path: path.clone(),
        value: GuardValue::Int(2),
    };
    let third = Guard::Eq {
        path: path.clone(),
        value: GuardValue::Int(3),
    };
    let fourth = Guard::Eq {
        path: path.clone(),
        value: GuardValue::Int(4),
    };
    let string = Guard::TypeIs {
        path: path.clone(),
        schema_type: "string".into(),
    };
    let pattern = Guard::MatchesPattern {
        path: path.clone(),
        pattern: "^a$".into(),
        templated: false,
    };
    let compound = Guard::AnyOf {
        alternatives: vec![vec![first.clone(), second.clone()]],
    };
    vec![
        Predicate::And(vec![first.clone().into(), second.clone().into()]),
        Predicate::And(vec![first.clone().into(), string.into()]),
        Predicate::And(vec![first.clone().into(), pattern.into()]),
        Predicate::Guard(compound.clone()),
        compound.into(),
        Predicate::Guard(Guard::AnyOf {
            alternatives: Vec::new(),
        }),
        Predicate::from(Guard::AnyOf {
            alternatives: Vec::new(),
        }),
        Predicate::Guard(Guard::AnyOf {
            alternatives: vec![
                vec![Guard::Truthy { path: path.clone() }],
                vec![Guard::Not { path: path.clone() }],
            ],
        }),
        Predicate::Guard(Guard::Or { paths: vec![path] }),
        // The first Boolean path conflicts on subject; the later path admits subject=1.
        Predicate::Or(vec![
            first.clone().into(),
            Predicate::And(vec![second.clone().into(), third.clone().into()]),
        ]),
        Predicate::Or(vec![
            Predicate::Guard(first.clone()).negated(),
            Predicate::And(vec![second.clone().into(), third.clone().into()]),
        ]),
        Predicate::Or(vec![
            Predicate::And(vec![first.into(), second.clone().into()]),
            Predicate::And(vec![third.into(), fourth.into()]),
        ]),
        // Every typed path conflicts, but the old path bound must still abstain.
        Predicate::And(vec![
            Predicate::And(vec![
                second.into(),
                Guard::Eq {
                    path: ValuesPath::parse("subject"),
                    value: GuardValue::Int(3),
                }
                .into(),
            ]),
            Predicate::And(
                (0..9)
                    .map(|index| Predicate::Or(vec![guard(index * 2), guard(index * 2 + 1)]))
                    .collect(),
            ),
        ]),
    ]
}

fn check_memoized_conjunction(predicate: Predicate) {
    let expected = normalize_uncached(predicate.clone());
    let memo = PredicateMemo::new();
    sim_assert_eq!(have: memo.may_be_true(predicate.clone()), want: expected != Predicate::False);
    sim_assert_eq!(have: memo.normalize(predicate), want: expected);
}

fn check_literal_conjunctions() {
    let path = ValuesPath::parse("subject");
    let guards = vec![
        Guard::Truthy { path: path.clone() },
        Guard::Not { path: path.clone() },
        Guard::Eq {
            path: path.clone(),
            value: GuardValue::Int(1),
        },
        Guard::NotEq {
            path: path.clone(),
            value: GuardValue::Int(1),
        },
        Guard::Eq {
            path: path.clone(),
            value: GuardValue::Int(2),
        },
        Guard::TypeIs {
            path: path.clone(),
            schema_type: "string".into(),
        },
        Guard::NotTypeIs {
            path: path.clone(),
            schema_type: "string".into(),
        },
        Guard::MatchesPattern {
            path: path.clone(),
            pattern: "^a$".into(),
            templated: false,
        },
        Guard::HasKey {
            path: path.clone(),
            key: "key".into(),
        },
        Guard::NotHasKey {
            path,
            key: "key".into(),
        },
    ];
    let literals = guards
        .into_iter()
        .flat_map(|guard| {
            let predicate = Predicate::Guard(guard);
            [predicate.clone(), predicate.negated()]
        })
        .collect::<Vec<_>>();
    for first in &literals {
        sim_assert_eq!(have: normalize_literal_conjunction(first).is_some(), want: true);
        check_memoized_conjunction(first.clone());
        for second in &literals {
            check_memoized_conjunction(Predicate::And(vec![first.clone(), second.clone()]));
            for third in &literals {
                check_memoized_conjunction(Predicate::And(vec![
                    first.clone(),
                    second.clone(),
                    third.clone(),
                ]));
            }
        }
    }
    for count in [43, 44, 45, 256, 257] {
        let predicate = Predicate::And((0..count).map(guard).collect());
        sim_assert_eq!(have: normalize_literal_conjunction(&predicate).is_some(), want: count <= 44);
        check_memoized_conjunction(predicate.clone());
        check_memoized_conjunction(Predicate::And(vec![predicate, guard(0).negated()]));
    }
    let aliases = Predicate::And(literals.into_iter().cycle().take(257).collect());
    sim_assert_eq!(have: normalize_literal_conjunction(&aliases), want: None);
    check_memoized_conjunction(aliases);

    // An early contradiction must not bypass rejection of an unsupported later factor.
    let unsupported = Predicate::And(vec![
        guard(0),
        guard(0).negated(),
        Predicate::Or(vec![guard(1), guard(2)]),
    ]);
    sim_assert_eq!(have: normalize_literal_conjunction(&unsupported), want: None);
    check_memoized_conjunction(unsupported);
}

fn check_structural_fixed_points() {
    let mut predicates = generated_predicates();
    predicates.extend(coupled_guard_predicates());
    predicates.extend(
        comparison_guards()
            .into_iter()
            .chain(collection_guards())
            .flat_map(|guard| [Predicate::Guard(guard.clone()), Predicate::from(guard)]),
    );
    let capped = Predicate::And((0..=super::MAX_BDD_NODES).map(guard).collect());
    predicates.push(Predicate::And(vec![
        capped.clone(),
        Predicate::Approximate(
            "bounded exact antecedent".into(),
            BTreeSet::new(),
            ApproximationRole::Control,
            Some(Box::new(capped)),
        ),
    ]));
    for predicate in predicates.clone() {
        predicates.extend([
            Predicate::And(vec![predicate.clone(), Predicate::True]),
            Predicate::Or(vec![Predicate::False, predicate.clone()]),
            Predicate::Not(Box::new(Predicate::Not(Box::new(predicate)))),
        ]);
    }
    for predicate in predicates {
        let simplified = simplify_structure(predicate.clone(), None);
        sim_assert_eq!(have: simplify_structure(simplified.clone(), None), want: simplified.clone());
        let memo = PredicateMemo::new();
        sim_assert_eq!(have: simplify_structure(predicate, Some(&memo)), want: simplified.clone());
        let populated = memo.simplified.borrow().len();
        sim_assert_eq!(have: simplify_structure(simplified.clone(), Some(&memo)), want: simplified);
        sim_assert_eq!(have: memo.simplified.borrow().len(), want: populated);
    }
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

    let mut feasibility_predicates = predicates.clone();
    feasibility_predicates.extend(coupled_guard_predicates());
    feasibility_predicates.extend(
        comparison_guards()
            .into_iter()
            .chain(collection_guards())
            .flat_map(|guard| [Predicate::Guard(guard.clone()), Predicate::from(guard)]),
    );
    let normal_form_cap = Predicate::And(
        (0..9)
            .map(|index| Predicate::Or(vec![guard(index * 2), guard(index * 2 + 1)]))
            .collect(),
    );
    for compound in [
        Guard::Or { paths: Vec::new() },
        Guard::AnyOf {
            alternatives: Vec::new(),
        },
        Guard::AnyOf {
            alternatives: vec![
                vec![Guard::Truthy {
                    path: ValuesPath::parse("a"),
                }],
                vec![Guard::Not {
                    path: ValuesPath::parse("a"),
                }],
            ],
        },
    ] {
        let compound = Predicate::Guard(compound);
        feasibility_predicates.extend([
            Predicate::And(vec![normal_form_cap.clone(), compound.clone()]),
            Predicate::Or(vec![normal_form_cap.clone(), compound]),
        ]);
    }
    feasibility_predicates.extend([
        Predicate::And(vec![guard(0), guard(0).negated()]),
        Predicate::And(vec![
            guard(0),
            guard(0).negated(),
            Predicate::Approximate(
                "contradictory opaque arm".to_owned(),
                BTreeSet::new(),
                ApproximationRole::Control,
                Some(Box::new(guard(0))),
            ),
        ]),
        Predicate::And(
            (0..=super::MAX_BDD_NODES)
                .map(guard)
                .chain([guard(0).negated()])
                .collect(),
        ),
    ]);
    for predicate in feasibility_predicates {
        let normalized = normalize_uncached(predicate.clone());
        let want = normalized != Predicate::False;
        let memo = PredicateMemo::new();
        sim_assert_eq!(have: memo.may_be_true(predicate.clone()), want: want);
        let populated = memo.feasibility.borrow().len();
        sim_assert_eq!(have: memo.may_be_true(predicate.clone()), want: want);
        sim_assert_eq!(have: memo.feasibility.borrow().len(), want: populated);
        sim_assert_eq!(have: memo.normalize(predicate.clone()), want: normalized);
        sim_assert_eq!(have: memo.may_be_true(predicate), want: want);
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
    check_structural_fixed_points();
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

    for predicate in generated_predicates() {
        let memo = PredicateMemo::new();
        sim_assert_eq!(have: memo.normalize(predicate.clone()), want: normalize_uncached(predicate.clone()));
        let normal_forms = memo.normal_forms.borrow().len();
        for wrapped in [
            Predicate::And(vec![predicate.clone(), Predicate::True]),
            Predicate::Or(vec![Predicate::False, predicate.clone()]),
            Predicate::Not(Box::new(Predicate::Not(Box::new(predicate)))),
        ] {
            let want = normalize_uncached(wrapped.clone());
            sim_assert_eq!(have: memo.may_be_true(wrapped.clone()), want: want != Predicate::False);
            sim_assert_eq!(have: memo.normal_forms.borrow().len(), want: normal_forms);
            sim_assert_eq!(have: memo.normalize(wrapped), want: want);
            sim_assert_eq!(have: memo.normal_forms.borrow().len(), want: normal_forms);
        }
    }
    check_literal_conjunctions();
}
