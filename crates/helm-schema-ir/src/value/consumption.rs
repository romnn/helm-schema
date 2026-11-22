//! Executed consumptions retain normalized input preimages and their runtime guards.

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::{
    FailValueRequirement, Guard, Predicate, RequirementAtom, RequirementBdd, RequirementNode,
    ValuesPath,
};

use super::guard::RuntimeGuard;
use super::{LoopId, State};

pub(super) const MAX_REQUIREMENT_FREEZE_WORK: usize = 262_144;
const MAX_REQUIREMENT_MAPPING_WORK: usize = 1_048_576;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ConsumptionBinder {
    call: u64,
    ordinal: u64,
}

// Each fold projects one independent scope; runtime decision IDs are its stable ordinals.
#[derive(Default)]
pub(crate) struct RequirementIdentities {
    scopes: ahash::AHashMap<usize, u64>,
    choices: ahash::AHashMap<ChoiceKey, helm_schema_core::RequirementChoice>,
    unknowns: ahash::AHashMap<usize, helm_schema_core::RequirementChoice>,
}

impl RequirementIdentities {
    fn scope(&mut self, scope: &RequirementScope) -> Option<u64> {
        let next = u64::try_from(self.scopes.len()).ok()?;
        Some(*self.scopes.entry(scope.address()).or_insert(next))
    }

    fn choice(&mut self, choice: &FrozenChoice) -> Option<helm_schema_core::RequirementChoice> {
        let FrozenChoice { scope, decision } = choice;
        let scope = self.scope(scope)?;
        Some(
            self.choices
                .entry(ChoiceKey {
                    scope,
                    decision: *decision,
                })
                .or_insert_with(|| helm_schema_core::RequirementChoice {
                    scope: scope.to_string(),
                    decision: *decision,
                })
                .clone(),
        )
    }

    fn unknown(&mut self, scope: &RequirementScope) -> Option<helm_schema_core::RequirementChoice> {
        let ordinal = self.scope(scope)?;
        Some(
            self.unknowns
                .entry(scope.address())
                .or_insert_with(|| helm_schema_core::RequirementChoice {
                    scope: format!("unknown:{ordinal}"),
                    decision: 0,
                })
                .clone(),
        )
    }

    fn get_unknown(
        &self,
        scope: &RequirementScope,
    ) -> Option<&helm_schema_core::RequirementChoice> {
        self.unknowns.get(&scope.address())
    }

