use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use ahash::{AHashMap, AHashSet};

use crate::{Guard, GuardDnf, Predicate, PredicateKind};

const FALSE: usize = 0;
const TRUE: usize = 1;
const MAX_BDD_NODES: usize = 4_096;
const MAX_BDD_WORK: usize = 16_384;
const MAX_NORMAL_FORM_PATHS: usize = 256;
const MAX_NORMAL_FORM_LITERALS: usize = 8_192;

/// Per-analysis cache for bounded Boolean predicate operations.
#[derive(Debug, Default)]
pub struct PredicateMemo {
    normalized: RefCell<AHashMap<Predicate, Predicate>>,
    simplified: RefCell<AHashMap<Predicate, Predicate>>,
    normal_forms: RefCell<AHashMap<Predicate, Predicate>>,
    implications: RefCell<AHashMap<(Predicate, Predicate), bool>>,
    feasibility: RefCell<AHashMap<Predicate, bool>>,
}

impl PredicateMemo {
    /// Creates an empty cache owned by one analysis run.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Canonicalizes a predicate and reuses an identical result from this run.
    #[must_use]
    pub fn normalize(&self, predicate: Predicate) -> Predicate {
        if normalization_leaf(&predicate) {
            return predicate;
        }
        if let Some(cached) = self.normalized.borrow().get(&predicate).cloned() {
            return cached;
        }
        let key = predicate.clone();
        let normalized = normalize_with_memo(predicate, self);
        self.normalized.borrow_mut().insert(key, normalized.clone());
        normalized
    }

    /// Normalizes within a shared work allowance, returning `None` on exhaustion.
    ///
    /// Input DAG nodes and Boolean apply steps consume the allowance before work begins.
    #[tracing::instrument(skip_all)]
    #[must_use]
    pub fn normalize_bounded(
        &self,
        predicate: Predicate,
        remaining: &mut usize,
    ) -> Option<Predicate> {
        let Some(size) = predicate.node_work(*remaining) else {
            *remaining = 0;
            tracing::debug!("widen exhausted predicate traversal");
            return None;
        };
        *remaining -= size;
        let predicate = if predicate.contains_approximation() {
            predicate
        } else {
            simplify_structure(predicate, self)
        };
        if normalization_leaf(&predicate) {
            return Some(predicate);
        }
        if let Some(cached) = self.normal_forms.borrow().get(&predicate).cloned() {
            let Some(size) = cached.node_work(*remaining) else {
                *remaining = 0;
                return None;
            };
            *remaining -= size;
            return Some(cached);
        }
        let result = normalize_literal_conjunction(&predicate)
            .or_else(|| normalize_simplified_with_work(predicate.clone(), remaining));
        if result.is_none() {
            tracing::debug!("widen exhausted predicate normalization");
        }
        let result = result?;
        self.normal_forms
            .borrow_mut()
            .insert(predicate, result.clone());
        Some(result)
    }

    /// Whether normalization can leave this predicate potentially true.
    ///
    /// Returns `true` for unknowns and exhausted Boolean budgets, matching
    /// [`Self::normalize`] without constructing the normalized output predicate.
    #[must_use]
    pub fn may_be_true(&self, predicate: Predicate) -> bool {
        if normalization_leaf(&predicate) {
            return predicate != Predicate::False;
        }
        if let Some(cached) = self.normalized.borrow().get(&predicate) {
            return cached != &Predicate::False;
        }
        if let Some(cached) = self.feasibility.borrow().get(&predicate) {
            return *cached;
        }
        let key = predicate.clone();
        let simplified = simplify_structure(predicate, self);
        if normalization_leaf(&simplified) {
            let feasible = simplified != Predicate::False;
            self.feasibility.borrow_mut().insert(key, feasible);
            return feasible;
        }
        let normal_form = self
            .normal_forms
            .borrow()
            .get(&simplified)
            .map(|cached| cached != &Predicate::False);
        let feasible = if let Some(feasible) = normal_form {
            feasible
        } else if simplified == Predicate::False {
            false
        } else if simplified == Predicate::True || simplified.contains_approximation() {
            true
        } else if let Some(normalized) = normalize_literal_conjunction(&simplified) {
            let feasible = normalized != Predicate::False;
            self.normal_forms
                .borrow_mut()
                .insert(simplified, normalized);
            feasible
        } else {
            feasible_simplified(&simplified)
        };
        self.feasibility.borrow_mut().insert(key, feasible);
        feasible
    }

