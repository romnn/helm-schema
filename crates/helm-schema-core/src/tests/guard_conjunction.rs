use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher as _};

use color_eyre::eyre;
use test_util::prelude::sim_assert_eq;

use super::{GuardConjunction, GuardPrefix};
use crate::{
    ConditionalGuard, ContractRequirementImplication, ContractRequirementTarget,
    ContractSchemaSignals, ValuesPath,
};

fn truthy(path: &str) -> ConditionalGuard {
    ConditionalGuard::Truthy {
        path: ValuesPath::parse(path),
    }
}

fn hash(value: &impl Hash) -> u64 {
    let mut state = DefaultHasher::new();
    value.hash(&mut state);
    state.finish()
}

#[test]
fn shared_rows_preserve_raw_order_and_canonical_flat_identity() -> eyre::Result<()> {
    let a = truthy("a");
    let b = truthy("b");
    let absent = ConditionalGuard::Absent {
        path: ValuesPath::parse("host"),
    };
    let prefix = GuardPrefix::new(vec![b.clone(), a.clone(), b.clone()]);
    let raw = GuardConjunction::prefixed(vec![a.clone()], prefix, vec![absent.clone()]);
    let expected_raw = vec![a.clone(), b.clone(), a.clone(), b.clone(), absent.clone()];
    sim_assert_eq!(have: raw.to_vec(), want: expected_raw.clone());
    sim_assert_eq!(have: serde_json::to_value(&raw)?, want: serde_json::to_value(&expected_raw)?);
    let canonical = raw.clone().canonicalized();
    let expected = vec![a, b, absent];
    sim_assert_eq!(have: canonical.to_vec(), want: expected.clone());
    sim_assert_eq!(have: serde_json::to_value(&canonical)?, want: serde_json::to_value(&expected)?);
    sim_assert_eq!(have: raw.to_vec(), want: expected_raw);
    sim_assert_eq!(have: canonical.iter().rev().cloned().collect::<Vec<_>>(), want: expected.into_iter().rev().collect::<Vec<_>>());
    Ok(())
}

#[test]
fn shared_row_order_and_hash_match_flat_rows_in_both_phases() {
    let prefix = GuardPrefix::new(vec![truthy("d"), truthy("b"), truthy("f")]);
    let mut shared = Vec::new();
    for before in [
        Vec::new(),
        vec![truthy("a")],
        vec![truthy("c"), truthy("e")],
    ] {
        for after in [Vec::new(), vec![truthy("b")], vec![truthy("g")]] {
            let row = GuardConjunction::prefixed(before.clone(), prefix.clone(), after);
            shared.push(row.clone());
            let canonical = row.canonicalized();
            let mut appended = canonical.clone();
            appended.push(truthy("a"));
            shared.push(canonical);
            shared.push(appended);
        }
    }
    for left in &shared {
        let flat_left = GuardConjunction::from(left.to_vec());
        let mut left_literals = left.to_vec();
        ConditionalGuard::canonicalize_conjunction(&mut left_literals);
        sim_assert_eq!(have: left.cmp(&flat_left), want: std::cmp::Ordering::Equal);
        sim_assert_eq!(have: hash(left), want: hash(&flat_left));
        sim_assert_eq!(have: hash(left), want: hash(&left.to_vec()));
        for right in &shared {
            sim_assert_eq!(have: left.cmp(right), want: left.to_vec().cmp(&right.to_vec()));
            sim_assert_eq!(have: left == right, want: left.to_vec() == right.to_vec());
            let mut right_literals = right.to_vec();
            ConditionalGuard::canonicalize_conjunction(&mut right_literals);
            sim_assert_eq!(
                have: left.is_subset_of(right),
                want: left_literals.iter().all(|guard| right_literals.binary_search(guard).is_ok()),
            );
        }
    }
}

