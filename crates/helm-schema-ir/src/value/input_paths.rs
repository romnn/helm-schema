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
    Leaf { object: ObjectId, path: ValuesPath },
    Branch { left: Link, right: Link },
}

impl InputPaths {
    pub(super) fn id(&self) -> u64 {
        self.generation
    }

    pub(super) fn get(&self, object: &ObjectId) -> Option<&ValuesPath> {
        if object.ordinal.bit_width() > self.height {
            return None;
        }
        let mut node = self.root.as_deref()?;
        let mut height = self.height;
        loop {
            match node {
                Node::Leaf {
                    object: target,
                    path,
                } => return (target == object).then_some(path),
                Node::Branch { left, right } => {
                    height = height.checked_sub(1)?;
                    node = if object.ordinal & (1 << height) == 0 {
                        left.as_deref()?
                    } else {
                        right.as_deref()?
                    };
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
        let ordinal = object.ordinal;
        let leaf = Rc::new(Node::Leaf { object, path });
        self.root = Some(insert(self.root.as_ref(), leaf, ordinal, self.height));
    }
}

fn insert(node: Option<&Rc<Node>>, leaf: Rc<Node>, ordinal: u64, height: u32) -> Rc<Node> {
    if height == 0 {
        return leaf;
    }
    let (left, right) = match node.map(Rc::as_ref) {
        Some(Node::Branch { left, right }) => (left.clone(), right.clone()),
        Some(Node::Leaf { object: _, path: _ }) | None => (None, None),
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