    /// Reports exact entailment and reuses an identical result from this run.
    #[must_use]
    pub fn exactly_implies(&self, antecedent: &Predicate, consequent: &Predicate) -> bool {
        let key = (antecedent.clone(), consequent.clone());
        if let Some(cached) = self.implications.borrow().get(&key) {
            return *cached;
        }
        let implies = exact_implies_uncached(antecedent, consequent);
        self.implications.borrow_mut().insert(key, implies);
        implies
    }
}

fn feasible_simplified(predicate: &Predicate) -> bool {
    let Some(mut bdd) = PredicateBdd::for_predicate(predicate) else {
        return true;
    };
    if bdd.atoms.iter().any(|atom| {
        matches!(
            atom,
            GuardAtom::Other(Guard::Or { paths: _ } | Guard::AnyOf { alternatives: _ })
        )
    }) {
        return normalize_simplified(predicate.clone()) != Predicate::False;
    }
    let Some(root) = bdd.build(predicate) else {
        return true;
    };
    if root == FALSE {
        return false;
    }
    if let Some(path) = bdd.first_true_path(root)
        && !GuardDnf::from_conjunction(path).is_never()
    {
        return true;
    }
    bdd.paths_to(root, TRUE).is_none_or(|paths| {
        paths
            .into_iter()
            .any(|path| !GuardDnf::from_conjunction(path).is_never())
    })
}

fn structural_leaf(predicate: &Predicate) -> bool {
    matches!(
        predicate.kind(),
        PredicateKind::True
            | PredicateKind::False
            | PredicateKind::Guard(_)
            | PredicateKind::Approximate {
                marker: _,
                paths: _,
                role: _,
                sound_subset: _,
            }
    )
}

// An atomic guard already costs one syntax node; its normal forms cannot improve it.
fn normalization_leaf(predicate: &Predicate) -> bool {
    if let PredicateKind::Guard(guard) = predicate.kind() {
        is_atomic_guard(guard)
    } else {
        structural_leaf(predicate)
    }
}

fn is_atomic_guard(guard: &Guard) -> bool {
    match guard {
        Guard::Or { paths: _ } | Guard::AnyOf { alternatives: _ } => false,
        Guard::Truthy { path: _ }
        | Guard::Not { path: _ }
        | Guard::Eq { path: _, value: _ }
        | Guard::NotEq { path: _, value: _ }
        | Guard::Absent { path: _ }
        | Guard::MatchesPattern {
            path: _,
            pattern: _,
            templated: _,
        }
        | Guard::NotMatchesPattern {
            path: _,
            pattern: _,
        }
        | Guard::MatchesValue {
            path: _,
            pattern: _,
        }
        | Guard::CollectionPattern {
            path: _,
            domain: _,
            quantifier: _,
            pattern: _,
        }
        | Guard::RangeKeyPrefix { path: _, prefix: _ }
        | Guard::RangeKeyEquals { path: _, key: _ }
        | Guard::RangeKeyMatches {
            path: _,
            pattern: _,
        }
        | Guard::Range { path: _ }
        | Guard::With { path: _ }
        | Guard::Default { path: _ }
        | Guard::TypeIs {
            path: _,
            schema_type: _,
        }
        | Guard::NotTypeIs {
            path: _,
            schema_type: _,
        }
        | Guard::IntGt { path: _, bound: _ }
        | Guard::IntLt { path: _, bound: _ }
        | Guard::AtMostOneMember { path: _ }
        | Guard::MinMembers { path: _, bound: _ }
        | Guard::HasKey { path: _, key: _ }
        | Guard::NotHasKey { path: _, key: _ }
        | Guard::ContainsEquals { path: _, value: _ }
        | Guard::ContainsMemberEquals {
            path: _,
            member: _,
            value: _,
        }
        | Guard::ContainsTruthyMember { path: _, member: _ } => true,
    }
}

