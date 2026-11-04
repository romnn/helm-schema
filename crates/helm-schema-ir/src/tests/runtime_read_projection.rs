use std::collections::BTreeSet;

use helm_schema_core::{ApproximationRole, Guard, GuardDnf, Predicate, PredicateKind, ValuesPath};
use test_util::prelude::sim_assert_eq;

use crate::ValueKind;
use crate::fragment_eval::ValueRead;
use crate::value::{
    DocumentExecution, Exit, FileOutput, Flow, Normal, ReadCoverage, RuntimeRead, State, Value,
};

fn state(reads: impl IntoIterator<Item = RuntimeRead>) -> State {
    let mut state = State::new(Value::nil());
    for read in reads {
        state.reads.insert(read);
    }
    state
}

fn document(states: impl IntoIterator<Item = State>) -> DocumentExecution {
    DocumentExecution::new(
        states
            .into_iter()
            .map(|state| Exit {
                state,
                flow: Flow::Normal(Normal {
                    value: Value::nil(),
                }),
            })
            .collect(),
        FileOutput::Text,
    )
}

fn original_reads(execution: &DocumentExecution) -> Vec<ValueRead> {
    execution
        .exits
        .iter()
        .flat_map(|exit| exit.state.reads.iter())
        .filter_map(|read| {
            let kind = match read.coverage {
                ReadCoverage::ExactPath => ValueKind::WidenedDependency,
                ReadCoverage::UnknownRegion => ValueKind::WidenedRegion,
                ReadCoverage::DefaultFallback => ValueKind::Scalar,
                ReadCoverage::SerializedInfluence => ValueKind::Serialized,
                ReadCoverage::MemberHost { allows_nil: _ }
                | ReadCoverage::MapArgument { allows_nil: _ }
                | ReadCoverage::RootMergeFallback
                | ReadCoverage::OpaqueValuesRoot => return None,
            };
            Some(ValueRead {
                values_path: read.path.clone(),
                kind,
                condition: GuardDnf::from_conjunction(
                    std::iter::once(read.condition.clone()).chain(
                        (read.coverage == ReadCoverage::DefaultFallback).then(|| {
                            Predicate::from(Guard::Default {
                                path: read.path.clone(),
                            })
                        }),
                    ),
                ),
                resource: None,
                provenance: Vec::new(),
                dependency: true,
            })
        })
        .collect()
}

#[test]
fn memoized_read_conditions_preserve_all_rows_and_coverage_lanes() {
    let coverages = [
        ReadCoverage::ExactPath,
        ReadCoverage::UnknownRegion,
        ReadCoverage::DefaultFallback,
        ReadCoverage::SerializedInfluence,
        ReadCoverage::MemberHost { allows_nil: false },
        ReadCoverage::MemberHost { allows_nil: true },
        ReadCoverage::MapArgument { allows_nil: false },
        ReadCoverage::MapArgument { allows_nil: true },
        ReadCoverage::RootMergeFallback,
        ReadCoverage::OpaqueValuesRoot,
    ];
    let shared = state(
        coverages
            .into_iter()
            .enumerate()
            .map(|(index, coverage)| RuntimeRead {
                coverage,
                path: ValuesPath::parse(&format!("input.key{index}")),
                condition: if index.is_multiple_of(2) {
                    Predicate::True
                } else {
                    Predicate::truthy_path("enabled")
                },
            }),
    );
    // Exit clones share persistent reads, but every projected observation remains present.
    let execution = document([shared.clone(), shared.clone(), shared]);
    let expected = original_reads(&execution);
    sim_assert_eq!(have: expected.len(), want: 12);
    let facts = execution.observed_facts().clone();
    for _ in 0..3 {
        sim_assert_eq!(have: execution.reads(), want: expected);
    }
    let mut projected = expected;
    projected.sort();
    projected.dedup();
    sim_assert_eq!(have: execution.projection_reads(), want: projected);
    sim_assert_eq!(have: execution.observed_facts(), want: &facts);
}

#[test]
fn default_fallback_paths_keep_their_combined_condition_transfer() {
    let condition = Predicate::Approximate(
        "fallback selection".to_owned(),
        BTreeSet::new(),
        ApproximationRole::Control,
        Some(Box::new(Predicate::from(Guard::Default {
            path: ValuesPath::parse("first"),
        }))),
    );
    let shared = state(["first", "second"].map(|path| RuntimeRead {
        coverage: ReadCoverage::DefaultFallback,
        path: ValuesPath::parse(path),
        condition: condition.clone(),
    }));
    let execution = document([shared.clone(), shared]);
    let expected = original_reads(&execution);
    sim_assert_eq!(have: expected.len(), want: 4);
    let first = execution.reads();
    let second = execution.reads();
    sim_assert_eq!(have: &first, want: &expected);
    sim_assert_eq!(have: &second, want: &expected);
    let defaults = first
        .iter()
        .chain(&second)
        .flat_map(|read| read.condition.disjuncts().iter().flatten())
        .filter_map(|predicate| match predicate.kind() {
            PredicateKind::Guard(guard @ Guard::Default { path: _ }) => {
                Some(std::ptr::from_ref(guard))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    sim_assert_eq!(have: defaults.len(), want: 8);
    sim_assert_eq!(have: defaults.into_iter().collect::<BTreeSet<_>>().len(), want: 8);
}

fn tautology(depth: usize, shared: bool) -> Predicate {
    if depth == 0 {
        let atom = Predicate::truthy_path("a");
        return Predicate::Or(vec![atom.clone(), Predicate::Not(atom)]);
    }
    let left = tautology(depth - 1, shared);
    let right = if shared {
        left.clone()
    } else {
        tautology(depth - 1, shared)
    };
    Predicate::Or(vec![left, right])
}

#[test]
fn equal_read_conditions_with_different_dag_sharing_keep_budget_abstention() {
    let shared = tautology(13, true);
    let unshared = tautology(13, false);
    sim_assert_eq!(have: &shared, want: &unshared);
    assert!(Predicate::True.exactly_implies(&shared));
    assert!(!Predicate::True.exactly_implies(&unshared));
    let read = |subset| RuntimeRead {
        coverage: ReadCoverage::ExactPath,
        path: ValuesPath::parse("input"),
        condition: Predicate::Approximate(
            "bounded selection".to_owned(),
            BTreeSet::new(),
            ApproximationRole::Control,
            Some(Box::new(subset)),
        ),
    };
    let shared = state([read(shared)]);
    let unshared = state([read(unshared)]);
    // Structural equality does not identify the distinct work charged by the bounded solver.
    let execution = document([shared.clone(), shared, unshared.clone(), unshared]);
    let expected = original_reads(&execution);
    sim_assert_eq!(have: expected.len(), want: 4);
    assert!(
        expected
            .iter()
            .take(2)
            .all(|read| read.condition.is_unconditional())
    );
    assert!(
        expected
            .iter()
            .skip(2)
            .all(|read| !read.condition.is_unconditional())
    );
    for _ in 0..2 {
        let actual = execution.reads();
        sim_assert_eq!(
            have: actual.iter().map(|read| read.condition.is_unconditional()).collect::<Vec<_>>(),
            want: vec![true, true, false, false]
        );
        sim_assert_eq!(have: actual, want: expected);
    }
}
