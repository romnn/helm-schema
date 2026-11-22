//! Symbolic live atom frontiers for one guarded-default projection.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::rc::Rc;

// A cache hit saves a second walk when a transition adds a fresh guard atom.
const RECENT_ATOMS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct AtomSet(usize);

impl AtomSet {
    pub(super) const EMPTY: Self = Self(0);
}

enum Shape {
    Empty,
    Atoms(Rc<[usize]>),
    Union { left: AtomSet, right: AtomSet },
}

#[derive(Clone, Copy)]
struct Bounds {
    min: usize,
    max: usize,
}

impl Bounds {
    const EMPTY: Self = Self {
        min: usize::MAX,
        max: 0,
    };

    fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}

struct Node {
    shape: Shape,
    bounds: Bounds,
}

#[derive(Default, PartialEq, Eq, Hash)]
struct Fingerprint {
    cardinality: usize,
    hash_sum: u64,
    hash_xor: u64,
}

impl Fingerprint {
    fn add(&mut self, ordinal: usize) {
        let mut hasher = DefaultHasher::new();
        ordinal.hash(&mut hasher);
        let hash = hasher.finish();
        self.cardinality += 1;
        self.hash_sum = self.hash_sum.wrapping_add(hash);
        self.hash_xor ^= hash;
    }
}

struct AtomMembership {
    ordinal: usize,
    outcomes: HashMap<AtomSet, bool>,
}

#[derive(Default)]
struct MembershipQueries {
    recent: VecDeque<AtomMembership>,
    pending: Vec<AtomSet>,
}

impl MembershipQueries {
    fn select(&mut self, ordinal: usize) {
        if let Some(index) = self
            .recent
            .iter()
            .position(|entry| entry.ordinal == ordinal)
            && let Some(entry) = self.recent.remove(index)
        {
            self.recent.push_front(entry);
            return;
        }
        if self.recent.len() == RECENT_ATOMS {
            drop(self.recent.pop_back());
        }
        self.recent.push_front(AtomMembership {
            ordinal,
            outcomes: HashMap::new(),
        });
    }
}

#[derive(Default)]
struct SetWalk {
    seen: Vec<usize>,
    generation: usize,
    pending: Vec<AtomSet>,
}

impl SetWalk {
    fn atoms(
        &mut self,
        nodes: &[Node],
        root: AtomSet,
        mut accept: impl FnMut(usize) -> bool,
    ) -> bool {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen.fill(0);
            self.generation = 1;
        }
        self.seen.resize(nodes.len(), 0);
        self.pending.clear();
        self.pending.push(root);
        while let Some(AtomSet(index)) = self.pending.pop() {
            let Some((node, seen)) = nodes.get(index).zip(self.seen.get_mut(index)) else {
                return false;
            };
            if *seen == self.generation {
                continue;
            }
            *seen = self.generation;
            match &node.shape {
                Shape::Empty => {}
                Shape::Atoms(atoms) => {
                    if !atoms.iter().copied().all(&mut accept) {
                        return false;
                    }
                }
                Shape::Union { left, right } => self.pending.extend([*left, *right]),
            }
        }
        true
    }
}

struct SetComparison {
    marks: Vec<[usize; 2]>,
    generation: usize,
    walk: SetWalk,
    attempts: usize,
}

impl SetComparison {
    fn next_generation(&mut self) -> usize {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.marks.fill([0; 2]);
            self.generation = 1;
        }
        self.generation
    }

    fn fingerprint(&mut self, nodes: &[Node], set: AtomSet) -> Option<Fingerprint> {
        let generation = self.next_generation();
        let mut fingerprint = Fingerprint::default();
        self.walk
            .atoms(nodes, set, |ordinal| {
                let Some([seen, _other]) = self.marks.get_mut(ordinal) else {
                    return false;
                };
                if *seen != generation {
                    *seen = generation;
                    fingerprint.add(ordinal);
                }
                true
            })
            .then_some(fingerprint)
    }

    fn equal(&mut self, nodes: &[Node], left: AtomSet, right: AtomSet) -> bool {
        self.attempts = self.attempts.saturating_add(1);
        let generation = self.next_generation();
        let mut cardinality = 0_usize;
        if !self.walk.atoms(nodes, left, |ordinal| {
            let Some([left, _right]) = self.marks.get_mut(ordinal) else {
                return false;
            };
            if *left != generation {
                *left = generation;
                cardinality += 1;
            }
            true
        }) {
            return false;
        }
        self.walk.atoms(nodes, right, |ordinal| {
            let Some([left, right]) = self.marks.get_mut(ordinal) else {
                return false;
            };
            if *left != generation {
                return false;
            }
            if *right != generation {
                *right = generation;
                cardinality -= 1;
            }
            true
        }) && cardinality == 0
    }
}

/// Interns source atom lists and symbolic unions without expanding them.
///
/// Only frontiers used as continuation keys compare by membership.
/// Fingerprinting walks each requested root once and counts each atom once.
/// Roots in the same bucket still require exact equivalence checks.
pub(super) struct AtomSets {
    nodes: Vec<Node>,
    leaves: HashMap<Rc<[usize]>, AtomSet>,
    unions: HashMap<(AtomSet, AtomSet), AtomSet>,
    frontiers: HashMap<Fingerprint, Vec<AtomSet>>,
    representatives: HashMap<AtomSet, AtomSet>,
    comparison: SetComparison,
    membership: RefCell<MembershipQueries>,
}

