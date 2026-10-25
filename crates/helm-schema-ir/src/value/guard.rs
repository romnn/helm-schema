use std::collections::BTreeMap;
use std::rc::Rc;

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

struct GuardedUnion {
    decision: Rc<FrozenTest>,
    taken: bool,
    survival: RuntimeGuard,
}

#[derive(Debug, Default)]
pub(super) struct DecisionStore {
    nodes: BTreeMap<NodeKey, RuntimeGuard>,
    applications: BTreeMap<ApplyKey, RuntimeGuard>,
    negations: BTreeMap<u64, RuntimeGuard>,
    projections: BTreeMap<u64, Projection>,
    tests: BTreeMap<u64, Rc<FrozenTest>>,
}

impl RuntimeGuard {
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

    fn id(&self) -> u64 {
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
    fn select(
        &mut self,
        test: Rc<FrozenTest>,
        yes: RuntimeGuard,
        no: RuntimeGuard,
    ) -> RuntimeGuard {
        if yes == no {
            return yes;
        }
        let key = NodeKey {
            decision: test.id,
            yes: yes.id(),
            no: no.id(),
        };
        let id = self.nodes.len() as u64 + 2;
        self.nodes
            .entry(key)
            .or_insert_with(|| RuntimeGuard::Branch(Rc::new(DecisionNode { id, test, yes, no })))
            .clone()
    }

    fn negate(&mut self, guard: &RuntimeGuard) -> RuntimeGuard {
        if let Some(negated) = self.negations.get(&guard.id()) {
            return negated.clone();
        }
        let result = match guard {
            RuntimeGuard::False => RuntimeGuard::True,
            RuntimeGuard::True => RuntimeGuard::False,
            RuntimeGuard::Branch(node) => {
                let yes = self.negate(&node.yes);
                let no = self.negate(&node.no);
                self.select(Rc::clone(&node.test), yes, no)
            }
        };
        self.negations.insert(guard.id(), result.clone());
        self.negations.insert(result.id(), guard.clone());
        result
    }

    fn apply(
        &mut self,
        operation: Operation,
        left: &RuntimeGuard,
        right: &RuntimeGuard,
    ) -> RuntimeGuard {
        if left == right {
            return left.clone();
        }
        match operation {
            Operation::And => {
                if matches!(left, RuntimeGuard::False) || matches!(right, RuntimeGuard::False) {
                    return RuntimeGuard::False;
                }
                if matches!(left, RuntimeGuard::True) {
                    return right.clone();
                }
                if matches!(right, RuntimeGuard::True) {
                    return left.clone();
                }
            }
            Operation::Or => {
                if matches!(left, RuntimeGuard::True) || matches!(right, RuntimeGuard::True) {
                    return RuntimeGuard::True;
                }
                if matches!(left, RuntimeGuard::False) {
                    return right.clone();
                }
                if matches!(right, RuntimeGuard::False) {
                    return left.clone();
                }
            }
        }
        let key = ApplyKey {
            operation,
            left: left.id().min(right.id()),
            right: left.id().max(right.id()),
        };
        if let Some(result) = self.applications.get(&key) {
            return result.clone();
        }
        // New decisions sort first, keeping independent appended conditions shallow to update.
        let Some(test) = left
            .decision()
            .into_iter()
            .chain(right.decision())
            .max_by_key(|test| test.id)
        else {
            return RuntimeGuard::False;
        };
        let yes = self.apply(
            operation,
            &left.arm(test.id, true),
            &right.arm(test.id, true),
        );
        let no = self.apply(
            operation,
            &left.arm(test.id, false),
            &right.arm(test.id, false),
        );
        let result = self.select(Rc::clone(test), yes, no);
        self.applications.insert(key, result.clone());
        result
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
    fn project(&mut self, guard: &RuntimeGuard) -> Projection {
        if let Some(projection) = self.projections.get(&guard.id()) {
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
                let yes = self.project(&node.yes);
                let no = self.project(&node.no);
                match node.test.coverage {
                    Coverage::Exact => {
                        let exact =
                            yes.necessary == yes.sufficient && no.necessary == no.sufficient;
                        let necessary = project_union(vec![
                            Predicate::all(vec![node.test.when_true.clone(), yes.necessary]),
                            Predicate::all(vec![node.test.when_false.clone(), no.necessary]),
                        ]);
                        // An exact decision partitions its domain. Adding both child proofs
                        // again is redundant and doubles expressions after normalization caps.
                        let sufficient = if exact {
                            necessary.clone()
                        } else {
                            project_union(vec![
                                Predicate::all(vec![node.test.when_true.clone(), yes.sufficient]),
                                Predicate::all(vec![node.test.when_false.clone(), no.sufficient]),
                            ])
                        };
                        Projection {
                            necessary,
                            sufficient,
                        }
                    }
                    Coverage::Partial => Projection {
                        necessary: project_union(vec![yes.necessary, no.necessary]),
                        sufficient: Predicate::all(vec![yes.sufficient, no.sufficient]),
                    },
                }
            }
        };
        self.projections.insert(guard.id(), result.clone());
        result
    }
}

fn project_union(mut parts: Vec<Predicate>) -> Predicate {
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
        return Predicate::False;
    };
    let common = terms.iter().skip(1).fold(first.clone(), |common, term| {
        common.intersection(term).cloned().collect()
    });
    if common.is_empty() {
        return Predicate::Or(parts).normalize_boolean();
    }
    // Retain a shared sequential prefix instead of distributing it over both decision arms.
    let suffix = Predicate::Or(
        terms
            .iter()
            .map(|term| Predicate::all(term.difference(&common).cloned().collect()))
            .collect(),
    )
    .normalize_boolean();
    Predicate::all(common.into_iter().chain([suffix]).collect())
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
        let mut active = left[..common].to_vec();
        let left_tail = &left[common..];
        let right_tail = &right[common..];
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
            let decision = self.guard_test(&mut store, left_guard.clone());
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
        let success = self.guard_test(&mut store, survival);
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
        self.guard_test(&mut store, runtime)
    }

    pub(super) fn runtime_path(&self, edges: &[DecisionEdge]) -> RuntimeGuard {
        self.guards.borrow_mut().conjunction(edges)
    }

    fn guard_test(&self, store: &mut DecisionStore, runtime: RuntimeGuard) -> Rc<FrozenTest> {
        if let Some(test) = store.tests.get(&runtime.id()) {
            return Rc::clone(test);
        }
        let Projection {
            necessary,
            sufficient,
        } = store.project(&runtime);
        let coverage = if necessary == sufficient {
            Coverage::Exact
        } else {
            Coverage::Partial
        };
        let id = self.next_decision.get();
        self.next_decision.set(id + 1);
        let guard_id = runtime.id();
        let test = Rc::new(FrozenTest {
            id,
            when_true: sufficient,
            when_false: necessary.negated(),
            coverage,
            runtime: Some(runtime),
        });
        store.tests.insert(guard_id, Rc::clone(&test));
        test
    }
}
