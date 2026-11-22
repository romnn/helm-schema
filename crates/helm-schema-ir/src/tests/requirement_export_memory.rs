use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{
    ConditionalGuard, ContractRequirementImplication, ContractRequirementTarget,
    FailValueRequirement, Guard, GuardPrefix, Predicate, ValuesPath,
};
use test_util::prelude::sim_assert_eq;

use super::{
    ExistingCoverage, FoldedRequirement, canonical_occurrences, emit_requirement, fold,
    fold_clauses, fold_with_budget,
};
use crate::value::{
    ConsumptionDomain, FrozenConsumption, RequirementGuardInterner, RequirementIdentities,
};

#[test]
fn retained_canonical_occurrences_match_reinterned_proofs_and_work() -> eyre::Result<()> {
    let mut source = FrozenConsumption::test_complementary_guards(None)?;
    let first = source
        .first()
        .ok_or_eyre("missing guarded occurrence")?
        .clone();
    source.extend(
        (0..12).map(|index| first.test_under(Predicate::truthy_path(format!("gate{index:02}")))),
    );
    source.reverse();
    source.push(first);
    let mut identities = RequirementIdentities::default();
    let mut guards = RequirementGuardInterner::default();
    let mut clauses = Vec::new();
    let rows = canonical_occurrences(
        &source.iter().collect::<Vec<_>>(),
        &mut identities,
        &mut guards,
        &GuardPrefix::default(),
        &mut clauses,
    );
    assert!(clauses.is_empty());
    sim_assert_eq!(have: rows.len(), want: source.len() - 1);

    let mut replay = RequirementGuardInterner::default();
    let replayed = rows
        .iter()
        .map(|row| {
            let occurrence = &row.occurrence;
            let key = occurrence
                .order_key(&mut identities, &mut replay)
                .ok_or_eyre("canonical occurrence cannot be rekeyed")?;
            occurrence
                .canonicalized(&key, &replay)
                .ok_or_eyre("replayed key cannot be materialized")
        })
        .collect::<eyre::Result<Vec<_>>>()?;
    let original = rows.iter().map(|row| &row.occurrence).collect::<Vec<_>>();
    let replayed = replayed.iter().collect::<Vec<_>>();
    sim_assert_eq!(have: &original, want: &replayed);

    let mut completed = false;
    let mut exhausted = false;
    for work in [8, 64, 128, 256, 1024, 4096] {
        let prove = |occurrences: &[&FrozenConsumption]| {
            let mut remaining = work;
            let folded = fold_with_budget(
                occurrences,
                &ExistingCoverage::default(),
                None,
                &mut remaining,
                &mut RequirementIdentities::default(),
            );
            (folded, remaining)
        };
        let (folded, remaining) = prove(&original);
        let (legacy_folded, legacy_remaining) = prove(&replayed);
        sim_assert_eq!(have: (&folded, remaining), want: (&legacy_folded, legacy_remaining));
        if let Some(folded) = folded {
            sim_assert_eq!(
                have: folded.guard.predicate().normalize_boolean(),
                want: Predicate::truthy_path("p"),
            );
            completed = true;
        } else {
            exhausted = true;
        }
    }
    assert!(completed && exhausted);
    Ok(())
}

#[test]
fn absence_exports_do_not_retain_doubled_guard_buffers() -> eyre::Result<()> {
    let source = FrozenConsumption::test_complementary_guards(Some(Predicate::True))?;
    let mut key = source.first().ok_or_eyre("source consumption")?.fold_key();
    let requirement = FailValueRequirement::SchemaType("string".to_owned());
    key.domain = ConsumptionDomain::Known {
        preimage: requirement.clone(),
        requires_presence: true,
    };
    let target = key.path.clone();
    let guard = ConditionalGuard::Truthy {
        path: ValuesPath::parse("enabled"),
    };
    let prefix = helm_schema_core::GuardPrefix::new(
        (0..32)
            .map(|index| ConditionalGuard::Truthy {
                path: ValuesPath::parse(&format!("flag{index:02}")),
            })
            .collect::<Vec<_>>(),
    );
    let mut expected = vec![guard.clone()];
    expected.extend_from_slice(prefix.as_slice());
    let implication = ContractRequirementImplication::new(
        expected.clone(),
        ContractRequirementTarget::Value,
        vec![requirement],
    );
    expected.push(ConditionalGuard::Absent {
        path: target.clone(),
    });
    let mut paths = BTreeMap::new();
    let mut terminals = Vec::new();
    emit_requirement(
        &mut paths,
        &mut terminals,
        key,
        FoldedRequirement {
            guard,
            covered: false,
            absence_covered: false,
            projection_complete: true,
            proved_impossible: false,
        },
        &BTreeSet::new(),
        &prefix,
    );
    sim_assert_eq!(have: &terminals, want: &vec![expected]);
    sim_assert_eq!(have: &paths.get(&target).ok_or_eyre("target requirements")?.requirement_implications, want: &vec![implication]);
    let terminal = terminals.first().ok_or_eyre("absence clause")?;
    assert!(
        terminal.owned_capacity() <= 2,
        "absence export retained excess guard storage: {} slots for {} guards",
        terminal.owned_capacity(),
        terminal.len()
    );
    Ok(())
}

