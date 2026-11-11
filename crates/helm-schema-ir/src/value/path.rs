//! Execution paths as immutable prefix links (ADR 0072 §2–3).
//!
//! A path is a chain of links from the render's root to one program point. Forks share their
//! prefix, so a fork is one allocation and a join finds the fork point by walking to the first
//! differing link. Each link carries one of three kinds of condition, which stay separate:
//!
//! - [`LinkKind::Branch`]: a template branch was taken (reachability).
//! - [`LinkKind::Survival`]: a check passed on the path. Helm rejects every document that failed
//!   it, so survival refines values and feasibility but never appears in an exported guard.
//! - [`LinkKind::Assume`]: an analysis validity assumption. Facts below it are claimed only
//!   under it, so it is exported and never dropped.
//!
//! Every link memoizes the runtime guard of its whole prefix and of its exported prefix
//! (branches and assumptions), so export shares prefixes instead of copying conjunctions.

use std::cell::OnceCell;
use std::rc::Rc;

use helm_schema_core::Predicate;

use super::guard::{DecisionStore, RuntimeGuard};
use super::{Coverage, DecisionEdge};

/// What a link's condition means for the facts recorded below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum LinkKind {
    Branch,
    Survival,
    Assume,
}

impl LinkKind {
    /// Whether the condition is part of the guard exported with a fact.
    pub(super) fn is_exported(self) -> bool {
        match self {
            Self::Branch | Self::Assume => true,
            Self::Survival => false,
        }
    }

    /// Whether the condition holds on every document reaching the link. An assumption is not
    /// such a fact: it restricts what the analysis claims, not which documents run.
    pub(super) fn is_feasibility(self) -> bool {
        match self {
            Self::Branch | Self::Survival => true,
            Self::Assume => false,
        }
    }

    fn kept(self, view: View) -> bool {
        match view {
            View::Exported => self.is_exported(),
            View::Feasibility => self.is_feasibility(),
            View::Whole => true,
        }
    }
}

/// Which links a guard or predicate of a path conjoins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    /// Branches and assumptions: the guard exported with a fact.
    Exported,
    /// Branches and survivals: the documents that reach the point.
    Feasibility,
    /// Every link: the documents that reach the point and on which the analysis claims it.
    Whole,
}

#[derive(Debug)]
pub(super) struct PathLink {
    parent: Path,
    edge: DecisionEdge,
    kind: LinkKind,
    depth: usize,
    /// The conjunction of the branch and survival links up to and including this one.
    guard: OnceCell<RuntimeGuard>,
    /// The conjunction of the exported links up to and including this one.
    exported: OnceCell<RuntimeGuard>,
    /// The possible predicate of the exported links.
    exported_predicate: OnceCell<Predicate>,
    /// The possible predicate of the branch and survival links.
    feasible_predicate: OnceCell<Predicate>,
    /// The possible predicate of every link.
    whole_predicate: OnceCell<Predicate>,
}

impl Drop for PathLink {
    fn drop(&mut self) {
        // Long chains drop iteratively instead of recursing once per link.
        let mut parent = self.parent.head.take();
        while let Some(link) = parent {
            match Rc::try_unwrap(link) {
                Ok(mut link) => parent = link.parent.head.take(),
                Err(_) => break,
            }
        }
    }
}

impl PathLink {
    pub(super) fn edge(&self) -> &DecisionEdge {
        &self.edge
    }

    pub(super) fn kind(&self) -> LinkKind {
        self.kind
    }

    fn same_condition(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.edge.taken == other.edge.taken
            && self.edge.test == other.edge.test
    }
}

/// One program point's chain of conditions; the empty path is the render's root.
#[derive(Clone, Debug, Default)]
pub(super) struct Path {
    head: Option<Rc<PathLink>>,
}

impl PartialEq for Path {
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }
        let mut left = self.head.as_ref();
        let mut right = other.head.as_ref();
        while let (Some(left_link), Some(right_link)) = (left, right) {
            if Rc::ptr_eq(left_link, right_link) {
                return true;
            }
            if !left_link.same_condition(right_link) {
                return false;
            }
            left = left_link.parent.head.as_ref();
            right = right_link.parent.head.as_ref();
        }
        left.is_none() && right.is_none()
    }
}
impl Eq for Path {}

