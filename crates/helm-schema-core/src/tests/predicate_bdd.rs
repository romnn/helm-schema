use std::collections::BTreeSet;

use color_eyre::eyre::{self, OptionExt as _};
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

#[test]
fn shared_predicate_dag_normalizes_without_unfolding_occurrences() -> eyre::Result<()> {
    let leaf = guard(0);
    let mut predicate = leaf.clone();
    // Forty shared levels represent over a trillion unfolded occurrences.
    for level in 0..40 {
        predicate = if level % 2 == 0 {
            Predicate::And(vec![predicate.clone(), predicate])
        } else {
            Predicate::Or(vec![predicate.clone(), predicate])
        };
    }
    assert!(predicate.syntax_size() > 1_000_000_000_000);
    sim_assert_eq!(have: predicate.dag_size(40), want: None);
    sim_assert_eq!(have: predicate.dag_size(41), want: Some(41));
    let mut bdd = PredicateBdd::for_predicate(&predicate).ok_or_eyre("exact DAG has one atom")?;
    sim_assert_eq!(have: bdd.build(&predicate), want: Some(2));
    sim_assert_eq!(have: bdd.built.len(), want: 41);
    let mut remaining = 256;
    sim_assert_eq!(have: PredicateMemo::new().normalize_bounded(predicate.clone(), &mut remaining), want: Some(leaf.clone()));
    sim_assert_eq!(have: predicate.normalize_boolean(), want: leaf);
    Ok(())
}

#[test]
fn owned_compound_guard_payloads_consume_the_input_allowance() {
    let predicate = Predicate::Guard(Guard::AnyOf {
        alternatives: vec![
            (0..5000)
                .map(|index| Guard::Truthy {
                    path: ValuesPath::from_segments([format!("key{index}")]),
                })
                .collect(),
        ],
    });
    sim_assert_eq!(have: predicate.dag_size(32), want: None);
    let mut remaining = 32;
    sim_assert_eq!(have: PredicateMemo::new().normalize_bounded(predicate, &mut remaining), want: None);
    sim_assert_eq!(have: remaining, want: 0);
}

#[test]
fn path_extraction_charges_before_copying_a_completed_path() -> eyre::Result<()> {
    let predicate = Predicate::And(vec![guard(0), guard(1)]);
    let mut bdd =
        PredicateBdd::for_predicate(&predicate).ok_or_eyre("exact conjunction has a BDD")?;
    let root = bdd.build(&predicate).ok_or_eyre("small conjunction fits")?;
    let mut remaining = 2;
    sim_assert_eq!(have: bdd.paths_to_with_work(root, TRUE, &mut remaining), want: None);
    sim_assert_eq!(have: remaining, want: 0);
    Ok(())
}

#[test]
fn exhausted_normalization_abstains_without_poisoning_a_later_proof() {
    let memo = PredicateMemo::new();
    let first = guard(0);
    let second = guard(1);
    let predicate = Predicate::Or(vec![
        Predicate::And(vec![first.clone(), second.clone()]),
        Predicate::And(vec![first.clone(), second.negated()]),
    ]);
    for allowance in [0, 6] {
        let mut remaining = allowance;
        sim_assert_eq!(have: memo.normalize_bounded(predicate.clone(), &mut remaining), want: None);
    }
    let mut remaining = 256;
    sim_assert_eq!(have: memo.normalize_bounded(predicate, &mut remaining), want: Some(first));
    assert!(remaining < 256);
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
        let simplified = simplify_structure(predicate.clone(), &PredicateMemo::new());
        sim_assert_eq!(have: simplify_structure(simplified.clone(), &PredicateMemo::new()), want: simplified.clone());
        let memo = PredicateMemo::new();
        sim_assert_eq!(have: simplify_structure(predicate, &memo), want: simplified.clone());
        let populated = memo.simplified.borrow().len();
        sim_assert_eq!(have: simplify_structure(simplified.clone(), &memo), want: simplified);
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
        want: Some(Guard::Truthy {
            path: ValuesPath::from_segments(["selected"]),
        })
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
    let shared = simplify_structure(
        Predicate::Or(vec![guard(1), guard(0).negated()]),
        &PredicateMemo::new(),
    );
    let negated = Predicate::Not(Box::new(shared.clone()));
    let root = simplify_structure(
        Predicate::And(vec![shared.clone(), negated.clone()]),
        &PredicateMemo::new(),
    );
    for predicate in [shared, negated, root] {
        let simplified = simplify_structure(predicate.clone(), &PredicateMemo::new());
        sim_assert_eq!(have: simplified.clone(), want: predicate.clone());
        sim_assert_eq!(have: same_compound_storage(&predicate, &simplified), want: true);
    }
    check_structural_fixed_points();
}