#[test]
fn literal_true_cone_folds_before_the_irrelevant_inventory_exhausts() -> eyre::Result<()> {
    let source = FrozenConsumption::test_complementary_guards(None)?;
    let literal = source
        .first()
        .ok_or_eyre("source consumption")?
        .test_literal_true();
    let guarded = (0..128)
        .map(|index| literal.test_under(Predicate::truthy_path(format!("flag{index:03}"))))
        .collect::<Vec<_>>();
    let guarded_refs = guarded.iter().collect::<Vec<_>>();
    let no_coverage = ExistingCoverage::default();
    let covered = ExistingCoverage {
        body: vec![Predicate::True],
        absence: vec![Predicate::from(Guard::Absent {
            path: literal.path.clone(),
        })],
    };
    let presence = Some(&literal.path);
    let allowance = 256;
    let one_fold = |occurrences: &[&FrozenConsumption], covers: &ExistingCoverage| {
        fold(
            occurrences,
            covers,
            presence,
            &mut RequirementIdentities::default(),
            allowance,
        )
    };
    assert!(one_fold(&guarded_refs, &no_coverage).is_none());

    let mut union = guarded_refs.clone();
    union.push(&literal);
    for (covers, expected_coverage) in [(&no_coverage, false), (&covered, true)] {
        let singleton = one_fold(&[&literal], covers).ok_or_eyre("literal true fold")?;
        sim_assert_eq!(have: singleton.guard.predicate(), want: Predicate::True);
        sim_assert_eq!(have: singleton.covered, want: expected_coverage);
        sim_assert_eq!(have: singleton.absence_covered, want: expected_coverage);
        assert!(singleton.projection_complete && !singleton.proved_impossible);
        let clauses = fold_clauses(&union, covers, presence, allowance);
        sim_assert_eq!(have: clauses.len(), want: 1);
        let clause = clauses.first().ok_or_eyre("complete unconditional union")?;
        assert!(clause.prefix.is_empty());
        sim_assert_eq!(have: &clause.folded, want: &singleton);
    }

    let mut activated = literal.clone();
    activated.append_activation(&[Guard::Truthy {
        path: ValuesPath::parse("dependency.enabled"),
    }]);
    let unnormalized = literal.test_under(Predicate::True);
    assert!(
        one_fold(&[&unnormalized], &no_coverage)
            .is_some_and(|part| part.guard.predicate() == Predicate::True)
    );
    for ineligible in [&activated, &unnormalized] {
        let mut union = guarded_refs.clone();
        union.push(ineligible);
        assert!(one_fold(&union, &no_coverage).is_none());
    }

    let independent = FrozenConsumption::test_complementary_guards(None)?;
    let unknown = independent
        .first()
        .ok_or_eyre("separate unresolved occurrence")?;
    sim_assert_eq!(have: unknown.fold_key(), want: literal.fold_key());
    eyre::ensure!(
        unknown.requirement_scope_key() != literal.requirement_scope_key(),
        "an unresolved choice must retain its own requirement scope"
    );
    union.push(unknown);
    let clauses = fold_clauses(&union, &no_coverage, presence, allowance);
    let unconditional = clauses
        .iter()
        .find(|clause| clause.folded.guard.predicate() == Predicate::True)
        .ok_or_eyre("unconditional part of the incomplete group")?;
    assert!(unconditional.prefix.is_empty());
    assert!(!unconditional.folded.projection_complete);
    Ok(())
}

