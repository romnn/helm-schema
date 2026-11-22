//! Shared, values-decidable conditions for executed requirements.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher as _};
use std::sync::Arc;

use ahash::{AHashMap, AHashSet};
use serde::{Serialize, Serializer};

use crate::{ConditionalGuard, Predicate, RequirementDecision, ValuesPath};

/// A node in a shared requirement condition.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementConditionNode {
    /// The condition cannot hold.
    False,
    /// The condition always holds.
    True,
    /// A guard evaluated as one unit, including a bounded requirement proof.
    Guard(ConditionalGuard),
    /// All successor conditions hold.
    And(Vec<usize>),
    /// At least one successor condition holds.
    Or(Vec<usize>),
    /// A values test selects one successor.
    Decision(RequirementDecision),
    /// An execution split adds its test's branch context before entering each successor.
    Split(RequirementDecision),
}

impl RequirementConditionNode {
    /// Iterates successors in deterministic traversal order.
    pub fn children(&self) -> impl DoubleEndedIterator<Item = usize> + '_ {
        let (parts, decision) = match self {
            Self::False | Self::True | Self::Guard(_) => (&[][..], None),
            Self::And(parts) | Self::Or(parts) => (parts.as_slice(), None),
            Self::Decision(RequirementDecision {
                test: _,
                when_true,
                when_false,
            })
            | Self::Split(RequirementDecision {
                test: _,
                when_true,
                when_false,
            }) => (&[][..], Some([*when_true, *when_false])),
        };
        parts.iter().copied().chain(decision.into_iter().flatten())
    }

    fn remap(
        &self,
        mut child: impl FnMut(usize) -> usize,
        mut guard: impl FnMut(&ConditionalGuard) -> ConditionalGuard,
    ) -> Self {
        match self {
            Self::False => Self::False,
            Self::True => Self::True,
            Self::Guard(value) => Self::Guard(guard(value)),
            Self::And(parts) => Self::And(parts.iter().copied().map(child).collect()),
            Self::Or(parts) => Self::Or(parts.iter().copied().map(child).collect()),
            Self::Decision(decision) | Self::Split(decision) => {
                let RequirementDecision {
                    test,
                    when_true,
                    when_false,
                } = decision;
                let decision = RequirementDecision {
                    test: guard(test),
                    when_true: child(*when_true),
                    when_false: child(*when_false),
                };
                if matches!(self, Self::Split(_)) {
                    Self::Split(decision)
                } else {
                    Self::Decision(decision)
                }
            }
        }
    }

    fn kind_rank(&self) -> u8 {
        match self {
            Self::False => 0,
            Self::True => 1,
            Self::Guard(_) => 2,
            Self::And(_) => 3,
            Self::Or(_) => 4,
            Self::Decision(_) => 5,
            Self::Split(_) => 6,
        }
    }
}

#[derive(Debug)]
struct Entry {
    node: RequirementConditionNode,
    fingerprint: u64,
}

/// Interns requirement condition nodes before freezing the shared graph.
///
/// IDs other than the two constants belong to the builder that issued them.
#[derive(Debug)]
pub struct RequirementConditionBuilder {
    entries: Vec<Entry>,
    interned: AHashMap<u64, Vec<usize>>,
}

impl Default for RequirementConditionBuilder {
    fn default() -> Self {
        let mut builder = Self {
            entries: Vec::new(),
            interned: AHashMap::new(),
        };
        builder.intern(RequirementConditionNode::False);
        builder.intern(RequirementConditionNode::True);
        builder
    }
}

impl RequirementConditionBuilder {
    /// The false terminal ID.
    pub const FALSE: usize = 0;

    /// The true terminal ID.
    pub const TRUE: usize = 1;

    /// Adds a guard unit or imports an existing requirement condition.
    pub fn guard(&mut self, mut guard: ConditionalGuard) -> usize {
        if let ConditionalGuard::Requirement(graph) = guard {
            return self.import(&graph, Clone::clone);
        }
        if let ConditionalGuard::Decision(graph) = &guard
            && graph.root() <= Self::TRUE
        {
            return graph.root();
        }
        guard.canonicalize_nested();
        self.intern(RequirementConditionNode::Guard(guard))
    }

