//! Executed consumptions retain normalized input preimages and their runtime guards.

use std::collections::BTreeSet;

use helm_schema_core::{
    FailValueRequirement, Guard, Predicate, RequirementAtom, RequirementBdd, RequirementNode,
    ValuesPath,
};

use super::guard::RuntimeGuard;
use super::{LoopId, State};

#[derive(Clone, Debug, Default)]
pub(super) struct RequirementScope {
    // Keep the identity alive after its interpreter is dropped.
    token: std::sync::Arc<()>,
}

impl RequirementScope {
    fn address(&self) -> usize {
        std::sync::Arc::as_ptr(&self.token).addr()
    }
}

impl PartialEq for RequirementScope {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.token, &other.token)
    }
}
impl Eq for RequirementScope {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FrozenChoice {
    pub(super) scope: RequirementScope,
    pub(super) decision: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ChoiceKey {
    scope: u64,
    decision: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct BinderKey {
    scope: u64,
    binder: LoopId,
}

// Addresses and runtime IDs are lookup keys; ordered traversal assigns the BDD ordinals.
#[derive(Default)]
pub(crate) struct RequirementIdentities {
    scopes: ahash::AHashMap<usize, u64>,
    choices: ahash::AHashMap<ChoiceKey, helm_schema_core::RequirementChoice>,
    binders: ahash::AHashMap<BinderKey, u64>,
}

impl RequirementIdentities {
    fn scope(&mut self, scope: &RequirementScope) -> Option<u64> {
        let next = u64::try_from(self.scopes.len()).ok()?;
        Some(*self.scopes.entry(scope.address()).or_insert(next))
    }

    fn choice(&mut self, choice: &FrozenChoice) -> Option<helm_schema_core::RequirementChoice> {
        let scope = self.scope(&choice.scope)?;
        let next = u64::try_from(self.choices.len()).ok()?;
        Some(
            self.choices
                .entry(ChoiceKey {
                    scope,
                    decision: choice.decision,
                })
                .or_insert_with(|| helm_schema_core::RequirementChoice {
                    scope: scope.to_string(),
                    decision: next,
                })
                .clone(),
        )
    }

    fn get_choice(&self, choice: &FrozenChoice) -> Option<&helm_schema_core::RequirementChoice> {
        let scope = *self.scopes.get(&choice.scope.address())?;
        self.choices.get(&ChoiceKey {
            scope,
            decision: choice.decision,
        })
    }

    fn binder(&mut self, scope: &RequirementScope, binder: LoopId) -> Option<u64> {
        let scope = self.scope(scope)?;
        let next = u64::try_from(self.binders.len()).ok()?;
        Some(
            *self
                .binders
                .entry(BinderKey { scope, binder })
                .or_insert(next),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ConsumptionDomain {
    Known {
        preimage: FailValueRequirement,
        requires_presence: bool,
    },
    KnownFailure,
    KnownAny,
    Opaque {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Consumption {
    pub(crate) path: ValuesPath,
    pub(crate) domain: ConsumptionDomain,
    guard: RuntimeGuard,
    activation: Vec<Guard>,
    scope: String,
    scope_identity: RequirementScope,
    binder: Vec<LoopId>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ConsumptionKey {
    pub(crate) path: ValuesPath,
    pub(crate) domain: ConsumptionDomain,
    scope: String,
    binder: Vec<u64>,
}

impl Consumption {
    pub(super) fn freeze(
        &self,
        remaining: &mut usize,
        memo: &mut ahash::AHashMap<(bool, u64), std::sync::Arc<FrozenRequirementGuard>>,
    ) -> Option<FrozenConsumption> {
        let Self {
            path,
            domain,
            guard,
            activation,
            scope,
            scope_identity,
            binder,
        } = self;
        // Known domains are preimages of the identifiable raw input frozen at consumption.
        // A transformed operand without identity never enters that lane.
        Some(FrozenConsumption {
            path: path.clone(),
            domain: domain.clone(),
            guard: guard.freeze_requirement(scope_identity, remaining, memo)?,
            activation: activation.clone(),
            scope: scope.clone(),
            scope_identity: scope_identity.clone(),
            binder: binder.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrozenConsumption {
    pub(crate) path: ValuesPath,
    pub(crate) domain: ConsumptionDomain,
    guard: std::sync::Arc<FrozenRequirementGuard>,
    activation: Vec<Guard>,
    scope: String,
    scope_identity: RequirementScope,
    binder: Vec<LoopId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FrozenRequirementGuard {
    False,
    True,
    Predicate(Predicate),
    Choice(FrozenChoice),
    Select {
        test: std::sync::Arc<Self>,
        yes: std::sync::Arc<Self>,
        no: std::sync::Arc<Self>,
    },
}

impl FrozenConsumption {
    pub(crate) fn touches_global_namespace(&self, namespace: &ValuesPath) -> bool {
        let is_global = |path: &ValuesPath| path == namespace || path.is_descendant_of(namespace);
        if is_global(&self.path) {
            return true;
        }
        let mut atoms = BTreeSet::new();
        let mut remaining = 16_384;
        if self
            .atoms(
                &mut atoms,
                &mut remaining,
                &mut RequirementIdentities::default(),
            )
            .is_none()
        {
            return true;
        }
        atoms.iter().any(|atom| match atom {
            RequirementAtom::Value(guard) => guard.value_paths().iter().any(is_global),
            RequirementAtom::Choice(_) => false,
        })
    }

    pub(crate) fn charge_grouping(&self, remaining: &mut usize, comparisons: usize) -> Option<()> {
        *remaining = remaining.checked_sub(1)?;
        if self.binder.len() > 128 {
            return None;
        }
        *remaining =
            remaining.checked_sub((self.binder.len() * 8 + 1).checked_mul(comparisons)?)?;
        RequirementAtom::charge_path_inventory(&self.path, remaining, comparisons)?;
        RequirementAtom::charge_text_inventory(&self.scope, remaining, comparisons)?;
        match &self.domain {
            ConsumptionDomain::Known {
                preimage,
                requires_presence: _,
            } => match preimage {
                FailValueRequirement::MemberHost {
                    handled_kinds,
                    complete_domain: _,
                } => {
                    if handled_kinds.len() > 64 {
                        return None;
                    }
                    for kind in handled_kinds {
                        RequirementAtom::charge_text_inventory(kind, remaining, comparisons)?;
                    }
                    Some(())
                }
                FailValueRequirement::SchemaType(kind)
                | FailValueRequirement::SchemaTypeEvenNull(kind) => {
                    RequirementAtom::charge_text_inventory(kind, remaining, comparisons)
                }
                FailValueRequirement::Iterable { allow_integer: _ } => Some(()),
                // Other preimages need an explicit grouping-cost transfer before export.
                _ => None,
            },
            ConsumptionDomain::KnownAny | ConsumptionDomain::KnownFailure => Some(()),
            ConsumptionDomain::Opaque { reason } => {
                RequirementAtom::charge_text_inventory(reason, remaining, comparisons)
            }
        }
    }

    pub(crate) fn fold_key(
        &self,
        identities: &mut RequirementIdentities,
    ) -> Option<ConsumptionKey> {
        let mut binder = Vec::new();
        for id in &self.binder {
            binder.push(identities.binder(&self.scope_identity, *id)?);
        }
        Some(ConsumptionKey {
            path: self.path.clone(),
            domain: self.domain.clone(),
            scope: self.scope.clone(),
            binder,
        })
    }

    pub(crate) fn append_activation(&mut self, guards: &[Guard]) {
        self.activation.extend(guards.iter().cloned());
    }

    pub(crate) fn atoms(
        &self,
        atoms: &mut BTreeSet<RequirementAtom>,
        remaining: &mut usize,
        identities: &mut RequirementIdentities,
    ) -> Option<()> {
        self.guard
            .atoms(atoms, remaining, &mut ahash::AHashSet::new(), identities)?;
        for guard in &self.activation {
            *remaining = remaining.checked_sub(1)?;
            RequirementAtom::charge_value_inventory(
                guard,
                remaining,
                2 + atoms.len().max(1).ilog2() as usize,
            )?;
            atoms.insert(RequirementAtom::Value(guard.clone()));
        }
        Some(())
    }

    pub(crate) fn import<'a>(
        &'a self,
        bdd: &mut RequirementBdd<'a, '_>,
        identities: &RequirementIdentities,
    ) -> Option<RequirementNode> {
        let bound = bound_collection(&self.path);
        let mut root =
            self.guard
                .import(bdd, &mut ahash::AHashMap::new(), bound.as_ref(), identities)?;
        for guard in &self.activation {
            let activation = bdd.atom(&RequirementAtom::Value(guard.clone()))?;
            root = bdd.and(root, activation)?;
        }
        Some(root)
    }
}

impl FrozenRequirementGuard {
    pub(super) fn check_depth(&self, depth: usize, remaining: &mut usize) -> Option<()> {
        let mut pending = vec![(self, depth)];
        while let Some((guard, depth)) = pending.pop() {
            *remaining = remaining.checked_sub(1)?;
            let depth = depth.checked_sub(1)?;
            if let Self::Select { test, yes, no } = guard {
                pending.extend([
                    (test.as_ref(), depth),
                    (yes.as_ref(), depth),
                    (no.as_ref(), depth),
                ]);
            }
        }
        Some(())
    }

    fn atoms(
        &self,
        atoms: &mut BTreeSet<RequirementAtom>,
        remaining: &mut usize,
        seen: &mut ahash::AHashSet<*const Self>,
        identities: &mut RequirementIdentities,
    ) -> Option<()> {
        *remaining = remaining.checked_sub(1)?;
        if !seen.insert(std::ptr::from_ref(self)) {
            return Some(());
        }
        match self {
            Self::False | Self::True => {}
            Self::Choice(choice) => {
                *remaining = remaining.checked_sub(4)?;
                atoms.insert(RequirementAtom::Choice(identities.choice(choice)?));
            }
            Self::Predicate(predicate) => {
                predicate_atoms(
                    predicate,
                    atoms,
                    remaining,
                    &mut ahash::AHashSet::new(),
                    128,
                )?;
            }
            Self::Select { test, yes, no } => {
                test.atoms(atoms, remaining, seen, identities)?;
                yes.atoms(atoms, remaining, seen, identities)?;
                no.atoms(atoms, remaining, seen, identities)?;
            }
        }
        Some(())
    }

    fn import<'a>(
        &'a self,
        bdd: &mut RequirementBdd<'a, '_>,
        memo: &mut ahash::AHashMap<*const Self, RequirementNode>,
        bound: Option<&ValuesPath>,
        identities: &RequirementIdentities,
    ) -> Option<RequirementNode> {
        bdd.charge_visit()?;
        if let Some(root) = memo.get(&std::ptr::from_ref(self)) {
            return Some(*root);
        }
        let root = match self {
            Self::False => helm_schema_core::RequirementBdd::never(),
            Self::True => helm_schema_core::RequirementBdd::always(),
            Self::Predicate(predicate) => {
                if bound.is_none() {
                    bdd.predicate(predicate)?
                } else {
                    import_predicate(predicate, bdd, bound)?
                }
            }
            Self::Choice(choice) => bdd.choice(identities.get_choice(choice)?)?,
            Self::Select { test, yes, no } => {
                let test = test.import(bdd, memo, bound, identities)?;
                let yes = yes.import(bdd, memo, bound, identities)?;
                let no = no.import(bdd, memo, bound, identities)?;
                let yes = bdd.and(test, yes)?;
                let not_test = bdd.not(test)?;
                let no = bdd.and(not_test, no)?;
                bdd.or(yes, no)?
            }
        };
        memo.insert(std::ptr::from_ref(self), root);
        Some(root)
    }

    fn map_paths(
        &self,
        map: &mut impl FnMut(ValuesPath) -> ValuesPath,
        remaining: &mut usize,
        memo: &mut ahash::AHashMap<*const Self, std::sync::Arc<Self>>,
    ) -> Option<std::sync::Arc<Self>> {
        *remaining = remaining.checked_sub(1)?;
        if let Some(mapped) = memo.get(&std::ptr::from_ref(self)) {
            return Some(std::sync::Arc::clone(mapped));
        }
        let mapped = std::sync::Arc::new(match self {
            Self::False => Self::False,
            Self::True => Self::True,
            Self::Choice(choice) => Self::Choice(choice.clone()),
            Self::Predicate(predicate) => {
                let size = predicate.node_work(*remaining)?;
                *remaining = remaining.checked_sub(size)?;
                Self::Predicate(predicate.clone().map_value_paths_shared(map))
            }
            Self::Select { test, yes, no } => Self::Select {
                test: test.map_paths(map, remaining, memo)?,
                yes: yes.map_paths(map, remaining, memo)?,
                no: no.map_paths(map, remaining, memo)?,
            },
        });
        memo.insert(std::ptr::from_ref(self), std::sync::Arc::clone(&mapped));
        Some(mapped)
    }
}

pub(crate) fn collect_requirement_atoms(
    predicate: &Predicate,
    atoms: &mut BTreeSet<RequirementAtom>,
    remaining: &mut usize,
) -> Option<()> {
    predicate_atoms(
        predicate,
        atoms,
        remaining,
        &mut ahash::AHashSet::new(),
        128,
    )
}

fn predicate_atoms(
    predicate: &Predicate,
    atoms: &mut BTreeSet<RequirementAtom>,
    remaining: &mut usize,
    seen: &mut ahash::AHashSet<Predicate>,
    depth: usize,
) -> Option<()> {
    let depth = depth.checked_sub(1)?;
    *remaining = remaining.checked_sub(1)?;
    if !seen.insert(predicate.clone()) {
        return Some(());
    }
    match predicate.kind() {
        helm_schema_core::PredicateKind::Guard(guard) => {
            RequirementAtom::charge_value_inventory(
                guard,
                remaining,
                2 + atoms.len().max(1).ilog2() as usize,
            )?;
            atoms.insert(RequirementAtom::Value(guard.clone()));
        }
        helm_schema_core::PredicateKind::Not(inner) => {
            predicate_atoms(inner, atoms, remaining, seen, depth)?;
        }
        helm_schema_core::PredicateKind::And(parts)
        | helm_schema_core::PredicateKind::Or(parts) => {
            for part in parts {
                predicate_atoms(part, atoms, remaining, seen, depth)?;
            }
        }
        helm_schema_core::PredicateKind::True | helm_schema_core::PredicateKind::False => {}
        helm_schema_core::PredicateKind::Approximate { .. } => return None,
    }
    Some(())
}

impl State {
    pub(super) fn record_consumption(
        &mut self,
        path: ValuesPath,
        domain: ConsumptionDomain,
        selected: &[super::DecisionEdge],
        live: &Predicate,
        condition: &super::output::RuntimeCondition,
        binder: &[LoopId],
    ) {
        let Some(guard) = condition.selected_requirement_guard(selected, live) else {
            return;
        };
        if guard.is_impossible() {
            return;
        }
        self.consumptions.push(Consumption {
            path,
            domain,
            guard,
            activation: Vec::new(),
            scope: condition.requirement_scope(),
            scope_identity: condition.requirement_identity(),
            binder: binder.to_vec(),
        });
    }
}

fn import_predicate(
    predicate: &Predicate,
    bdd: &mut RequirementBdd<'_, '_>,
    bound: Option<&ValuesPath>,
) -> Option<RequirementNode> {
    bdd.charge_visit()?;
    match predicate.kind() {
        helm_schema_core::PredicateKind::True => Some(helm_schema_core::RequirementBdd::always()),
        helm_schema_core::PredicateKind::False => Some(helm_schema_core::RequirementBdd::never()),
        helm_schema_core::PredicateKind::Guard(guard) => {
            if matches!(guard, Guard::Range { path } if Some(path) == bound) {
                return Some(helm_schema_core::RequirementBdd::always());
            }
            if matches!(guard, Guard::Not { path } if Some(path) == bound) {
                return Some(helm_schema_core::RequirementBdd::never());
            }
            bdd.atom(&RequirementAtom::Value(guard.clone()))
        }
        helm_schema_core::PredicateKind::Not(inner) => {
            let root = import_predicate(inner, bdd, bound)?;
            bdd.not(root)
        }
        helm_schema_core::PredicateKind::And(parts) => {
            let mut root = helm_schema_core::RequirementBdd::always();
            for part in parts {
                let part = import_predicate(part, bdd, bound)?;
                root = bdd.and(root, part)?;
            }
            Some(root)
        }
        helm_schema_core::PredicateKind::Or(parts) => {
            let mut root = helm_schema_core::RequirementBdd::never();
            for part in parts {
                let part = import_predicate(part, bdd, bound)?;
                root = bdd.or(root, part)?;
            }
            Some(root)
        }
        helm_schema_core::PredicateKind::Approximate { .. } => None,
    }
}

fn bound_collection(path: &ValuesPath) -> Option<ValuesPath> {
    let members = path
        .segments()
        .filter(|segment| segment.is_each_member())
        .count();
    if members != 1 {
        return None;
    }
    Some(ValuesPath::from_segments(
        path.segments()
            .take_while(|segment| !segment.is_each_member())
            .cloned(),
    ))
}

#[tracing::instrument(skip_all)]
pub(crate) fn map_consumption_paths(
    consumptions: &mut Vec<FrozenConsumption>,
    map: &mut impl FnMut(ValuesPath) -> ValuesPath,
    shared: bool,
) {
    let mut remaining = 1_048_576;
    let mut memo = ahash::AHashMap::new();
    for consumption in consumptions.iter_mut() {
        if !shared {
            memo.clear();
        }
        let Some(guard) = consumption.guard.map_paths(map, &mut remaining, &mut memo) else {
            consumptions.clear();
            return;
        };
        consumption.path = map(consumption.path.clone());
        consumption.guard = guard;
        consumption.activation = std::mem::take(&mut consumption.activation)
            .into_iter()
            .map(|guard| guard.map_value_paths(map))
            .collect();
    }
}

#[cfg(test)]
#[path = "../tests/consumption_observations.rs"]
mod tests;
