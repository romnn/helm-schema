use std::collections::{BTreeMap, BTreeSet};

use crate::{Guard, GuardDnf, Predicate};

const FALSE: usize = 0;
const TRUE: usize = 1;
const MAX_BDD_NODES: usize = 4_096;
const MAX_NORMAL_FORM_PATHS: usize = 256;
const MAX_NORMAL_FORM_LITERALS: usize = 8_192;

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

struct PredicateBdd {
    atoms: Vec<Guard>,
    atom_indices: BTreeMap<Guard, usize>,
    nodes: Vec<BddNode>,
    unique_nodes: BTreeMap<BddNode, usize>,
    apply_cache: BTreeMap<(BooleanOp, usize, usize), usize>,
    negation_cache: BTreeMap<usize, usize>,
}

pub(crate) fn normalize(predicate: Predicate) -> Predicate {
    let predicate = simplify_structure(predicate);
    if predicate.contains_approximation() {
        return predicate;
    }
    let Some(mut bdd) = PredicateBdd::for_predicate(&predicate) else {
        return predicate;
    };
    let Some(root) = bdd.build(&predicate) else {
        return predicate;
    };
    let mut candidates = vec![predicate];
    if let Some(paths) = bdd.paths_to(root, TRUE) {
        candidates.push(predicate_from_dnf(&GuardDnf::from_disjunction(paths)));
    }
    if let Some(paths) = bdd.paths_to(root, FALSE) {
        candidates.push(predicate_from_false_paths(paths));
    }
    candidates
        .into_iter()
        .min_by_key(|candidate| (predicate_size(candidate), candidate.clone()))
        .unwrap_or(Predicate::False)
}

fn simplify_structure(predicate: Predicate) -> Predicate {
    match predicate {
        Predicate::Not(inner) => match simplify_structure(*inner) {
            Predicate::True => Predicate::False,
            Predicate::False => Predicate::True,
            Predicate::Not(inner) => *inner,
            inner => Predicate::Not(Box::new(inner)),
        },
        Predicate::And(predicates) => {
            let mut factors = BTreeSet::new();
            for predicate in predicates.into_iter().map(simplify_structure) {
                match predicate {
                    Predicate::False => return Predicate::False,
                    Predicate::True => {}
                    Predicate::And(inner) => factors.extend(inner),
                    predicate => {
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
                let Predicate::Approximate {
                    sound_subset: Some(sound_subset),
                    ..
                } = factor
                else {
                    return true;
                };
                // The subset implies the opaque runtime condition. When the
                // other exact conjuncts already imply that subset, the
                // runtime condition is known true and contributes nothing.
                !exact_implies(&exact, sound_subset)
            });
            match factors.len() {
                0 => Predicate::True,
                1 => factors.pop_first().unwrap_or(Predicate::True),
                _ => Predicate::And(factors.into_iter().collect()),
            }
        }
        Predicate::Or(predicates) => {
            let mut alternatives = BTreeSet::new();
            for predicate in predicates.into_iter().map(simplify_structure) {
                match predicate {
                    Predicate::True => return Predicate::True,
                    Predicate::False => {}
                    Predicate::Or(inner) => alternatives.extend(inner),
                    predicate => {
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
        predicate => predicate,
    }
}

pub(crate) fn exact_implies(antecedent: &Predicate, consequent: &Predicate) -> bool {
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

impl PredicateBdd {
    fn for_predicate(predicate: &Predicate) -> Option<Self> {
        let mut atoms = BTreeSet::new();
        collect_atoms(predicate, &mut atoms)?;
        let atoms = atoms.into_iter().collect::<Vec<_>>();
        let atom_indices = atoms
            .iter()
            .cloned()
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

    fn build(&mut self, predicate: &Predicate) -> Option<usize> {
        match predicate {
            Predicate::True => Some(TRUE),
            Predicate::False => Some(FALSE),
            Predicate::Guard(guard) => {
                let (atom, positive) = canonical_guard(guard);
                let variable = *self.atom_indices.get(&atom)?;
                let node = self.make_node(variable, FALSE, TRUE)?;
                if positive {
                    Some(node)
                } else {
                    self.negated(node)
                }
            }
            Predicate::Not(inner) => {
                let inner = self.build(inner)?;
                self.negated(inner)
            }
            Predicate::And(predicates) => {
                let mut result = TRUE;
                for predicate in predicates {
                    let next = self.build(predicate)?;
                    result = self.apply(BooleanOp::And, result, next)?;
                }
                Some(result)
            }
            Predicate::Or(predicates) => {
                let mut result = FALSE;
                for predicate in predicates {
                    let next = self.build(predicate)?;
                    result = self.apply(BooleanOp::Or, result, next)?;
                }
                Some(result)
            }
            Predicate::Approximate { .. } => None,
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
        let atom = Predicate::from(self.atoms.get(node.variable)?.clone());
        current.push(atom.negated());
        self.collect_paths(node.when_false, target, current, paths, literal_count)?;
        current.pop();
        current.push(atom);
        self.collect_paths(node.when_true, target, current, paths, literal_count)?;
        current.pop();
        Some(())
    }
}

fn collect_atoms(predicate: &Predicate, atoms: &mut BTreeSet<Guard>) -> Option<()> {
    match predicate {
        Predicate::True | Predicate::False => Some(()),
        Predicate::Guard(guard) => {
            atoms.insert(canonical_guard(guard).0);
            Some(())
        }
        Predicate::Not(inner) => collect_atoms(inner, atoms),
        Predicate::And(predicates) | Predicate::Or(predicates) => {
            for predicate in predicates {
                collect_atoms(predicate, atoms)?;
            }
            Some(())
        }
        Predicate::Approximate { .. } => None,
    }
}

fn canonical_guard(guard: &Guard) -> (Guard, bool) {
    match guard {
        Guard::Not { path } => (Guard::Truthy { path: path.clone() }, false),
        Guard::NotEq { path, value } => (
            Guard::Eq {
                path: path.clone(),
                value: value.clone(),
            },
            false,
        ),
        Guard::NotTypeIs { path, schema_type } => (
            Guard::TypeIs {
                path: path.clone(),
                schema_type: schema_type.clone(),
            },
            false,
        ),
        guard => (guard.clone(), true),
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
    match predicate {
        Predicate::Not(inner) => 1 + predicate_size(inner),
        Predicate::And(predicates) | Predicate::Or(predicates) => {
            1 + predicates.iter().map(predicate_size).sum::<usize>()
        }
        Predicate::True
        | Predicate::False
        | Predicate::Approximate { .. }
        | Predicate::Guard(_) => 1,
    }
}