    /// Adds a conjunction of nodes from this builder.
    pub fn and(&mut self, parts: impl IntoIterator<Item = usize>) -> usize {
        self.boolean(
            parts,
            Self::TRUE,
            Some(Self::FALSE),
            RequirementConditionNode::And,
        )
    }

    /// Adds a disjunction of nodes from this builder.
    pub fn or(&mut self, parts: impl IntoIterator<Item = usize>) -> usize {
        self.boolean(
            parts,
            Self::FALSE,
            Some(Self::TRUE),
            RequirementConditionNode::Or,
        )
    }

    /// Unions proof arms that lowering may judge in different contexts.
    ///
    /// An unconditional proof arm stays alongside other arms so lowering can withdraw them
    /// independently.
    pub fn union(&mut self, parts: impl IntoIterator<Item = usize>) -> usize {
        self.boolean(parts, Self::FALSE, None, RequirementConditionNode::Or)
    }

    /// Adds a values test with two successors from this builder.
    pub fn decision(
        &mut self,
        mut test: ConditionalGuard,
        when_true: usize,
        when_false: usize,
    ) -> usize {
        if when_true == when_false {
            return when_true;
        }
        test.canonicalize_nested();
        self.intern(RequirementConditionNode::Decision(RequirementDecision {
            test,
            when_true,
            when_false,
        }))
    }

    /// Adds a split whose test selects the execution context as well as the successor.
    ///
    /// Equal successors retain both branch contexts when the split can hold.
    pub fn split(
        &mut self,
        mut test: ConditionalGuard,
        when_true: usize,
        when_false: usize,
    ) -> usize {
        if when_true == Self::FALSE && when_false == Self::FALSE {
            return Self::FALSE;
        }
        test.canonicalize_nested();
        self.intern(RequirementConditionNode::Split(RequirementDecision {
            test,
            when_true,
            when_false,
        }))
    }

    /// Freezes the nodes so that multiple roots can share them.
    #[must_use]
    pub fn freeze(self) -> RequirementConditions {
        RequirementConditions {
            entries: self.entries.into(),
        }
    }

    fn boolean(
        &mut self,
        parts: impl IntoIterator<Item = usize>,
        identity: usize,
        absorbing: Option<usize>,
        node: impl FnOnce(Vec<usize>) -> RequirementConditionNode,
    ) -> usize {
        let mut children = Vec::new();
        for part in parts {
            if Some(part) == absorbing {
                return part;
            }
            if part != identity {
                children.push(part);
            }
        }
        children.sort_unstable_by(|left, right| compare_children(&self.entries, *left, *right));
        children.dedup();
        match children.as_slice() {
            [] => identity,
            [only] => *only,
            _ => self.intern(node(children)),
        }
    }

    fn import(
        &mut self,
        graph: &RequirementCondition,
        mut guard: impl FnMut(&ConditionalGuard) -> ConditionalGuard,
    ) -> usize {
        let mut ids = AHashMap::new();
        for id in graph.reachable_ids() {
            let Some(node) = graph.node(id) else {
                continue;
            };
            let node = node.remap(|child| mapped_id(&ids, child), &mut guard);
            let mapped = match node {
                RequirementConditionNode::False => Self::FALSE,
                RequirementConditionNode::True => Self::TRUE,
                RequirementConditionNode::Guard(guard) => self.guard(guard),
                RequirementConditionNode::And(parts) => self.and(parts),
                RequirementConditionNode::Or(parts) => self.union(parts),
                RequirementConditionNode::Decision(RequirementDecision {
                    test,
                    when_true,
                    when_false,
                }) => self.decision(test, when_true, when_false),
                RequirementConditionNode::Split(RequirementDecision {
                    test,
                    when_true,
                    when_false,
                }) => self.split(test, when_true, when_false),
            };
            ids.insert(id, mapped);
        }
        mapped_id(&ids, graph.root)
    }

