use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use ahash::{AHashMap, AHashSet};
use helm_schema_core::{Conjunction, Predicate, PredicateKind};

use super::{Arena, Coverage, DecisionEdge, FrozenTest};

#[derive(Debug, PartialEq, Eq, Hash)]
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
    alias_free: bool,
    queries: Rc<GuardQueries>,
    retained_work: Cell<Option<usize>>,
}

impl Drop for DecisionNode {
    fn drop(&mut self) {
        self.queries.conjunctions_dirty.set(true);
        let mut pending = Vec::new();
        self.take_children(&mut pending);
        while let Some(RuntimeGuard::Branch(node)) = pending.pop() {
            if let Ok(mut node) = Rc::try_unwrap(node) {
                node.take_children(&mut pending);
            }
        }
    }
}

impl DecisionNode {
    fn take_children(&mut self, pending: &mut Vec<RuntimeGuard>) {
        for guard in [
            std::mem::replace(&mut self.yes, RuntimeGuard::False),
            std::mem::replace(&mut self.no, RuntimeGuard::False),
        ] {
            if matches!(&guard, RuntimeGuard::Branch(_)) {
                pending.push(guard);
            }
        }
        if self.test.runtime.is_some()
            && Rc::strong_count(&self.test) == 1
            && let Some(guard) = Rc::make_mut(&mut self.test).runtime.take()
            && matches!(&guard, RuntimeGuard::Branch(_))
        {
            pending.push(guard);
        }
    }
}

