use std::collections::BTreeSet;
use std::sync::Arc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{ConditionalGuard, Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use super::predicate_to_guard;

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