    fn intern(&mut self, node: RequirementConditionNode) -> usize {
        let fingerprint = self.fingerprint(&node);
        if let Some(ids) = self.interned.get(&fingerprint)
            && let Some(id) = ids.iter().find(|id| {
                self.entries
                    .get(**id)
                    .is_some_and(|entry| entry.node == node)
            })
        {
            return *id;
        }
        debug_assert!(node.children().all(|child| child < self.entries.len()));
        let id = self.entries.len();
        self.interned.entry(fingerprint).or_default().push(id);
        self.entries.push(Entry { node, fingerprint });
        id
    }

    fn fingerprint(&self, node: &RequirementConditionNode) -> u64 {
        let child_hash = |child| fingerprint(&self.entries, child);
        let mut state = DefaultHasher::new();
        std::mem::discriminant(node).hash(&mut state);
        match node {
            RequirementConditionNode::False | RequirementConditionNode::True => {}
            RequirementConditionNode::Guard(guard) => guard.hash(&mut state),
            RequirementConditionNode::And(parts) | RequirementConditionNode::Or(parts) => {
                parts.len().hash(&mut state);
                for part in parts {
                    child_hash(*part).hash(&mut state);
                }
            }
            RequirementConditionNode::Decision(RequirementDecision {
                test,
                when_true,
                when_false,
            })
            | RequirementConditionNode::Split(RequirementDecision {
                test,
                when_true,
                when_false,
            }) => {
                test.hash(&mut state);
                child_hash(*when_true).hash(&mut state);
                child_hash(*when_false).hash(&mut state);
            }
        }
        state.finish()
    }
}

/// An immutable arena shared by roots from one requirement fold.
pub struct RequirementConditions {
    entries: Arc<[Entry]>,
}

impl RequirementConditions {
    /// Selects a root by its ID in the builder that created this arena.
    #[must_use]
    pub fn root(&self, root: usize, projection_exact: bool) -> RequirementCondition {
        debug_assert!(root < self.entries.len());
        RequirementCondition {
            entries: Arc::clone(&self.entries),
            root,
            projection_exact,
        }
    }
}

/// An immutable projected requirement condition rooted in a shared arena.
#[derive(Clone)]
pub struct RequirementCondition {
    entries: Arc<[Entry]>,
    root: usize,
    projection_exact: bool,
}

impl RequirementCondition {
    /// Whether universal projection covered the entire original execution guard.
    #[must_use]
    pub fn projection_is_exact(&self) -> bool {
        self.projection_exact
    }

    /// Returns the root node ID in this arena.
    #[must_use]
    pub fn root(&self) -> usize {
        self.root
    }

    /// Selects another arena node without transferring whole-union completeness.
    #[must_use]
    pub fn subroot(&self, id: usize) -> Option<Self> {
        self.node(id)?;
        Some(Self {
            entries: Arc::clone(&self.entries),
            root: id,
            projection_exact: self.projection_exact && id == self.root,
        })
    }

    /// Returns a node in this root's arena.
    #[must_use]
    pub fn node(&self, id: usize) -> Option<&RequirementConditionNode> {
        node(&self.entries, id)
    }

    /// Identifies this arena for memoization while the root remains alive.
    #[must_use]
    pub fn arena_id(&self) -> usize {
        self.entries.as_ptr() as usize
    }

    /// Returns reachable IDs in deterministic child-before-parent traversal order.
    #[must_use]
    pub fn reachable_ids(&self) -> Vec<usize> {
        reachable_ids(&self.entries, self.root)
    }

    /// Iterates reachable guard units and decision tests.
    pub fn guards(&self) -> impl Iterator<Item = &ConditionalGuard> {
        self.reachable_ids()
            .into_iter()
            .filter_map(|id| match self.node(id)? {
                RequirementConditionNode::Guard(guard)
                | RequirementConditionNode::Decision(RequirementDecision {
                    test: guard,
                    when_true: _,
                    when_false: _,
                })
                | RequirementConditionNode::Split(RequirementDecision {
                    test: guard,
                    when_true: _,
                    when_false: _,
                }) => Some(guard),
                RequirementConditionNode::False
                | RequirementConditionNode::True
                | RequirementConditionNode::And(_)
                | RequirementConditionNode::Or(_) => None,
            })
    }

