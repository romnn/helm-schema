use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};

use helm_schema_core::{Predicate, PredicateKind};

use super::{Arena, Coverage, DecisionEdge, FrozenTest};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ExactDecisionKey {
    pub(super) when_true: Predicate,
    pub(super) when_false: Predicate,
}

/// Runtime choices remain distinct even when their input predicates are unknown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RuntimeGuard {
    False,
    True,
    Branch(Rc<DecisionNode>),
}

#[derive(Debug)]
pub(super) struct DecisionNode {
    id: u64,
    test: Rc<FrozenTest>,
    yes: RuntimeGuard,
    no: RuntimeGuard,
    input_exact: bool,
}

impl PartialEq for DecisionNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for DecisionNode {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Operation {
    And,
    Or,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct NodeKey {
    decision: u64,
    yes: u64,
    no: u64,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ApplyKey {
    operation: Operation,
    left: u64,
    right: u64,
}

#[derive(Clone, Debug)]
struct Projection {
    necessary: Predicate,
    sufficient: Predicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ProjectionPurpose {
    Control,
    Budget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ProjectionKey {
    guard: u64,
    purpose: ProjectionPurpose,
}

struct GuardedUnion {
    decision: Rc<FrozenTest>,
    taken: bool,
    survival: RuntimeGuard,
}

#[derive(Debug)]
pub(super) enum CachedGuard {
    False,
    True,
    Branch(Weak<DecisionNode>),
}

impl CachedGuard {
    pub(super) fn new(guard: &RuntimeGuard) -> Self {
        match guard {
            RuntimeGuard::False => Self::False,
            RuntimeGuard::True => Self::True,
            RuntimeGuard::Branch(node) => Self::Branch(Rc::downgrade(node)),
        }
    }

    pub(super) fn upgrade(&self) -> Option<RuntimeGuard> {
        match self {
            Self::False => Some(RuntimeGuard::False),
            Self::True => Some(RuntimeGuard::True),
            Self::Branch(node) => node.upgrade().map(RuntimeGuard::Branch),
        }
    }
}

trait GuardWork {
    type Error;

    fn charge(&mut self) -> Result<(), Self::Error>;
}

struct ExactGuardWork;

impl GuardWork for ExactGuardWork {
    type Error = std::convert::Infallible;

    fn charge(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub(super) struct GuardWorkBudget {
    remaining: usize,
}

impl GuardWorkBudget {
    pub(super) fn new(limit: usize) -> Self {
        Self { remaining: limit }
    }
}

impl GuardWork for GuardWorkBudget {
    type Error = super::BoundReason;

    fn charge(&mut self) -> Result<(), Self::Error> {
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or(super::BoundReason::BudgetNodes)?;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(super) struct DecisionStore {
    next_node: u64,
    collection_limit: usize,
    working: Vec<Rc<DecisionNode>>,
    nodes: BTreeMap<NodeKey, Weak<DecisionNode>>,
    applications: BTreeMap<ApplyKey, CachedGuard>,
    negations: BTreeMap<u64, CachedGuard>,
    projections: BTreeMap<ProjectionKey, Projection>,
    tests: BTreeMap<ProjectionKey, Weak<FrozenTest>>,
}

impl RuntimeGuard {
    pub(super) fn is_input_exact(&self) -> bool {
        match self {
            Self::False | Self::True => true,
            Self::Branch(node) => node.input_exact,
        }
    }

    pub(super) fn is_impossible(&self) -> bool {
        matches!(self, Self::False)
    }

    pub(super) fn implies_test(&self, test: &FrozenTest) -> Option<bool> {
        let compatible = |outcome| match &test.runtime {
            Some(other) => self.compatible(true, other, outcome),
            None => self.possible_atom(test.id, outcome, true),
        };
        if !compatible(false) {
            Some(true)
        } else if !compatible(true) {
            Some(false)
        } else {
            None
        }
    }

    pub(super) fn id(&self) -> u64 {
        match self {
            Self::False => 0,
            Self::True => 1,
            Self::Branch(node) => node.id,
        }
    }

    fn decision(&self) -> Option<&Rc<FrozenTest>> {
        match self {
            Self::False | Self::True => None,
            Self::Branch(node) => Some(&node.test),
        }
    }

    fn arm(&self, decision: u64, taken: bool) -> Self {
        match self {
            Self::Branch(node) if node.test.id == decision => {
                if taken {
                    node.yes.clone()
                } else {
                    node.no.clone()
                }
            }
            Self::False | Self::True | Self::Branch(_) => self.clone(),
        }
    }

    fn possible_atom(&self, decision: u64, taken: bool, outcome: bool) -> bool {
        let mut pending = vec![self];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(guard) = pending.pop() {
            if !visited.insert(guard.id()) {
                continue;
            }
            match guard {
                Self::False if !outcome => return true,
                Self::True if outcome => return true,
                Self::False | Self::True => {}
                Self::Branch(node) if node.test.id == decision => {
                    pending.push(if taken { &node.yes } else { &node.no });
                }
                Self::Branch(node) => {
                    pending.extend([&node.yes, &node.no]);
                }
            }
        }
        false
    }

    fn compatible(&self, outcome: bool, other: &Self, other_outcome: bool) -> bool {
        let mut pending = vec![(self, other)];
        let mut visited = std::collections::BTreeSet::new();
        while let Some((left, right)) = pending.pop() {
            if !visited.insert((left.id(), right.id())) {
                continue;
            }
            let left_constant = match left {
                Self::False => Some(false),
                Self::True => Some(true),
                Self::Branch(_) => None,
            };
            let right_constant = match right {
                Self::False => Some(false),
                Self::True => Some(true),
                Self::Branch(_) => None,
            };
            if left_constant.is_some_and(|value| value != outcome)
                || right_constant.is_some_and(|value| value != other_outcome)
            {
                continue;
            }
            match (left, right) {
                (Self::False | Self::True, Self::False | Self::True) => return true,
                (Self::Branch(left), Self::Branch(right)) if left.test.id == right.test.id => {
                    pending.extend([(&left.yes, &right.yes), (&left.no, &right.no)]);
                }
                (Self::Branch(node), Self::Branch(other)) if node.test.id > other.test.id => {
                    pending.extend([(&node.yes, right), (&node.no, right)]);
                }
                (Self::Branch(node), Self::False | Self::True) => {
                    pending.extend([(&node.yes, right), (&node.no, right)]);
                }
                (Self::False | Self::True | Self::Branch(_), Self::Branch(node)) => {
                    pending.extend([(left, &node.yes), (left, &node.no)]);
                }
            }
        }
        false
    }
}

impl FrozenTest {
    pub(super) fn possible_predicate(&self, taken: bool) -> Predicate {
        if self.coverage == Coverage::Exact {
            if taken {
                self.when_true.clone()
            } else {
                self.when_false.clone()
            }
        } else if self.runtime.is_none() {
            Predicate::True
        } else if taken {
            self.when_false.clone().negated()
        } else {
            self.when_true.clone().negated()
        }
    }

    pub(super) fn implies_outcome(&self, taken: bool, other: &Self) -> Option<bool> {
        if self.id == other.id {
            return Some(taken);
        }
        let compatible = |outcome| match (&self.runtime, &other.runtime) {
            (Some(left), Some(right)) => left.compatible(taken, right, outcome),
            (Some(left), None) => left.possible_atom(other.id, outcome, taken),
            (None, Some(right)) => right.possible_atom(self.id, taken, outcome),
            (None, None) => true,
        };
        if !compatible(false) {
            Some(true)
        } else if !compatible(true) {
            Some(false)
        } else {
            None
        }
    }
}

impl DecisionStore {
    fn collect_unused(&mut self) {
        if self.nodes.len() < self.collection_limit.max(4096) {
            return;
        }
        // Only execution handles keep guards alive across collection.
        self.working.clear();
        self.nodes.retain(|_, node| node.strong_count() != 0);
        let live: BTreeSet<_> = [0, 1]
            .into_iter()
            .chain(
                self.nodes
                    .values()
                    .filter_map(Weak::upgrade)
                    .map(|node| node.id),
            )
            .collect();
        self.applications.retain(|key, result| {
            live.contains(&key.left) && live.contains(&key.right) && result.upgrade().is_some()
        });
        self.negations
            .retain(|guard, result| live.contains(guard) && result.upgrade().is_some());
        self.projections.retain(|key, _| live.contains(&key.guard));
        self.tests
            .retain(|key, test| live.contains(&key.guard) && test.strong_count() != 0);
        self.collection_limit = self.nodes.len().saturating_mul(2).max(4096);
    }

    fn select(
        &mut self,
        test: Rc<FrozenTest>,
        yes: RuntimeGuard,
        no: RuntimeGuard,
    ) -> RuntimeGuard {
        match self.select_with_work(test, yes, no, &mut ExactGuardWork) {
            Ok(guard) => guard,
            Err(never) => match never {},
        }
    }

    fn select_with_work<W: GuardWork>(
        &mut self,
        test: Rc<FrozenTest>,
        yes: RuntimeGuard,
        no: RuntimeGuard,
        work: &mut W,
    ) -> Result<RuntimeGuard, W::Error> {
        if yes == no {
            return Ok(yes);
        }
        let key = NodeKey {
            decision: test.id,
            yes: yes.id(),
            no: no.id(),
        };
        if let Some(node) = self.nodes.get(&key).and_then(Weak::upgrade) {
            return Ok(RuntimeGuard::Branch(node));
        }
        work.charge()?;
        let id = self.next_node + 2;
        self.next_node += 1;
        let input_exact =
            test.coverage == Coverage::Exact && yes.is_input_exact() && no.is_input_exact();
        let node = Rc::new(DecisionNode {
            id,
            test,
            yes,
            no,
            input_exact,
        });
        // Keep intermediate results reusable until collection, including throughout recursive apply.
        self.working.push(Rc::clone(&node));
        self.nodes.insert(key, Rc::downgrade(&node));
        Ok(RuntimeGuard::Branch(node))
    }

    fn negate(&mut self, guard: &RuntimeGuard) -> RuntimeGuard {
        match self.negate_with_work(guard, &mut ExactGuardWork) {
            Ok(guard) => guard,
            Err(never) => match never {},
        }
    }

    fn negate_with_work<W: GuardWork>(
        &mut self,
        guard: &RuntimeGuard,
        work: &mut W,
    ) -> Result<RuntimeGuard, W::Error> {
        if let Some(negated) = self
            .negations
            .get(&guard.id())
            .and_then(CachedGuard::upgrade)
        {
            return Ok(negated);
        }
        work.charge()?;
        let result = match guard {
            RuntimeGuard::False => RuntimeGuard::True,
            RuntimeGuard::True => RuntimeGuard::False,
            RuntimeGuard::Branch(node) => {
                let yes = self.negate_with_work(&node.yes, work)?;
                let no = self.negate_with_work(&node.no, work)?;
                self.select_with_work(Rc::clone(&node.test), yes, no, work)?
            }
        };
        self.negations.insert(guard.id(), CachedGuard::new(&result));
        self.negations.insert(result.id(), CachedGuard::new(guard));
        Ok(result)
    }

    fn apply(
        &mut self,
        operation: Operation,
        left: &RuntimeGuard,
        right: &RuntimeGuard,
    ) -> RuntimeGuard {
        match self.apply_with_work(operation, left, right, &mut ExactGuardWork) {
            Ok(guard) => guard,
            Err(never) => match never {},
        }
    }

    fn apply_with_work<W: GuardWork>(
        &mut self,
        operation: Operation,
        left: &RuntimeGuard,
        right: &RuntimeGuard,
        work: &mut W,
    ) -> Result<RuntimeGuard, W::Error> {
        if left == right {
            return Ok(left.clone());
        }
        match operation {
            Operation::And => {
                if matches!(left, RuntimeGuard::False) || matches!(right, RuntimeGuard::False) {
                    return Ok(RuntimeGuard::False);
                }
                if matches!(left, RuntimeGuard::True) {
                    return Ok(right.clone());
                }
                if matches!(right, RuntimeGuard::True) {
                    return Ok(left.clone());
                }
            }
            Operation::Or => {
                if matches!(left, RuntimeGuard::True) || matches!(right, RuntimeGuard::True) {
                    return Ok(RuntimeGuard::True);
                }
                if matches!(left, RuntimeGuard::False) {
                    return Ok(right.clone());
                }
                if matches!(right, RuntimeGuard::False) {
                    return Ok(left.clone());
                }
            }
        }
        let key = ApplyKey {
            operation,
            left: left.id().min(right.id()),
            right: left.id().max(right.id()),
        };
        if let Some(result) = self.applications.get(&key).and_then(CachedGuard::upgrade) {
            return Ok(result);
        }
        work.charge()?;
        // New decisions sort first, keeping independent appended conditions shallow to update.
        let Some(test) = left
            .decision()
            .into_iter()
            .chain(right.decision())
            .max_by_key(|test| test.id)
        else {
            return Ok(RuntimeGuard::False);
        };
        let yes = self.apply_with_work(
            operation,
            &left.arm(test.id, true),
            &right.arm(test.id, true),
            work,
        )?;
        let no = self.apply_with_work(
            operation,
            &left.arm(test.id, false),
            &right.arm(test.id, false),
            work,
        )?;
        let result = self.select_with_work(Rc::clone(test), yes, no, work)?;
        self.applications.insert(key, CachedGuard::new(&result));
        Ok(result)
    }

    fn choice_with_work<W: GuardWork>(
        &mut self,
        condition: &RuntimeGuard,
        yes: &RuntimeGuard,
        no: &RuntimeGuard,
        work: &mut W,
    ) -> Result<RuntimeGuard, W::Error> {
        if condition.is_impossible() {
            return Ok(no.clone());
        }
        if condition == &RuntimeGuard::True || yes == no {
            return Ok(yes.clone());
        }
        if no.is_impossible() {
            return self.apply_with_work(Operation::And, condition, yes, work);
        }
        if yes == &RuntimeGuard::True {
            return self.apply_with_work(Operation::Or, condition, no, work);
        }
        let opposite = self.negate_with_work(condition, work)?;
        if yes.is_impossible() {
            return self.apply_with_work(Operation::And, &opposite, no, work);
        }
        if no == &RuntimeGuard::True {
            return self.apply_with_work(Operation::Or, &opposite, yes, work);
        }
        let selected_yes = self.apply_with_work(Operation::And, condition, yes, work)?;
        let selected_no = self.apply_with_work(Operation::And, &opposite, no, work)?;
        self.apply_with_work(Operation::Or, &selected_yes, &selected_no, work)
    }

    fn edge(&mut self, edge: &DecisionEdge) -> RuntimeGuard {
        let guard = if let Some(guard) = &edge.test.runtime {
            guard.clone()
        } else if let Some(taken) = edge.test.constant_outcome() {
            if taken {
                RuntimeGuard::True
            } else {
                RuntimeGuard::False
            }
        } else {
            self.select(
                Rc::clone(&edge.test),
                RuntimeGuard::True,
                RuntimeGuard::False,
            )
        };
        if edge.taken {
            guard
        } else {
            self.negate(&guard)
        }
    }

    fn conjunction(&mut self, edges: &[DecisionEdge]) -> RuntimeGuard {
        let mut result = RuntimeGuard::True;
        for edge in edges {
            let next = self.edge(edge);
            result = self.apply(Operation::And, &result, &next);
        }
        result
    }

    #[tracing::instrument(skip_all)]
    fn project(&mut self, guard: &RuntimeGuard, purpose: ProjectionPurpose) -> Projection {
        let key = ProjectionKey {
            guard: guard.id(),
            purpose,
        };
        if let Some(projection) = self.projections.get(&key) {
            return projection.clone();
        }
        let result = match guard {
            RuntimeGuard::False => Projection {
                necessary: Predicate::False,
                sufficient: Predicate::False,
            },
            RuntimeGuard::True => Projection {
                necessary: Predicate::True,
                sufficient: Predicate::True,
            },
            RuntimeGuard::Branch(node) => {
                let yes = self.project(&node.yes, purpose);
                let no = self.project(&node.no, purpose);
                match node.test.coverage {
                    Coverage::Exact => {
                        let exact =
                            yes.necessary == yes.sufficient && no.necessary == no.sufficient;
                        let necessary = project_union(
                            vec![
                                Predicate::all(vec![node.test.when_true.clone(), yes.necessary]),
                                Predicate::all(vec![node.test.when_false.clone(), no.necessary]),
                            ],
                            purpose,
                        );
                        // An exact decision partitions its domain. Adding both child proofs
                        // again is redundant and doubles expressions after normalization caps.
                        let sufficient = if exact {
                            necessary.clone().unwrap_or(Predicate::False)
                        } else {
                            project_union(
                                vec![
                                    Predicate::all(vec![
                                        node.test.when_true.clone(),
                                        yes.sufficient,
                                    ]),
                                    Predicate::all(vec![
                                        node.test.when_false.clone(),
                                        no.sufficient,
                                    ]),
                                ],
                                purpose,
                            )
                            .unwrap_or(Predicate::False)
                        };
                        Projection {
                            necessary: necessary.unwrap_or(Predicate::True),
                            sufficient,
                        }
                    }
                    Coverage::Partial => Projection {
                        necessary: project_union(vec![yes.necessary, no.necessary], purpose)
                            .unwrap_or(Predicate::True),
                        sufficient: bounded_projection(
                            Predicate::all(vec![yes.sufficient, no.sufficient]),
                            purpose,
                        )
                        .unwrap_or(Predicate::False),
                    },
                }
            }
        };
        self.projections.insert(key, result.clone());
        result
    }
}

fn bounded_projection(predicate: Predicate, purpose: ProjectionPurpose) -> Option<Predicate> {
    if purpose == ProjectionPurpose::Control {
        return Some(predicate);
    }
    // Budget counters can encode exponential cardinality formulas after otherwise ordinary joins.
    // Bound only their exported syntax; runtime guards and semantic control proofs stay exact.
    let mut pending = vec![&predicate];
    let mut terms = 0;
    while let Some(predicate) = pending.pop() {
        terms += 1;
        if terms > 256 {
            tracing::debug!(reason = ?super::BoundReason::BudgetPredicate, "widen budget guard projection");
            return None;
        }
        match predicate.kind() {
            PredicateKind::And(parts) | PredicateKind::Or(parts) => pending.extend(parts),
            PredicateKind::Not(inner) => pending.push(inner),
            PredicateKind::Approximate {
                marker: _,
                paths: _,
                role: _,
                sound_subset,
            } => {
                pending.extend(sound_subset);
            }
            PredicateKind::True | PredicateKind::False | PredicateKind::Guard(_) => {}
        }
    }
    Some(predicate)
}

fn project_union(mut parts: Vec<Predicate>, purpose: ProjectionPurpose) -> Option<Predicate> {
    parts.retain(|part| part != &Predicate::False);
    let terms: Vec<std::collections::BTreeSet<_>> = parts
        .iter()
        .map(|part| match part.kind() {
            PredicateKind::And(parts) => parts.iter().cloned().collect(),
            PredicateKind::True
            | PredicateKind::False
            | PredicateKind::Guard(_)
            | PredicateKind::Not(_)
            | PredicateKind::Or(_)
            | PredicateKind::Approximate { .. } => std::collections::BTreeSet::from([part.clone()]),
        })
        .collect();
    let Some(first) = terms.first() else {
        return Some(Predicate::False);
    };
    let common = terms.iter().skip(1).fold(first.clone(), |common, term| {
        common.intersection(term).cloned().collect()
    });
    if common.is_empty() {
        return bounded_projection(Predicate::Or(parts), purpose)
            .and_then(|value| bounded_projection(value.normalize_boolean(), purpose));
    }
    // Retain a shared sequential prefix instead of distributing it over both decision arms.
    let suffix = bounded_projection(
        Predicate::Or(
            terms
                .iter()
                .map(|term| Predicate::all(term.difference(&common).cloned().collect()))
                .collect(),
        ),
        purpose,
    )?
    .normalize_boolean();
    bounded_projection(
        Predicate::all(common.into_iter().chain([suffix]).collect()),
        purpose,
    )
}

impl Arena {
    #[tracing::instrument(skip_all)]
    pub(super) fn join_paths(
        &self,
        left: &[DecisionEdge],
        right: &[DecisionEdge],
    ) -> super::state::ContinuationJoin {
        let common = left
            .iter()
            .zip(right)
            .take_while(|(left, right)| left.test == right.test && left.taken == right.taken)
            .count();
        let mut active = left.iter().take(common).cloned().collect::<Vec<_>>();
        let left_tail = left.get(common..).unwrap_or_default();
        let right_tail = right.get(common..).unwrap_or_default();
        let mut store = self.guards.borrow_mut();
        let left_guard = store.conjunction(left_tail);
        let right_guard = store.conjunction(right_tail);
        let opposite = left_tail
            .first()
            .zip(right_tail.first())
            .filter(|(left, right)| left.test == right.test && left.taken != right.taken);
        let union = if let Some((left, _)) = opposite {
            let survival = store.apply(Operation::Or, &left_guard, &right_guard);
            GuardedUnion {
                decision: Rc::clone(&left.test),
                taken: left.taken,
                survival,
            }
        } else if store
            .apply(Operation::And, &left_guard, &right_guard)
            .is_impossible()
        {
            let decision =
                self.guard_test(&mut store, left_guard.clone(), ProjectionPurpose::Control);
            let survival = store.apply(Operation::Or, &left_guard, &right_guard);
            GuardedUnion {
                decision,
                taken: true,
                survival,
            }
        } else {
            // Overlapping abstract paths can carry different outcomes for the same input.
            // Keep both using an independent runtime choice, gated by each source path.
            let decision = self.decision(Predicate::False, Predicate::False, Coverage::Partial);
            let choice = store.edge(&DecisionEdge {
                test: Rc::clone(&decision),
                taken: true,
            });
            let opposite = store.negate(&choice);
            let yes = store.apply(Operation::And, &choice, &left_guard);
            let no = store.apply(Operation::And, &opposite, &right_guard);
            let survival = store.apply(Operation::Or, &yes, &no);
            GuardedUnion {
                decision,
                taken: true,
                survival,
            }
        };
        let GuardedUnion {
            decision,
            taken,
            survival,
        } = union;
        let known = store.conjunction(&active);
        let success = self.guard_test(&mut store, survival, ProjectionPurpose::Control);
        if success.constant_outcome() != Some(true) && known.implies_test(&success) != Some(true) {
            active.push(DecisionEdge {
                test: success,
                taken: true,
            });
        }
        super::state::ContinuationJoin {
            decision,
            active,
            taken,
        }
    }

    pub(super) fn choose_test(
        &self,
        decision: Rc<FrozenTest>,
        yes: Rc<FrozenTest>,
        no: Rc<FrozenTest>,
    ) -> Rc<FrozenTest> {
        self.surviving_decision(
            &[
                DecisionEdge {
                    test: Rc::clone(&decision),
                    taken: true,
                },
                DecisionEdge {
                    test: yes,
                    taken: true,
                },
            ],
            &[
                DecisionEdge {
                    test: decision,
                    taken: false,
                },
                DecisionEdge {
                    test: no,
                    taken: true,
                },
            ],
        )
    }

    pub(super) fn surviving_decision(
        &self,
        yes: &[DecisionEdge],
        no: &[DecisionEdge],
    ) -> Rc<FrozenTest> {
        let mut store = self.guards.borrow_mut();
        let yes = store.conjunction(yes);
        let no = store.conjunction(no);
        let runtime = store.apply(Operation::Or, &yes, &no);
        self.guard_test(&mut store, runtime, ProjectionPurpose::Control)
    }

    pub(super) fn runtime_path(&self, edges: &[DecisionEdge]) -> RuntimeGuard {
        self.guards.borrow_mut().conjunction(edges)
    }

    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::guard_storage",
        fields(nodes = store.nodes.len(), applications = store.applications.len(), projections = store.projections.len(), tests = store.tests.len())
    )]
    fn guard_test(
        &self,
        store: &mut DecisionStore,
        runtime: RuntimeGuard,
        purpose: ProjectionPurpose,
    ) -> Rc<FrozenTest> {
        store.collect_unused();
        let key = ProjectionKey {
            guard: runtime.id(),
            purpose,
        };
        if let Some(test) = store.tests.get(&key).and_then(Weak::upgrade) {
            return test;
        }
        let Projection {
            necessary,
            sufficient,
        } = store.project(&runtime, purpose);
        let coverage = if necessary == sufficient {
            Coverage::Exact
        } else {
            Coverage::Partial
        };
        let id = self.next_decision.get();
        self.next_decision.set(id + 1);
        let test = Rc::new(FrozenTest {
            id,
            when_true: sufficient,
            when_false: necessary.negated(),
            coverage,
            runtime: Some(runtime),
        });
        store.tests.insert(key, Rc::downgrade(&test));
        test
    }
}

impl Arena {
    pub(super) fn select_runtime_guard_bounded(
        &self,
        condition: &RuntimeGuard,
        yes: &RuntimeGuard,
        no: &RuntimeGuard,
        work: &mut GuardWorkBudget,
    ) -> Result<RuntimeGuard, super::BoundReason> {
        self.guards
            .borrow_mut()
            .choice_with_work(condition, yes, no, work)
    }

    pub(super) fn union_runtime_guards(
        &self,
        left: &RuntimeGuard,
        right: &RuntimeGuard,
    ) -> RuntimeGuard {
        self.guards.borrow_mut().apply(Operation::Or, left, right)
    }

    pub(super) fn intersect_runtime_guards(
        &self,
        left: &RuntimeGuard,
        right: &RuntimeGuard,
    ) -> RuntimeGuard {
        self.guards.borrow_mut().apply(Operation::And, left, right)
    }

    pub(super) fn budget_test(&self, guard: RuntimeGuard) -> Rc<FrozenTest> {
        self.guard_test(
            &mut self.guards.borrow_mut(),
            guard,
            ProjectionPurpose::Budget,
        )
    }
}

#[cfg(test)]
#[path = "../tests/runtime_budget_work.rs"]
mod tests;
