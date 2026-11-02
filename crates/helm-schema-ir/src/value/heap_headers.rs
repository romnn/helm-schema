//! A persistent allocation index; object contents remain in the heap history.

use std::rc::Rc;

use super::heap_query::QueryBudget;
use super::{HeapNode, Multiplicity, ObjectId, ObjectVersion};

#[cfg(test)]
#[path = "../tests/runtime_heap_headers.rs"]
mod tests;

type AllocationLink = Option<Rc<AllocationNode>>;

#[derive(Debug)]
enum AllocationNode {
    Leaf {
        singletons: u64,
        summaries: u64,
    },
    Branch {
        left: AllocationLink,
        right: AllocationLink,
        classes: usize,
    },
}

const LEAF_BITS: u32 = 6;
const LEAF_MASK: u64 = (1_u64 << LEAF_BITS) - 1;

/// Each update shares every bit-prefix outside its one bounded, 64-bit path.
/// Joins union allocation metadata and never copy object storage.
#[derive(Clone, Debug, Default)]
pub(super) struct AllocationIndex {
    root: AllocationLink,
    height: u32,
    singleton_prefix: u64,
}

impl AllocationIndex {
    pub(super) fn len(&self) -> usize {
        count(&self.root)
    }

    pub(super) fn get(&self, object: &ObjectId) -> Option<Multiplicity> {
        if object.ordinal < self.singleton_prefix {
            return Some(Multiplicity::Singleton);
        }
        self.lookup(object.ordinal)
    }

    fn lookup(&self, ordinal: u64) -> Option<Multiplicity> {
        if ordinal.bit_width() > self.height {
            return None;
        }
        let mut node = self.root.as_deref()?;
        let mut remaining = self.height;
        loop {
            match node {
                AllocationNode::Leaf {
                    singletons,
                    summaries,
                } => {
                    let bit = 1_u64 << (ordinal & LEAF_MASK);
                    return if singletons & bit != 0 {
                        Some(Multiplicity::Singleton)
                    } else if summaries & bit != 0 {
                        Some(Multiplicity::Summary)
                    } else {
                        None
                    };
                }
                AllocationNode::Branch {
                    left,
                    right,
                    classes: _,
                } => {
                    remaining = remaining.checked_sub(1)?;
                    node = if ordinal & (1_u64 << remaining) == 0 {
                        left.as_deref()?
                    } else {
                        right.as_deref()?
                    };
                }
            }
        }
    }

    pub(super) fn for_node(node: &HeapNode) -> Self {
        match node {
            HeapNode::Empty => Self::default(),
            HeapNode::Install {
                parent,
                object,
                storage,
                kind: _,
            } => {
                let ObjectVersion {
                    revision: _,
                    multiplicity,
                    data: _,
                } = storage.as_ref();
                if parent.allocations.get(object) == Some(*multiplicity) {
                    return parent.allocations.clone();
                }
                let height = parent.allocations.height.max(object.ordinal.bit_width());
                let root = raised(&parent.allocations.root, parent.allocations.height, height);
                let mut index = Self {
                    root: install(&root, object.ordinal, *multiplicity, height),
                    height,
                    singleton_prefix: parent.allocations.singleton_prefix,
                };
                if *multiplicity == Multiplicity::Summary {
                    index.singleton_prefix = index.singleton_prefix.min(object.ordinal);
                } else if object.ordinal == index.singleton_prefix {
                    while index.singleton_prefix < u64::MAX
                        && index.lookup(index.singleton_prefix) == Some(Multiplicity::Singleton)
                    {
                        index.singleton_prefix += 1;
                    }
                }
                index
            }
            HeapNode::Registration { parent, paths: _ }
            | HeapNode::Write {
                parent,
                object: _,
                key: _,
                field: _,
            }
            | HeapNode::WriteSlot {
                parent,
                object: _,
                index: _,
                value: _,
            }
            | HeapNode::UnknownWrite {
                parent,
                objects: _,
                inputs: _,
            } => parent.allocations.clone(),
            HeapNode::Join {
                decision: _,
                yes,
                no,
            } => {
                let height = yes.allocations.height.max(no.allocations.height);
                let singleton_prefix = yes
                    .allocations
                    .singleton_prefix
                    .min(no.allocations.singleton_prefix);
                let yes = raised(&yes.allocations.root, yes.allocations.height, height);
                let no = raised(&no.allocations.root, no.allocations.height, height);
                Self {
                    root: union(&yes, &no),
                    height,
                    singleton_prefix,
                }
            }
        }
    }

