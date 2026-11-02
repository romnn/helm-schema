//! A bounded projection of mutations onto original input paths.

use std::collections::BTreeSet;
use std::rc::Rc;

use helm_schema_core::ValuesPath;

use super::Reach;
use super::{HeapNode, HeapVersion, ObjectId};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct InputWrites {
    entries: Rc<BTreeSet<ValuesPath>>,
}

impl InputWrites {
    pub(super) fn for_node(node: &HeapNode, paths: &super::input_paths::InputPaths) -> Self {
        let mut parents = node.parents();
        let Some(first) = parents.next() else {
            return Self::default();
        };
        let mut result = Self::at_paths(first, paths);
        if let Some(second) = parents.next() {
            return result.union(&Self::at_paths(second, paths));
        }
        match node {
            HeapNode::Install {
                parent: _,
                object,
                storage: _,
                kind: super::InstallKind::Mutation,
            } => {
                result.object(paths, object, None);
            }
            HeapNode::Write {
                parent: _,
                object,
                key,
                field: _,
            } => result.object(paths, object, Some(key)),
            HeapNode::WriteSlot {
                parent: _,
                object,
                index,
                value: _,
            } => result.object(paths, object, Some(&index.to_string())),
            HeapNode::UnknownWrite {
                parent: _,
                objects,
                inputs: _,
            } => match objects {
                Reach::Top => result.add(&ValuesPath::default()),
                Reach::Bounded(bounds) => {
                    for object in bounds.iter() {
                        result.object(paths, object, None);
                    }
                }
            },
            HeapNode::Empty
            | HeapNode::Registration {
                parent: _,
                paths: _,
            }
            | HeapNode::Install {
                parent: _,
                object: _,
                storage: _,
                kind: _,
            }
            | HeapNode::Join {
                decision: _,
                yes: _,
                no: _,
            } => {}
        }
        result
    }

    fn at_paths(parent: &HeapVersion, paths: &super::input_paths::InputPaths) -> Self {
        let mut result = parent.index.input_writes.clone();
        if paths.id() == parent.input_paths.id() || result.entries.contains(&ValuesPath::default())
        {
            return result;
        }
        for (object, new) in paths.added_paths(&parent.input_paths) {
            let old = parent.input_paths.original_paths(&object);
            if old.is_empty() {
                if prior_mutation(parent, &object) {
                    result.add(&new);
                }
            } else {
                for written in parent.index.input_writes.entries.iter() {
                    for original in old {
                        if let Some(projected) = rebase(written, original, &new) {
                            result.add(&projected);
                        }
                    }
                }
            }
        }
        result
    }

    pub(super) fn paths(&self) -> impl Iterator<Item = &ValuesPath> {
        self.entries.iter()
    }

    fn object(
        &mut self,
        paths: &super::input_paths::InputPaths,
        object: &ObjectId,
        key: Option<&str>,
    ) {
        for path in paths.original_paths(object) {
            let mut path = path.clone();
            if let Some(key) = key {
                path.push(key);
            }
            self.add(&path);
        }
    }

    fn add(&mut self, path: &ValuesPath) {
        let path = ValuesPath::from_segments(
            path.segments()
                .take_while(|segment| segment.literal().is_some()),
        );
        if self.entries.iter().any(|prefix| covers(prefix, &path)) {
            return;
        }
        let entries = Rc::make_mut(&mut self.entries);
        entries.retain(|prefix| !covers(&path, prefix));
        entries.insert(path);
        if entries.len() > 64 {
            *entries = BTreeSet::from([ValuesPath::default()]);
        }
    }

    fn union(&self, other: &Self) -> Self {
        let mut result = self.clone();
        if !Rc::ptr_eq(&self.entries, &other.entries) {
            for path in other.entries.iter() {
                result.add(path);
            }
        }
        result
    }
}

fn covers(parent: &ValuesPath, child: &ValuesPath) -> bool {
    parent == child || child.is_descendant_of(parent)
}

fn rebase(written: &ValuesPath, old: &ValuesPath, new: &ValuesPath) -> Option<ValuesPath> {
    if covers(written, old) {
        Some(new.clone())
    } else if covers(old, written) {
        Some(ValuesPath::from_segments(
            new.segments()
                .chain(written.segments().skip(old.segments().count())),
        ))
    } else {
        None
    }
}

// Registration adds a name for existing contents; it does not erase earlier mutations.
fn prior_mutation(root: &HeapVersion, object: &ObjectId) -> bool {
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    while let Some(version) = pending.pop() {
        if !seen.insert(version.id.ordinal) {
            continue;
        }
        if let Some(previous) = version.index.skip_root(object) {
            pending.push(previous);
            continue;
        }
        match &version.node {
            HeapNode::Empty => {}
            HeapNode::Registration { parent, paths: _ } => pending.push(parent),
            HeapNode::Install {
                parent,
                object: target,
                storage: _,
                kind,
            } => {
                if target == object {
                    if *kind == super::InstallKind::Mutation {
                        return true;
                    }
                    if parent.allocations.get(object).is_none() {
                        continue;
                    }
                }
                pending.push(parent);
            }
            HeapNode::Write {
                parent,
                object: target,
                key: _,
                field: _,
            }
            | HeapNode::WriteSlot {
                parent,
                object: target,
                index: _,
                value: _,
            } => {
                if target == object {
                    return true;
                }
                pending.push(parent);
            }
            HeapNode::UnknownWrite {
                parent,
                objects,
                inputs: _,
            } => {
                match objects {
                    Reach::Top => return true,
                    Reach::Bounded(bounds) => {
                        if bounds.contains(object) {
                            return true;
                        }
                    }
                }
                pending.push(parent);
            }
            HeapNode::Join {
                decision: _,
                yes,
                no,
            } => {
                pending.push(no);
                pending.push(yes);
            }
        }
    }
    false
}

#[cfg(test)]
#[path = "../tests/runtime_input_writes.rs"]
mod tests;
