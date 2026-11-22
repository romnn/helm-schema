use std::cell::Cell;
use std::rc::Rc;

use test_util::prelude::sim_assert_eq;

use super::{History, fingerprint};

#[derive(Debug)]
struct Counted {
    value: usize,
    clones: Rc<Cell<usize>>,
}

impl Clone for Counted {
    fn clone(&self) -> Self {
        self.clones.set(self.clones.get() + 1);
        Self {
            value: self.value,
            clones: Rc::clone(&self.clones),
        }
    }
}

#[test]
fn a_fork_is_constant_and_append_copies_only_a_partial_tail_chunk() {
    for length in [0, 1, 15, 16, 17, 127, 128] {
        let clones = Rc::new(Cell::new(0));
        let mut base: History<_> = (0..length)
            .map(|value| Counted {
                value,
                clones: Rc::clone(&clones),
            })
            .collect();
        let frozen = base.clone();
        sim_assert_eq!(have: clones.get(), want: 0);
        assert!(Rc::ptr_eq(&base.chunks, &frozen.chunks));
        base.push(Counted {
            value: length,
            clones: Rc::clone(&clones),
        });
        sim_assert_eq!(have: clones.get(), want: length % 16);
        sim_assert_eq!(have: frozen.iter().map(|value| value.value).collect::<Vec<_>>(), want: (0..length).collect::<Vec<_>>());
        sim_assert_eq!(have: base.iter().map(|value| value.value).collect::<Vec<_>>(), want: (0..=length).collect::<Vec<_>>());
        for (left, right) in base
            .chunks
            .iter()
            .zip(frozen.chunks.iter())
            .take(length / 16)
        {
            assert!(Rc::ptr_eq(left, right));
        }
    }
}

#[test]
fn mutable_iteration_detaches_only_visited_chunks_and_never_changes_a_frozen_history() {
    let clones = Rc::new(Cell::new(0));
    let mut live: History<_> = (0..33)
        .map(|value| Counted {
            value,
            clones: Rc::clone(&clones),
        })
        .collect();
    let frozen = live.clone();
    for value in live.iter_mut().take(1) {
        value.value = 900;
    }
    sim_assert_eq!(have: clones.get(), want: 16);
    sim_assert_eq!(have: frozen.iter().map(|value| value.value).collect::<Vec<_>>(), want: (0..33).collect::<Vec<_>>());
    let mut expected: Vec<_> = (0..33).collect();
    if let Some(first) = expected.first_mut() {
        *first = 900;
    }
    sim_assert_eq!(have: live.iter().map(|value| value.value).collect::<Vec<_>>(), want: expected);
    for (left, right) in live.chunks.iter().zip(frozen.chunks.iter()).skip(1) {
        assert!(Rc::ptr_eq(left, right));
    }
}

#[test]
fn ordered_bounded_append_clear_and_membership_match_the_existing_vector_policy() {
    let mut history = History::default();
    let mut vector = Vec::new();
    for index in 0..140 {
        if history.len() < 128 {
            history.push(index);
        }
        if vector.len() < 128 {
            vector.push(index);
        }
        sim_assert_eq!(have: history.iter().copied().collect::<Vec<_>>(), want: vector.clone());
    }
    assert!(history.iter().any(|value| *value == 127));
    assert!(!history.iter().any(|value| *value == 128));
    let frozen = history.clone();
    history.clear();
    sim_assert_eq!(have: history.len(), want: 0);
    sim_assert_eq!(have: frozen.iter().len(), want: 128);
}

#[test]
fn joining_an_unchanged_history_does_not_compare_any_elements() {
    for length in [0, 1, 15, 16, 17, 128, 1024] {
        let history: History<_> = (0..length).collect();
        let branch = history.clone();
        let comparisons = Cell::new(0);
        let joined = history.union_by_hashed(
            &branch,
            |left, right| {
                comparisons.set(comparisons.get() + 1);
                left == right
            },
            fingerprint,
        );
        sim_assert_eq!(have: joined.iter().copied().collect::<Vec<_>>(), want: (0..length).collect::<Vec<_>>());
        sim_assert_eq!(have: comparisons.get(), want: 0);
    }
}

