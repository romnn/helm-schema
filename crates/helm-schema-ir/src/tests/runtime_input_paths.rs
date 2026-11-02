use helm_schema_core::ValuesPath;
use test_util::prelude::sim_assert_eq;

use super::{InputPaths, Node};
use crate::value::ObjectId;

fn recomputed(current: &InputPaths, previous: &InputPaths) -> Vec<(ObjectId, ValuesPath)> {
    let mut added = Vec::new();
    let mut pending = Vec::new();
    pending.extend(current.root.as_deref());
    while let Some(node) = pending.pop() {
        match node {
            Node::Leaf { object, paths } => {
                for path in paths {
                    if !previous.original_paths(object).contains(path) {
                        added.push((object.clone(), path.clone()));
                    }
                }
            }
            Node::Branch { left, right } => {
                pending.extend(right.as_deref());
                pending.extend(left.as_deref());
            }
        }
    }
    added
}

#[test]
fn structural_path_delta_matches_full_recomputation_across_sparse_versions() {
    let mut current = InputPaths::default();
    let mut versions = vec![current.clone()];
    for (ordinal, path) in [
        (0, "zero"),
        (1, "one"),
        (7, "seven"),
        (64, "large"),
        (1 << 40, "sparse"),
        (1, "replacement"),
        (1, "one"),
        (u64::MAX, "last"),
        (0, ""),
    ] {
        current.insert(ObjectId { ordinal }, ValuesPath::parse(path));
        versions.push(current.clone());
    }
    // Independent branches retain different path sets at the same ordinal.
    let mut branch = InputPaths::default();
    branch.insert(ObjectId { ordinal: 1 }, ValuesPath::parse("other"));
    versions.push(branch);
    for current in &versions {
        for previous in &versions {
            sim_assert_eq!(have: current.added_paths(previous), want: recomputed(current, previous));
        }
    }
}

#[test]
fn root_height_growth_preserves_shared_prefixes_and_ordinal_order() {
    let mut current = InputPaths::default();
    current.insert(ObjectId { ordinal: 0 }, ValuesPath::parse("zero"));
    let previous = current.clone();
    current.insert(ObjectId { ordinal: u64::MAX }, ValuesPath::parse("last"));
    current.insert(ObjectId { ordinal: 64 }, ValuesPath::parse("middle"));

    // Height alignment retains the shared zero branch in both comparison directions.
    sim_assert_eq!(have: current.added_paths(&previous), want: vec![
        (ObjectId { ordinal: 64 }, ValuesPath::parse("middle")),
        (ObjectId { ordinal: u64::MAX }, ValuesPath::parse("last")),
    ]);
    sim_assert_eq!(have: previous.added_paths(&current), want: Vec::new());
}

#[test]
fn reregistered_paths_keep_latest_identity_without_duplicate_deltas() {
    let object = ObjectId { ordinal: 7 };
    let first = ValuesPath::parse("first");
    let second = ValuesPath::parse("second");
    let mut current = InputPaths::default();
    current.insert(object.clone(), first.clone());
    let previous = current.clone();
    current.insert(object.clone(), second.clone());
    current.insert(object.clone(), first.clone());

    sim_assert_eq!(have: current.get(&object), want: Some(&first));
    sim_assert_eq!(have: current.original_paths(&object), want: &[second.clone(), first.clone()]);
    sim_assert_eq!(have: current.same_mapping(&previous, &object), want: true);
    sim_assert_eq!(have: current.added_paths(&previous), want: vec![(object.clone(), second)]);
    let shared = current.clone();
    let generation = current.id();
    current.insert(object, first);
    sim_assert_eq!(have: current.id(), want: generation);
    sim_assert_eq!(have: current.added_paths(&shared), want: Vec::new());
}