#[test]
fn structural_simplification_changes_only_the_required_branch() {
    let shared = simplify_structure(
        Predicate::Or(vec![guard(0), guard(1)]),
        &PredicateMemo::new(),
    );
    let changed = Predicate::And(vec![
        Predicate::True,
        Predicate::Not(Box::new(Predicate::Not(Box::new(shared.clone())))),
    ]);
    let simplified = simplify_structure(changed, &PredicateMemo::new());
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

#[test]
fn requirement_fold_projects_after_combining_occurrences() -> eyre::Result<()> {
    let choice = crate::RequirementChoice {
        scope: "chart".to_owned(),
        decision: 17,
    };
    let atoms = vec![
        crate::RequirementAtom::Choice(choice.clone()),
        crate::RequirementAtom::Value(Guard::Truthy {
            path: ValuesPath::parse("enabled"),
        }),
    ];
    let mut work = 16_384;
    let mut bdd =
        crate::RequirementBdd::new(&atoms, &mut work).ok_or_eyre("inventory exhausted")?;
    let unknown = bdd.choice(&choice).ok_or_eyre("choice exhausted")?;
    let enabled = bdd
        .atom(atoms.get(1).ok_or_eyre("enabled atom missing")?)
        .ok_or_eyre("value atom exhausted")?;
    let yes = bdd.and(unknown, enabled).ok_or_eyre("true arm exhausted")?;
    let not_unknown = bdd.not(unknown).ok_or_eyre("complement exhausted")?;
    let no = bdd
        .and(not_unknown, enabled)
        .ok_or_eyre("false arm exhausted")?;
    let root = bdd.or(yes, no).ok_or_eyre("aggregation exhausted")?;
    let projected = bdd
        .finish(root)
        .ok_or_eyre("projection exhausted")?
        .predicate()
        .normalize_boolean();
    sim_assert_eq!(have: projected, want: Predicate::truthy_path("enabled"));
    Ok(())
}

#[test]
fn requirement_fold_exhaustion_omits_a_positive_obligation() -> eyre::Result<()> {
    let atoms = vec![crate::RequirementAtom::Value(Guard::Truthy {
        path: ValuesPath::parse("enabled"),
    })];
    for allowance in [0, 1, 2] {
        let mut work = allowance;
        let projected = crate::RequirementBdd::new(&atoms, &mut work).and_then(|mut bdd| {
            let root = bdd.atom(atoms.first()?)?;
            bdd.finish(root)
        });
        sim_assert_eq!(have: projected, want: None);
    }
    let mut work = 16_384;
    let mut bdd =
        crate::RequirementBdd::new(&atoms, &mut work).ok_or_eyre("inventory exhausted")?;
    let root = bdd
        .atom(atoms.first().ok_or_eyre("positive atom missing")?)
        .ok_or_eyre("atom exhausted")?;
    let projected = bdd
        .finish(root)
        .ok_or_eyre("positive projection exhausted")?;
    sim_assert_eq!(have: projected.predicate().normalize_boolean(), want: Predicate::truthy_path("enabled"));
    Ok(())
}

#[test]
fn requirement_fold_never_correlates_distinct_unknown_choices() -> eyre::Result<()> {
    for (scope, decision) in [("chart", 18), ("dependency", 17)] {
        let first = crate::RequirementChoice {
            scope: "chart".to_owned(),
            decision: 17,
        };
        let second = crate::RequirementChoice {
            scope: scope.to_owned(),
            decision,
        };
        let atoms = vec![
            crate::RequirementAtom::Choice(first.clone()),
            crate::RequirementAtom::Choice(second.clone()),
        ];
        let mut work = 16_384;
        let mut bdd =
            crate::RequirementBdd::new(&atoms, &mut work).ok_or_eyre("inventory exhausted")?;
        let yes = bdd.choice(&first).ok_or_eyre("first choice exhausted")?;
        let no = bdd.choice(&second).ok_or_eyre("second choice exhausted")?;
        let no = bdd.not(no).ok_or_eyre("complement exhausted")?;
        let root = bdd.or(yes, no).ok_or_eyre("aggregation exhausted")?;
        let projected = bdd.finish(root).ok_or_eyre("projection exhausted")?;
        sim_assert_eq!(have: projected.predicate().normalize_boolean(), want: Predicate::False);
    }
    Ok(())
}

#[test]
fn requirement_fold_eliminates_atoms_without_exact_encodings() -> eyre::Result<()> {
    for guard in [
        Guard::IntGt {
            path: ValuesPath::parse("count"),
            bound: 0,
        },
        Guard::Range {
            path: ValuesPath::parse("count"),
        },
    ] {
        let atoms = vec![crate::RequirementAtom::Value(guard)];
        let mut work = 16_384;
        let mut bdd =
            crate::RequirementBdd::new(&atoms, &mut work).ok_or_eyre("inventory exhausted")?;
        let root = bdd
            .atom(atoms.first().ok_or_eyre("atom missing")?)
            .ok_or_eyre("atom exhausted")?;
        let projected = bdd.finish(root).ok_or_eyre("projection exhausted")?;
        sim_assert_eq!(have: projected.predicate().normalize_boolean(), want: Predicate::False);
    }
    Ok(())
}

#[test]
fn requirement_export_depth_and_bytes_have_positive_caps() {
    let mut graph = crate::RequirementGuard {
        decisions: Vec::new(),
        root: 1,
        projection_exact: true,
    };
    for index in 0..129 {
        graph.decisions.push(crate::RequirementDecision {
            test: crate::ConditionalGuard::Truthy {
                path: ValuesPath::parse(&format!("flag{index}")),
            },
            when_true: graph.root,
            when_false: 0,
        });
        graph.root = graph.decisions.len() + 1;
    }
    let mut work = 16_384;
    sim_assert_eq!(have: graph.check_export_budget(&mut work), want: None);
    graph.decisions.truncate(1);
    graph.root = 2;
    let mut work = 16_384;
    sim_assert_eq!(have: graph.check_export_budget(&mut work), want: Some(()));
    graph.decisions.clear();
    graph.root = 1;
    for index in 0..100 {
        graph.decisions.push(crate::RequirementDecision {
            test: crate::ConditionalGuard::Eq {
                path: ValuesPath::parse(&format!("flag{index}")),
                value: crate::GuardValue::String("x".repeat(500)),
            },
            when_true: 1,
            when_false: 0,
        });
    }
    let mut work = 16_384;
    sim_assert_eq!(have: graph.check_export_budget(&mut work), want: None);
}

#[test]
fn requirement_inventory_bounds_payloads_and_empty_root_paths() {
    let atom = crate::RequirementAtom::Value(Guard::Eq {
        path: ValuesPath::parse("mode"),
        value: crate::GuardValue::String("x".repeat(8193)),
    });
    let mut work = 16_384;
    assert!(crate::RequirementBdd::new(&[atom], &mut work).is_none());
    assert!(work < 16_384);
    let atom = crate::RequirementAtom::Value(Guard::Or {
        paths: vec![ValuesPath::default(); 1000],
    });
    let mut work = 10;
    sim_assert_eq!(have: atom.charge_inventory(&mut work, 1), want: None);
    let mut work = 2000;
    sim_assert_eq!(have: atom.charge_inventory(&mut work, 1), want: Some(()));
    assert!(work < 1000);
}

#[test]
fn requirement_fold_closes_complementary_opaque_cones_beyond_the_dnf_cap() -> eyre::Result<()> {
    let choices = (0..10)
        .map(|decision| crate::RequirementChoice {
            scope: "chart".to_owned(),
            decision,
        })
        .collect::<Vec<_>>();
    let enabled = crate::RequirementAtom::Value(Guard::Truthy {
        path: ValuesPath::parse("enabled"),
    });
    let atoms = std::iter::once(enabled.clone())
        .chain(choices.iter().cloned().map(crate::RequirementAtom::Choice))
        .collect::<Vec<_>>();
    let mut work = 16_384;
    let mut bdd =
        crate::RequirementBdd::new(&atoms, &mut work).ok_or_eyre("inventory exhausted")?;
    let mut parity = crate::RequirementBdd::never();
    for choice in &choices {
        let choice = bdd.choice(choice).ok_or_eyre("choice exhausted")?;
        let not_choice = bdd.not(choice).ok_or_eyre("choice complement exhausted")?;
        let not_parity = bdd.not(parity).ok_or_eyre("parity complement exhausted")?;
        let odd = bdd
            .and(not_parity, choice)
            .ok_or_eyre("odd arm exhausted")?;
        let even = bdd
            .and(parity, not_choice)
            .ok_or_eyre("even arm exhausted")?;
        parity = bdd.or(odd, even).ok_or_eyre("parity exhausted")?;
    }
    let enabled = bdd.atom(&enabled).ok_or_eyre("enabled atom exhausted")?;
    let yes = bdd
        .and(parity, enabled)
        .ok_or_eyre("first occurrence exhausted")?;
    let crate::RequirementNode(yes_index) = yes;
    assert!(
        bdd.bdd.paths_to(yes_index, TRUE).is_none(),
        "the old bounded DNF cannot enumerate this positive cone"
    );
    let opposite = bdd.not(parity).ok_or_eyre("complement exhausted")?;
    let no = bdd
        .and(opposite, enabled)
        .ok_or_eyre("second occurrence exhausted")?;
    let root = bdd.or(yes, no).ok_or_eyre("aggregation exhausted")?;
    let projected = bdd.finish(root).ok_or_eyre("projection exhausted")?;
    sim_assert_eq!(have: projected.predicate().normalize_boolean(), want: Predicate::truthy_path("enabled"));
    Ok(())
}