    pub(super) fn objects(&self, budget: &mut QueryBudget) -> Option<Vec<ObjectId>> {
        if !budget.charge(1) {
            return None;
        }
        let mut objects = Vec::new();
        let mut pending = Vec::new();
        if let Some(root) = &self.root {
            pending.push(Cursor {
                node: root,
                ordinal: 0,
                remaining: self.height,
            });
        }
        while let Some(Cursor {
            node,
            ordinal,
            remaining,
        }) = pending.pop()
        {
            match node.as_ref() {
                AllocationNode::Leaf {
                    singletons,
                    summaries,
                } => {
                    let mut occupied = singletons | summaries;
                    // Compression changes storage, not the logical binary-trie query allowance.
                    if !budget.charge(logical_leaf_nodes(occupied, remaining)) {
                        return None;
                    }
                    while occupied != 0 {
                        let bit = occupied.trailing_zeros();
                        objects.push(ObjectId {
                            ordinal: ordinal | u64::from(bit),
                        });
                        occupied &= occupied - 1;
                    }
                }
                AllocationNode::Branch {
                    left,
                    right,
                    classes: _,
                } => {
                    if !budget.charge(1) {
                        return None;
                    }
                    let bit = remaining.checked_sub(1)?;
                    // Visit the lower prefix first so pages and their ascending bits stay ordered.
                    if let Some(right) = right {
                        pending.push(Cursor {
                            node: right,
                            ordinal: ordinal | (1_u64 << bit),
                            remaining: bit,
                        });
                    }
                    if let Some(left) = left {
                        pending.push(Cursor {
                            node: left,
                            ordinal,
                            remaining: bit,
                        });
                    }
                }
            }
        }
        Some(objects)
    }
}

struct Cursor<'index> {
    node: &'index Rc<AllocationNode>,
    ordinal: u64,
    remaining: u32,
}

fn count(link: &AllocationLink) -> usize {
    link.as_ref().map_or(0, |node| match node.as_ref() {
        AllocationNode::Leaf {
            singletons,
            summaries,
        } => (singletons | summaries).count_ones() as usize,
        AllocationNode::Branch {
            left: _,
            right: _,
            classes,
        } => *classes,
    })
}

fn leaf(singletons: u64, summaries: u64) -> AllocationLink {
    (singletons | summaries != 0).then(|| {
        Rc::new(AllocationNode::Leaf {
            singletons,
            summaries,
        })
    })
}

fn branch(left: AllocationLink, right: AllocationLink) -> AllocationLink {
    let classes = count(&left) + count(&right);
    (classes > 0).then(|| {
        Rc::new(AllocationNode::Branch {
            left,
            right,
            classes,
        })
    })
}

fn raised(root: &AllocationLink, height: u32, target: u32) -> AllocationLink {
    let mut root = root.clone();
    // A leaf already represents zero-prefixed pages through six logical levels.
    for _ in height.max(LEAF_BITS)..target {
        root = branch(root, None);
    }
    root
}