impl PartialEq for DecisionNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for DecisionNode {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Operation {
    And,
    Or,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NodeKey {
    decision: u64,
    yes: u64,
    no: u64,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ApplyKey {
    operation: Operation,
    left: u64,
    right: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ChoiceKey {
    condition: u64,
    yes: u64,
    no: u64,
}

#[derive(Clone, Debug)]
struct Projection {
    necessary: Predicate,
    sufficient: Predicate,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ProjectionUnionKey {
    parts: Vec<Predicate>,
}

const MAX_PROJECTION_UNIONS: usize = 4_096;
const MAX_PROJECTION_WORK: usize = 4_096;

#[derive(Debug, PartialEq, Eq, Hash)]
struct DecisionEdgeKey {
    decision: u64,
    taken: bool,
}

const MAX_CONJUNCTIONS: usize = 16;

#[derive(Debug, PartialEq, Eq, Hash)]
struct AtomQuery {
    guard: u64,
    decision: u64,
    taken: bool,
    outcome: bool,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct CompatibilityQuery {
    left: u64,
    right: u64,
    left_outcome: bool,
    right_outcome: bool,
}

const MAX_GUARD_QUERIES: usize = 4_096;
const MAX_GUARD_MEMO_ENTRIES: usize = 32_768;

// Node IDs never repeat within this context; Boolean results retain no execution handles.
#[derive(Default)]
struct GuardQueries {
    atoms: RefCell<AHashMap<AtomQuery, bool>>,
    compatibilities: RefCell<AHashMap<CompatibilityQuery, bool>>,
    conjunctions_dirty: Cell<bool>,
}

impl std::fmt::Debug for GuardQueries {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GuardQueries")
            .finish_non_exhaustive()
    }
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

    pub(super) fn consume(&mut self, amount: usize) -> Result<(), super::BoundReason> {
        let Some(remaining) = self.remaining.checked_sub(amount) else {
            self.remaining = 0;
            return Err(super::BoundReason::BudgetNodes);
        };
        self.remaining = remaining;
        Ok(())
    }

    // Cached results still retain their full graph.
    pub(super) fn include<'a>(
        &mut self,
        guards: impl IntoIterator<Item = &'a RuntimeGuard>,
    ) -> Result<usize, super::BoundReason> {
        let mut guards = guards
            .into_iter()
            .filter(|guard| matches!(guard, RuntimeGuard::Branch(_)));
        let root = guards.next();
        let second = guards.next();
        if second.is_none()
            && let Some(RuntimeGuard::Branch(node)) = root
            && let Some(work) = node.retained_work.get()
        {
            self.consume(work)?;
            return Ok(work);
        }
        let mut pending = root
            .into_iter()
            .chain(second)
            .chain(guards)
            .collect::<Vec<_>>();
        let mut visited = AHashSet::new();
        while let Some(guard) = pending.pop() {
            let RuntimeGuard::Branch(node) = guard else {
                continue;
            };
            let key = Rc::as_ptr(node);
            if visited.contains(&key) {
                continue;
            }
            self.charge()?;
            visited.insert(key);
            pending.extend([&node.yes, &node.no]);
            if let Some(runtime) = &node.test.runtime {
                pending.push(runtime);
            }
        }
        if second.is_none()
            && let Some(RuntimeGuard::Branch(node)) = root
        {
            node.retained_work.set(Some(visited.len()));
        }
        Ok(visited.len())
    }
}

impl GuardWork for GuardWorkBudget {
    type Error = super::BoundReason;

    fn charge(&mut self) -> Result<(), Self::Error> {
        self.consume(1)
    }
}

#[derive(Debug, Default)]
pub(super) struct DecisionStore {
    next_node: u64,
    collection_limit: usize,
    working: Vec<Rc<DecisionNode>>,
    nodes: AHashMap<NodeKey, Weak<DecisionNode>>,
    applications: AHashMap<ApplyKey, CachedGuard>,
    choices: AHashMap<ChoiceKey, CachedGuard>,
    negations: AHashMap<u64, CachedGuard>,
    projections: AHashMap<u64, Projection>,
    projection_unions: AHashMap<ProjectionUnionKey, Predicate>,
    tests: AHashMap<u64, Weak<FrozenTest>>,
    conjunctions: Vec<(Vec<DecisionEdgeKey>, CachedGuard)>,
    queries: Rc<GuardQueries>,
}

#[derive(Clone, Copy)]
struct Goal<'a> {
    guard: &'a RuntimeGuard,
    value: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct GuardProofKey {
    context: usize,
    id: u64,
    value: bool,
}
#[derive(Clone)]
struct Search<'a> {
    pending: Vec<Goal<'a>>,
    bindings: AHashMap<GuardProofKey, bool>,
    expanded: AHashSet<GuardProofKey>,
}

impl<'a> Search<'a> {
    fn enter(&mut self, node: &'a DecisionNode, value: bool, taken: bool) {
        if let Some(runtime) = &node.test.runtime {
            self.pending.push(Goal {
                guard: runtime,
                value: taken,
            });
        } else {
            self.bindings.insert(
                GuardProofKey {
                    context: Rc::as_ptr(&node.queries) as usize,
                    id: node.test.id,
                    value: false,
                },
                taken,
            );
        }
        // A local contradiction can reject the branch before scanning ambient history.
        self.pending.push(Goal {
            guard: if taken { &node.yes } else { &node.no },
            value,
        });
    }
}

impl RuntimeGuard {
    fn is_alias_free(&self) -> bool {
        match self {
            Self::False | Self::True => true,
            Self::Branch(node) => node.alias_free,
        }
    }

    pub(super) fn is_input_exact(&self) -> bool {
        match self {
            Self::False | Self::True => true,
            Self::Branch(node) => node.input_exact,
        }
    }

    pub(super) fn is_impossible(&self) -> bool {
        match self {
            Self::False => true,
            Self::True => false,
            Self::Branch(node) if node.alias_free => false,
            Self::Branch(_) => self.compatible(true, &Self::True, true) == Some(false),
        }
    }

    pub(super) fn implies_test(&self, test: &FrozenTest) -> Option<bool> {
        let compatible = |outcome| match &test.runtime {
            Some(other) => self.compatible(true, other, outcome),
            None => self.possible_atom(test.id, outcome, true),
        };
        if compatible(false) == Some(false) {
            Some(true)
        } else if compatible(true) == Some(false) {
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

    fn possible_atom(&self, decision: u64, taken: bool, outcome: bool) -> Option<bool> {
        let Self::Branch(node) = self else {
            return Some(matches!(self, Self::True) == outcome);
        };
        let key = AtomQuery {
            guard: node.id,
            decision,
            taken,
            outcome,
        };
        if let Some(result) = node.queries.atoms.borrow().get(&key) {
            return Some(*result);
        }
        let result = self.possible_atom_uncached(decision, taken, outcome)?;
        let mut cache = node.queries.atoms.borrow_mut();
        if cache.len() >= MAX_GUARD_QUERIES {
            cache.clear();
        }
        cache.insert(key, result);
        Some(result)
    }

    fn possible_atom_uncached(&self, decision: u64, taken: bool, outcome: bool) -> Option<bool> {
        let mut work = GuardWorkBudget::new(MAX_PROJECTION_WORK);
        if let Self::Branch(root) = self
            && root.alias_free
        {
            let mut pending = vec![self];
            let mut visited = AHashSet::new();
            loop {
                let Some(guard) = pending.pop() else {
                    return Some(false);
                };
                work.consume(1).ok()?;
                match guard {
                    Self::False if !outcome => return Some(true),
                    Self::True if outcome => return Some(true),
                    Self::False | Self::True => {}
                    Self::Branch(node) if !Rc::ptr_eq(&root.queries, &node.queries) => break,
                    Self::Branch(node) if !visited.insert(Rc::as_ptr(node)) => {}
                    Self::Branch(node) if node.test.id == decision => {
                        pending.push(if taken { &node.yes } else { &node.no });
                    }
                    Self::Branch(node) => pending.extend([&node.yes, &node.no]),
                }
            }
        }
        self.satisfiable(
            &Self::True,
            outcome,
            true,
            Some(&DecisionEdgeKey { decision, taken }),
            &mut work,
        )
    }

    pub(super) fn overlaps(&self, other: &Self) -> bool {
        self.compatible(true, other, true).unwrap_or(true)
    }

    fn compatible(&self, outcome: bool, other: &Self, other_outcome: bool) -> Option<bool> {
        let queries = match (self, other) {
            (Self::Branch(left), Self::Branch(right)) => {
                if !Rc::ptr_eq(&left.queries, &right.queries) {
                    return self.compatible_uncached(outcome, other, other_outcome);
                }
                &left.queries
            }
            (Self::Branch(node), Self::False | Self::True)
            | (Self::False | Self::True, Self::Branch(node)) => &node.queries,
            (Self::False | Self::True, Self::False | Self::True) => {
                return Some(
                    matches!(self, Self::True) == outcome
                        && matches!(other, Self::True) == other_outcome,
                );
            }
        };
        let key = CompatibilityQuery {
            left: self.id(),
            right: other.id(),
            left_outcome: outcome,
            right_outcome: other_outcome,
        };
        if let Some(result) = queries.compatibilities.borrow().get(&key) {
            return Some(*result);
        }
        let result = self.compatible_uncached(outcome, other, other_outcome)?;
        let mut cache = queries.compatibilities.borrow_mut();
        if cache.len() >= MAX_GUARD_QUERIES {
            cache.clear();
        }
        cache.insert(key, result);
        Some(result)
    }

    fn compatible_uncached(
        &self,
        outcome: bool,
        other: &Self,
        other_outcome: bool,
    ) -> Option<bool> {
        self.satisfiable(
            other,
            outcome,
            other_outcome,
            None,
            &mut GuardWorkBudget::new(MAX_PROJECTION_WORK),
        )
    }
    #[tracing::instrument(skip_all, target = "helm_schema_ir::value::guard_proofs")]
    fn satisfiable<'a>(
        &'a self,
        other: &'a Self,
        outcome: bool,
        other_outcome: bool,
        binding: Option<&DecisionEdgeKey>,
        work: &mut GuardWorkBudget,
    ) -> Option<bool> {
        let mut initial = Search {
            pending: vec![
                Goal {
                    guard: self,
                    value: outcome,
                },
                Goal {
                    guard: other,
                    value: other_outcome,
                },
            ],
            bindings: AHashMap::new(),
            expanded: AHashSet::new(),
        };
        if let Some(binding) = binding {
            let context = match self {
                Self::Branch(node) => Rc::as_ptr(&node.queries) as usize,
                Self::False | Self::True => 0,
            };
            initial.bindings.insert(
                GuardProofKey {
                    context,
                    id: binding.decision,
                    value: false,
                },
                binding.taken,
            );
        }
        let mut alternatives = vec![initial];
        while let Some(mut search) = alternatives.pop() {
            let mut consistent = true;
            while let Some(Goal { guard, value }) = search.pending.pop() {
                work.consume(1).ok()?;
                let node = match guard {
                    Self::False | Self::True => {
                        if matches!(guard, Self::True) != value {
                            consistent = false;
                            break;
                        }
                        continue;
                    }
                    Self::Branch(node) => node,
                };
                let context = Rc::as_ptr(&node.queries) as usize;
                if !search.expanded.insert(GuardProofKey {
                    context,
                    id: node.id,
                    value,
                }) {
                    continue;
                }
                let atom = GuardProofKey {
                    context,
                    id: node.test.id,
                    value: false,
                };
                let fixed = if node.test.runtime.is_none() {
                    search.bindings.get(&atom).copied()
                } else {
                    None
                };
                let possible = |taken| {
                    let arm = if taken { &node.yes } else { &node.no };
                    fixed.is_none_or(|fixed| fixed == taken)
                        && !matches!((arm, value), (Self::False, true) | (Self::True, false))
                };
                let yes = possible(true);
                let no = possible(false);
                if yes && no {
                    work.consume(
                        search.pending.len() + search.bindings.len() + search.expanded.len() + 1,
                    )
                    .ok()?;
                    let mut opposite = search.clone();
                    opposite.enter(node, value, false);
                    alternatives.push(opposite);
                }
                if yes {
                    search.enter(node, value, true);
                } else if no {
                    search.enter(node, value, false);
                } else {
                    consistent = false;
                    break;
                }
            }
            if consistent {
                return Some(true);
            }
        }
        Some(false)
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
        } else if self.runtime.is_none() && self.coverage == Coverage::Partial {
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
            (None, None) => Some(true),
        };
        if compatible(false) == Some(false) {
            Some(true)
        } else if compatible(true) == Some(false) {
            Some(false)
        } else {
            None
        }
    }
}

impl DecisionStore {
    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::guard_storage"
    )]
    fn collect_unused(&mut self) {
        if self.nodes.len() < self.collection_limit.max(4096) {
            return;
        }
        // Only execution handles keep guards alive across collection.
        self.conjunctions.clear();
        self.working.clear();
        self.nodes.retain(|_, node| node.strong_count() != 0);
        let live: AHashSet<_> = [0, 1]
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
        self.choices.retain(|key, result| {
            live.contains(&key.condition)
                && live.contains(&key.yes)
                && live.contains(&key.no)
                && result.upgrade().is_some()
        });
        self.negations
            .retain(|guard, result| live.contains(guard) && result.upgrade().is_some());
        self.projections.retain(|key, _| live.contains(key));
        self.tests
            .retain(|key, test| live.contains(key) && test.strong_count() != 0);
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
        let input_exact = test.runtime.as_ref().map_or(
            test.coverage == Coverage::Exact,
            RuntimeGuard::is_input_exact,
        ) && yes.is_input_exact()
            && no.is_input_exact();
        let alias_free = test.runtime.is_none() && yes.is_alias_free() && no.is_alias_free();
        let node = Rc::new(DecisionNode {
            id,
            test,
            yes,
            no,
            input_exact,
            alias_free,
            queries: Rc::clone(&self.queries),
            retained_work: Cell::default(),
        });
        // Keep intermediate results reusable until collection, including throughout recursive apply.
        self.working.push(Rc::clone(&node));
        self.nodes.insert(key, Rc::downgrade(&node));
        Ok(RuntimeGuard::Branch(node))
    }