/// Independent occurrences remain shared when a bounded union needs values refinement.
#[test]
fn independent_occurrences_under_a_split_keep_a_bounded_clause_count() -> eyre::Result<()> {
    let source = FrozenConsumption::test_complementary_guards(None)?;
    let literal = source
        .first()
        .ok_or_eyre("source consumption")?
        .test_literal_true();
    let independent = (0..24)
        .map(|index| Predicate::truthy_path(format!("independent{index:03}")))
        .collect::<Vec<_>>();
    let dependent = (0..12)
        .map(|index| {
            Predicate::all(vec![
                Predicate::truthy_path(format!("choice{index:03}")),
                Predicate::truthy_path("gate"),
            ])
        })
        .collect::<Vec<_>>();
    let expected =
        Predicate::Or(independent.iter().chain(&dependent).cloned().collect()).normalize_boolean();
    let occurrences = independent
        .iter()
        .chain(&dependent)
        .map(|predicate| literal.test_under(predicate.clone()))
        .collect::<Vec<_>>();
    let mut refs = occurrences.iter().collect::<Vec<_>>();
    let allowance = 128;
    assert!(
        fold(
            &refs,
            &ExistingCoverage::default(),
            None,
            &mut RequirementIdentities::default(),
            allowance
        )
        .is_none()
    );
    let mut canonical = None::<serde_json::Value>;
    for reversed in [false, true] {
        if reversed {
            refs.reverse();
        }
        let clauses = fold_clauses(&refs, &ExistingCoverage::default(), None, allowance);
        assert!(
            clauses.len() <= 4,
            "independent occurrences expanded into {} clauses",
            clauses.len()
        );
        let mut reachable = 0;
        for clause in &clauses {
            if let ConditionalGuard::Requirement(graph) = &clause.folded.guard {
                reachable += graph.reachable_ids().len();
            }
        }
        assert!(reachable > 0, "the law must exercise shared refinement");
        assert!(
            reachable <= 16 * occurrences.len(),
            "refinement expanded into {reachable} graph nodes"
        );
        let actual = Predicate::Or(
            clauses
                .iter()
                .map(|clause| clause.folded.guard.predicate())
                .collect(),
        )
        .normalize_boolean();
        let memo = helm_schema_core::PredicateMemo::new();
        sim_assert_eq!(have: memo.exactly_implies(&actual, &expected), want: true);
        sim_assert_eq!(have: memo.exactly_implies(&expected, &actual), want: true);
        let wire = serde_json::to_value(
            clauses
                .iter()
                .map(|clause| &clause.folded.guard)
                .collect::<Vec<_>>(),
        )?;
        if let Some(expected) = &canonical {
            sim_assert_eq!(have: &wire, want: expected);
        } else {
            canonical = Some(wire);
        }
    }
    Ok(())
}

/// A leaf whose cofactor exhausts its own allowance cannot block the remaining union's proof.
#[test]
fn an_uncofactored_leaf_does_not_repeat_the_same_refinement() -> eyre::Result<()> {
    let source = FrozenConsumption::test_complementary_guards(None)?;
    let literal = source
        .first()
        .ok_or_eyre("source consumption")?
        .test_literal_true();
    let mut shared = Predicate::truthy_path("z");
    for _ in 0..17 {
        shared = Predicate::And(vec![shared.clone(), shared]);
    }
    let large = literal.test_under(Predicate::And(vec![Predicate::truthy_path("a"), shared]));
    let mut occurrences = source
        .iter()
        .map(|occurrence| occurrence.test_under(Predicate::truthy_path("a")))
        .collect::<Vec<_>>();
    occurrences.push(large);
    let refs = occurrences.iter().collect::<Vec<_>>();
    let expected = Predicate::all(vec![
        Predicate::truthy_path("a"),
        Predicate::Or(vec![
            Predicate::truthy_path("p"),
            Predicate::truthy_path("z"),
        ]),
    ])
    .normalize_boolean();
    let mut recovered = false;
    for work in [64, 96, 128, 192, 256] {
        let clauses = fold_clauses(&refs, &ExistingCoverage::default(), None, work);
        if clauses
            .iter()
            .any(|clause| matches!(clause.folded.guard, ConditionalGuard::Requirement(_)))
        {
            let actual = Predicate::Or(
                clauses
                    .iter()
                    .map(|clause| clause.folded.guard.predicate())
                    .collect(),
            )
            .normalize_boolean();
            let memo = helm_schema_core::PredicateMemo::new();
            if memo.exactly_implies(&actual, &expected) && memo.exactly_implies(&expected, &actual)
            {
                recovered = true;
                break;
            }
        }
    }
    assert!(
        recovered,
        "an exhausted leaf prevented complementary choices from cancelling"
    );
    Ok(())
}

