use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use helm_schema_core::{
    ConditionalGuard, Predicate, PredicateKind, PredicateResidualAtom, PredicateResidualNode,
    PredicateResiduals, RequirementCondition, RequirementConditionBuilder,
    RequirementConditionNode, RequirementDecision,
};

mod live_atoms;

use live_atoms::{AtomSet, AtomSets};

type LiveAtoms = Option<AtomSet>;
type Residuals = Vec<Option<PredicateResidualNode>>;
type ResidualRoots = BTreeMap<Residuals, usize>;

pub(crate) struct ActivatedRequirement {
    pub(crate) condition: RequirementCondition,
    pub(crate) active: BTreeSet<usize>,
    pub(crate) unknown: BTreeSet<usize>,
}

pub(crate) struct GuardedActivation {
    pub(crate) predicate: Predicate,
    pub(crate) guard_count: usize,
}

struct GuardResiduals {
    when_true: GuardResidual,
    when_false: GuardResidual,
}

#[derive(Clone)]
struct GuardResidual {
    node: Option<PredicateResidualNode>,
    proves: Rc<[usize]>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ResidualVisit {
    node: usize,
    live_after: LiveAtoms,
    residuals: Residuals,
}

struct PendingConjunct {
    node: usize,
    live_after: LiveAtoms,
}

#[derive(Clone)]
struct ConjunctCheck {
    visit: ResidualVisit,
    prefix: usize,
}

enum Work<'a> {
    Visit(ResidualVisit),
    FinishUnion {
        owner: ResidualVisit,
        branches: Vec<ResidualVisit>,
    },
    FinishDecision {
        owner: ResidualVisit,
        test: &'a ConditionalGuard,
        when_true: ResidualVisit,
        when_false: ResidualVisit,
    },
    FinishConjunct {
        owner: ResidualVisit,
        remaining: Vec<PendingConjunct>,
        checks: Vec<ConjunctCheck>,
    },
}

struct ActivationPartition<'a> {
    graph: &'a RequirementCondition,
    activations: &'a [GuardedActivation],
    solver: PredicateResiduals<'a>,
    atom_sets: AtomSets,
    builder: RequirementConditionBuilder,
    copies: HashMap<usize, usize>,
    guards: HashMap<usize, GuardResiduals>,
    live: HashMap<usize, LiveAtoms>,
    memo: HashMap<ResidualVisit, ResidualRoots>,
}

/// Groups proof paths by the guarded default documents they entail.
///
/// For each activation, a residual represents the remaining counterexamples after
/// the guards encountered so far. Existentially forget variables once no future
/// guard can read them. Different histories with the same residual then share the
/// same continuation, even when the activation is a parity of many split tests.
pub(crate) fn partition_activation(
    graph: &RequirementCondition,
    extras: &Predicate,
    activations: &[GuardedActivation],
) -> Option<Vec<ActivatedRequirement>> {
    if activations.is_empty() {
        return Some(vec![ActivatedRequirement {
            condition: graph.clone(),
            active: BTreeSet::new(),
            unknown: BTreeSet::new(),
        }]);
    }
    let order = graph.reachable_ids();
    let predicates = order
        .iter()
        .filter_map(|&id| match graph.node(id)? {
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
            }) => Some((id, guard.predicate())),
            RequirementConditionNode::False
            | RequirementConditionNode::True
            | RequirementConditionNode::And(_)
            | RequirementConditionNode::Or(_) => None,
        })
        .collect::<HashMap<_, _>>();
    let mut solver = PredicateResiduals::new(
        activations
            .iter()
            .map(|activation| &activation.predicate)
            .chain(std::iter::once(extras))
            .chain(predicates.values()),
    );
    let initial = activations
        .iter()
        .map(|activation| {
            let activation = solver.import(&activation.predicate)?;
            solver.not(activation)
        })
        .collect::<Vec<_>>();
    let extras = GuardResidual {
        node: solver.import(extras),
        proves: direct_activations(extras, activations),
    };
    let mut partition = ActivationPartition {
        graph,
        activations,
        atom_sets: AtomSets::new(solver.atom_count()),
        solver,
        builder: RequirementConditionBuilder::default(),
        copies: HashMap::new(),
        guards: HashMap::new(),
        live: HashMap::new(),
        memo: HashMap::new(),
    };
    for id in order {
        partition.prepare(id, predicates.get(&id))?;
    }
    let roots = partition.resolve(graph.root(), &initial, &extras)?;
    tracing::info_span!(
        target: "helm_schema_gen::counters",
        "activation_partition_storage",
        visits = partition.memo.len() + 1,
        frontier_comparisons = partition.atom_sets.frontier_comparisons(),
    )
    .in_scope(|| {});
    let conditions = partition.builder.freeze();
    Some(
        roots
            .iter()
            .map(|(residuals, &id)| {
                let mut active = BTreeSet::new();
                let mut unknown = BTreeSet::new();
                for (index, residual) in residuals.iter().enumerate() {
                    match residual {
                        Some(PredicateResidualNode::FALSE) => {
                            active.insert(index);
                        }
                        Some(PredicateResidualNode::TRUE) => {}
                        Some(_) | None => {
                            unknown.insert(index);
                        }
                    }
                }
                ActivatedRequirement {
                    condition: if roots.len() == 1 {
                        graph.clone()
                    } else {
                        conditions.root(id, false)
                    },
                    active,
                    unknown,
                }
            })
            .collect(),
    )
}

