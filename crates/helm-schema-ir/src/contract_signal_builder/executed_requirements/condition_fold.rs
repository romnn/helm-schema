//! Shares raw residual unions until their universal projection has completed.

use std::collections::BTreeMap;
use std::sync::Arc;

use helm_schema_core::{
    ConditionalGuard, Guard, GuardPrefix, RequirementAtom, RequirementBdd,
    RequirementConditionBuilder, ValuesPath,
};

use super::{
    ExistingCoverage, FoldedClause, FoldedRequirement, MAX_ITEM_WORK, OrderedOccurrence,
    canonical_occurrences, fold_or_uncovered, split_literal, unknown_requirement,
};
use crate::value::{
    FrozenConsumption, GuardWalk, RequirementGuardInterner, RequirementIdentities,
    RequirementOrderKey,
};

/// A canonical Patricia set of occurrence IDs; zero denotes the empty union.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RawShape {
    Empty,
    Leaf(usize),
    Branch { bit: u32, zero: usize, one: usize },
}

impl RawShape {
    /// The bit a branch discriminates; below every branch for a leaf or the empty set.
    fn level(self) -> i64 {
        match self {
            Self::Branch {
                bit,
                zero: _,
                one: _,
            } => i64::from(bit),
            Self::Empty | Self::Leaf(_) => -1,
        }
    }
}

struct RawNode {
    shape: RawShape,
    representative: usize,
    first_variable: Option<Arc<Guard>>,
    support_complete: bool,
}

struct RawUnion {
    occurrences: Vec<FrozenConsumption>,
    variables: BTreeMap<Guard, SplitVariable>,
    occurrence_ids: BTreeMap<RequirementOrderKey, usize>,
    guards: RequirementGuardInterner,
    identities: RequirementIdentities,
    nodes: Vec<RawNode>,
    unique: BTreeMap<RawShape, usize>,
    unions: BTreeMap<(usize, usize), usize>,
    cofactors: BTreeMap<(usize, usize, bool), (usize, usize)>,
}

struct SplitVariable {
    ordinal: usize,
    guard: Arc<Guard>,
    splittable: bool,
}

impl RawUnion {
    fn node(&self, id: usize) -> &RawNode {
        const EMPTY: RawNode = RawNode {
            shape: RawShape::Empty,
            representative: 0,
            first_variable: None,
            support_complete: true,
        };
        self.nodes.get(id).unwrap_or(&EMPTY)
    }

    fn new(identities: RequirementIdentities, guards: RequirementGuardInterner) -> Self {
        Self {
            occurrences: Vec::new(),
            variables: BTreeMap::new(),
            occurrence_ids: BTreeMap::new(),
            guards,
            identities,
            nodes: vec![RawNode {
                shape: RawShape::Empty,
                representative: 0,
                first_variable: None,
                support_complete: true,
            }],
            unique: BTreeMap::from([(RawShape::Empty, 0)]),
            unions: BTreeMap::new(),
            cofactors: BTreeMap::new(),
        }
    }

    fn occurrence(&mut self, occurrence: &FrozenConsumption) -> Option<usize> {
        if occurrence.is_raw_impossible() {
            return Some(0);
        }
        let key = occurrence.order_key(&mut self.identities, &mut self.guards)?;
        if let Some(id) = self.occurrence_ids.get(&key) {
            return self.unique.get(&RawShape::Leaf(*id)).copied();
        }
        let occurrence = occurrence.canonicalized(&key, &self.guards)?;
        Some(self.insert_canonical(key, occurrence))
    }