impl From<Vec<DecisionEdge>> for Path {
    fn from(edges: Vec<DecisionEdge>) -> Self {
        Self::from_edges(edges)
    }
}

/// The fork point of two paths and the links each took after it, in root-to-leaf order.
pub(super) struct Divergence<'a> {
    pub(super) common: Path,
    pub(super) left: Vec<&'a PathLink>,
    pub(super) right: Vec<&'a PathLink>,
}

impl Path {
    pub(super) fn len(&self) -> usize {
        self.head.as_ref().map_or(0, |link| link.depth)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// Whether both paths are the same allocation.
    pub(super) fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.head, &other.head) {
            (Some(left), Some(right)) => Rc::ptr_eq(left, right),
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => false,
        }
    }

    /// The path extended by one condition.
    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::counters",
        name = "path_link"
    )]
    pub(super) fn pushed(&self, edge: DecisionEdge, kind: LinkKind) -> Self {
        Self {
            head: Some(Rc::new(PathLink {
                parent: self.clone(),
                edge,
                kind,
                depth: self.len() + 1,
                guard: OnceCell::new(),
                exported: OnceCell::new(),
                exported_predicate: OnceCell::new(),
                feasible_predicate: OnceCell::new(),
                whole_predicate: OnceCell::new(),
            })),
        }
    }

    pub(super) fn push(&mut self, edge: DecisionEdge, kind: LinkKind) {
        *self = self.pushed(edge, kind);
    }

    pub(super) fn push_branch(&mut self, edge: DecisionEdge) {
        self.push(edge, LinkKind::Branch);
    }

    /// Appends branch links.
    pub(super) fn extend(&mut self, edges: impl IntoIterator<Item = DecisionEdge>) {
        for edge in edges {
            self.push_branch(edge);
        }
    }

    /// Drops the last link.
    pub(super) fn pop(&mut self) {
        self.truncate(self.len().saturating_sub(1));
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Keeps the prefix of `depth` links.
    pub(super) fn truncate(&mut self, depth: usize) {
        while self.len() > depth {
            let parent = self
                .head
                .as_ref()
                .map(|link| link.parent.clone())
                .unwrap_or_default();
            *self = parent;
        }
    }

    /// The links in root-to-leaf order.
    pub(super) fn links(&self) -> Vec<&PathLink> {
        let mut links = Vec::with_capacity(self.len());
        let mut cursor = self.head.as_deref();
        while let Some(link) = cursor {
            links.push(link);
            cursor = link.parent.head.as_deref();
        }
        links.reverse();
        links
    }

    /// Every edge, whatever its kind, in root-to-leaf order.
    pub(super) fn edges(&self) -> Vec<DecisionEdge> {
        self.links()
            .into_iter()
            .map(|link| link.edge.clone())
            .collect()
    }

    /// A path of branch links.
    pub(super) fn from_edges(edges: impl IntoIterator<Item = DecisionEdge>) -> Self {
        let mut path = Self::default();
        for edge in edges {
            path.push_branch(edge);
        }
        path
    }

    /// Whether every test on the path has exact coverage.
    pub(super) fn coverage_exact(&self) -> bool {
        self.all_exact(|_| true)
    }

    /// Whether every exported test on the path has exact coverage.
    pub(super) fn exported_coverage_exact(&self) -> bool {
        self.all_exact(|link| link.kind.is_exported())
    }

    fn all_exact(&self, considered: impl Fn(&PathLink) -> bool) -> bool {
        let mut cursor = self.head.as_deref();
        while let Some(link) = cursor {
            if considered(link) && link.edge.test.coverage != Coverage::Exact {
                return false;
            }
            cursor = link.parent.head.as_deref();
        }
        true
    }

    /// The longest shared prefix and the two tails after it.
    pub(super) fn diverge<'a>(&'a self, other: &'a Self) -> Divergence<'a> {
        let left = self.links();
        let right = other.links();
        let common = left
            .iter()
            .zip(&right)
            .take_while(|(left, right)| std::ptr::eq(**left, **right) || left.same_condition(right))
            .count();
        let mut prefix = self.clone();
        prefix.truncate(common);
        Divergence {
            common: prefix,
            left: left.get(common..).unwrap_or_default().to_vec(),
            right: right.get(common..).unwrap_or_default().to_vec(),
        }
    }

    /// Whether the first links of this path are `prefix`.
    pub(super) fn prefix_is(&self, prefix: &Self) -> bool {
        let mut head = self.clone();
        head.truncate(prefix.len());
        head == *prefix
    }

    /// The same conditions with the links at `depths` (1-based) re-kinded as `kind`.
    pub(super) fn relabeled(&self, depths: &[usize], kind: LinkKind) -> Self {
        let Some(lowest) = depths.iter().copied().min() else {
            return self.clone();
        };
        let links = self.links();
        let mut path = self.clone();
        path.truncate(lowest.saturating_sub(1));
        for link in links.get(lowest.saturating_sub(1)..).unwrap_or_default() {
            let link_kind = if depths.contains(&link.depth) {
                kind
            } else {
                link.kind
            };
            path.push(link.edge.clone(), link_kind);
        }
        path
    }
}