    /// Returns the values paths tested by reachable nodes.
    #[must_use]
    pub fn value_paths(&self) -> BTreeSet<ValuesPath> {
        let mut paths = BTreeSet::new();
        let mut visited = AHashSet::new();
        for test in self.guards() {
            test.collect_value_paths(&mut paths, &mut visited);
        }
        paths
    }

    /// Maps the tested values paths and rebuilds their structural identities.
    #[must_use]
    pub fn map_value_paths(&self, map: &mut impl FnMut(ValuesPath) -> ValuesPath) -> Self {
        let mut builder = RequirementConditionBuilder::default();
        let mut changed = false;
        let root = builder.import(self, |guard| {
            let mapped = guard.clone().map_value_paths(map);
            changed |= &mapped != guard;
            mapped
        });
        if changed {
            builder.freeze().root(root, self.projection_exact)
        } else {
            self.clone()
        }
    }

    /// Reconstructs the Boolean predicate with shared successors.
    #[must_use]
    pub fn predicate(&self) -> Predicate {
        let mut predicates = AHashMap::<usize, Predicate>::new();
        let mut projected = AHashMap::new();
        for id in self.reachable_ids() {
            let Some(node) = self.node(id) else {
                continue;
            };
            let child = |child: &usize| predicates.get(child).cloned().unwrap_or(Predicate::False);
            let predicate = match node {
                RequirementConditionNode::False => Predicate::False,
                RequirementConditionNode::True => Predicate::True,
                RequirementConditionNode::Guard(guard) => guard.shared_predicate(&mut projected),
                RequirementConditionNode::And(parts) => {
                    Predicate::And(parts.iter().map(child).collect())
                }
                RequirementConditionNode::Or(parts) => {
                    Predicate::Or(parts.iter().map(child).collect())
                }
                RequirementConditionNode::Decision(RequirementDecision {
                    test,
                    when_true,
                    when_false,
                })
                | RequirementConditionNode::Split(RequirementDecision {
                    test,
                    when_true,
                    when_false,
                }) => {
                    let test = test.shared_predicate(&mut projected);
                    Predicate::Or(vec![
                        Predicate::And(vec![test.clone(), child(when_true)]),
                        Predicate::And(vec![test.negated(), child(when_false)]),
                    ])
                }
            };
            predicates.insert(id, predicate);
        }
        predicates.remove(&self.root).unwrap_or(Predicate::False)
    }

    pub(crate) fn excludes_atom_value(&self, atom: &ConditionalGuard, value: bool) -> bool {
        let mut possible = AHashMap::new();
        for id in self.reachable_ids() {
            let Some(node) = self.node(id) else {
                continue;
            };
            let child = |child: &usize| possible.get(child).copied().unwrap_or(true);
            let may_hold = match node {
                RequirementConditionNode::False => false,
                RequirementConditionNode::True => true,
                RequirementConditionNode::Guard(guard) => !guard.excludes_atom_value(atom, value),
                RequirementConditionNode::And(parts) => parts.iter().all(child),
                RequirementConditionNode::Or(parts) => parts.iter().any(child),
                RequirementConditionNode::Decision(RequirementDecision {
                    test,
                    when_true,
                    when_false,
                })
                | RequirementConditionNode::Split(RequirementDecision {
                    test,
                    when_true,
                    when_false,
                }) => {
                    if test == atom {
                        child(if value { when_true } else { when_false })
                    } else {
                        (!test.excludes_atom_value(atom, value) && child(when_true))
                            || child(when_false)
                    }
                }
            };
            possible.insert(id, may_hold);
        }
        possible.get(&self.root).is_some_and(|possible| !possible)
    }

    fn fingerprint(&self) -> u64 {
        fingerprint(&self.entries, self.root)
    }