    fn negate(
        &mut self,
        guard: &RuntimeGuard,
        next_decision: &Cell<u64>,
        work: &mut GuardWorkBudget,
    ) -> RuntimeGuard {
        match guard {
            RuntimeGuard::False => return RuntimeGuard::True,
            RuntimeGuard::True => return RuntimeGuard::False,
            RuntimeGuard::Branch(_) => {}
        }
        if !guard.is_alias_free() {
            work.remaining = 0;
        }
        if let Ok(guard) = self.negate_with_work(guard, work) {
            return guard;
        }
        let result = self.deferred_choice(
            guard.clone(),
            RuntimeGuard::False,
            RuntimeGuard::True,
            next_decision,
        );
        self.negations.insert(guard.id(), CachedGuard::new(&result));
        self.negations.insert(result.id(), CachedGuard::new(guard));
        result
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
        next_decision: &Cell<u64>,
        work: &mut GuardWorkBudget,
    ) -> RuntimeGuard {
        if !left.is_alias_free() || !right.is_alias_free() {
            work.remaining = 0;
        }
        if let Ok(guard) = self.apply_with_work(operation, left, right, work) {
            return guard;
        }
        let result = match operation {
            Operation::And => self.deferred_choice(
                left.clone(),
                right.clone(),
                RuntimeGuard::False,
                next_decision,
            ),
            Operation::Or => self.deferred_choice(
                left.clone(),
                RuntimeGuard::True,
                right.clone(),
                next_decision,
            ),
        };
        self.cache_application(
            ApplyKey {
                operation,
                left: left.id().min(right.id()),
                right: left.id().max(right.id()),
            },
            &result,
        );
        result
    }

