//! Immutable original-input identities retained by frozen heap versions.

use std::rc::Rc;

use helm_schema_core::ValuesPath;

use super::ObjectId;

type Link = Option<Rc<Node>>;

#[derive(Clone, Debug, Default)]
pub(super) struct InputPaths {
    generation: u64,
    height: u32,
    root: Link,
}

#[derive(Debug)]
enum Node {
    Leaf {
        object: ObjectId,
        paths: Vec<ValuesPath>,
    },
    Branch {
        left: Link,
        right: Link,
    },
}

impl InputPaths {
    pub(super) fn added_paths(&self, previous: &Self) -> Vec<(ObjectId, ValuesPath)> {
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

    pub(super) fn id(&self) -> u64 {
        self.generation
    }

    pub(super) fn get(&self, object: &ObjectId) -> Option<&ValuesPath> {
        self.original_paths(object).last()
    }

    /// Returns every original path, ordered by its most recent registration.
    pub(super) fn original_paths(&self, object: &ObjectId) -> &[ValuesPath] {
        if object.ordinal.bit_width() > self.height {
            return &[];
        }
        let Some(mut node) = self.root.as_deref() else {
            return &[];
        };
        let mut height = self.height;
        loop {
            match node {
                Node::Leaf {
                    object: target,
                    paths,
                } => return if target == object { paths } else { &[] },
                Node::Branch { left, right } => {
                    let Some(next) = height.checked_sub(1) else {
                        return &[];
                    };
                    height = next;
                    let child = if object.ordinal & (1 << height) == 0 {
                        left.as_deref()
                    } else {
                        right.as_deref()
                    };
                    let Some(child) = child else {
                        return &[];
                    };
                    node = child;
                }
            }
        }
    }

    pub(super) fn contains_key(&self, object: &ObjectId) -> bool {
        self.get(object).is_some()
    }

    pub(super) fn same_mapping(&self, other: &Self, object: &ObjectId) -> bool {
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

    pub(super) fn lookup_work(&self) -> usize {
        self.height as usize + 1
    }

    pub(super) fn insert(&mut self, object: ObjectId, path: ValuesPath) {
        if self.get(&object) == Some(&path) {
            return;
        }
        self.generation += 1;
        while self.height < object.ordinal.bit_width() {
            if self.root.is_some() {
                self.root = Some(Rc::new(Node::Branch {
                    left: self.root.take(),
                    right: None,
                }));
            }
            self.height += 1;
        }
        let mut paths = self.original_paths(&object).to_vec();
        paths.retain(|known| known != &path);
        paths.push(path);
        let ordinal = object.ordinal;
        let leaf = Rc::new(Node::Leaf { object, paths });
        self.root = Some(insert(self.root.as_ref(), leaf, ordinal, self.height));
    }
}

fn insert(node: Option<&Rc<Node>>, leaf: Rc<Node>, ordinal: u64, height: u32) -> Rc<Node> {
    if height == 0 {
        return leaf;
    }
    let (left, right) = match node.map(Rc::as_ref) {
        Some(Node::Branch { left, right }) => (left.clone(), right.clone()),
        Some(Node::Leaf {
            object: _,
            paths: _,
        })
        | None => (None, None),
    };
    let next = height - 1;
    if ordinal & (1 << next) == 0 {
        Rc::new(Node::Branch {
            left: Some(insert(left.as_ref(), leaf, ordinal, next)),
            right,
        })
    } else {
        Rc::new(Node::Branch {
            left,
            right: Some(insert(right.as_ref(), leaf, ordinal, next)),
        })
    }
}

#[cfg(test)]
#[path = "../tests/runtime_input_paths.rs"]
mod tests;
