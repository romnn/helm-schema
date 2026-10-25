use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use helm_schema_core::{Predicate, ValuesPath};
use test_util::prelude::sim_assert_eq;

use super::{Link, ReadSet};
use crate::value::{ReadCoverage, RuntimeRead};

fn read(index: usize) -> RuntimeRead {
    RuntimeRead {
        coverage: if index % 2 == 0 {
            ReadCoverage::ExactPath
        } else {
            ReadCoverage::UnknownRegion
        },
        path: ValuesPath::parse(&format!("key{index:04}")),
        condition: Predicate::truthy_path(&format!("flag{}", index % 3)),
    }
}

fn invariant(root: &Link) -> (usize, usize) {
    let Some(node) = root else {
        return (0, 0);
    };
    let (left_height, left_length) = invariant(&node.left);
    let (right_height, right_length) = invariant(&node.right);
    assert!(left_height.abs_diff(right_height) <= 1);
    sim_assert_eq!(have: node.height, want: 1 + left_height.max(right_height));
    sim_assert_eq!(have: node.length, want: 1 + left_length + right_length);
    (node.height, node.length)
}

fn addresses(set: &ReadSet) -> BTreeMap<RuntimeRead, *const RuntimeRead> {
    set.iter()
        .map(|entry| (entry.clone(), std::ptr::from_ref(entry)))
        .collect()
}

#[test]
fn insertion_and_union_match_complete_sorted_read_sets_and_avl_invariants() {
    let mut left = ReadSet::default();
    let mut right = ReadSet::default();
    let mut expected = BTreeSet::new();
    for index in (0..257).map(|index| (index * 73) % 257) {
        let entry = read(index);
        expected.insert(entry.clone());
        if index % 3 == 0 {
            left.insert(entry);
        } else {
            right.insert(entry);
        }
        invariant(&left.root);
        invariant(&right.root);
    }
    let union = left.union(&right);
    invariant(&union.root);
    sim_assert_eq!(have: union.iter().cloned().collect::<Vec<_>>(), want: expected.into_iter().collect::<Vec<_>>());
    let reverse = right.union(&left);
    sim_assert_eq!(have: union, want: reverse);
    sim_assert_eq!(have: union.iter().len(), want: 257);
}

#[test]
fn forks_duplicates_and_identical_unions_keep_the_same_root_and_entries() {
    let mut original: ReadSet = (0..128).map(read).collect();
    let frozen = original.clone();
    let old_addresses = addresses(&frozen);
    assert!(
        original
            .root
            .as_ref()
            .zip(frozen.root.as_ref())
            .is_some_and(|(left, right)| Rc::ptr_eq(left, right))
    );
    assert!(!original.insert(read(17)));
    assert!(
        original
            .root
            .as_ref()
            .zip(frozen.root.as_ref())
            .is_some_and(|(left, right)| Rc::ptr_eq(left, right))
    );
    let identical = original.union(&frozen);
    assert!(
        original
            .root
            .as_ref()
            .zip(identical.root.as_ref())
            .is_some_and(|(left, right)| Rc::ptr_eq(left, right))
    );
    original.insert(read(1000));
    let merged = original.union(&frozen);
    let merged_addresses = addresses(&merged);
    for (entry, address) in old_addresses {
        sim_assert_eq!(have: merged_addresses.get(&entry), want: Some(&address));
    }
    sim_assert_eq!(have: frozen.len(), want: 128);
    sim_assert_eq!(have: original.len(), want: 129);
    original.clear();
    sim_assert_eq!(have: original.len(), want: 0);
    sim_assert_eq!(have: frozen.len(), want: 128);
}

#[test]
fn divergent_forks_union_without_cloning_any_shared_read_entry() {
    let base: ReadSet = (0..128).map(read).collect();
    let mut left = base.clone();
    let mut right = base.clone();
    left.insert(read(1000));
    right.insert(read(2000));
    let union = left.union(&right);
    invariant(&union.root);
    let actual = addresses(&union);
    for (entry, address) in addresses(&base) {
        sim_assert_eq!(have: actual.get(&entry), want: Some(&address));
    }
    sim_assert_eq!(have: union.len(), want: 130);
}
