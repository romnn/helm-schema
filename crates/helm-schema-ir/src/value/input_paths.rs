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

    pub(super) fn visit<'paths>(
        &'paths self,
        mut visit: impl FnMut(&'paths ObjectId, &'paths ValuesPath),
    ) {
        let mut pending = self.root.as_deref().into_iter().collect::<Vec<_>>();
        while let Some(node) = pending.pop() {
            match node {
                Node::Leaf { object, paths } => {
                    if let Some(path) = paths.last() {
                        visit(object, path);
                    }
                }
                Node::Branch { left, right } => {
                    pending.extend(right.as_deref());
                    pending.extend(left.as_deref());
                }
            }
        }
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
