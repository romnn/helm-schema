use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{ConditionalGuard, Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;
use tracing_subscriber::prelude::*;

use super::predicate_to_guard;

#[derive(Default)]
struct RequirementBounds {
    coverage: AtomicUsize,
    demand: AtomicUsize,
}

struct RequirementBoundLayer(Arc<RequirementBounds>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for RequirementBoundLayer {
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let counter = match attributes.metadata().name() {
            "requirement_coverage_bound" => &self.0.coverage,
            "requirement_bound" => &self.0.demand,
            _ => return,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn failure_requirement_counts_exhaustion_after_retrying_without_coverage() -> eyre::Result<()> {
    use crate::analysis_db::IrAnalysisDb;
    use crate::value::ConsumptionDomain;

    let db = IrAnalysisDb::new(&helm_schema_ast::DefineIndex::new());
    let mut consumption = None;
    crate::tests::support::with_execution(
        "{{ eq .Values.enabled true }}",
        None,
        &db,
        |execution| {
            consumption = execution
                .observed_facts()
                .consumptions
                .iter()
                .find(|consumption| consumption.domain == ConsumptionDomain::KnownFailure)
                .cloned();
        },
    );
    let mut consumption = consumption.ok_or_eyre("comparison did not record a failure")?;
    // Even with a small coverage clause, charging the guard inventory exhausts the demand.
    let guards = vec![
        crate::Guard::Truthy {
            path: ValuesPath::parse("activation"),
        };
        8192
    ];
    consumption.append_activation(&guards);
    let mut terminal = vec![
        vec![ConditionalGuard::Truthy {
            path: ValuesPath::parse("already_rejected"),
        }]
        .into(),
    ];
    let bounds = Arc::new(RequirementBounds::default());
    let subscriber =
        tracing_subscriber::registry().with(RequirementBoundLayer(Arc::clone(&bounds)));
    tracing::subscriber::with_default(subscriber, || {
        super::executed_requirements::record_executed_requirements(
            &mut std::collections::BTreeMap::new(),
            &mut terminal,
            &[consumption],
        );
    });
    sim_assert_eq!(have: bounds.coverage.load(Ordering::Relaxed), want: 1);
    sim_assert_eq!(have: bounds.demand.load(Ordering::Relaxed), want: 1);
    Ok(())
}

fn compound_payloads(guard: &ConditionalGuard) -> BTreeSet<usize> {
    let mut payloads = BTreeSet::new();
    let mut pending = vec![guard];
    while let Some(guard) = pending.pop() {
        match guard {
            ConditionalGuard::Not(inner) => {
                if payloads.insert(Arc::as_ptr(inner) as usize) {
                    pending.push(inner.as_ref());
                }
            }
            ConditionalGuard::AllOf(children) | ConditionalGuard::AnyOf(children)
                if payloads.insert(Arc::as_ptr(children) as usize) =>
            {
                pending.extend(children.iter());
            }
            _ => {}
        }
    }
    payloads
}

#[test]
fn projecting_a_shared_boolean_dag_keeps_linear_compound_storage() -> eyre::Result<()> {
    for levels in [2usize, 5, 8] {
        let mut predicate = Predicate::truthy_path("seed");
        // Each level references the same earlier formula in both positive and negative polarity.
        for level in 0..levels {
            predicate = Predicate::Or(vec![
                Predicate::And(vec![
                    Predicate::truthy_path(format!("a{level}")),
                    predicate.clone(),
                ]),
                Predicate::And(vec![
                    Predicate::truthy_path(format!("b{level}")),
                    predicate.negated(),
                ]),
            ]);
        }
        let projected =
            predicate_to_guard(&predicate, None).ok_or_eyre("guard projection missing")?;
        // One disjunction, two conjunctions, and one negation suffice for each shared level.
        sim_assert_eq!(have: compound_payloads(&projected).len(), want: 4 * levels);
        sim_assert_eq!(have: projected.predicate(), want: predicate);
    }
    Ok(())
}

#[test]
fn target_context_is_not_reused_from_a_nested_literal_occurrence() -> eyre::Result<()> {
    struct Case {
        source: Predicate,
        expected: ConditionalGuard,
    }

    let target = Predicate::truthy_path("target");
    let foreign = Predicate::truthy_path("foreign");
    let nested = [
        Case {
            source: target.negated(),
            expected: ConditionalGuard::Not(Arc::new(ConditionalGuard::Truthy {
                path: ValuesPath::parse("target"),
            })),
        },
        Case {
            source: Predicate::Or(vec![target.clone(), foreign]),
            expected: ConditionalGuard::AnyOf(
                vec![
                    ConditionalGuard::Truthy {
                        path: ValuesPath::parse("foreign"),
                    },
                    ConditionalGuard::Truthy {
                        path: ValuesPath::parse("target"),
                    },
                ]
                .into(),
            ),
        },
    ];
    for Case { source, expected } in nested {
        let projected = predicate_to_guard(&source, Some("target"))
            .ok_or_eyre("load-bearing target guard disappeared")?;
        sim_assert_eq!(have: projected, want: expected);
        // The nested occurrence is literal, while the later direct occurrence is target-filtered.
        // A cache keyed only by the shared source predicate would incorrectly accept the latter.
        let conjunction = Predicate::And(vec![source, target.clone()]);
        sim_assert_eq!(have: predicate_to_guard(&conjunction, Some("target")), want: None);
    }
    Ok(())
}

#[test]
fn canonicalization_and_mapping_preserve_shared_payloads_and_serialization() -> eyre::Result<()> {
    let shared = Arc::new(vec![
        ConditionalGuard::Truthy {
            path: ValuesPath::parse("a"),
        },
        ConditionalGuard::Truthy {
            path: ValuesPath::parse("b"),
        },
    ]);
    let guard = ConditionalGuard::AnyOf(
        vec![
            ConditionalGuard::Not(Arc::new(ConditionalGuard::AllOf(Arc::clone(&shared)))),
            ConditionalGuard::AllOf(shared),
        ]
        .into(),
    );
    let payloads = compound_payloads(&guard);
    let mut guards = vec![guard.clone(), guard];
    ConditionalGuard::canonicalize_conjunction(&mut guards);
    sim_assert_eq!(have: guards.len(), want: 1);
    let canonical = guards.first().ok_or_eyre("canonical guard missing")?;
    sim_assert_eq!(have: compound_payloads(canonical), want: payloads.clone());

    let mut visited = Vec::new();
    let mapped = canonical.clone().map_value_paths(&mut |path| {
        visited.push(path.clone());
        path
    });
    // Sharing changes storage without suppressing callbacks for repeated logical occurrences.
    sim_assert_eq!(have: visited, want: ["a", "b", "a", "b"].map(ValuesPath::parse).to_vec());
    sim_assert_eq!(have: compound_payloads(&mapped), want: payloads);
    sim_assert_eq!(
        have: serde_json::to_value(mapped)?,
        want: serde_json::json!({
            "any_of": [
                { "not": { "all_of": [
                    { "truthy": { "path": "a" } },
                    { "truthy": { "path": "b" } }
                ] } },
                { "all_of": [
                    { "truthy": { "path": "a" } },
                    { "truthy": { "path": "b" } }
                ] }
            ]
        }),
    );
    Ok(())
}

fn assert_shared_conjunct_survives_split(
    occurrences: &[crate::value::FrozenConsumption],
) -> eyre::Result<()> {
    use super::executed_requirements::{ExistingCoverage, fold_clauses};
    let occurrences = occurrences
        .iter()
        .filter(|item| matches!(item.domain, crate::value::ConsumptionDomain::Known { .. }))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: occurrences.len(), want: 2);
    let expected = Predicate::truthy_path("p");
    let fold = |occurrences: &[&crate::value::FrozenConsumption], work| {
        let clauses = fold_clauses(occurrences, &ExistingCoverage::default(), None, work);
        let split = clauses
            .iter()
            .any(|clause| matches!(clause.folded.guard, ConditionalGuard::Requirement(_)));
        let guard = Predicate::Or(
            clauses
                .iter()
                .map(|clause| {
                    Predicate::all(
                        clause
                            .prefix
                            .iter()
                            .map(ConditionalGuard::predicate)
                            .chain([clause.folded.guard.predicate()])
                            .collect(),
                    )
                })
                .collect(),
        )
        .normalize_boolean();
        (split, guard)
    };
    let work = (16..256)
        .find(|work| {
            let (split, guard) = fold(&occurrences, *work);
            split && guard == expected
        })
        .ok_or_eyre("no bounded Shannon split preserved p")?;
    let first = *occurrences.first().ok_or_eyre("first occurrence")?;
    let second = *occurrences.last().ok_or_eyre("second occurrence")?;
    let (_, positive_only) = fold(&[first], helm_schema_core::RequirementBdd::WORK_BUDGET);
    let (_, negative_only) = fold(&[second], helm_schema_core::RequirementBdd::WORK_BUDGET);
    sim_assert_eq!(have: Predicate::Or(vec![positive_only, negative_only]).normalize_boolean(), want: Predicate::False);
    let split_clauses = fold_clauses(&occurrences, &ExistingCoverage::default(), None, work);
    assert!(
        split_clauses
            .iter()
            .all(|clause| !clause.folded.projection_complete),
        "a split cannot promote an unproved union to complete base ownership"
    );
    for order in [
        vec![first, second],
        vec![second, first],
        vec![first, first, second, second],
        vec![first, second, first, second],
    ] {
        let (split, guard) = fold(&order, work);
        assert!(split, "the law must exercise an exhausted union");
        sim_assert_eq!(have: guard, want: expected);
    }
    Ok(())
}

/// A forced split retains p after complementary unresolved guards cancel, in every input order.
#[test]
fn shannon_requirement_splits_preserve_shared_conjuncts_independently_of_order() -> eyre::Result<()>
{
    let occurrences = crate::value::FrozenConsumption::test_complementary_guards(None)?;
    assert_shared_conjunct_survives_split(&occurrences)?;
    let bound = occurrences
        .iter()
        .map(|occurrence| {
            let mut occurrence =
                occurrence.test_under(Predicate::from(helm_schema_core::Guard::Range {
                    path: ValuesPath::parse("items"),
                }));
            occurrence.path = ValuesPath::parse("items.*.bad");
            occurrence
        })
        .collect::<Vec<_>>();
    assert_shared_conjunct_survives_split(&bound)
}

/// Wildcard values atoms are universally projected choices and cannot select a Shannon split.
#[test]
fn shannon_requirement_splits_do_not_select_wildcard_atoms() -> eyre::Result<()> {
    let occurrences = crate::value::FrozenConsumption::test_complementary_guards(Some(
        Predicate::truthy_path("a.*.enabled"),
    ))?;
    assert_shared_conjunct_survives_split(&occurrences)
}

/// Equal runtime ordinals from independent interpreters are independent unresolved choices.
#[test]
fn requirement_scope_projection_does_not_correlate_equal_choice_ordinals() -> eyre::Result<()> {
    use super::executed_requirements::{ExistingCoverage, fold_clauses};
    let first = crate::value::FrozenConsumption::test_complementary_guards(None)?;
    let second = crate::value::FrozenConsumption::test_complementary_guards(None)?;
    let positive = first.first().ok_or_eyre("first scope's positive arm")?;
    let negative = second.last().ok_or_eyre("second scope's negative arm")?;
    for order in [vec![positive, negative], vec![negative, positive]] {
        let clauses = fold_clauses(
            &order,
            &ExistingCoverage::default(),
            None,
            helm_schema_core::RequirementBdd::WORK_BUDGET,
        );
        let guard = Predicate::Or(
            clauses
                .iter()
                .map(|clause| {
                    Predicate::all(
                        clause
                            .prefix
                            .iter()
                            .map(ConditionalGuard::predicate)
                            .chain([clause.folded.guard.predicate()])
                            .collect(),
                    )
                })
                .collect(),
        )
        .normalize_boolean();
        sim_assert_eq!(have: guard, want: Predicate::False);
    }
    Ok(())
}

/// A failed group fold cannot turn a proved-empty fallback into an unknown use.
#[test]
fn local_impossibility_survives_incomplete_group_ownership() -> eyre::Result<()> {
    use super::executed_requirements::{ExistingCoverage, fold_clauses};
    let occurrences = crate::value::FrozenConsumption::test_complementary_guards(None)?;
    let impossible = occurrences
        .first()
        .ok_or_eyre("occurrence")?
        .test_impossible_choice();
    let clauses = fold_clauses(&[&impossible], &ExistingCoverage::default(), None, 0);
    sim_assert_eq!(have: clauses.len(), want: 1);
    assert!(clauses.iter().all(|clause| clause.folded.proved_impossible));
    assert!(
        clauses
            .iter()
            .all(|clause| !clause.folded.projection_complete)
    );
    Ok(())
}