fn direct_activations(proof: &Predicate, activations: &[GuardedActivation]) -> Rc<[usize]> {
    let mut terms = vec![proof];
    let mut seen = HashSet::new();
    let mut proved = BTreeSet::new();
    while let Some(term) = terms.pop() {
        if !seen.insert(std::ptr::from_ref(term)) {
            continue;
        }
        for (index, activation) in activations.iter().enumerate() {
            if term == &activation.predicate && !term.contains_approximation() {
                proved.insert(index);
            }
        }
        if let PredicateKind::And(children) = term.kind() {
            terms.extend(children);
        }
    }
    proved.into_iter().collect()
}

impl<'input> ActivationPartition<'input> {
    fn prepare(&mut self, id: usize, predicate: Option<&'input Predicate>) -> Option<()> {
        let mut live = Some(AtomSet::EMPTY);
        if let Some(predicate) = predicate {
            live = self.solver.atoms(predicate).map(|atoms| {
                self.atom_sets.leaf(
                    atoms
                        .into_iter()
                        .map(PredicateResidualAtom::ordinal)
                        .collect(),
                )
            });
            let when_true = self.solver.import(predicate);
            let when_false = when_true.and_then(|node| self.solver.not(node));
            self.guards.insert(
                id,
                GuardResiduals {
                    when_true: GuardResidual {
                        node: when_true,
                        proves: direct_activations(predicate, self.activations),
                    },
                    when_false: GuardResidual {
                        node: when_false,
                        proves: direct_activations(&predicate.negated(), self.activations),
                    },
                },
            );
        }
        let copy = match self.graph.node(id)? {
            RequirementConditionNode::True => RequirementConditionBuilder::TRUE,
            RequirementConditionNode::False => RequirementConditionBuilder::FALSE,
            RequirementConditionNode::Guard(guard) => self.builder.guard(guard.clone()),
            RequirementConditionNode::And(children) | RequirementConditionNode::Or(children) => {
                let mut roots = Vec::with_capacity(children.len());
                for child in children {
                    roots.push(*self.copies.get(child)?);
                    live = self.together(live, *self.live.get(child)?);
                }
                if matches!(self.graph.node(id)?, RequirementConditionNode::And(_)) {
                    self.builder.and(roots)
                } else {
                    self.builder.union(roots)
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
                live = self.together(live, *self.live.get(when_true)?);
                live = self.together(live, *self.live.get(when_false)?);
                self.builder.split(
                    test.clone(),
                    *self.copies.get(when_true)?,
                    *self.copies.get(when_false)?,
                )
            }
        };
        self.live.insert(id, live);
        self.copies.insert(id, copy);
        Some(())
    }

    fn advance(
        &mut self,
        residuals: &Residuals,
        guard: Option<PredicateResidualNode>,
        proved: &[usize],
        live: &LiveAtoms,
    ) -> Residuals {
        let mut result = residuals
            .iter()
            .enumerate()
            .map(|(index, &residual)| {
                if residual != Some(PredicateResidualNode::TRUE)
                    && proved.binary_search(&index).is_ok()
                {
                    return Some(PredicateResidualNode::FALSE);
                }
                match residual {
                    Some(PredicateResidualNode::FALSE | PredicateResidualNode::TRUE) => residual,
                    Some(residual) => {
                        let restricted = self.solver.and(residual, guard?)?;
                        match live {
                            Some(live) => self.solver.forget_where(restricted, |atom| {
                                self.atom_sets.contains(*live, atom.ordinal())
                            }),
                            None => Some(restricted),
                        }
                    }
                    None => None,
                }
            })
            .collect::<Residuals>();
        if let Some(winner) = result
            .iter()
            .enumerate()
            .filter(|&(_, residual)| *residual == Some(PredicateResidualNode::FALSE))
            .map(|(index, _)| self.activation_rank(index))
            .max()
        {
            for (index, residual) in result.iter_mut().enumerate() {
                if self.activation_rank(index) < winner {
                    *residual = Some(PredicateResidualNode::TRUE);
                }
            }
        }
        result
    }

    fn activation_rank(&self, index: usize) -> (usize, usize) {
        let guard_count = self
            .activations
            .get(index)
            .map_or(0, |activation| activation.guard_count);
        (guard_count, index)
    }

    fn together(&mut self, left: LiveAtoms, right: LiveAtoms) -> LiveAtoms {
        self.atom_sets.union(left?, right?)
    }

    fn request(
        &mut self,
        node: usize,
        residuals: &Residuals,
        guard: Option<&GuardResidual>,
        live_after: LiveAtoms,
    ) -> Option<ResidualVisit> {
        let live_after = live_after.and_then(|live| self.atom_sets.frontier(live));
        let live = self.together(*self.live.get(&node)?, live_after);
        let no_proofs: &[usize] = &[];
        let (guard, proved) = guard
            .map_or((Some(PredicateResidualNode::TRUE), no_proofs), |guard| {
                (guard.node, guard.proves.as_ref())
            });
        let residuals = self.advance(residuals, guard, proved, &live);
        Some(ResidualVisit {
            node,
            live_after,
            residuals,
        })
    }

    fn resolve(
        &mut self,
        root: usize,
        residuals: &Residuals,
        extras: &GuardResidual,
    ) -> Option<ResidualRoots> {
        let request = self.request(root, residuals, Some(extras), Some(AtomSet::EMPTY))?;
        // Resume a parent only after its requested children have been memoized.
        let mut pending = vec![Work::Visit(request.clone())];
        while let Some(work) = pending.pop() {
            match work {
                Work::Visit(visit) => self.schedule(visit, &mut pending)?,
                Work::FinishUnion { owner, branches } => {
                    let roots = self.finish_union(&branches)?;
                    self.memo.insert(owner, roots);
                }
                Work::FinishDecision {
                    owner,
                    test,
                    when_true,
                    when_false,
                } => {
                    let roots = self.finish_decision(test, &when_true, &when_false)?;
                    self.memo.insert(owner, roots);
                }
                Work::FinishConjunct {
                    owner,
                    remaining,
                    checks,
                } => {
                    let prefixes = self.finish_conjunct(&checks)?;
                    self.schedule_conjunct(owner, remaining, prefixes, &mut pending)?;
                }
            }
        }
        self.memo.remove(&request)
    }

    fn schedule(&mut self, visit: ResidualVisit, pending: &mut Vec<Work<'input>>) -> Option<()> {
        if self.memo.contains_key(&visit) {
            return Some(());
        }
        let ResidualVisit {
            node,
            live_after,
            ref residuals,
        } = visit;
        let graph = self.graph;
        let roots = match graph.node(node)? {
            RequirementConditionNode::False => BTreeMap::new(),
            _ if residuals.iter().all(|residual| {
                matches!(
                    residual,
                    Some(PredicateResidualNode::TRUE | PredicateResidualNode::FALSE)
                )
            }) =>
            {
                BTreeMap::from([(residuals.clone(), *self.copies.get(&node)?)])
            }
            RequirementConditionNode::True => {
                BTreeMap::from([(residuals.clone(), RequirementConditionBuilder::TRUE)])
            }
            RequirementConditionNode::Guard(_) => {
                let guard = self.guards.get(&node)?.when_true.clone();
                let next = self.advance(residuals, guard.node, &guard.proves, &live_after);
                BTreeMap::from([(next, *self.copies.get(&node)?)])
            }
            RequirementConditionNode::And(children) => {
                let mut future = live_after;
                let mut remaining = Vec::with_capacity(children.len());
                for &child in children.iter().rev() {
                    remaining.push(PendingConjunct {
                        node: child,
                        live_after: future,
                    });
                    future = self.together(*self.live.get(&child)?, future);
                }
                let prefixes =
                    BTreeMap::from([(residuals.clone(), RequirementConditionBuilder::TRUE)]);
                return self.schedule_conjunct(visit, remaining, prefixes, pending);
            }
            RequirementConditionNode::Or(children) => {
                let branches = children
                    .iter()
                    .map(|&child| self.request(child, residuals, None, live_after))
                    .collect::<Option<Vec<_>>>()?;
                pending.push(Work::FinishUnion {
                    owner: visit,
                    branches: branches.clone(),
                });
                pending.extend(branches.into_iter().rev().map(Work::Visit));
                return Some(());
            }
            RequirementConditionNode::Decision(decision)
            | RequirementConditionNode::Split(decision) => {
                return self.schedule_decision(visit, decision, pending);
            }
        };
        self.memo.insert(visit, roots);
        Some(())
    }

    fn schedule_decision(
        &mut self,
        owner: ResidualVisit,
        decision: &'input RequirementDecision,
        pending: &mut Vec<Work<'input>>,
    ) -> Option<()> {
        let RequirementDecision {
            test,
            when_true,
            when_false,
        } = decision;
        let GuardResiduals {
            when_true: yes,
            when_false: no,
        } = self.guards.get(&owner.node)?;
        let (yes, no) = (yes.clone(), no.clone());
        let when_true = self.request(*when_true, &owner.residuals, Some(&yes), owner.live_after)?;
        let when_false =
            self.request(*when_false, &owner.residuals, Some(&no), owner.live_after)?;
        pending.push(Work::FinishDecision {
            owner,
            test,
            when_true: when_true.clone(),
            when_false: when_false.clone(),
        });
        pending.push(Work::Visit(when_false));
        pending.push(Work::Visit(when_true));
        Some(())
    }

    fn schedule_conjunct(
        &mut self,
        owner: ResidualVisit,
        mut remaining: Vec<PendingConjunct>,
        prefixes: ResidualRoots,
        pending: &mut Vec<Work<'input>>,
    ) -> Option<()> {
        let Some(PendingConjunct { node, live_after }) =
            remaining.pop().filter(|_| !prefixes.is_empty())
        else {
            self.memo.insert(owner, prefixes);
            return Some(());
        };
        let checks = prefixes
            .into_iter()
            .map(|(residuals, prefix)| {
                let visit = self.request(node, &residuals, None, live_after)?;
                Some(ConjunctCheck { visit, prefix })
            })
            .collect::<Option<Vec<_>>>()?;
        pending.push(Work::FinishConjunct {
            owner,
            remaining,
            checks: checks.clone(),
        });
        pending.extend(
            checks
                .into_iter()
                .rev()
                .map(|check| Work::Visit(check.visit)),
        );
        Some(())
    }

    fn finish_union(&mut self, branches: &[ResidualVisit]) -> Option<ResidualRoots> {
        let mut roots = BTreeMap::<Residuals, Vec<usize>>::new();
        for branch in branches {
            for (state, &root) in self.memo.get(branch)? {
                roots.entry(state.clone()).or_default().push(root);
            }
        }
        Some(self.union(roots))
    }

    fn finish_decision(
        &mut self,
        test: &ConditionalGuard,
        when_true: &ResidualVisit,
        when_false: &ResidualVisit,
    ) -> Option<ResidualRoots> {
        let high = self.memo.get(when_true)?;
        let low = self.memo.get(when_false)?;
        let states = high.keys().chain(low.keys()).collect::<BTreeSet<_>>();
        Some(
            states
                .into_iter()
                .map(|state| {
                    let when_true = high
                        .get(state)
                        .copied()
                        .unwrap_or(RequirementConditionBuilder::FALSE);
                    let when_false = low
                        .get(state)
                        .copied()
                        .unwrap_or(RequirementConditionBuilder::FALSE);
                    (
                        state.clone(),
                        self.builder.split(test.clone(), when_true, when_false),
                    )
                })
                .collect(),
        )
    }

    fn finish_conjunct(&mut self, checks: &[ConjunctCheck]) -> Option<ResidualRoots> {
        let mut roots = BTreeMap::<Residuals, Vec<usize>>::new();
        for ConjunctCheck { visit, prefix } in checks {
            for (state, &root) in self.memo.get(visit)? {
                roots
                    .entry(state.clone())
                    .or_default()
                    .push(self.builder.and([*prefix, root]));
            }
        }
        Some(self.union(roots))
    }

    fn union(&mut self, roots: BTreeMap<Residuals, Vec<usize>>) -> ResidualRoots {
        roots
            .into_iter()
            .map(|(residual, roots)| (residual, self.builder.union(roots)))
            .collect()
    }
}

#[cfg(test)]
#[path = "../tests/requirement_activation.rs"]
mod tests;