impl super::Arena {
    /// The runtime guard of the branches and survivals on the path: what feasibility and value
    /// refinement may assume.
    pub(super) fn path_guard(&self, path: &Path) -> RuntimeGuard {
        Self::link_guard(path, View::Feasibility, &mut |guard, edge| {
            self.conjoin_edge(guard, edge)
        })
    }

    /// The runtime guard exported with a fact: branches and assumptions, never survival.
    pub(super) fn exported_path_guard(&self, path: &Path) -> RuntimeGuard {
        Self::link_guard(path, View::Exported, &mut |guard, edge| {
            self.conjoin_edge(guard, edge)
        })
    }

    /// [`Self::path_guard`] while the decision store is already borrowed.
    pub(super) fn path_guard_in(&self, store: &mut DecisionStore, path: &Path) -> RuntimeGuard {
        Self::link_guard(path, View::Feasibility, &mut |guard, edge| {
            Self::conjoin_edge_in(store, guard, edge, &self.next_decision)
        })
    }

    /// [`Self::exported_path_guard`] while the decision store is already borrowed.
    pub(super) fn exported_path_guard_in(
        &self,
        store: &mut DecisionStore,
        path: &Path,
    ) -> RuntimeGuard {
        Self::link_guard(path, View::Exported, &mut |guard, edge| {
            Self::conjoin_edge_in(store, guard, edge, &self.next_decision)
        })
    }

    fn link_guard(
        path: &Path,
        view: View,
        conjoin: &mut dyn FnMut(&RuntimeGuard, &DecisionEdge) -> RuntimeGuard,
    ) -> RuntimeGuard {
        // Fill the memo from the deepest computed ancestor down, without recursion.
        let mut pending = Vec::new();
        let mut cursor = path.head.as_ref();
        let mut base = RuntimeGuard::True;
        while let Some(link) = cursor {
            let memo = if view == View::Exported {
                &link.exported
            } else {
                &link.guard
            };
            if let Some(guard) = memo.get() {
                base = guard.clone();
                break;
            }
            pending.push(link);
            cursor = link.parent.head.as_ref();
        }
        for link in pending.into_iter().rev() {
            let guard = if link.kind.kept(view) {
                conjoin(&base, &link.edge)
            } else {
                base
            };
            let memo = if view == View::Exported {
                &link.exported
            } else {
                &link.guard
            };
            base = memo.get_or_init(|| guard).clone();
        }
        base
    }
}

impl Path {
    /// The possible predicate of the exported links: the condition attached to recorded reads.
    pub(super) fn exported_predicate(&self) -> Predicate {
        self.predicate(View::Exported)
    }

    /// The possible predicate of every link: where the analysis claims that the point is
    /// reached.
    pub(super) fn whole_predicate(&self) -> Predicate {
        self.predicate(View::Whole)
    }

