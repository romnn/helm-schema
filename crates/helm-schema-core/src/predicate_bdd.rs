use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::{Guard, GuardDnf, Predicate, PredicateKind};

const FALSE: usize = 0;
const TRUE: usize = 1;
const MAX_BDD_NODES: usize = 4_096;
const MAX_NORMAL_FORM_PATHS: usize = 256;
const MAX_NORMAL_FORM_LITERALS: usize = 8_192;

/// Per-analysis cache for bounded Boolean predicate operations.
#[derive(Debug, Default)]
pub struct PredicateMemo {
    normalized: RefCell<HashMap<Predicate, Predicate>>,
    implications: RefCell<HashMap<(Predicate, Predicate), bool>>,
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
        if let Some(cached) = self.normalized.borrow().get(&predicate).cloned() {
            return cached;
        }
        let key = predicate.clone();
        let normalized = normalize_with_memo(predicate, Some(self));
        self.normalized.borrow_mut().insert(key, normalized.clone());
        normalized
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BooleanOp {
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BddNode {
    variable: usize,
    when_false: usize,
    when_true: usize,
}

struct PredicateBdd<'a> {
    atoms: Vec<GuardAtom<'a>>,
    atom_indices: BTreeMap<GuardAtom<'a>, usize>,
    nodes: Vec<BddNode>,
    unique_nodes: BTreeMap<BddNode, usize>,
    apply_cache: BTreeMap<(BooleanOp, usize, usize), usize>,
    negation_cache: BTreeMap<usize, usize>,
}

pub(crate) fn normalize(predicate: Predicate) -> Predicate {
    normalize_with_memo(predicate, None)
}

#[cfg(test)]
fn normalize_uncached(predicate: Predicate) -> Predicate {
    normalize_with_memo(predicate, None)
}

fn normalize_with_memo(predicate: Predicate, memo: Option<&PredicateMemo>) -> Predicate {
    let predicate = simplify_structure(predicate, memo);
    if predicate.contains_approximation() {
        return predicate;
    }
    let Some(mut bdd) = PredicateBdd::for_predicate(&predicate) else {
        return predicate;
    };
    let Some(root) = bdd.build(&predicate) else {
        return predicate;
    };
    let true_paths = bdd.paths_to(root, TRUE);
    let false_paths = bdd.paths_to(root, FALSE);
    drop(bdd);
    let mut candidates = vec![predicate];
    if let Some(paths) = true_paths {
        candidates.push(predicate_from_dnf(&GuardDnf::from_disjunction(paths)));
    }
    if let Some(paths) = false_paths {
        candidates.push(predicate_from_false_paths(paths));
    }
    candidates
        .into_iter()
        .min_by_key(|candidate| (predicate_size(candidate), candidate.clone()))
        .unwrap_or(Predicate::False)
}

fn simplify_structure(predicate: Predicate, memo: Option<&PredicateMemo>) -> Predicate {
    match predicate.kind() {
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
                !memo.map_or_else(
                    || exact_implies_uncached(&exact, sound_subset),
                    |memo| memo.exactly_implies(&exact, sound_subset),
                )
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
        _ => predicate,
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
        let mut atoms = BTreeSet::new();
        collect_atoms(predicate, &mut atoms)?;
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
            unique_nodes: BTreeMap::new(),
            apply_cache: BTreeMap::new(),
            negation_cache: BTreeMap::new(),
        })
    }

    fn build(&mut self, predicate: &'a Predicate) -> Option<usize> {
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
        let mut paths = Vec::new();
        let mut current = Vec::new();
        let mut literal_count = 0;
        self.collect_paths(root, target, &mut current, &mut paths, &mut literal_count)?;
        Some(paths)
    }

    fn collect_paths(
        &self,
        node: usize,
        target: usize,
        current: &mut Vec<Predicate>,
        paths: &mut Vec<Vec<Predicate>>,
        literal_count: &mut usize,
    ) -> Option<()> {
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
        let atom = Predicate::from(self.atoms.get(node.variable)?.to_guard());
        current.push(atom.negated());
        self.collect_paths(node.when_false, target, current, paths, literal_count)?;
        current.pop();
        current.push(atom);
        self.collect_paths(node.when_true, target, current, paths, literal_count)?;
        current.pop();
        Some(())
    }
}

fn collect_atoms<'a>(predicate: &'a Predicate, atoms: &mut BTreeSet<GuardAtom<'a>>) -> Option<()> {
    match predicate.kind() {
        PredicateKind::True | PredicateKind::False => Some(()),
        PredicateKind::Guard(guard) => {
            atoms.insert(GuardAtom::new(guard));
            Some(())
        }
        PredicateKind::Not(inner) => collect_atoms(inner, atoms),
        PredicateKind::And(predicates) | PredicateKind::Or(predicates) => {
            for predicate in predicates {
                collect_atoms(predicate, atoms)?;
            }
            Some(())
        }
        PredicateKind::Approximate { .. } => None,
    }
}

#[derive(Clone, Copy)]
enum GuardAtom<'a> {
    Truthy(&'a crate::ValuesPath, bool),
    Eq(&'a crate::ValuesPath, &'a crate::GuardValue, bool),
    TypeIs(&'a crate::ValuesPath, &'a str, bool),
    Other(&'a Guard),
}

impl<'a> GuardAtom<'a> {
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

fn predicate_size(predicate: &Predicate) -> usize {
    match predicate.kind() {
        PredicateKind::Not(inner) => 1 + predicate_size(inner),
        PredicateKind::And(predicates) | PredicateKind::Or(predicates) => {
            1 + predicates.iter().map(predicate_size).sum::<usize>()
        }
        PredicateKind::True
        | PredicateKind::False
        | PredicateKind::Approximate { .. }
        | PredicateKind::Guard(_) => 1,
    }
}

#[cfg(test)]
#[path = "tests/predicate_bdd.rs"]
mod tests;