#[test]
fn removing_a_literal_preserves_flat_order_and_shares_unchanged_prefixes() {
    let a = truthy("a");
    let b = truthy("b");
    let extra = truthy("extra");
    let nested = ConditionalGuard::AllOf(vec![b.clone(), a.clone()].into());
    let canonical_nested = ConditionalGuard::AllOf(vec![a, b].into());
    let prefix = GuardPrefix::new(vec![nested.clone(), truthy("prefix")]);
    let raw = GuardConjunction::prefixed(vec![extra.clone()], prefix.clone(), vec![extra.clone()]);
    let canonical = raw.clone().canonicalized();
    let mut appended = canonical.clone();
    appended.push(canonical_nested.clone());
    for row in [&raw, &canonical, &appended] {
        for guard in [&extra, &nested, &canonical_nested] {
            let expected = row
                .iter()
                .filter(|literal| *literal != guard)
                .cloned()
                .collect::<Vec<_>>();
            sim_assert_eq!(have: row.without(guard).to_vec(), want: expected);
        }
    }
    assert!(
        canonical
            .without(&extra)
            .canonical_parts()
            .is_some_and(|(stored, _)| { stored.shares_storage(&prefix) })
    );
    assert!(
        canonical
            .without(&canonical_nested)
            .canonical_parts()
            .is_some_and(|(stored, _)| { !stored.shares_storage(&prefix) })
    );
    assert!(appended.is_subset_of(&canonical));
    assert!(canonical.is_subset_of(&appended));
    assert!(!canonical.is_subset_of(&canonical.without(&canonical_nested)));
}

#[test]
fn appending_to_a_canonical_row_preserves_its_original_prefix_and_flat_order() {
    let a = truthy("a");
    let b = truthy("b");
    let c = truthy("c");
    let prefix = GuardPrefix::new(vec![c.clone(), a.clone()]);
    let original =
        GuardConjunction::prefixed(vec![b.clone()], prefix.clone(), Vec::new()).canonicalized();
    let mut appended = original.clone();
    appended.push(a.clone());
    appended.push(b.clone());
    let mut guards = appended.iter();
    sim_assert_eq!(have: guards.next(), want: Some(&a));
    sim_assert_eq!(have: guards.next_back(), want: Some(&b));
    sim_assert_eq!(have: guards.next_back(), want: Some(&a));
    sim_assert_eq!(have: guards.cloned().collect::<Vec<_>>(), want: vec![b.clone(), c.clone()]);
    assert!(appended.canonical_parts().is_none());
    assert!(appended.owned_capacity() <= 3);

    appended.canonicalize();
    sim_assert_eq!(have: &appended, want: &original);
    assert!(appended.canonical_parts().is_some_and(|(stored, extras)| {
        stored.shares_storage(&prefix) && extras == std::slice::from_ref(&b)
    }));
    assert!(appended.owned_capacity() <= 1);
}

#[test]
fn signal_constructors_share_prefixes_and_preserve_terminal_multiplicity() {
    let a = truthy("a");
    let b = truthy("b");
    let prefix = GuardPrefix::new(vec![b.clone(), a.clone()]);
    let row = GuardConjunction::prefixed(vec![a.clone()], prefix.clone(), Vec::new());
    let implication = ContractRequirementImplication::from_conjunction(
        row.clone(),
        ContractRequirementTarget::Value,
        Vec::new(),
    );
    let signals = ContractSchemaSignals::from_conjunctions(
        std::collections::BTreeMap::new(),
        vec![row.clone(), GuardConjunction::default(), row],
    );
    let terminals = signals.terminal_clauses();
    sim_assert_eq!(have: terminals.iter().map(GuardConjunction::to_vec).collect::<Vec<_>>(), want: vec![vec![a.clone(), b.clone()], Vec::new(), vec![a, b]]);
    for clause in terminals.iter().filter(|clause| !clause.is_empty()) {
        sim_assert_eq!(have: clause, want: &implication.outer_guards);
        assert!(clause.canonical_parts().is_some_and(|(stored, extras)| {
            stored.shares_storage(&prefix) && extras.is_empty()
        }));
    }
}

#[test]
fn fallback_rows_share_one_prefix_without_retaining_literal_copies() {
    let prefix = GuardPrefix::new(
        (0..64)
            .map(|index| truthy(&format!("flag{index:02}")))
            .collect(),
    );
    let rows = (0..5000)
        .map(|index| {
            GuardConjunction::prefixed(
                vec![truthy(&format!("row{index:04}"))],
                prefix.clone(),
                Vec::new(),
            )
            .canonicalized()
        })
        .collect::<Vec<_>>();
    sim_assert_eq!(have: rows.iter().map(GuardConjunction::len).sum::<usize>(), want: 325_000);
    assert!(rows.iter().all(|row| {
        row.canonical_parts()
            .is_some_and(|(stored, _)| stored.shares_storage(&prefix))
    }));
    assert!(
        rows.iter()
            .map(GuardConjunction::owned_capacity)
            .sum::<usize>()
            <= rows.len()
    );
}