    /// The possible predicate of the branches and survivals: what feasibility may assume.
    pub(super) fn feasible_predicate(&self) -> Predicate {
        self.predicate(View::Feasibility)
    }

    fn predicate(&self, view: View) -> Predicate {
        let path = self;
        let Some(head) = path.head.as_ref() else {
            return Predicate::True;
        };
        let memo = match view {
            View::Exported => &head.exported_predicate,
            View::Feasibility => &head.feasible_predicate,
            View::Whole => &head.whole_predicate,
        };
        memo.get_or_init(|| {
            Predicate::all(
                path.links()
                    .into_iter()
                    .filter(|link| link.kind.kept(view))
                    .map(|link| link.edge.test.possible_predicate(link.edge.taken))
                    .collect(),
            )
        })
        .clone()
    }
}

/// Re-kinds as survival every branch link below the first `entry` links that no continuing
/// path left by its other outcome.
///
/// `paths` must be every continuation of the region that starts after `entry`: Helm rejects
/// every document that took a link's other outcome when none of them continues, so later facts
/// need not carry the link (ADR 0072 §2). A path that does not extend `entry` makes the set
/// incomplete, and then nothing is re-kinded.
///
/// Sibling subtrees describe disjoint documents only where they split on the two outcomes of one
/// test. Elsewhere (a joined link beside an unjoined exit, or more than two branches) a document
/// may continue through a sibling, so nothing below such a node is re-kinded.
pub(super) fn settle_survival(entry: &Path, paths: &mut [&mut Path]) {
    // A trie over the links below the entry: node ids, child counts and paths ending there.
    type Key = (usize, bool, LinkKind);
    let depth = entry.len();
    if paths
        .iter()
        .any(|path| path.len() < depth || !path.prefix_is(entry))
    {
        return;
    }
    let mut children = std::collections::HashMap::<(usize, Key), usize>::new();
    let mut fanout = vec![0_usize];
    let mut ends = vec![false];
    let mut keys = vec![Vec::<Key>::new()];
    let mut nodes = Vec::with_capacity(paths.len());
    for path in paths.iter() {
        let mut node = 0;
        let mut visited = Vec::new();
        for link in path.links().into_iter().skip(depth) {
            let key = (
                Rc::as_ptr(&link.edge.test) as usize,
                link.edge.taken,
                link.kind,
            );
            let next = fanout.len();
            let child = *children.entry((node, key)).or_insert(next);
            if child == next {
                fanout.push(0);
                ends.push(false);
                keys.push(Vec::new());
                if let Some(count) = fanout.get_mut(node) {
                    *count += 1;
                }
                if let Some(node_keys) = keys.get_mut(node) {
                    node_keys.push(key);
                }
            }
            visited.push(node);
            node = child;
        }
        if let Some(end) = ends.get_mut(node) {
            *end = true;
        }
        nodes.push(visited);
    }
    // A node is disjoint when no path ends above it and every split on the way to it is the two
    // outcomes of one test.
    let splits_one_test = |node: usize| {
        ends.get(node) == Some(&false)
            && match keys.get(node).map(Vec::as_slice) {
                Some([_] | []) => true,
                Some([(left, left_taken, _), (right, right_taken, _)]) => {
                    left == right && left_taken != right_taken
                }
                Some(_) | None => false,
            }
    };
    let disjoint = |visited: &[usize], index: usize| {
        visited
            .get(..index)
            .is_some_and(|ancestors| ancestors.iter().all(|node| splits_one_test(*node)))
    };
    for (path, visited) in paths.iter_mut().zip(nodes) {
        let links = path.links();
        let survived = links
            .iter()
            .skip(depth)
            .zip(&visited)
            .enumerate()
            .filter(|(index, (link, parent))| {
                link.kind == LinkKind::Branch
                    && fanout.get(**parent) == Some(&1)
                    && ends.get(**parent) == Some(&false)
                    && disjoint(&visited, *index)
            })
            .map(|(_, (link, _))| link.depth)
            .collect::<Vec<_>>();
        drop(links);
        if !survived.is_empty() {
            **path = path.relabeled(&survived, LinkKind::Survival);
        }
    }
}