impl AtomSets {
    pub(super) fn new(atom_count: usize) -> Self {
        Self {
            nodes: vec![Node {
                shape: Shape::Empty,
                bounds: Bounds::EMPTY,
            }],
            leaves: HashMap::new(),
            unions: HashMap::new(),
            frontiers: HashMap::new(),
            representatives: HashMap::new(),
            comparison: SetComparison {
                marks: vec![[0; 2]; atom_count],
                generation: 0,
                walk: SetWalk::default(),
                attempts: 0,
            },
            membership: RefCell::default(),
        }
    }

    pub(super) fn leaf(&mut self, atoms: Vec<usize>) -> AtomSet {
        debug_assert!(atoms.is_sorted());
        if atoms.is_empty() {
            return AtomSet::EMPTY;
        }
        let atoms: Rc<[usize]> = atoms.into();
        *self.leaves.entry(Rc::clone(&atoms)).or_insert_with(|| {
            let bounds = Bounds {
                min: atoms.first().copied().unwrap_or(usize::MAX),
                max: atoms.last().copied().unwrap_or(0),
            };
            let set = AtomSet(self.nodes.len());
            self.nodes.push(Node {
                shape: Shape::Atoms(atoms),
                bounds,
            });
            set
        })
    }

    pub(super) fn union(&mut self, left: AtomSet, right: AtomSet) -> Option<AtomSet> {
        if left == AtomSet::EMPTY || left == right {
            return Some(right);
        }
        if right == AtomSet::EMPTY {
            return Some(left);
        }
        let (left, right) = (left.min(right), left.max(right));
        if let Some(&set) = self.unions.get(&(left, right)) {
            return Some(set);
        }
        let bounds = self.node(left)?.bounds.union(self.node(right)?.bounds);
        let set = AtomSet(self.nodes.len());
        self.nodes.push(Node {
            shape: Shape::Union { left, right },
            bounds,
        });
        self.unions.insert((left, right), set);
        Some(set)
    }

    pub(super) fn frontier(&mut self, set: AtomSet) -> Option<AtomSet> {
        if set == AtomSet::EMPTY {
            return Some(set);
        }
        if let Some(&representative) = self.representatives.get(&set) {
            return Some(representative);
        }
        let fingerprint = self.comparison.fingerprint(&self.nodes, set)?;
        let candidates = self.frontiers.entry(fingerprint).or_default();
        let representative = candidates
            .iter()
            .copied()
            .find(|&candidate| self.comparison.equal(&self.nodes, candidate, set));
        let representative = representative.unwrap_or_else(|| {
            candidates.push(set);
            set
        });
        self.representatives.insert(set, representative);
        Some(representative)
    }

    pub(super) fn frontier_comparisons(&self) -> usize {
        self.comparison.attempts
    }

    pub(super) fn contains(&self, set: AtomSet, ordinal: usize) -> bool {
        if let Some(answer) = self.local_membership(set, ordinal) {
            return answer;
        }
        let mut queries = self.membership.borrow_mut();
        queries.select(ordinal);
        let MembershipQueries { recent, pending } = &mut *queries;
        let Some(AtomMembership {
            ordinal: _,
            outcomes,
        }) = recent.front_mut()
        else {
            return true;
        };
        pending.clear();
        pending.push(set);
        while let Some(root) = pending.pop() {
            if outcomes.contains_key(&root) {
                continue;
            }
            let answer = if let Some(answer) = self.local_membership(root, ordinal) {
                answer
            } else if let Some(Node {
                shape: Shape::Union { left, right },
                bounds: _,
            }) = self.node(root)
            {
                let left = *left;
                let right = *right;
                match (outcomes.get(&left), outcomes.get(&right)) {
                    (Some(true), _) | (_, Some(true)) => true,
                    (Some(false), Some(false)) => false,
                    (None, _) => {
                        pending.extend([root, left]);
                        continue;
                    }
                    (Some(false), None) => {
                        pending.extend([root, right]);
                        continue;
                    }
                }
            } else {
                true
            };
            outcomes.insert(root, answer);
        }
        outcomes.get(&set).copied().unwrap_or(true)
    }

    fn local_membership(&self, set: AtomSet, ordinal: usize) -> Option<bool> {
        let Some(Node { shape, bounds }) = self.node(set) else {
            // Unavailable support cannot authorize forgetting an atom.
            return Some(true);
        };
        if ordinal < bounds.min || ordinal > bounds.max {
            return Some(false);
        }
        match shape {
            Shape::Empty => Some(false),
            Shape::Atoms(atoms) => Some(atoms.binary_search(&ordinal).is_ok()),
            Shape::Union { .. } => None,
        }
    }

    fn node(&self, AtomSet(index): AtomSet) -> Option<&Node> {
        self.nodes.get(index)
    }
}

#[cfg(test)]
#[path = "../tests/requirement_activation_live_atoms.rs"]
mod tests;