    fn cache_application(&mut self, key: ApplyKey, result: &RuntimeGuard) {
        if self.applications.len() >= MAX_GUARD_MEMO_ENTRIES {
            self.applications.clear();
            self.queries.conjunctions_dirty.set(true);
        }
        self.applications.insert(key, CachedGuard::new(result));
    }

    fn deferred_choice(
        &mut self,
        condition: RuntimeGuard,
        yes: RuntimeGuard,
        no: RuntimeGuard,
        next_decision: &Cell<u64>,
    ) -> RuntimeGuard {
        tracing::debug!(target: "helm_schema_ir::value::guard_boundary", "defer exhausted runtime guard construction");
        let id = next_decision.get();
        next_decision.set(id + 1);
        let test = Rc::new(FrozenTest {
            id,
            when_true: Predicate::False,
            when_false: Predicate::False,
            coverage: Coverage::Partial,
            runtime: Some(condition),
        });
        self.select(test, yes, no)
    }

    fn choice(
        &mut self,
        condition: &RuntimeGuard,
        yes: &RuntimeGuard,
        no: &RuntimeGuard,
        next_decision: &Cell<u64>,
        work: &mut GuardWorkBudget,
    ) -> RuntimeGuard {
        if [condition, yes, no]
            .into_iter()
            .any(|guard| !guard.is_alias_free())
        {
            work.remaining = 0;
        }
        if let Ok(guard) = self.choice_with_work(condition, yes, no, work) {
            return guard;
        }
        let result =
            self.deferred_choice(condition.clone(), yes.clone(), no.clone(), next_decision);
        self.choices.insert(
            ChoiceKey {
                condition: condition.id(),
                yes: yes.id(),
                no: no.id(),
            },
            CachedGuard::new(&result),
        );
        result
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
        self.cache_application(key, &result);
        Ok(result)
    }