    fn get_choice(&self, choice: &FrozenChoice) -> Option<&helm_schema_core::RequirementChoice> {
        let FrozenChoice { scope, decision } = choice;
        let scope = *self.scopes.get(&scope.address())?;
        self.choices.get(&ChoiceKey {
            scope,
            decision: *decision,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ConsumptionKey {
    pub(crate) path: ValuesPath,
    pub(crate) domain: ConsumptionDomain,
    scope: String,
    binder: Vec<ConsumptionBinder>,
}

impl Consumption {
    pub(super) fn history_fingerprint(&self) -> u64 {
        let Self {
            path,
            domain: _,
            guard,
            activation: _,
            scope: _,
            scope_identity,
            binder,
        } = self;
        super::history::fingerprint(&(path, guard.id(), scope_identity.address(), binder))
    }

    pub(super) fn freeze(
        &self,
        remaining: &mut usize,
        memo: &mut ahash::AHashMap<(bool, u64), FrozenRequirementEntry>,
    ) -> FrozenConsumption {
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
        FrozenConsumption {
            path: path.clone(),
            domain: domain.clone(),
            guard: guard.freeze_requirement(scope_identity, remaining, memo).unwrap_or_else(|| {
                let _bound = tracing::debug_span!(target: "helm_schema_ir::counters", "requirement_freeze_bound").entered();
                std::sync::Arc::new(FrozenRequirementGuard::Unknown(scope_identity.clone()))
            }),
            activation: activation.clone(),
            scope: scope.clone(),
            scope_identity: scope_identity.clone(),
            binder: binder.clone(),
        }
    }
}

/// A frozen subtree and its cache-independent work and nesting charges.
#[derive(Clone)]
pub(super) struct FrozenRequirementEntry {
    pub(super) guard: std::sync::Arc<FrozenRequirementGuard>,
    pub(super) work: usize,
    pub(super) depth: usize,
}

impl FrozenRequirementEntry {
    pub(super) fn charged(&self, remaining: &mut usize, depth: usize) -> Option<Self> {
        if self.depth > depth {
            return None;
        }
        *remaining = remaining.checked_sub(self.work)?;
        Some(Self {
            guard: std::sync::Arc::clone(&self.guard),
            work: self.work,
            depth: self.depth,
        })
    }
}

struct MappedRequirementEntry {
    guard: std::sync::Arc<FrozenRequirementGuard>,
    work: usize,
}

/// Guard nodes and predicates one fold's atom inventory already walked.
#[derive(Default)]
pub(crate) struct GuardWalk {
    guards: ahash::AHashSet<*const FrozenRequirementGuard>,
    predicates: ahash::AHashSet<Predicate>,
}

/// Guard nodes already imported into one fold's graph.
#[derive(Default)]
pub(crate) struct GuardImports(ahash::AHashMap<*const FrozenRequirementGuard, RequirementNode>);

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
    /// A positive root whose reach could not be frozen or mapped.
    Unknown(RequirementScope),
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

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RequirementOrder {
    False,
    True,
    Predicate(Predicate),
    Choice(helm_schema_core::RequirementChoice),
    Unknown(helm_schema_core::RequirementChoice),
    Select { test: usize, yes: usize, no: usize },
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RequirementOrderKey {
    activation: Vec<Guard>,
    root: usize,
}

#[derive(Default)]
struct OrderInventory {
    seen: ahash::AHashMap<*const FrozenRequirementGuard, usize>,
    unique: BTreeMap<RequirementOrder, usize>,
    nodes: Vec<RequirementOrder>,
}

struct InternedRequirementGuard {
    order: RequirementOrder,
    guard: std::sync::Arc<FrozenRequirementGuard>,
}

#[derive(Default)]
pub(crate) struct RequirementGuardInterner {
    unique: BTreeMap<RequirementOrder, usize>,
    entries: Vec<InternedRequirementGuard>,
}

impl RequirementGuardInterner {
    fn intern(
        &mut self,
        nodes: &[RequirementOrder],
        root: usize,
        scope: &RequirementScope,
    ) -> Option<usize> {
        if root >= nodes.len() || nodes.iter().enumerate().any(|(index, node)| {
            matches!(node, RequirementOrder::Select { test, yes, no } if *test >= index || *yes >= index || *no >= index)
        }) {
            return None;
        }
        let mut indices = Vec::new();
        for node in nodes {
            let canonical = match node {
                RequirementOrder::Select { test, yes, no } => RequirementOrder::Select {
                    test: *indices.get(*test)?,
                    yes: *indices.get(*yes)?,
                    no: *indices.get(*no)?,
                },
                RequirementOrder::False
                | RequirementOrder::True
                | RequirementOrder::Predicate(_)
                | RequirementOrder::Choice(_)
                | RequirementOrder::Unknown(_) => node.clone(),
            };
            let index = if let Some(index) = self.unique.get(&canonical) {
                *index
            } else {
                let guard = match &canonical {
                    RequirementOrder::False => FrozenRequirementGuard::False,
                    RequirementOrder::True => FrozenRequirementGuard::True,
                    RequirementOrder::Predicate(predicate) => {
                        FrozenRequirementGuard::Predicate(predicate.clone())
                    }
                    RequirementOrder::Choice(choice) => {
                        FrozenRequirementGuard::Choice(FrozenChoice {
                            scope: scope.clone(),
                            decision: choice.decision,
                        })
                    }
                    RequirementOrder::Unknown(_) => FrozenRequirementGuard::Unknown(scope.clone()),
                    RequirementOrder::Select { test, yes, no } => FrozenRequirementGuard::Select {
                        test: std::sync::Arc::clone(&self.entries.get(*test)?.guard),
                        yes: std::sync::Arc::clone(&self.entries.get(*yes)?.guard),
                        no: std::sync::Arc::clone(&self.entries.get(*no)?.guard),
                    },
                };
                let index = self.entries.len();
                self.entries.push(InternedRequirementGuard {
                    order: canonical.clone(),
                    guard: std::sync::Arc::new(guard),
                });
                self.unique.insert(canonical, index);
                index
            };
            indices.push(index);
        }
        indices.get(root).copied()
    }

    /// Compares cone-local roots by content, independently of their insertion indices.
    pub(crate) fn compare_keys(
        &self,
        left: &RequirementOrderKey,
        right: &RequirementOrderKey,
    ) -> std::cmp::Ordering {
        left.activation
            .cmp(&right.activation)
            .then_with(|| self.compare_roots(left.root, right.root))
    }

    fn compare_roots(&self, left: usize, right: usize) -> std::cmp::Ordering {
        use RequirementOrder::{Choice, False, Predicate, Select, True, Unknown};
        use std::cmp::Ordering;
        if left == right {
            return Ordering::Equal;
        }
        match (self.entries.get(left), self.entries.get(right)) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(left), Some(right)) => match (&left.order, &right.order) {
                (
                    Select {
                        test: left_test,
                        yes: left_yes,
                        no: left_no,
                    },
                    Select {
                        test: right_test,
                        yes: right_yes,
                        no: right_no,
                    },
                ) => self
                    .compare_roots(*left_test, *right_test)
                    .then_with(|| self.compare_roots(*left_yes, *right_yes))
                    .then_with(|| self.compare_roots(*left_no, *right_no)),
                (
                    False | True | Predicate(_) | Choice(_) | Unknown(_),
                    False
                    | True
                    | Predicate(_)
                    | Choice(_)
                    | Unknown(_)
                    | Select {
                        test: _,
                        yes: _,
                        no: _,
                    },
                )
                | (
                    Select {
                        test: _,
                        yes: _,
                        no: _,
                    },
                    False | True | Predicate(_) | Choice(_) | Unknown(_),
                ) => left.order.cmp(&right.order),
            },
        }
    }
}

impl FrozenConsumption {
    pub(crate) fn is_raw_impossible(&self) -> bool {
        matches!(self.guard.as_ref(), FrozenRequirementGuard::False)
    }

    /// Reports a literal true execution guard with no added chart activation.
    pub(crate) fn is_raw_unconditional(&self) -> bool {
        let Self {
            path: _,
            domain: _,
            guard,
            activation,
            scope: _,
            scope_identity: _,
            binder: _,
        } = self;
        matches!(guard.as_ref(), FrozenRequirementGuard::True) && activation.is_empty()
    }

    pub(crate) fn order_key(
        &self,
        identities: &mut RequirementIdentities,
        interner: &mut RequirementGuardInterner,
    ) -> Option<RequirementOrderKey> {
        let mut activation = self.activation.clone();
        activation.sort();
        let mut inventory = OrderInventory::default();
        let root = self
            .guard
            .order_key(identities, &mut 65_536, &mut inventory)?;
        // A rejected walk owns only this scratch inventory; no partial nodes enter the cone.
        let root = interner.intern(&inventory.nodes, root, &self.scope_identity)?;
        Some(RequirementOrderKey { activation, root })
    }

    /// Clones the root already interned from this occurrence's bounded inventory.
    pub(crate) fn canonicalized(
        &self,
        key: &RequirementOrderKey,
        interner: &RequirementGuardInterner,
    ) -> Option<Self> {
        let mut result = self.clone();
        result.guard = std::sync::Arc::clone(&interner.entries.get(key.root)?.guard);
        result.activation.clone_from(&key.activation);
        Some(result)
    }

    /// Whether this values test retains its ordinary meaning when imported at this member.
    pub(crate) fn can_split_on(&self, atom: &Guard) -> bool {
        let bound = bound_collection(&self.path);
        !matches!(atom, Guard::Range { path } | Guard::Not { path } if Some(path) == bound.as_ref())
    }

    /// Shannon cofactor on one exact values atom; unresolved choices remain shared.
    pub(crate) fn cofactor(&self, atom: &Guard, taken: bool) -> Option<Self> {
        let mut result = self.clone();
        result.activation.clear();
        for guard in &self.activation {
            match RequirementBdd::cofactor_atom(guard, atom, taken) {
                Some(false) => {
                    result.guard = std::sync::Arc::new(FrozenRequirementGuard::False);
                    result.activation.clear();
                    return Some(result);
                }
                Some(true) => {}
                None => result.activation.push(guard.clone()),
            }
        }
        let mut remaining = 65_536;
        result.guard = self.guard.cofactor(
            atom,
            taken,
            bound_collection(&self.path).as_ref(),
            &mut remaining,
            &mut ahash::AHashMap::new(),
        )?;
        Some(result)
    }

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
                &mut GuardWalk::default(),
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

    pub(crate) fn fold_key(&self) -> ConsumptionKey {
        let Self {
            path,
            domain,
            guard: _,
            activation: _,
            scope,
            scope_identity: _,
            binder,
        } = self;
        ConsumptionKey {
            path: path.clone(),
            domain: domain.clone(),
            scope: scope.clone(),
            binder: binder
                .iter()
                .map(|LoopId { call, ordinal }| ConsumptionBinder {
                    call: *call,
                    ordinal: *ordinal,
                })
                .collect(),
        }
    }

    pub(crate) fn requirement_scope_key(&self) -> usize {
        self.scope_identity.address()
    }

    pub(crate) fn append_activation(&mut self, guards: &[Guard]) {
        self.activation.extend(guards.iter().cloned());
    }

    /// Inventories this occurrence's atoms; `walk` skips guard nodes an earlier occurrence of the
    /// same fold already charged.
    pub(crate) fn atoms(
        &self,
        atoms: &mut BTreeSet<RequirementAtom>,
        remaining: &mut usize,
        walk: &mut GuardWalk,
        identities: &mut RequirementIdentities,
    ) -> Option<()> {
        self.guard.atoms(atoms, remaining, walk, identities)?;
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

    /// Imports this occurrence's guard. `shared` memoizes guard nodes across the occurrences of
    /// one fold: they share a path, so each node imports to the same graph node.
    pub(crate) fn import<'a>(
        &'a self,
        bdd: &mut RequirementBdd<'a, '_>,
        shared: &mut GuardImports,
        identities: &RequirementIdentities,
    ) -> Option<RequirementNode> {
        let bound = bound_collection(&self.path);
        let mut root = self
            .guard
            .import(bdd, &mut shared.0, bound.as_ref(), identities)?;
        for guard in &self.activation {
            let activation = bdd.atom(&RequirementAtom::Value(guard.clone()))?;
            root = bdd.and(root, activation)?;
        }
        Some(root)
    }
}

impl FrozenRequirementGuard {
    fn order_key(
        &self,
        identities: &mut RequirementIdentities,
        remaining: &mut usize,
        inventory: &mut OrderInventory,
    ) -> Option<usize> {
        *remaining = remaining.checked_sub(1)?;
        let pointer = std::ptr::from_ref(self);
        if let Some(key) = inventory.seen.get(&pointer) {
            return Some(*key);
        }
        let key = match self {
            Self::Unknown(scope) => RequirementOrder::Unknown(identities.unknown(scope)?),
            Self::False => RequirementOrder::False,
            Self::True => RequirementOrder::True,
            Self::Predicate(predicate) => RequirementOrder::Predicate(predicate.clone()),
            Self::Choice(choice) => RequirementOrder::Choice(identities.choice(choice)?),
            Self::Select { test, yes, no } => RequirementOrder::Select {
                test: test.order_key(identities, remaining, inventory)?,
                yes: yes.order_key(identities, remaining, inventory)?,
                no: no.order_key(identities, remaining, inventory)?,
            },
        };
        let next = inventory.nodes.len();
        let index = *inventory.unique.entry(key.clone()).or_insert(next);
        if index == next {
            inventory.nodes.push(key);
        }
        inventory.seen.insert(pointer, index);
        Some(index)
    }

    fn cofactor(
        &self,
        atom: &Guard,
        taken: bool,
        bound: Option<&ValuesPath>,
        remaining: &mut usize,
        memo: &mut ahash::AHashMap<*const Self, std::sync::Arc<Self>>,
    ) -> Option<std::sync::Arc<Self>> {
        *remaining = remaining.checked_sub(1)?;
        let pointer = std::ptr::from_ref(self);
        if let Some(guard) = memo.get(&pointer) {
            return Some(std::sync::Arc::clone(guard));
        }
        let guard = match self {
            Self::False | Self::True | Self::Choice(_) | Self::Unknown(_) => {
                std::sync::Arc::new(self.clone())
            }
            Self::Predicate(predicate) => {
                let cofactor = cofactor_predicate(predicate, atom, taken, bound, remaining)?;
                std::sync::Arc::new(match cofactor.kind() {
                    helm_schema_core::PredicateKind::True => Self::True,
                    helm_schema_core::PredicateKind::False => Self::False,
                    _ => Self::Predicate(cofactor),
                })
            }
            Self::Select { test, yes, no } => {
                let test = test.cofactor(atom, taken, bound, remaining, memo)?;
                match test.as_ref() {
                    Self::True => yes.cofactor(atom, taken, bound, remaining, memo)?,
                    Self::False => no.cofactor(atom, taken, bound, remaining, memo)?,
                    _ => {
                        let yes = yes.cofactor(atom, taken, bound, remaining, memo)?;
                        let no = no.cofactor(atom, taken, bound, remaining, memo)?;
                        if yes == no {
                            yes
                        } else {
                            std::sync::Arc::new(Self::Select { test, yes, no })
                        }
                    }
                }
            }
        };
        memo.insert(pointer, std::sync::Arc::clone(&guard));
        Some(guard)
    }

    fn atoms(
        &self,
        atoms: &mut BTreeSet<RequirementAtom>,
        remaining: &mut usize,
        walk: &mut GuardWalk,
        identities: &mut RequirementIdentities,
    ) -> Option<()> {
        *remaining = remaining.checked_sub(1)?;
        if !walk.guards.insert(std::ptr::from_ref(self)) {
            return Some(());
        }
        match self {
            Self::False | Self::True => {}
            Self::Unknown(scope) => {
                *remaining = remaining.checked_sub(4)?;
                atoms.insert(RequirementAtom::Choice(identities.unknown(scope)?));
            }
            Self::Choice(choice) => {
                *remaining = remaining.checked_sub(4)?;
                atoms.insert(RequirementAtom::Choice(identities.choice(choice)?));
            }
            Self::Predicate(predicate) => {
                predicate_atoms(predicate, atoms, remaining, &mut walk.predicates, 128)?;
            }
            Self::Select { test, yes, no } => {
                test.atoms(atoms, remaining, walk, identities)?;
                yes.atoms(atoms, remaining, walk, identities)?;
                no.atoms(atoms, remaining, walk, identities)?;
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
            Self::Unknown(scope) => bdd.choice(identities.get_unknown(scope)?)?,
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
        memo: &mut ahash::AHashMap<*const Self, MappedRequirementEntry>,
    ) -> Option<std::sync::Arc<Self>> {
        let initial = *remaining;
        if let Some(mapped) = memo.get(&std::ptr::from_ref(self)) {
            *remaining = remaining.checked_sub(mapped.work)?;
            return Some(std::sync::Arc::clone(&mapped.guard));
        }
        *remaining = remaining.checked_sub(1)?;
        let mapped = std::sync::Arc::new(match self {
            Self::Unknown(scope) => Self::Unknown(scope.clone()),
            Self::False => Self::False,
            Self::True => Self::True,
            Self::Choice(choice) => Self::Choice(choice.clone()),
            Self::Predicate(predicate) => {
                let size = predicate.node_work(MAX_REQUIREMENT_MAPPING_WORK)?;
                *remaining = remaining.checked_sub(size)?;
                Self::Predicate(predicate.clone().map_value_paths_shared(map))
            }
            Self::Select { test, yes, no } => Self::Select {
                test: test.map_paths(map, remaining, memo)?,
                yes: yes.map_paths(map, remaining, memo)?,
                no: no.map_paths(map, remaining, memo)?,
            },
        });
        memo.insert(
            std::ptr::from_ref(self),
            MappedRequirementEntry {
                guard: std::sync::Arc::clone(&mapped),
                work: initial - *remaining,
            },
        );
        Some(mapped)
    }
}

fn cofactor_predicate(
    predicate: &Predicate,
    atom: &Guard,
    taken: bool,
    bound: Option<&ValuesPath>,
    remaining: &mut usize,
) -> Option<Predicate> {
    use helm_schema_core::PredicateKind;
    *remaining = remaining.checked_sub(1)?;
    Some(match predicate.kind() {
        PredicateKind::Guard(guard) => match bound_guard_constant(guard, bound)
            .or_else(|| RequirementBdd::cofactor_atom(guard, atom, taken))
        {
            Some(true) => Predicate::True,
            Some(false) => Predicate::False,
            None => predicate.clone(),
        },
        PredicateKind::True | PredicateKind::False | PredicateKind::Approximate { .. } => {
            predicate.clone()
        }
        PredicateKind::Not(inner) => {
            cofactor_predicate(inner, atom, taken, bound, remaining)?.negated()
        }
        PredicateKind::And(parts) => {
            let mut projected = Vec::new();
            for part in parts {
                let part = cofactor_predicate(part, atom, taken, bound, remaining)?;
                match part.kind() {
                    PredicateKind::False => return Some(Predicate::False),
                    PredicateKind::True => {}
                    _ => projected.push(part),
                }
            }
            Predicate::all(projected)
        }
        PredicateKind::Or(parts) => {
            let mut projected = Vec::new();
            for part in parts {
                let part = cofactor_predicate(part, atom, taken, bound, remaining)?;
                match part.kind() {
                    PredicateKind::True => return Some(Predicate::True),
                    PredicateKind::False => {}
                    _ => projected.push(part),
                }
            }
            Predicate::Or(projected).normalize_boolean()
        }
    })
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
    /// Records a proved failure independently of a raw operand identity.
    pub(super) fn record_failure(&mut self, live: &Predicate) {
        if live == &Predicate::False {
            return;
        }
        let condition = super::output::RuntimeCondition::at_requirement(self);
        let binder = self.loops.clone();
        self.record_consumption(
            ValuesPath::default(),
            ConsumptionDomain::KnownFailure,
            &[],
            live,
            &condition,
            &binder,
        );
    }

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
            if let Some(value) = bound_guard_constant(guard, bound) {
                return Some(if value {
                    RequirementBdd::always()
                } else {
                    RequirementBdd::never()
                });
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

fn bound_guard_constant(guard: &Guard, bound: Option<&ValuesPath>) -> Option<bool> {
    match guard {
        Guard::Range { path } if Some(path) == bound => Some(true),
        Guard::Not { path } if Some(path) == bound => Some(false),
        _ => None,
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
    consumptions: &mut [FrozenConsumption],
    map: &mut impl FnMut(ValuesPath) -> ValuesPath,
    shared: bool,
) {
    let mut memo = ahash::AHashMap::new();
    for consumption in consumptions.iter_mut() {
        let FrozenConsumption {
            path,
            domain: _,
            guard,
            activation,
            scope: _,
            scope_identity,
            binder: _,
        } = consumption;
        let mut remaining = MAX_REQUIREMENT_MAPPING_WORK;
        if !shared {
            memo.clear();
        }
        let mapped = guard.map_paths(map, &mut remaining, &mut memo).unwrap_or_else(|| {
            let _bound = tracing::debug_span!(target: "helm_schema_ir::counters", "requirement_mapping_bound").entered();
            std::sync::Arc::new(FrozenRequirementGuard::Unknown(scope_identity.clone()))
        });
        *path = map(path.clone());
        *guard = mapped;
        *activation = std::mem::take(activation)
            .into_iter()
            .map(|guard| guard.map_value_paths(map))
            .collect();
    }
}

#[cfg(test)]
#[path = "../tests/consumption_observations.rs"]
mod tests;