    fn insert_canonical(
        &mut self,
        key: RequirementOrderKey,
        occurrence: FrozenConsumption,
    ) -> usize {
        let mut atoms = std::collections::BTreeSet::new();
        let support_complete = occurrence
            .atoms(
                &mut atoms,
                &mut { MAX_ITEM_WORK },
                &mut GuardWalk::default(),
                &mut self.identities,
            )
            .is_some();
        let variables = atoms
            .into_iter()
            .filter_map(|atom| match atom {
                RequirementAtom::Value(guard) => Some(RequirementBdd::cofactor_variable(&guard)),
                RequirementAtom::Choice(_) => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let first_variable = variables.into_iter().find_map(|atom| {
            let ordinal = self.variables.len();
            let variable = self
                .variables
                .entry(atom.clone())
                .or_insert_with(|| SplitVariable {
                    ordinal,
                    splittable: split_literal(&atom, true).is_some()
                        && split_literal(&atom, false).is_some(),
                    guard: Arc::new(atom),
                });
            (variable.splittable && occurrence.can_split_on(&variable.guard))
                .then(|| Arc::clone(&variable.guard))
        });
        let occurrence_id = self.occurrences.len();
        self.occurrences.push(occurrence);
        self.occurrence_ids.insert(key, occurrence_id);
        let root = self.nodes.len();
        let shape = RawShape::Leaf(occurrence_id);
        self.nodes.push(RawNode {
            shape,
            representative: occurrence_id,
            first_variable,
            support_complete,
        });
        self.unique.insert(shape, root);
        root
    }

    fn branch(&mut self, bit: u32, zero: usize, one: usize) -> usize {
        if zero == 0 {
            return one;
        }
        if one == 0 {
            return zero;
        }
        let shape = RawShape::Branch { bit, zero, one };
        if let Some(root) = self.unique.get(&shape) {
            return *root;
        }
        let first_variable = self
            .node(zero)
            .first_variable
            .iter()
            .chain(self.node(one).first_variable.iter())
            .min()
            .cloned();
        let root = self.nodes.len();
        self.nodes.push(RawNode {
            shape,
            representative: self.node(zero).representative,
            first_variable,
            support_complete: self.node(zero).support_complete && self.node(one).support_complete,
        });
        self.unique.insert(shape, root);
        root
    }

    fn union(&mut self, left: usize, right: usize) -> usize {
        if left == 0 || left == right {
            return right;
        }
        if right == 0 {
            return left;
        }
        let key = (left.min(right), left.max(right));
        if let Some(root) = self.unions.get(&key) {
            return *root;
        }
        let left_node = self.node(left);
        let right_node = self.node(right);
        let left_key = left_node.representative;
        let right_key = right_node.representative;
        let left_shape = left_node.shape;
        let right_shape = right_node.shape;
        let differing = left_key ^ right_key;
        let join_level = if differing == 0 {
            -1
        } else {
            i64::from(usize::BITS - 1 - differing.leading_zeros())
        };
        let root = if join_level > left_shape.level().max(right_shape.level()) {
            let bit = u32::try_from(join_level).unwrap_or(0);
            if left_key & (1 << bit) == 0 {
                self.branch(bit, left, right)
            } else {
                self.branch(bit, right, left)
            }
        } else {
            match (left_shape, right_shape) {
                (
                    RawShape::Branch { bit, zero, one },
                    RawShape::Branch {
                        bit: other_bit,
                        zero: other_zero,
                        one: other_one,
                    },
                ) if bit == other_bit => {
                    let zero = self.union(zero, other_zero);
                    let one = self.union(one, other_one);
                    self.branch(bit, zero, one)
                }
                (
                    RawShape::Branch { bit, zero, one },
                    RawShape::Empty
                    | RawShape::Leaf(_)
                    | RawShape::Branch {
                        bit: _,
                        zero: _,
                        one: _,
                    },
                ) if left_shape.level() > right_shape.level() => {
                    if right_key & (1 << bit) == 0 {
                        let zero = self.union(zero, right);
                        self.branch(bit, zero, one)
                    } else {
                        let one = self.union(one, right);
                        self.branch(bit, zero, one)
                    }
                }
                (
                    RawShape::Empty
                    | RawShape::Leaf(_)
                    | RawShape::Branch {
                        bit: _,
                        zero: _,
                        one: _,
                    },
                    RawShape::Branch { bit, zero, one },
                ) => {
                    if left_key & (1 << bit) == 0 {
                        let zero = self.union(left, zero);
                        self.branch(bit, zero, one)
                    } else {
                        let one = self.union(left, one);
                        self.branch(bit, zero, one)
                    }
                }
                (
                    RawShape::Empty
                    | RawShape::Leaf(_)
                    | RawShape::Branch {
                        bit: _,
                        zero: _,
                        one: _,
                    },
                    RawShape::Empty | RawShape::Leaf(_),
                ) => left,
            }
        };
        self.unions.insert(key, root);
        root
    }

    fn cofactor(&mut self, root: usize, atom: &Guard, taken: bool) -> (usize, usize) {
        let Some(variable) = self.variables.get(atom).map(|variable| variable.ordinal) else {
            return (0, root);
        };
        self.cofactor_variable(root, atom, taken, variable)
    }

    fn cofactor_variable(
        &mut self,
        root: usize,
        atom: &Guard,
        taken: bool,
        variable: usize,
    ) -> (usize, usize) {
        let node = self.node(root);
        // This is U in U ∨ ITE(x, high, low): its storage and identity survive both edges.
        if node.support_complete
            && node
                .first_variable
                .as_ref()
                .is_none_or(|first| first.as_ref() > atom)
        {
            return (root, 0);
        }
        let key = (root, variable, taken);
        if let Some(cofactor) = self.cofactors.get(&key) {
            return *cofactor;
        }
        let result = match node.shape {
            RawShape::Empty => (root, 0),
            RawShape::Leaf(id) => self
                .occurrences
                .get(id)
                .and_then(|occurrence| occurrence.cofactor(atom, taken))
                .and_then(|occurrence| self.occurrence(&occurrence))
                .map(|cofactor| (cofactor, 0))
                .unwrap_or_else(|| {
                    let _bound = tracing::debug_span!(
                        target: "helm_schema_ir::counters", "requirement_cofactor_bound"
                    )
                    .entered();
                    // This leaf is projected separately under the branch, so it cannot force
                    // the remaining raw union to split the same uncofactored atom again.
                    (0, root)
                }),
            RawShape::Branch { zero, one, .. } => {
                let (zero, zero_failed) = self.cofactor_variable(zero, atom, taken, variable);
                let (one, one_failed) = self.cofactor_variable(one, atom, taken, variable);
                (self.union(zero, one), self.union(zero_failed, one_failed))
            }
        };
        self.cofactors.insert(key, result);
        result
    }

    fn occurrence_ids(&self, root: usize, ids: &mut Vec<usize>) {
        match self.node(root).shape {
            RawShape::Empty => {}
            RawShape::Leaf(id) => ids.push(id),
            RawShape::Branch { zero, one, .. } => {
                self.occurrence_ids(zero, ids);
                self.occurrence_ids(one, ids);
            }
        }
    }

    fn build_from_leaves(&mut self, leaves: &[usize]) -> usize {
        let Some(first) = leaves.first().copied() else {
            return 0;
        };
        let Some(last) = leaves.last().copied().filter(|last| *last != first) else {
            return first;
        };
        let first_key = self.node(first).representative;
        let last_key = self.node(last).representative;
        let bit = usize::BITS - 1 - (first_key ^ last_key).leading_zeros();
        let middle =
            leaves.partition_point(|leaf| self.node(*leaf).representative & (1 << bit) == 0);
        let (zero, one) = leaves.split_at(middle);
        let zero = self.build_from_leaves(zero);
        let one = self.build_from_leaves(one);
        self.branch(bit, zero, one)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Disposition {
    covered: bool,
    absence_covered: bool,
}

#[derive(Clone)]
struct Projected {
    roots: BTreeMap<Disposition, usize>,
    unknown: bool,
    proved_impossible: bool,
}

impl Default for Projected {
    fn default() -> Self {
        Self {
            roots: BTreeMap::new(),
            unknown: false,
            proved_impossible: true,
        }
    }
}

impl Projected {
    fn leaf(folded: FoldedRequirement, graph: &mut RequirementConditionBuilder) -> Self {
        let FoldedRequirement {
            guard,
            covered,
            absence_covered,
            projection_complete: _,
            proved_impossible,
        } = folded;
        let root = graph.guard(guard);
        Self {
            roots: if root == RequirementConditionBuilder::FALSE {
                BTreeMap::new()
            } else {
                BTreeMap::from([(
                    Disposition {
                        covered,
                        absence_covered,
                    },
                    root,
                )])
            },
            unknown: root == RequirementConditionBuilder::FALSE && !proved_impossible,
            proved_impossible,
        }
    }

    fn union(mut self, other: Self, graph: &mut RequirementConditionBuilder) -> Self {
        self.unknown |= other.unknown;
        self.proved_impossible &= other.proved_impossible;
        for (disposition, root) in other.roots {
            self.roots
                .entry(disposition)
                .and_modify(|previous| *previous = graph.union([*previous, root]))
                .or_insert(root);
        }
        self
    }
}

struct Projection<'a> {
    raw: RawUnion,
    graph: RequirementConditionBuilder,
    covers: &'a ExistingCoverage,
    presence: Option<&'a ValuesPath>,
    work: usize,
    memo: BTreeMap<(usize, usize, usize), (Projected, usize)>,
    fallback: BTreeMap<usize, Projected>,
    alone: BTreeMap<usize, (bool, Projected)>,
}

impl Projection<'_> {
    fn fold(&mut self, root: usize, parts: usize, depth: usize) -> (Projected, usize) {
        let key = (root, parts, depth);
        if let Some(projected) = self.memo.get(&key) {
            return projected.clone();
        }
        let mut ids = Vec::new();
        self.raw.occurrence_ids(root, &mut ids);
        let folded = fold_or_uncovered(
            &ids.iter()
                .filter_map(|id| self.raw.occurrences.get(*id))
                .collect::<Vec<_>>(),
            self.covers,
            self.presence,
            &mut self.raw.identities,
            self.work,
        );
        let (result, spent) = if let Some(folded) = folded {
            (Projected::leaf(folded, &mut self.graph), 1)
        } else {
            self.refine(root, parts, depth, &ids)
        };
        self.memo.insert(key, (result.clone(), spent));
        (result, spent)
    }

    fn refine(
        &mut self,
        root: usize,
        parts: usize,
        depth: usize,
        ids: &[usize],
    ) -> (Projected, usize) {
        let atom = self.raw.node(root).first_variable.clone().filter(|atom| {
            ids.iter().all(|id| {
                self.raw
                    .occurrences
                    .get(*id)
                    .is_some_and(|occurrence| occurrence.can_split_on(atom))
            })
        });
        if let Some(atom) = atom {
            if parts <= 1 || depth == 0 {
                let _bound = tracing::debug_span!(
                    target: "helm_schema_ir::counters", "requirement_refinement_bound",
                    parts_exhausted = parts <= 1,
                    depth_exhausted = depth == 0
                )
                .entered();
                return (self.fallback(root, true), 1);
            }
            let (low, low_failed) = self.raw.cofactor(root, &atom, false);
            let (high, high_failed) = self.raw.cofactor(root, &atom, true);
            // Cached children debit their full cold refinement charge, so a warm graph
            // cannot turn a proof that exceeded its local allowance into a completed one.
            // A child left without refinement still projects its fitting occurrences
            // jointly in `fallback`, so its shared proofs survive.
            let (high, high_spent) = self.fold(high, parts - 1, depth - 1);
            let (low, low_spent) = self.fold(low, parts.saturating_sub(1 + high_spent), depth - 1);
            let high_failed = self.fallback(high_failed, false);
            let low_failed = self.fallback(low_failed, false);
            let high = high.union(high_failed, &mut self.graph);
            let low = low.union(low_failed, &mut self.graph);
            let mut result = Projected {
                roots: BTreeMap::new(),
                unknown: low.unknown || high.unknown,
                proved_impossible: low.proved_impossible && high.proved_impossible,
            };
            if let Some(test) = split_literal(&atom, true) {
                for disposition in low.roots.keys().chain(high.roots.keys()) {
                    let high = high
                        .roots
                        .get(disposition)
                        .copied()
                        .unwrap_or(RequirementConditionBuilder::FALSE);
                    let low = low
                        .roots
                        .get(disposition)
                        .copied()
                        .unwrap_or(RequirementConditionBuilder::FALSE);
                    let root = self.graph.split(test.clone(), high, low);
                    if root != RequirementConditionBuilder::FALSE {
                        result.roots.insert(*disposition, root);
                    }
                }
            }
            (result, 1 + high_spent + low_spent)
        } else {
            (self.fallback(root, true), 1)
        }
    }

    /// Projects a union no split could refine. An occurrence that exceeds its own allowance
    /// widens alone; the rest still project jointly, so complementary unresolved choices among
    /// them keep their shared proof (ADR 0072 §10). `tried` says the whole union's joint fold
    /// already failed on this allowance, which the same occurrences would repeat.
    fn fallback(&mut self, root: usize, tried: bool) -> Projected {
        if let Some(projected) = self.fallback.get(&root) {
            return projected.clone();
        }
        let mut ids = Vec::new();
        self.raw.occurrence_ids(root, &mut ids);
        let mut fitting = Vec::new();
        let mut result = Projected::default();
        let mut widened = false;
        for id in ids {
            let (fits, alone) = self.alone(id);
            if fits {
                fitting.push((id, alone));
            } else {
                widened = true;
                result = result.union(alone, &mut self.graph);
            }
        }
        let joint = (fitting.len() > 1 && (widened || !tried))
            .then(|| {
                fold_or_uncovered(
                    &fitting
                        .iter()
                        .filter_map(|(id, _)| self.raw.occurrences.get(*id))
                        .collect::<Vec<_>>(),
                    self.covers,
                    self.presence,
                    &mut self.raw.identities,
                    self.work,
                )
            })
            .flatten();
        if let Some(folded) = joint {
            result = result.union(Projected::leaf(folded, &mut self.graph), &mut self.graph);
        } else {
            for (_, alone) in fitting {
                result = result.union(alone, &mut self.graph);
            }
        }
        self.fallback.insert(root, result.clone());
        result
    }

    /// One occurrence projected by itself, and whether it fits the union's own allowance. One
    /// that does not still gets the single-occurrence budget before it widens to unknown.
    fn alone(&mut self, id: usize) -> (bool, Projected) {
        if let Some(alone) = self.alone.get(&id) {
            return alone.clone();
        }
        let Some(occurrence) = self.raw.occurrences.get(id) else {
            return (true, Projected::default());
        };
        let fitted = fold_or_uncovered(
            &[occurrence],
            self.covers,
            self.presence,
            &mut self.raw.identities,
            self.work,
        );
        let fits = fitted.is_some();
        let folded = if fits || self.work >= RequirementBdd::WORK_BUDGET {
            fitted
        } else {
            fold_or_uncovered(
                &[occurrence],
                self.covers,
                self.presence,
                &mut self.raw.identities,
                RequirementBdd::WORK_BUDGET,
            )
        };
        let projected = if let Some(folded) = folded {
            Projected::leaf(folded, &mut self.graph)
        } else {
            let _bound = tracing::debug_span!(
                target: "helm_schema_ir::counters", "requirement_occurrence_bound"
            )
            .entered();
            Projected {
                unknown: true,
                proved_impossible: false,
                ..Projected::default()
            }
        };
        self.alone.insert(id, (fits, projected.clone()));
        (fits, projected)
    }
}

pub(super) fn fold_scope(
    occurrences: &[&FrozenConsumption],
    covers: &ExistingCoverage,
    presence: Option<&ValuesPath>,
    work: usize,
) -> Vec<FoldedClause> {
    let mut identities = RequirementIdentities::default();
    let mut interner = RequirementGuardInterner::default();
    let mut clauses = Vec::new();
    let prefix = GuardPrefix::default();
    let occurrences = canonical_occurrences(
        occurrences,
        &mut identities,
        &mut interner,
        &prefix,
        &mut clauses,
    );
    if let Some(folded) = fold_or_uncovered(
        &occurrences
            .iter()
            .map(|ordered| &ordered.occurrence)
            .collect::<Vec<_>>(),
        covers,
        presence,
        &mut identities,
        work,
    ) {
        clauses.push(FoldedClause { prefix, folded });
        return clauses;
    }
    let mut projection = Projection {
        raw: RawUnion::new(identities, interner),
        graph: RequirementConditionBuilder::default(),
        covers,
        presence,
        work,
        memo: BTreeMap::new(),
        fallback: BTreeMap::new(),
        alone: BTreeMap::new(),
    };
    let occurrence_count = occurrences.len();
    let mut leaves = Vec::new();
    for OrderedOccurrence { key, occurrence } in occurrences {
        if !occurrence.is_raw_impossible() {
            leaves.push(projection.raw.insert_canonical(key, occurrence));
        }
    }
    leaves.sort_unstable_by_key(|leaf| projection.raw.node(*leaf).representative);
    leaves.dedup();
    let root = projection.raw.build_from_leaves(&leaves);
    let (projected, _) = if leaves.len() == occurrence_count {
        // The unsplit union already exhausted on the same ordered occurrences.
        projection.refine(root, 256, 64, &(0..occurrence_count).collect::<Vec<_>>())
    } else {
        projection.fold(root, 256, 64)
    };
    let graph = projection.graph.freeze();
    for (disposition, root) in projected.roots {
        clauses.push(FoldedClause {
            prefix: prefix.clone(),
            folded: FoldedRequirement {
                guard: ConditionalGuard::Requirement(graph.root(root, false)),
                covered: disposition.covered,
                absence_covered: disposition.absence_covered,
                projection_complete: false,
                proved_impossible: false,
            },
        });
    }
    if (projected.unknown || projected.proved_impossible)
        && let Some(mut folded) = unknown_requirement()
    {
        folded.proved_impossible = !projected.unknown && projected.proved_impossible;
        clauses.push(FoldedClause { prefix, folded });
    }
    clauses
}

#[cfg(test)]
#[path = "../../tests/requirement_cofactor_bounds.rs"]
mod tests;