    fn choice_with_work<W: GuardWork>(
        &mut self,
        condition: &RuntimeGuard,
        yes: &RuntimeGuard,
        no: &RuntimeGuard,
        work: &mut W,
    ) -> Result<RuntimeGuard, W::Error> {
        self.choice_query(condition, yes, no, work, &mut BTreeMap::new())
    }

    fn choice_query<W: GuardWork>(
        &mut self,
        condition: &RuntimeGuard,
        yes: &RuntimeGuard,
        no: &RuntimeGuard,
        work: &mut W,
        results: &mut BTreeMap<ChoiceKey, RuntimeGuard>,
    ) -> Result<RuntimeGuard, W::Error> {
        if matches!(condition, RuntimeGuard::False) {
            return Ok(no.clone());
        }
        if condition == &RuntimeGuard::True || yes == no {
            return Ok(yes.clone());
        }
        if matches!(no, RuntimeGuard::False) {
            return self.apply_with_work(Operation::And, condition, yes, work);
        }
        if yes == &RuntimeGuard::True {
            return self.apply_with_work(Operation::Or, condition, no, work);
        }
        if matches!(yes, RuntimeGuard::False) && no == &RuntimeGuard::True {
            return self.negate_with_work(condition, work);
        }
        let key = ChoiceKey {
            condition: condition.id(),
            yes: yes.id(),
            no: no.id(),
        };
        if let Some(result) = results.get(&key) {
            return Ok(result.clone());
        }
        if let Some(result) = self.choices.get(&key).and_then(CachedGuard::upgrade) {
            results.insert(key, result.clone());
            return Ok(result);
        }
        work.charge()?;
        let Some(test) = condition
            .decision()
            .into_iter()
            .chain(yes.decision())
            .chain(no.decision())
            .max_by_key(|test| test.id)
        else {
            return Ok(RuntimeGuard::False);
        };
        let selected_yes = self.choice_query(
            &condition.arm(test.id, true),
            &yes.arm(test.id, true),
            &no.arm(test.id, true),
            work,
            results,
        )?;
        let selected_no = self.choice_query(
            &condition.arm(test.id, false),
            &yes.arm(test.id, false),
            &no.arm(test.id, false),
            work,
            results,
        )?;
        let result = self.select_with_work(Rc::clone(test), selected_yes, selected_no, work)?;
        // Constant results can add cache entries without growing the node interner.
        // Keep query-local results independent of eviction, so one query never loses its work.
        if self.choices.len() >= MAX_GUARD_MEMO_ENTRIES {
            self.choices.clear();
        }
        self.choices.insert(key.clone(), CachedGuard::new(&result));
        results.insert(key, result.clone());
        Ok(result)
    }

    fn edge(
        &mut self,
        edge: &DecisionEdge,
        next_decision: &Cell<u64>,
        work: &mut GuardWorkBudget,
    ) -> RuntimeGuard {
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
            self.negate(&guard, next_decision, work)
        }
    }

    fn conjunction(&mut self, edges: &[DecisionEdge], next_decision: &Cell<u64>) -> RuntimeGuard {
        // A cached path can skip component work only while those cache entries survive.
        if self.queries.conjunctions_dirty.replace(false) {
            self.conjunctions.clear();
        }
        if edges.len() < 2 {
            return self.conjunction_uncached(edges, next_decision);
        }
        if let Some(result) =
            self.conjunctions
                .iter()
                .rev()
                .find(|(key, _)| {
                    key.len() == edges.len()
                        && key.iter().zip(edges).all(|(key, edge)| {
                            key.decision == edge.test.id && key.taken == edge.taken
                        })
                })
                .and_then(|(_, result)| result.upgrade())
        {
            return result;
        }
        let result = self.conjunction_uncached(edges, next_decision);
        // Capture only while every component cache hit remains valid; keep results weak.
        if !self.queries.conjunctions_dirty.get() {
            if self.conjunctions.len() >= MAX_CONJUNCTIONS {
                self.conjunctions.clear();
            }
            let key = edges
                .iter()
                .map(|DecisionEdge { test, taken }| DecisionEdgeKey {
                    decision: test.id,
                    taken: *taken,
                })
                .collect();
            self.conjunctions.push((key, CachedGuard::new(&result)));
        }
        result
    }