/// (6a1 rework item 3, ADR 0072 §10) A large occurrence with no values atom to split on widens
/// alone: after the split on `p`, the small union of `p ∧ c` and `p ∧ ¬c` still projects `c`
/// jointly, so the proof of `p` survives beside it.
#[test]
fn an_unsplittable_large_occurrence_keeps_the_small_union_proof() -> eyre::Result<()> {
    let source = FrozenConsumption::test_complementary_guards(None)?;
    let large = source
        .first()
        .ok_or_eyre("source consumption")?
        .test_literal_true()
        .test_under_choices(256);
    let mut occurrences = source.clone();
    occurrences.push(large);
    let mut refs = occurrences.iter().collect::<Vec<_>>();
    let work = 64;
    assert!(
        fold(
            &refs,
            &ExistingCoverage::default(),
            None,
            &mut RequirementIdentities::default(),
            work,
        )
        .is_none(),
        "the large occurrence must exhaust the joint fold"
    );
    let expected = Predicate::truthy_path("p");
    for reversed in [false, true] {
        if reversed {
            refs.reverse();
        }
        let clauses = fold_clauses(&refs, &ExistingCoverage::default(), None, work);
        let actual = Predicate::Or(
            clauses
                .iter()
                .map(|clause| clause.folded.guard.predicate())
                .collect(),
        )
        .normalize_boolean();
        let memo = helm_schema_core::PredicateMemo::new();
        sim_assert_eq!(have: memo.exactly_implies(&actual, &expected), want: true, "{actual:?}");
        sim_assert_eq!(have: memo.exactly_implies(&expected, &actual), want: true, "{actual:?}");
    }
    Ok(())
}

#[test]
fn reversing_different_sharing_keeps_the_same_fold_budget_edge() -> eyre::Result<()> {
    fn branch(first: Predicate, second: Predicate) -> Predicate {
        Predicate::And(vec![
            Predicate::Or(vec![first, Predicate::truthy_path("left")]),
            Predicate::Or(vec![second, Predicate::truthy_path("right")]),
        ])
    }
    let common = Predicate::Or(vec![
        Predicate::truthy_path("a"),
        Predicate::truthy_path("b"),
    ]);
    let shared = branch(common.clone(), common);
    let fresh = || {
        Predicate::Or(vec![
            Predicate::truthy_path("a"),
            Predicate::truthy_path("b"),
        ])
    };
    let unshared = branch(fresh(), fresh());
    sim_assert_eq!(have: &shared, want: &unshared);
    let source = FrozenConsumption::test_complementary_guards(None)?;
    let base = source
        .first()
        .ok_or_eyre("base occurrence")?
        .test_literal_true();
    let first = base.test_under(shared);
    let second = base.test_under(unshared);

    let prove = |order: &[&FrozenConsumption], mut work: usize| {
        let mut identities = RequirementIdentities::default();
        let mut guards = RequirementGuardInterner::default();
        let rows = canonical_occurrences(
            order,
            &mut identities,
            &mut guards,
            &GuardPrefix::default(),
            &mut Vec::new(),
        );
        let occurrences = rows.iter().map(|row| &row.occurrence).collect::<Vec<_>>();
        let result = fold_with_budget(
            &occurrences,
            &ExistingCoverage::default(),
            None,
            &mut work,
            &mut identities,
        );
        (result.map(|row| row.guard), work)
    };
    let forward = [&first, &second];
    let reverse = [&second, &first];
    let edge = (1..1024)
        .find(|&work| prove(&forward, work).0.is_some())
        .ok_or_eyre("fold must fit")?;
    for work in [edge - 1, edge, edge + 1] {
        sim_assert_eq!(have: prove(&reverse, work), want: prove(&forward, work), "work {work}");
    }
    Ok(())
}