// The certificate guarantees both old normal forms finish within every bound.
// Distinct literals have one true path; the false-path CNF cannot beat its encoding.
fn normalize_literal_conjunction(predicate: &Predicate) -> Option<Predicate> {
    let parts = if let PredicateKind::And(parts) = predicate.kind() {
        parts
    } else {
        std::slice::from_ref(predicate)
    };
    let count = parts.len();
    if count > MAX_NORMAL_FORM_PATHS {
        return None;
    }
    let pairs = count.saturating_mul(count.saturating_add(1));
    if pairs.saturating_mul(2) > MAX_BDD_NODES || pairs / 2 > MAX_NORMAL_FORM_LITERALS {
        return None;
    }
    let mut literals = BTreeMap::new();
    let mut contradictory = false;
    for part in parts {
        let (literal, negated) = match part.kind() {
            PredicateKind::Not(inner) => (inner, true),
            _ => (part, false),
        };
        let PredicateKind::Guard(guard) = literal.kind() else {
            return None;
        };
        if !is_atomic_guard(guard) {
            return None;
        }
        let atom = GuardAtom::new(guard);
        let positive = atom.is_positive() != negated;
        contradictory |= literals
            .insert(atom, positive)
            .is_some_and(|old| old != positive);
    }
    if contradictory {
        return Some(Predicate::False);
    }
    let condition = GuardDnf::from_conjunction(literals.into_iter().map(|(atom, positive)| {
        let literal = Predicate::from(atom.to_guard());
        if positive { literal } else { literal.negated() }
    }));
    let normalized = predicate_from_dnf(&condition);
    Some(
        [predicate.clone(), normalized]
            .into_iter()
            .min_by_key(|candidate| (candidate.syntax_size(), candidate.clone()))
            .unwrap_or(Predicate::False),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum BooleanOp {
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BddNode {
    variable: usize,
    when_false: usize,
    when_true: usize,
}

struct PredicateBdd<'a> {
    atoms: Vec<GuardAtom<'a>>,
    atom_indices: BTreeMap<GuardAtom<'a>, usize>,
    nodes: Vec<BddNode>,
    unique_nodes: AHashMap<BddNode, usize>,
    apply_cache: AHashMap<(BooleanOp, usize, usize), usize>,
    negation_cache: AHashMap<usize, usize>,
    built: AHashMap<usize, usize>,
    remaining_work: usize,
    memoize_walks: bool,
}

pub(crate) fn normalize(predicate: Predicate) -> Predicate {
    normalize_with_memo(predicate, &PredicateMemo::new())
}

#[cfg(test)]
fn normalize_uncached(predicate: Predicate) -> Predicate {
    normalize_with_memo(predicate, &PredicateMemo::new())
}

fn normalize_with_memo(predicate: Predicate, memo: &PredicateMemo) -> Predicate {
    let predicate = simplify_structure(predicate, memo);
    if normalization_leaf(&predicate) {
        return predicate;
    }
    if let Some(cached) = memo.normal_forms.borrow().get(&predicate).cloned() {
        return cached;
    }
    let key = predicate.clone();
    let normalized = normalize_literal_conjunction(&predicate)
        .unwrap_or_else(|| normalize_simplified(predicate));
    memo.normal_forms
        .borrow_mut()
        .insert(key, normalized.clone());
    normalized
}

#[tracing::instrument(skip_all)]
fn normalize_simplified(predicate: Predicate) -> Predicate {
    let mut remaining = MAX_BDD_WORK;
    normalize_simplified_with_work(predicate.clone(), &mut remaining).unwrap_or(predicate)
}

fn normalize_simplified_with_work(
    predicate: Predicate,
    remaining: &mut usize,
) -> Option<Predicate> {
    if predicate.contains_approximation() {
        return Some(predicate);
    }
    let mut bdd = PredicateBdd::for_predicate(&predicate)?;
    bdd.remaining_work = *remaining;
    let root = bdd.build(&predicate);
    *remaining = bdd.remaining_work;
    let root = root?;
    let true_paths = bdd.paths_to_with_work(root, TRUE, remaining);
    let false_paths = bdd.paths_to_with_work(root, FALSE, remaining);
    drop(bdd);
    if *remaining == 0 {
        return None;
    }
    // Normal-form conversion normalizes its emitted literals as new input.
    for paths in [&true_paths, &false_paths].into_iter().flatten() {
        let cost = paths.iter().map(|path| path.len() + 1).sum::<usize>();
        let Some(next) = remaining.checked_sub(cost) else {
            *remaining = 0;
            return None;
        };
        *remaining = next;
    }
    let mut candidates = vec![predicate];
    if let Some(paths) = true_paths {
        candidates.push(predicate_from_dnf(&GuardDnf::from_disjunction(paths)));
    }
    if let Some(paths) = false_paths {
        candidates.push(predicate_from_false_paths(paths));
    }
    Some(
        candidates
            .into_iter()
            .min_by_key(|candidate| (candidate.syntax_size(), candidate.clone()))
            .unwrap_or(Predicate::False),
    )
}

fn simplify_structure(predicate: Predicate, memo: &PredicateMemo) -> Predicate {
    if structural_leaf(&predicate) {
        return predicate;
    }
    if let Some(cached) = memo.simplified.borrow().get(&predicate).cloned() {
        return cached;
    }
    let key = predicate.clone();
    let simplified = simplify_structure_uncached(predicate, memo);
    let mut cache = memo.simplified.borrow_mut();
    cache.insert(key, simplified.clone());
    // Removing approximate factors leaves the exact entailment antecedent unchanged.
    cache
        .entry(simplified.clone())
        .or_insert_with(|| simplified.clone());
    simplified
}

#[tracing::instrument(skip_all)]
fn simplify_structure_uncached(predicate: Predicate, memo: &PredicateMemo) -> Predicate {
    let simplified = match predicate.kind() {
        PredicateKind::Not(inner) => {
            let inner = simplify_structure(inner.clone(), memo);
            match inner.kind() {
                PredicateKind::True => Predicate::False,
                PredicateKind::False => Predicate::True,
                PredicateKind::Not(nested) => nested.clone(),
                _ => Predicate::Not(Box::new(inner)),
            }
        }
        PredicateKind::And(predicates) => {
            let mut factors = BTreeSet::new();
            for predicate in predicates
                .iter()
                .cloned()
                .map(|predicate| simplify_structure(predicate, memo))
            {
                match predicate.kind() {
                    PredicateKind::False => return Predicate::False,
                    PredicateKind::True => {}
                    PredicateKind::And(inner) => factors.extend(inner.iter().cloned()),
                    _ => {
                        factors.insert(predicate);
                    }
                }
            }
            let exact = Predicate::all(
                factors
                    .iter()
                    .filter(|factor| !factor.contains_approximation())
                    .cloned()
                    .collect(),
            );
            factors.retain(|factor| {
                let PredicateKind::Approximate {
                    sound_subset: Some(sound_subset),
                    ..
                } = factor.kind()
                else {
                    return true;
                };
                // The subset implies the opaque runtime condition. When the
                // other exact conjuncts already imply that subset, the
                // runtime condition is known true and contributes nothing.
                !memo.exactly_implies(&exact, sound_subset)
            });
            match factors.len() {
                0 => Predicate::True,
                1 => factors.pop_first().unwrap_or(Predicate::True),
                _ => Predicate::And(factors.into_iter().collect()),
            }
        }
        PredicateKind::Or(predicates) => {
            let mut alternatives = BTreeSet::new();
            for predicate in predicates
                .iter()
                .cloned()
                .map(|predicate| simplify_structure(predicate, memo))
            {
                match predicate.kind() {
                    PredicateKind::True => return Predicate::True,
                    PredicateKind::False => {}
                    PredicateKind::Or(inner) => alternatives.extend(inner.iter().cloned()),
                    _ => {
                        alternatives.insert(predicate);
                    }
                }
            }
            match alternatives.len() {
                0 => Predicate::False,
                1 => alternatives.pop_first().unwrap_or(Predicate::False),
                _ => Predicate::Or(alternatives.into_iter().collect()),
            }
        }
        _ => predicate.clone(),
    };
    if simplified == predicate {
        predicate
    } else {
        simplified
    }
}

pub(crate) fn exact_implies(antecedent: &Predicate, consequent: &Predicate) -> bool {
    exact_implies_uncached(antecedent, consequent)
}

fn exact_implies_uncached(antecedent: &Predicate, consequent: &Predicate) -> bool {
    if antecedent.contains_approximation() || consequent.contains_approximation() {
        return false;
    }
    let counterexample = Predicate::And(vec![
        antecedent.clone(),
        Predicate::Not(Box::new(consequent.clone())),
    ]);
    let Some(mut bdd) = PredicateBdd::for_predicate(&counterexample) else {
        return false;
    };
    bdd.build(&counterexample) == Some(FALSE)
}

#[cfg(test)]
fn memo_sizes(memo: &PredicateMemo) -> (usize, usize) {
    (
        memo.normalized.borrow().len(),
        memo.implications.borrow().len(),
    )
}

impl<'a> PredicateBdd<'a> {
    fn for_predicate(predicate: &'a Predicate) -> Option<Self> {
        predicate.node_work(MAX_BDD_WORK)?;
        let mut atoms = BTreeSet::new();
        let memoize_walks = predicate.syntax_size() > crate::predicate::DIRECT_WALK_NODES;
        collect_atoms(predicate, &mut atoms, &mut AHashSet::new(), memoize_walks)?;
        let atoms = atoms.into_iter().collect::<Vec<_>>();
        let atom_indices = atoms
            .iter()
            .copied()
            .enumerate()
            .map(|(index, atom)| (atom, index))
            .collect();
        Some(Self {
            atoms,
            atom_indices,
            nodes: Vec::new(),
            unique_nodes: AHashMap::new(),
            apply_cache: AHashMap::new(),
            negation_cache: AHashMap::new(),
            built: AHashMap::new(),
            remaining_work: MAX_BDD_WORK,
            memoize_walks,
        })
    }

    fn build(&mut self, predicate: &'a Predicate) -> Option<usize> {
        if self.memoize_walks
            && let Some(result) = self.built.get(&predicate.node_id())
        {
            return Some(*result);
        }
        self.remaining_work = self.remaining_work.checked_sub(1)?;
        let result = self.build_uncached(predicate)?;
        if self.memoize_walks {
            self.built.insert(predicate.node_id(), result);
        }
        Some(result)
    }

    fn build_uncached(&mut self, predicate: &'a Predicate) -> Option<usize> {
        match predicate.kind() {
            PredicateKind::True => Some(TRUE),
            PredicateKind::False => Some(FALSE),
            PredicateKind::Guard(guard) => {
                let atom = GuardAtom::new(guard);
                let variable = *self.atom_indices.get(&atom)?;
                let node = self.make_node(variable, FALSE, TRUE)?;
                if atom.is_positive() {
                    Some(node)
                } else {
                    self.negated(node)
                }
            }
            PredicateKind::Not(inner) => {
                let inner = self.build(inner)?;
                self.negated(inner)
            }
            PredicateKind::And(predicates) => {
                let mut result = TRUE;
                for predicate in predicates {
                    let next = self.build(predicate)?;
                    result = self.apply(BooleanOp::And, result, next)?;
                }
                Some(result)
            }
            PredicateKind::Or(predicates) => {
                let mut result = FALSE;
                for predicate in predicates {
                    let next = self.build(predicate)?;
                    result = self.apply(BooleanOp::Or, result, next)?;
                }
                Some(result)
            }
            PredicateKind::Approximate { .. } => None,
        }
    }

    fn apply(&mut self, op: BooleanOp, mut left: usize, mut right: usize) -> Option<usize> {
        self.remaining_work = self.remaining_work.checked_sub(1)?;
        if left > right {
            std::mem::swap(&mut left, &mut right);
        }
        let terminal = match op {
            BooleanOp::And if left == FALSE => Some(FALSE),
            BooleanOp::And if left == TRUE => Some(right),
            BooleanOp::Or if left == FALSE => Some(right),
            BooleanOp::Or if left == TRUE => Some(TRUE),
            _ if left == right => Some(left),
            _ => None,
        };
        if let Some(terminal) = terminal {
            return Some(terminal);
        }
        let key = (op, left, right);
        if let Some(result) = self.apply_cache.get(&key) {
            return Some(*result);
        }
        let left_variable = self.variable(left);
        let right_variable = self.variable(right);
        let variable = left_variable.min(right_variable);
        let (left_false, left_true) = self.cofactors(left, variable);
        let (right_false, right_true) = self.cofactors(right, variable);
        let when_false = self.apply(op, left_false, right_false)?;
        let when_true = self.apply(op, left_true, right_true)?;
        let result = self.make_node(variable, when_false, when_true)?;
        self.apply_cache.insert(key, result);
        Some(result)
    }

    fn negated(&mut self, node: usize) -> Option<usize> {
        self.remaining_work = self.remaining_work.checked_sub(1)?;
        match node {
            FALSE => return Some(TRUE),
            TRUE => return Some(FALSE),
            _ => {}
        }
        if let Some(negated) = self.negation_cache.get(&node) {
            return Some(*negated);
        }
        let current = self.node(node)?;
        let when_false = self.negated(current.when_false)?;
        let when_true = self.negated(current.when_true)?;
        let negated = self.make_node(current.variable, when_false, when_true)?;
        self.negation_cache.insert(node, negated);
        self.negation_cache.insert(negated, node);
        Some(negated)
    }

    fn make_node(&mut self, variable: usize, when_false: usize, when_true: usize) -> Option<usize> {
        if when_false == when_true {
            return Some(when_false);
        }
        let node = BddNode {
            variable,
            when_false,
            when_true,
        };
        if let Some(existing) = self.unique_nodes.get(&node) {
            return Some(*existing);
        }
        if self.nodes.len() >= MAX_BDD_NODES {
            return None;
        }
        let id = self.nodes.len() + 2;
        self.nodes.push(node);
        self.unique_nodes.insert(node, id);
        Some(id)
    }

    fn node(&self, id: usize) -> Option<BddNode> {
        self.nodes.get(id.checked_sub(2)?).copied()
    }

    fn variable(&self, id: usize) -> usize {
        self.node(id).map_or(usize::MAX, |node| node.variable)
    }

    fn cofactors(&self, id: usize, variable: usize) -> (usize, usize) {
        self.node(id).map_or((id, id), |node| {
            if node.variable == variable {
                (node.when_false, node.when_true)
            } else {
                (id, id)
            }
        })
    }

    fn paths_to(&self, root: usize, target: usize) -> Option<Vec<Vec<Predicate>>> {
        self.paths_to_with_work(root, target, &mut { MAX_BDD_WORK })
    }

    fn paths_to_with_work(
        &self,
        root: usize,
        target: usize,
        remaining: &mut usize,
    ) -> Option<Vec<Vec<Predicate>>> {
        let mut paths = Vec::new();
        let mut current = Vec::new();
        let mut literal_count = 0;
        self.collect_paths(
            root,
            target,
            &mut current,
            &mut paths,
            &mut literal_count,
            remaining,
        )?;
        Some(paths)
    }

    fn first_true_path(&self, mut node: usize) -> Option<Vec<Predicate>> {
        let mut path = Vec::new();
        let mut remaining = MAX_BDD_WORK;
        while node != TRUE {
            let current = self.node(node)?;
            let atom = self
                .atoms
                .get(current.variable)?
                .predicate_with_work(&mut remaining)?;
            if current.when_false == FALSE {
                path.push(atom);
                node = current.when_true;
            } else {
                path.push(atom.negated());
                node = current.when_false;
            }
        }
        Some(path)
    }

    fn collect_paths(
        &self,
        node: usize,
        target: usize,
        current: &mut Vec<Predicate>,
        paths: &mut Vec<Vec<Predicate>>,
        literal_count: &mut usize,
        remaining: &mut usize,
    ) -> Option<()> {
        let cost = if node == target { current.len() + 1 } else { 1 };
        let Some(next) = remaining.checked_sub(cost) else {
            *remaining = 0;
            return None;
        };
        *remaining = next;
        if node == target {
            if paths.len() >= MAX_NORMAL_FORM_PATHS
                || *literal_count + current.len() > MAX_NORMAL_FORM_LITERALS
            {
                return None;
            }
            *literal_count += current.len();
            paths.push(current.clone());
            return Some(());
        }
        if node == FALSE || node == TRUE {
            return Some(());
        }
        let node = self.node(node)?;
        let atom = self
            .atoms
            .get(node.variable)?
            .predicate_with_work(remaining)?;
        current.push(atom.negated());
        self.collect_paths(
            node.when_false,
            target,
            current,
            paths,
            literal_count,
            remaining,
        )?;
        current.pop();
        current.push(atom);
        self.collect_paths(
            node.when_true,
            target,
            current,
            paths,
            literal_count,
            remaining,
        )?;
        current.pop();
        Some(())
    }
}

fn collect_atoms<'a>(
    predicate: &'a Predicate,
    atoms: &mut BTreeSet<GuardAtom<'a>>,
    visited: &mut AHashSet<usize>,
    memoize_walks: bool,
) -> Option<()> {
    if memoize_walks && !visited.insert(predicate.node_id()) {
        return Some(());
    }
    match predicate.kind() {
        PredicateKind::Approximate { .. } => return None,
        PredicateKind::Guard(guard) => {
            atoms.insert(GuardAtom::new(guard));
        }
        PredicateKind::True
        | PredicateKind::False
        | PredicateKind::Not(_)
        | PredicateKind::And(_)
        | PredicateKind::Or(_) => {}
    }
    for child in predicate.children() {
        collect_atoms(child, atoms, visited, memoize_walks)?;
    }
    Some(())
}

#[derive(Clone, Copy)]
enum GuardAtom<'a> {
    Truthy(&'a crate::ValuesPath, bool),
    Eq(&'a crate::ValuesPath, &'a crate::GuardValue, bool),
    TypeIs(&'a crate::ValuesPath, &'a str, bool),
    Other(&'a Guard),
}

impl<'a> GuardAtom<'a> {
    fn predicate_with_work(self, remaining: &mut usize) -> Option<Predicate> {
        let cost = match self {
            Self::Other(guard) => crate::predicate::guard_syntax_size(guard),
            _ => 1,
        };
        let Some(next) = remaining.checked_sub(cost) else {
            *remaining = 0;
            return None;
        };
        *remaining = next;
        Some(Predicate::from(self.to_guard()))
    }

    fn new(guard: &'a Guard) -> Self {
        match guard {
            Guard::Truthy { path } => Self::Truthy(path, true),
            Guard::Not { path } => Self::Truthy(path, false),
            Guard::Eq { path, value } => Self::Eq(path, value, true),
            Guard::NotEq { path, value } => Self::Eq(path, value, false),
            Guard::TypeIs { path, schema_type } => Self::TypeIs(path, schema_type, true),
            Guard::NotTypeIs { path, schema_type } => Self::TypeIs(path, schema_type, false),
            _ => Self::Other(guard),
        }
    }

    fn is_positive(self) -> bool {
        match self {
            Self::Truthy(_, positive) | Self::Eq(_, _, positive) | Self::TypeIs(_, _, positive) => {
                positive
            }
            Self::Other(_) => true,
        }
    }

    fn to_guard(self) -> Guard {
        match self {
            Self::Truthy(path, _) => Guard::Truthy { path: path.clone() },
            Self::Eq(path, value, _) => Guard::Eq {
                path: path.clone(),
                value: value.clone(),
            },
            Self::TypeIs(path, schema_type, _) => Guard::TypeIs {
                path: path.clone(),
                schema_type: schema_type.to_string(),
            },
            Self::Other(guard) => guard.clone(),
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Truthy(_, _) => 0,
            Self::Eq(_, _, _) => 2,
            Self::TypeIs(_, _, _) => 15,
            Self::Other(guard) => guard_rank(guard),
        }
    }
}

impl PartialEq for GuardAtom<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for GuardAtom<'_> {}

impl PartialOrd for GuardAtom<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GuardAtom<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank()
            .cmp(&other.rank())
            .then_with(|| match (*self, *other) {
                (Self::Truthy(left, _), Self::Truthy(right, _)) => left.cmp(right),
                (Self::Eq(left_path, left_value, _), Self::Eq(right_path, right_value, _)) => {
                    left_path
                        .cmp(right_path)
                        .then_with(|| left_value.cmp(right_value))
                }
                (
                    Self::TypeIs(left_path, left_type, _),
                    Self::TypeIs(right_path, right_type, _),
                ) => left_path
                    .cmp(right_path)
                    .then_with(|| left_type.cmp(right_type)),
                (Self::Other(left), Self::Other(right)) => left.cmp(right),
                _ => std::cmp::Ordering::Equal,
            })
    }
}