    fn conjunction_uncached(
        &mut self,
        edges: &[DecisionEdge],
        next_decision: &Cell<u64>,
    ) -> RuntimeGuard {
        let mut result = RuntimeGuard::True;
        let mut work = GuardWorkBudget::new(MAX_PROJECTION_WORK);
        for edge in edges {
            let next = self.edge(edge, next_decision, &mut work);
            result = self.apply(Operation::And, &result, &next, next_decision, &mut work);
        }
        result
    }

    fn project(
        &mut self,
        guard: &RuntimeGuard,
        memo: &helm_schema_core::PredicateMemo,
    ) -> Projection {
        let mut remaining = MAX_PROJECTION_WORK;
        let Projection {
            necessary,
            sufficient,
        } = self.project_with_work(guard, &mut remaining);
        let exact = necessary == sufficient;
        let necessary = normalize_projection(necessary, memo, &mut remaining);
        let sufficient = if exact {
            necessary.clone().unwrap_or(Predicate::False)
        } else {
            normalize_projection(sufficient, memo, &mut remaining).unwrap_or(Predicate::False)
        };
        let result = Projection {
            necessary: necessary.unwrap_or(Predicate::True),
            sufficient,
        };
        // A completed bounded query remains a sound answer even when it widened.
        self.projections.insert(guard.id(), result.clone());
        result
    }

    #[tracing::instrument(skip_all)]
    fn project_with_work(&mut self, guard: &RuntimeGuard, remaining: &mut usize) -> Projection {
        let key = guard.id();
        if let Some(projection) = self.projections.get(&key) {
            return projection.clone();
        }
        if matches!(guard, RuntimeGuard::Branch(_)) {
            let Some(next) = remaining.checked_sub(1) else {
                tracing::debug!(reason = ?super::BoundReason::BudgetPredicate, "widen exhausted runtime guard projection");
                return Projection {
                    necessary: Predicate::True,
                    sufficient: Predicate::False,
                };
            };
            *remaining = next;
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
                let yes = self.project_with_work(&node.yes, remaining);
                // A partial arm with both outcomes possible absorbs the sibling's input proofs.
                if node.test.coverage == Coverage::Partial
                    && yes.necessary == Predicate::True
                    && yes.sufficient == Predicate::False
                {
                    if *remaining > 0 {
                        self.projections.insert(key, yes.clone());
                    }
                    return yes;
                }
                let no = self.project_with_work(&node.no, remaining);
                match node.test.coverage {
                    Coverage::Exact => {
                        let exact =
                            yes.necessary == yes.sufficient && no.necessary == no.sufficient;
                        let necessary = self.project_union_with_work(
                            vec![
                                Predicate::all(vec![node.test.when_true.clone(), yes.necessary]),
                                Predicate::all(vec![node.test.when_false.clone(), no.necessary]),
                            ],
                            remaining,
                        );
                        // An exact decision partitions its domain. Adding both child proofs
                        // again is redundant and doubles expressions after normalization caps.
                        let sufficient = if exact {
                            necessary.clone().unwrap_or(Predicate::False)
                        } else {
                            self.project_union_with_work(
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
                                remaining,
                            )
                            .unwrap_or(Predicate::False)
                        };
                        Projection {
                            necessary: necessary.unwrap_or(Predicate::True),
                            sufficient,
                        }
                    }
                    Coverage::Partial | Coverage::CertifiedPartial => {
                        self.project_partial(node, yes, no, remaining)
                    }
                }
            }
        };
        if *remaining > 0 {
            self.projections.insert(key, result.clone());
        }
        result
    }

    fn project_partial(
        &mut self,
        node: &DecisionNode,
        yes: Projection,
        no: Projection,
        remaining: &mut usize,
    ) -> Projection {
        // Certificates are optional precision; retain their local syntax ceiling as well.
        const MAX_CERTIFICATE_SYNTAX: usize = 64;
        let fallback = Projection {
            necessary: self
                .project_union_with_work(
                    vec![yes.necessary.clone(), no.necessary.clone()],
                    remaining,
                )
                .unwrap_or(Predicate::True),
            sufficient: charged_projection(
                Predicate::all(vec![yes.sufficient.clone(), no.sufficient.clone()]),
                remaining,
            )
            .map(fold_conjunction_constants)
            .unwrap_or(Predicate::False),
        };
        if node.test.coverage != Coverage::CertifiedPartial {
            return fallback;
        }
        // Charge certificate traversal before inspecting or composing its predicates.
        if charged_projection(
            Predicate::all(vec![
                node.test.when_true.clone(),
                node.test.when_false.clone(),
            ]),
            remaining,
        )
        .is_none()
            || node.test.when_true.contains_approximation()
            || node.test.when_false.contains_approximation()
        {
            return fallback;
        }
        let necessary = self.project_union_with_work(
            vec![
                Predicate::all(vec![node.test.when_false.negated(), yes.necessary]),
                Predicate::all(vec![node.test.when_true.negated(), no.necessary]),
            ],
            remaining,
        );
        let sufficient = self.project_union_with_work(
            vec![
                Predicate::all(vec![node.test.when_true.clone(), yes.sufficient.clone()]),
                Predicate::all(vec![node.test.when_false.clone(), no.sufficient.clone()]),
                Predicate::all(vec![yes.sufficient, no.sufficient]),
            ],
            remaining,
        );
        match (
            necessary.filter(|predicate| predicate.syntax_size() <= MAX_CERTIFICATE_SYNTAX),
            sufficient.filter(|predicate| predicate.syntax_size() <= MAX_CERTIFICATE_SYNTAX),
        ) {
            (Some(necessary), Some(sufficient)) => Projection {
                necessary,
                sufficient,
            },
            (Some(_) | None, None) | (None, Some(_)) => fallback,
        }
    }

    // Frozen formulas retain neither runtime guards nor heaps.
    // Exhaustion belongs to the current query, so only successful projections are cached.
    fn project_union_with_work(
        &mut self,
        parts: Vec<Predicate>,
        remaining: &mut usize,
    ) -> Option<Predicate> {
        let key = ProjectionUnionKey { parts };
        if let Some(result) = self.projection_unions.get(&key) {
            return Some(result.clone());
        }
        let result = project_union(key.parts.clone(), remaining)?;
        if self.projection_unions.len() >= MAX_PROJECTION_UNIONS {
            self.projection_unions.clear();
        }
        self.projection_unions.insert(key, result.clone());
        Some(result)
    }
}