fn install(
    link: &AllocationLink,
    ordinal: u64,
    multiplicity: Multiplicity,
    remaining: u32,
) -> AllocationLink {
    let mut left = None;
    let mut right = None;
    match link.as_deref() {
        Some(AllocationNode::Leaf {
            singletons,
            summaries,
        }) => {
            return install_leaf(*singletons, *summaries, ordinal, multiplicity);
        }
        Some(AllocationNode::Branch {
            left: prior_left,
            right: prior_right,
            classes: _,
        }) => {
            left = prior_left.clone();
            right = prior_right.clone();
        }
        None if remaining <= LEAF_BITS => return install_leaf(0, 0, ordinal, multiplicity),
        None => {}
    }
    // Every branch is above a bitmap page, so its remaining height exceeds six.
    let bit = remaining.saturating_sub(1);
    if ordinal & (1_u64 << bit) == 0 {
        left = install(&left, ordinal, multiplicity, bit);
    } else {
        right = install(&right, ordinal, multiplicity, bit);
    }
    branch(left, right)
}

fn install_leaf(
    singletons: u64,
    summaries: u64,
    ordinal: u64,
    multiplicity: Multiplicity,
) -> AllocationLink {
    let bit = 1_u64 << (ordinal & LEAF_MASK);
    match multiplicity {
        Multiplicity::Singleton => leaf(singletons | bit, summaries & !bit),
        Multiplicity::Summary => leaf(singletons & !bit, summaries | bit),
    }
}

fn same(yes: &AllocationLink, no: &AllocationLink) -> bool {
    match (yes, no) {
        (None, None) => true,
        (Some(yes), Some(no)) => Rc::ptr_eq(yes, no),
        (None, Some(_)) | (Some(_), None) => false,
    }
}

fn union(yes: &AllocationLink, no: &AllocationLink) -> AllocationLink {
    let (yes, no) = match (yes, no) {
        (Some(yes), Some(no)) if !Rc::ptr_eq(yes, no) => (yes, no),
        (yes, no) => return yes.clone().or_else(|| no.clone()),
    };
    match (yes.as_ref(), no.as_ref()) {
        (
            AllocationNode::Leaf {
                singletons: yes_singletons,
                summaries: yes_summaries,
            },
            AllocationNode::Leaf {
                singletons: no_singletons,
                summaries: no_summaries,
            },
        ) => {
            let summaries = yes_summaries | no_summaries;
            let singletons = (yes_singletons | no_singletons) & !summaries;
            if singletons == *yes_singletons && summaries == *yes_summaries {
                Some(Rc::clone(yes))
            } else if singletons == *no_singletons && summaries == *no_summaries {
                Some(Rc::clone(no))
            } else {
                leaf(singletons, summaries)
            }
        }
        (
            AllocationNode::Branch {
                left: yes_left,
                right: yes_right,
                classes: _,
            },
            AllocationNode::Branch {
                left: no_left,
                right: no_right,
                classes: _,
            },
        ) => {
            let left = union(yes_left, no_left);
            let right = union(yes_right, no_right);
            if same(&left, yes_left) && same(&right, yes_right) {
                Some(Rc::clone(yes))
            } else if same(&left, no_left) && same(&right, no_right) {
                Some(Rc::clone(no))
            } else {
                branch(left, right)
            }
        }
        (
            AllocationNode::Branch {
                left,
                right,
                classes: _,
            },
            AllocationNode::Leaf {
                singletons: _,
                summaries: _,
            },
        ) => {
            let joined = union(left, &Some(Rc::clone(no)));
            if same(&joined, left) {
                Some(Rc::clone(yes))
            } else {
                branch(joined, right.clone())
            }
        }
        (
            AllocationNode::Leaf {
                singletons: _,
                summaries: _,
            },
            AllocationNode::Branch {
                left,
                right,
                classes: _,
            },
        ) => {
            let joined = union(&Some(Rc::clone(yes)), left);
            if same(&joined, left) {
                Some(Rc::clone(no))
            } else {
                branch(joined, right.clone())
            }
        }
    }
}

fn logical_leaf_nodes(occupied: u64, remaining: u32) -> usize {
    let mut nodes = 0;
    for shift in 0..=remaining.min(LEAF_BITS) {
        let mut prefixes = 0_u64;
        let mut bits = occupied;
        while bits != 0 {
            prefixes |= 1_u64 << (bits.trailing_zeros() >> shift);
            bits &= bits - 1;
        }
        nodes += prefixes.count_ones() as usize;
    }
    nodes
}