#[test]
fn joined_suffixes_keep_prefix_order_and_compare_only_new_observations() {
    for length in [0, 1, 15, 16, 17, 128, 1024] {
        let parent: History<_> = (0..length).collect();
        let mut left = parent.clone();
        let mut right = parent.clone();
        left.push(length);
        right.push(length + 1);
        right.push(length);
        let comparisons = Cell::new(0);
        let joined = left.union_by_hashed(
            &right,
            |left, right| {
                comparisons.set(comparisons.get() + 1);
                left == right
            },
            fingerprint,
        );
        sim_assert_eq!(have: joined.iter().copied().collect::<Vec<_>>(), want: (0..length + 2).collect::<Vec<_>>());
        sim_assert_eq!(have: parent.iter().copied().collect::<Vec<_>>(), want: (0..length).collect::<Vec<_>>());
        sim_assert_eq!(have: comparisons.get(), want: 1);
    }
}

#[test]
fn disjoint_suffix_membership_does_not_rescan_the_inherited_history() {
    let parent: History<_> = (0..4096_u64).collect();
    let mut left = parent.clone();
    left.push(4096);
    let mut right = parent.clone();
    for value in (4097..5121).rev() {
        right.push(value);
    }
    right.push(4096);
    right.push(5);
    let mut expected = left.iter().copied().collect::<Vec<_>>();
    let mut linear_comparisons = 0;
    for value in right.iter().skip(parent.len()) {
        if !expected.iter().any(|existing| {
            linear_comparisons += 1;
            existing == value
        }) {
            expected.push(*value);
        }
    }
    let fingerprints = Cell::new(0);
    let comparisons = Cell::new(0);
    let joined = left.union_by_hashed(
        &right,
        |left, right| {
            comparisons.set(comparisons.get() + 1);
            left == right
        },
        |value| {
            fingerprints.set(fingerprints.get() + 1);
            *value
        },
    );
    sim_assert_eq!(
        have: joined.iter().copied().collect::<Vec<_>>(),
        want: expected,
    );
    sim_assert_eq!(have: comparisons.get(), want: 2);
    sim_assert_eq!(have: fingerprints.get(), want: 4097 + 1026);
    assert!(linear_comparisons > 4_000_000);
}

#[test]
fn fingerprints_only_select_collisions_for_exact_ordered_membership() {
    let left: History<_> = [1, 2, 1].into_iter().collect();
    let right: History<_> = [3, 2, 4, 3, 1, 5].into_iter().collect();
    let joined = left.union_by_hashed(&right, PartialEq::eq, |_| 0);
    sim_assert_eq!(
        have: joined.iter().copied().collect::<Vec<_>>(),
        want: vec![1, 2, 1, 3, 4, 5],
    );
}

#[test]
fn new_observations_equal_to_the_prefix_keep_the_existing_set_policy() {
    let parent: History<_> = (0..33).collect();
    let left = parent.clone();
    let mut right = parent.clone();
    right.push(5);
    right.push(33);
    let joined = left.union_by_hashed(&right, PartialEq::eq, fingerprint);
    sim_assert_eq!(have: joined.iter().copied().collect::<Vec<_>>(), want: (0..34).collect::<Vec<_>>());

    let mut rewritten = parent.clone();
    if let Some(value) = rewritten.iter_mut().next() {
        *value = 34;
    }
    let joined = joined.union_by_hashed(&rewritten, PartialEq::eq, fingerprint);
    sim_assert_eq!(have: joined.iter().copied().collect::<Vec<_>>(), want: (0..35).collect::<Vec<_>>());
}