fn normalize_projection(
    predicate: Predicate,
    memo: &helm_schema_core::PredicateMemo,
    remaining: &mut usize,
) -> Option<Predicate> {
    // Normalization may exhaust its allowance without invalidating the constructed proof.
    let normalized = memo
        .normalize_bounded(predicate.clone(), remaining)
        .unwrap_or(predicate);
    (normalized.syntax_size() <= MAX_PROJECTION_WORK).then_some(normalized)
}

fn charged_projection(predicate: Predicate, remaining: &mut usize) -> Option<Predicate> {
    let Some(size) = predicate.node_work(*remaining) else {
        *remaining = 0;
        tracing::debug!(reason = ?super::BoundReason::BudgetPredicate, "widen guard projection");
        return None;
    };
    *remaining -= size;
    Some(predicate)
}

fn project_union(mut parts: Vec<Predicate>, remaining: &mut usize) -> Option<Predicate> {
    charged_projection(Predicate::Or(parts.clone()), remaining)?;
    parts = parts
        .into_iter()
        .map(fold_conjunction_constants)
        .filter(|part| part != &Predicate::False)
        .collect();
    match parts.as_slice() {
        [] => return Some(Predicate::False),
        [part] => return Some(part.clone()),
        _ => {}
    }
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
        return Some(Predicate::Or(parts));
    }
    // Retain the shared prefix in the DAG; normalize only the complete query.
    let suffix = Predicate::Or(
        terms
            .iter()
            .map(|term| Predicate::all(term.difference(&common).cloned().collect()))
            .collect(),
    );
    Some(fold_conjunction_constants(Predicate::all(
        common.into_iter().chain([suffix]).collect(),
    )))
}