    fn canonical(&self) -> CanonicalCondition {
        let (nodes, root) = canonical_nodes(&self.entries, self.root);
        CanonicalCondition {
            nodes,
            root,
            projection_exact: self.projection_exact,
        }
    }
}

fn node(entries: &[Entry], id: usize) -> Option<&RequirementConditionNode> {
    entries.get(id).map(
        |Entry {
             node,
             fingerprint: _,
         }| node,
    )
}

fn fingerprint(entries: &[Entry], id: usize) -> u64 {
    entries.get(id).map_or(
        0,
        |Entry {
             node: _,
             fingerprint,
         }| *fingerprint,
    )
}

fn compare_children(entries: &[Entry], left: usize, right: usize) -> Ordering {
    if left == right {
        return Ordering::Equal;
    }
    let (Some(left_node), Some(right_node)) = (node(entries, left), node(entries, right)) else {
        return left.cmp(&right);
    };
    let order = match (left_node, right_node) {
        (RequirementConditionNode::Guard(left), RequirementConditionNode::Guard(right)) => {
            return left.cmp(right);
        }
        (RequirementConditionNode::Decision(left), RequirementConditionNode::Decision(right))
        | (RequirementConditionNode::Split(left), RequirementConditionNode::Split(right)) => {
            left.test.cmp(&right.test)
        }
        _ => left_node.kind_rank().cmp(&right_node.kind_rank()),
    };
    // Compare exact graph shapes when fingerprints collide.
    order
        .then_with(|| fingerprint(entries, left).cmp(&fingerprint(entries, right)))
        .then_with(|| canonical_nodes(entries, left).cmp(&canonical_nodes(entries, right)))
}

fn reachable_ids(entries: &[Entry], root: usize) -> Vec<usize> {
    let mut visited = AHashSet::new();
    let mut pending = vec![(root, false)];
    let mut ordered = Vec::new();
    while let Some((id, children_done)) = pending.pop() {
        if children_done {
            ordered.push(id);
        } else if visited.insert(id) {
            let Some(node) = node(entries, id) else {
                continue;
            };
            pending.push((id, true));
            pending.extend(node.children().rev().map(|child| (child, false)));
        }
    }
    ordered
}

fn canonical_nodes(entries: &[Entry], root: usize) -> (Vec<RequirementConditionNode>, usize) {
    let mut ids = AHashMap::new();
    let mut nodes = Vec::new();
    for id in reachable_ids(entries, root) {
        if id <= RequirementConditionBuilder::TRUE {
            continue;
        }
        if let Some(node) = node(entries, id) {
            nodes.push(node.remap(|child| mapped_id(&ids, child), Clone::clone));
            ids.insert(id, nodes.len() + 1);
        }
    }
    (nodes, mapped_id(&ids, root))
}

fn mapped_id(ids: &AHashMap<usize, usize>, id: usize) -> usize {
    if id <= RequirementConditionBuilder::TRUE {
        return id;
    }
    ids.get(&id)
        .copied()
        .unwrap_or(RequirementConditionBuilder::FALSE)
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct CanonicalCondition {
    nodes: Vec<RequirementConditionNode>,
    root: usize,
    projection_exact: bool,
}

impl fmt::Debug for RequirementCondition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.canonical(), formatter)
    }
}

impl PartialEq for RequirementCondition {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for RequirementCondition {}

impl Ord for RequirementCondition {
    fn cmp(&self, other: &Self) -> Ordering {
        self.projection_exact
            .cmp(&other.projection_exact)
            .then_with(|| {
                if self.root == other.root && Arc::ptr_eq(&self.entries, &other.entries) {
                    return Ordering::Equal;
                }
                self.fingerprint()
                    .cmp(&other.fingerprint())
                    .then_with(|| self.canonical().cmp(&other.canonical()))
            })
    }
}

impl PartialOrd for RequirementCondition {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for RequirementCondition {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.projection_exact.hash(state);
        self.fingerprint().hash(state);
    }
}

impl Serialize for RequirementCondition {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.canonical().serialize(serializer)
    }
}

#[cfg(test)]
#[path = "tests/requirement_dag.rs"]
mod tests;