fn guard_rank(guard: &Guard) -> u8 {
    match guard {
        Guard::Truthy { .. } => 0,
        Guard::Not { .. } => 1,
        Guard::Eq { .. } => 2,
        Guard::NotEq { .. } => 3,
        Guard::Absent { .. } => 4,
        Guard::MatchesPattern { .. } => 5,
        Guard::NotMatchesPattern { .. } => 6,
        Guard::RangeKeyPrefix { .. } => 7,
        Guard::RangeKeyEquals { .. } => 8,
        Guard::RangeKeyMatches { .. } => 9,
        Guard::Or { .. } => 10,
        Guard::AnyOf { .. } => 11,
        Guard::Range { .. } => 12,
        Guard::With { .. } => 13,
        Guard::Default { .. } => 14,
        Guard::TypeIs { .. } => 15,
        Guard::NotTypeIs { .. } => 16,
        Guard::IntGt { .. } => 17,
        Guard::IntLt { .. } => 18,
        Guard::AtMostOneMember { .. } => 19,
        Guard::MinMembers { .. } => 20,
        Guard::HasKey { .. } => 21,
        Guard::NotHasKey { .. } => 22,
        Guard::ContainsEquals { .. } => 23,
        Guard::ContainsMemberEquals { .. } => 24,
        Guard::ContainsTruthyMember { .. } => 25,
        Guard::MatchesValue {
            path: _,
            pattern: _,
        } => 27,
        Guard::CollectionPattern {
            path: _,
            domain: _,
            quantifier: _,
            pattern: _,
        } => 26,
    }
}

fn predicate_from_dnf(condition: &GuardDnf) -> Predicate {
    if condition.is_never() {
        return Predicate::False;
    }
    if condition.is_unconditional() {
        return Predicate::True;
    }
    let alternatives = condition
        .disjuncts()
        .iter()
        .map(|conjunction| Predicate::all(conjunction.iter().cloned().collect()))
        .collect::<Vec<_>>();
    match alternatives.as_slice() {
        [] => Predicate::False,
        [predicate] => predicate.clone(),
        _ => Predicate::Or(alternatives),
    }
}

fn predicate_from_false_paths(paths: Vec<Vec<Predicate>>) -> Predicate {
    let clauses = paths
        .into_iter()
        .map(|path| {
            let literals = path
                .into_iter()
                .map(|literal| literal.negated())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            match literals.as_slice() {
                [] => Predicate::False,
                [literal] => literal.clone(),
                _ => Predicate::Or(literals),
            }
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    Predicate::all(clauses)
}

#[cfg(test)]
#[path = "tests/predicate_bdd.rs"]
mod tests;
