use helm_schema_core::ValuesPath;
use std::rc::Rc;
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

impl InputPaths {
    fn added_paths(&self, previous: &Self) -> Vec<(ObjectId, ValuesPath)> {
        let mut old = previous.root.clone();
        let mut height = previous.height;
        while height > self.height {
            old = match old.as_deref() {
                Some(Node::Branch { left, right: _ }) => left.clone(),
                Some(Node::Leaf {
                    object: _,
                    paths: _,
                })
                | None => None,
            };
            height -= 1;
        }
        while height < self.height {
            old = Some(Rc::new(Node::Branch {
                left: old,
                right: None,
            }));
            height += 1;
        }
        let mut added = Vec::new();
        let mut pending = vec![(self.root.as_ref(), old.as_ref())];
        while let Some((current, previous)) = pending.pop() {
            let Some(current) = current else {
                continue;
            };
            if previous.is_some_and(|previous| Rc::ptr_eq(current, previous)) {
                continue;
            }
            match current.as_ref() {
                Node::Leaf { object, paths } => {
                    let previous_paths = match previous.map(Rc::as_ref) {
                        Some(Node::Leaf {
                            object: target,
                            paths,
                        }) if target == object => paths.as_slice(),
                        Some(
                            Node::Leaf {
                                object: _,
                                paths: _,
                            }
                            | Node::Branch { left: _, right: _ },
                        )
                        | None => &[],
                    };
                    for path in paths {
                        if !previous_paths.contains(path) {
                            added.push((object.clone(), path.clone()));
                        }
                    }
                }
                Node::Branch { left, right } => {
                    let (previous_left, previous_right) = match previous.map(Rc::as_ref) {
                        Some(Node::Branch { left, right }) => (left.as_ref(), right.as_ref()),
                        Some(Node::Leaf {
                            object: _,
                            paths: _,
                        })
                        | None => (None, None),
                    };
                    pending.push((right.as_ref(), previous_right));
                    pending.push((left.as_ref(), previous_left));
                }
            }
        }
        added
    }

    pub(in crate::value) fn same_mapping(&self, other: &Self, object: &ObjectId) -> bool {
        if self.height != other.height {
            return self.get(object) == other.get(object);
        }
        if object.ordinal.bit_width() > self.height {
            return true;
        }
        let mut left = self.root.as_ref();
        let mut right = other.root.as_ref();
        let mut remaining = self.height;
        loop {
            match (left, right) {
                (Some(left), Some(right)) if Rc::ptr_eq(left, right) => return true,
                (Some(left_node), Some(right_node)) => {
                    let (
                        Node::Branch {
                            left: left_zero,
                            right: left_one,
                        },
                        Node::Branch {
                            left: right_zero,
                            right: right_one,
                        },
                    ) = (left_node.as_ref(), right_node.as_ref())
                    else {
                        return self.get(object) == other.get(object);
                    };
                    let Some(bit) = remaining.checked_sub(1) else {
                        return self.get(object) == other.get(object);
                    };
                    remaining = bit;
                    (left, right) = if object.ordinal & (1_u64 << bit) == 0 {
                        (left_zero.as_ref(), right_zero.as_ref())
                    } else {
                        (left_one.as_ref(), right_one.as_ref())
                    };
                }
                (Some(_) | None, None) | (None, Some(_)) => {
                    return self.get(object) == other.get(object);
                }
            }
        }
    }
}

#[test]
fn input_path_visits_latest_registrations_at_sparse_identities() {
    use crate::value::{ObjectId, input_paths::InputPaths};
    use helm_schema_core::ValuesPath;
    use std::collections::BTreeMap;
    let mut paths = InputPaths::default();
    for (ordinal, path) in [
        (0, "old"),
        (7, "middle"),
        (u64::MAX, "last"),
        (0, "current"),
    ] {
        paths.insert(ObjectId { ordinal }, ValuesPath::parse(path));
    }
    let mut actual = BTreeMap::new();
    paths.visit(|object, path| {
        actual.insert(object.ordinal, path.clone());
    });
    sim_assert_eq!(have: actual, want: BTreeMap::from([(0, ValuesPath::parse("current")), (7, ValuesPath::parse("middle")), (u64::MAX, ValuesPath::parse("last"))]));
}