fn fold_conjunction_constants(predicate: Predicate) -> Predicate {
    let PredicateKind::And(parts) = predicate.kind() else {
        return predicate;
    };
    let parts = Conjunction::new(parts.iter().cloned()).into_vec();
    if parts.contains(&Predicate::False) {
        Predicate::False
    } else {
        Predicate::all(parts)
    }
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
        let mut work = GuardWorkBudget::new(MAX_PROJECTION_WORK);
        let left_guard = store.conjunction(left_tail, &self.next_decision);
        let right_guard = store.conjunction(right_tail, &self.next_decision);
        let opposite = left_tail
            .first()
            .zip(right_tail.first())
            .filter(|(left, right)| left.test == right.test && left.taken != right.taken);
        let source = if let Some((left, _)) = opposite {
            Some(DecisionEdge {
                test: Rc::clone(&left.test),
                taken: left.taken,
            })
        } else if store
            .apply(
                Operation::And,
                &left_guard,
                &right_guard,
                &self.next_decision,
                &mut work,
            )
            .is_impossible()
        {
            Some(DecisionEdge {
                test: self.guard_test(&mut store, left_guard.clone()),
                taken: true,
            })
        } else {
            None
        };
        let union = if let Some(DecisionEdge {
            test: decision,
            taken,
        }) = source
        {
            let survival = store.apply(
                Operation::Or,
                &left_guard,
                &right_guard,
                &self.next_decision,
                &mut work,
            );
            GuardedUnion {
                decision,
                taken,
                survival,
            }
        } else {
            // Overlapping abstract paths can carry different outcomes for the same input.
            // Keep both using an independent runtime choice, gated by each source path.
            let decision = self.decision(Predicate::False, Predicate::False, Coverage::Partial);
            let choice = store.edge(
                &DecisionEdge {
                    test: Rc::clone(&decision),
                    taken: true,
                },
                &self.next_decision,
                &mut work,
            );
            let survival = store.choice(
                &choice,
                &left_guard,
                &right_guard,
                &self.next_decision,
                &mut work,
            );
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
        let known = store.conjunction(&active, &self.next_decision);
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
        // Selecting true/false is the original test, including its independently known subsets.
        if yes.constant_outcome() == Some(true) && no.constant_outcome() == Some(false) {
            return decision;
        }
        let mut store = self.guards.borrow_mut();
        let condition = store.edge(
            &DecisionEdge {
                test: decision,
                taken: true,
            },
            &self.next_decision,
            &mut GuardWorkBudget::new(MAX_PROJECTION_WORK),
        );
        let yes = store.edge(
            &DecisionEdge {
                test: yes,
                taken: true,
            },
            &self.next_decision,
            &mut GuardWorkBudget::new(MAX_PROJECTION_WORK),
        );
        let no = store.edge(
            &DecisionEdge {
                test: no,
                taken: true,
            },
            &self.next_decision,
            &mut GuardWorkBudget::new(MAX_PROJECTION_WORK),
        );
        let runtime = store.choice(
            &condition,
            &yes,
            &no,
            &self.next_decision,
            &mut GuardWorkBudget::new(MAX_PROJECTION_WORK),
        );
        self.guard_test(&mut store, runtime)
    }

    pub(super) fn runtime_path(&self, edges: &[DecisionEdge]) -> RuntimeGuard {
        self.guards
            .borrow_mut()
            .conjunction(edges, &self.next_decision)
    }

    #[tracing::instrument(
        skip_all,
        level = "debug",
        target = "helm_schema_ir::value::guard_storage",
        fields(nodes = store.nodes.len(), applications = store.applications.len(), projections = store.projections.len(), tests = store.tests.len())
    )]
    fn guard_test(&self, store: &mut DecisionStore, runtime: RuntimeGuard) -> Rc<FrozenTest> {
        store.collect_unused();
        let key = runtime.id();
        if let Some(test) = store.tests.get(&key).and_then(Weak::upgrade) {
            return test;
        }
        let Projection {
            necessary,
            sufficient,
        } = store.project(&runtime, &self.predicate_memo);
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
    pub(super) fn complement_runtime_guard(&self, guard: &RuntimeGuard) -> RuntimeGuard {
        self.guards.borrow_mut().negate(
            guard,
            &self.next_decision,
            &mut GuardWorkBudget::new(MAX_PROJECTION_WORK),
        )
    }

    pub(super) fn select_runtime_guard(
        &self,
        condition: &RuntimeGuard,
        yes: &RuntimeGuard,
        no: &RuntimeGuard,
    ) -> RuntimeGuard {
        self.guards.borrow_mut().choice(
            condition,
            yes,
            no,
            &self.next_decision,
            &mut GuardWorkBudget::new(MAX_PROJECTION_WORK),
        )
    }

    pub(super) fn select_runtime_guard_bounded(
        &self,
        condition: &RuntimeGuard,
        yes: &RuntimeGuard,
        no: &RuntimeGuard,
        work: &mut GuardWorkBudget,
    ) -> Result<RuntimeGuard, super::BoundReason> {
        let result = self
            .guards
            .borrow_mut()
            .choice_with_work(condition, yes, no, work)?;
        work.include([&result])?;
        Ok(result)
    }

    pub(super) fn union_runtime_guards(
        &self,
        left: &RuntimeGuard,
        right: &RuntimeGuard,
    ) -> RuntimeGuard {
        self.guards.borrow_mut().apply(
            Operation::Or,
            left,
            right,
            &self.next_decision,
            &mut GuardWorkBudget::new(MAX_PROJECTION_WORK),
        )
    }

    pub(super) fn intersect_runtime_guards(
        &self,
        left: &RuntimeGuard,
        right: &RuntimeGuard,
    ) -> RuntimeGuard {
        self.guards.borrow_mut().apply(
            Operation::And,
            left,
            right,
            &self.next_decision,
            &mut GuardWorkBudget::new(MAX_PROJECTION_WORK),
        )
    }

    pub(super) fn budget_test(&self, guard: RuntimeGuard) -> Rc<FrozenTest> {
        self.guard_test(&mut self.guards.borrow_mut(), guard)
    }
}

#[cfg(test)]
#[path = "../tests/runtime_budget_work.rs"]
mod tests;
