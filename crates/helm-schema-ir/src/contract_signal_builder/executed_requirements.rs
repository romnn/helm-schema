//! Export identical input preimages from captured runtime guards.

use super::{
    BTreeMap, ConditionalGuard, ContractPathAccumulator, ContractRequirementImplication,
    ContractRequirementTarget, ValuesPath, path_accumulator,
};
use crate::value::{ConsumptionDomain, ConsumptionKey, FrozenConsumption};
use helm_schema_core::{GuardConjunction, GuardPrefix};

/// Work one consumption's grouping inventory, or the absence index of one terminal clause, may take.
const MAX_ITEM_WORK: usize = 1 << 16;
/// Key comparisons charged per inventory unit of one item: an ordered lookup among up to 2^30
/// entries. A constant, so whether an item fits never depends on how many other items exist.
const ITEM_COMPARISONS: usize = 32;
/// Steps one group's coverage read may take (the corpus peak is under 80k).
const MAX_GROUP_COVERAGE_WORK: usize = 1 << 18;

#[tracing::instrument(skip_all, fields(consumptions = consumptions.len(), terminals_before = terminal_clauses.len(), terminals_after = tracing::field::Empty, terminal_literals = tracing::field::Empty, terminal_owned_slots = tracing::field::Empty, implication_literals = tracing::field::Empty, implication_owned_slots = tracing::field::Empty))]
pub(super) fn record_executed_requirements(
    paths: &mut BTreeMap<ValuesPath, ContractPathAccumulator>,
    terminal_clauses: &mut Vec<GuardConjunction>,
    consumptions: &[FrozenConsumption],
) {
    if consumptions.is_empty() {
        return;
    }
    // Every allowance below belongs to one consumption or one clause: exhausting it widens that
    // item alone, never the chart's other requirements (ADR 0072 §10).
    let absence_by_path = absence_coverage(terminal_clauses);
    let dependency_roots = collect_dependency_roots(paths);
    let Grouped {
        groups,
        overflow,
        opaque,
    } = group_consumptions(consumptions);
    for path in opaque {
        record_opaque_use_without_guard(paths, path);
    }
    for (key, occurrences) in groups.into_iter().chain(overflow) {
        if matches!(key.domain, ConsumptionDomain::KnownAny) {
            continue;
        }
        // Each group reads its coverage on a fixed allowance of its own, so no group's demand
        // depends on what the groups before it spent. A read that exhausts it proves no
        // coverage: the demand is still folded, only possibly redundant (ADR 0072 §10).
        let mut allowance = MAX_GROUP_COVERAGE_WORK;
        let covers = existing_coverage(
            paths,
            terminal_clauses,
            &absence_by_path,
            &key,
            &mut allowance,
        )
        .unwrap_or_else(|| {
            let _bound = tracing::debug_span!(
                target: "helm_schema_ir::counters",
                "requirement_coverage_bound"
            )
            .entered();
            ExistingCoverage::default()
        });
        let presence = matches!(
            &key.domain,
            ConsumptionDomain::Known {
                requires_presence: true,
                ..
            }
        ) && key.path.item_parent().is_none()
            && !dependency_roots.contains(&key.path);
        let presence = presence.then_some(&key.path);
        for clause in fold_clauses(
            &occurrences,
            &covers,
            presence,
            helm_schema_core::RequirementBdd::WORK_BUDGET,
        ) {
            emit_requirement(
                paths,
                terminal_clauses,
                key.clone(),
                clause.folded,
                &dependency_roots,
                &clause.prefix,
            );
        }
    }
    tracing::Span::current().record("terminals_after", terminal_clauses.len());
    let span = tracing::Span::current();
    if !span.is_disabled() {
        span.record(
            "terminal_literals",
            terminal_clauses
                .iter()
                .map(GuardConjunction::len)
                .sum::<usize>(),
        );
        span.record(
            "terminal_owned_slots",
            terminal_clauses
                .iter()
                .map(GuardConjunction::owned_capacity)
                .sum::<usize>(),
        );
        let rows = paths
            .values()
            .flat_map(|path| &path.requirement_implications);
        let (literals, capacity) = rows.fold((0_usize, 0_usize), |(literals, capacity), row| {
            (
                literals + row.outer_guards.len(),
                capacity + row.outer_guards.owned_capacity(),
            )
        });
        span.record("implication_literals", literals);
        span.record("implication_owned_slots", capacity);
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FoldedClause {
    pub(super) prefix: GuardPrefix,
    pub(super) folded: FoldedRequirement,
}

struct OrderedOccurrence<T> {
    key: crate::value::RequirementOrderKey,
    occurrence: T,
}

/// Splits a union by Shannon cofactors of values atoms, never by occurrences.
/// Universal projection commutes with these exact partitions, so complementary unresolved
/// literals stay together and a split cannot erase their shared values conjunct.
#[tracing::instrument(skip_all, fields(occurrences = occurrences.len(), clauses = tracing::field::Empty))]
pub(super) fn fold_clauses(
    occurrences: &[&FrozenConsumption],
    covers: &ExistingCoverage,
    presence: Option<&ValuesPath>,
    work: usize,
) -> Vec<FoldedClause> {
    // Choice sets from different interpreters are disjoint, so universal projection
    // distributes over their union. Scope addresses only partition; output order uses content.
    let mut scopes = BTreeMap::<usize, Vec<&FrozenConsumption>>::new();
    for occurrence in occurrences {
        scopes
            .entry(occurrence.requirement_scope_key())
            .or_default()
            .push(occurrence);
    }
    let mut clauses = scopes
        .into_values()
        .flat_map(|scope| fold_scope_clauses(&scope, covers, presence, work))
        .collect::<Vec<_>>();
    // Base ownership belongs to the original union, not an exact-looking projected part.
    let complete = clauses
        .iter()
        .all(|clause| clause.folded.projection_complete);
    for clause in &mut clauses {
        clause.folded.projection_complete &= complete;
    }
    clauses.sort();
    clauses.dedup();
    tracing::Span::current().record("clauses", clauses.len());
    clauses
}

mod condition_fold;

fn fold_scope_clauses(
    occurrences: &[&FrozenConsumption],
    covers: &ExistingCoverage,
    presence: Option<&ValuesPath>,
    work: usize,
) -> Vec<FoldedClause> {
    condition_fold::fold_scope(occurrences, covers, presence, work)
}

fn canonical_occurrences(
    occurrences: &[&FrozenConsumption],
    identities: &mut crate::value::RequirementIdentities,
    interner: &mut crate::value::RequirementGuardInterner,
    prefix: &GuardPrefix,
    clauses: &mut Vec<FoldedClause>,
) -> Vec<OrderedOccurrence<FrozenConsumption>> {
    let mut keyed = Vec::new();
    for &occurrence in occurrences {
        if let Some(key) = occurrence.order_key(identities, interner) {
            keyed.push(OrderedOccurrence { key, occurrence });
        } else {
            let _bound =
                tracing::debug_span!(target: "helm_schema_ir::counters", "requirement_order_bound")
                    .entered();
            if let Some(folded) = unknown_requirement() {
                clauses.push(FoldedClause {
                    prefix: prefix.clone(),
                    folded,
                });
            }
        }
    }
    keyed.sort_by(|left, right| interner.compare_keys(&left.key, &right.key));
    keyed.dedup_by(|left, right| interner.compare_keys(&left.key, &right.key).is_eq());
    keyed.into_iter().filter_map(|OrderedOccurrence { key, occurrence }| {
        let canonical = occurrence.canonicalized(&key, interner);
        if canonical.is_none() {
            let _bound = tracing::debug_span!(target: "helm_schema_ir::counters", "requirement_order_bound").entered();
            if let Some(folded) = unknown_requirement() {
                clauses.push(FoldedClause { prefix: prefix.clone(), folded });
            }
        }
        canonical.map(|occurrence| OrderedOccurrence { key, occurrence })
    }).collect()
}

fn unknown_requirement() -> Option<FoldedRequirement> {
    let mut remaining = MAX_ITEM_WORK;
    let bdd = helm_schema_core::RequirementBdd::new(&[], &mut remaining)?;
    Some(FoldedRequirement {
        guard: bdd.finish(helm_schema_core::RequirementBdd::never())?,
        covered: false,
        absence_covered: true,
        projection_complete: false,
        proved_impossible: false,
    })
}

fn split_literal(atom: &helm_schema_core::Guard, taken: bool) -> Option<ConditionalGuard> {
    let predicate = helm_schema_core::Predicate::from(atom.clone());
    let predicate = if taken {
        predicate
    } else {
        predicate.negated()
    };
    let mut atoms = std::collections::BTreeSet::new();
    let mut remaining = MAX_ITEM_WORK;
    crate::value::collect_requirement_atoms(&predicate, &mut atoms, &mut remaining)?;
    let atoms = atoms.into_iter().collect::<Vec<_>>();
    let mut bdd = helm_schema_core::RequirementBdd::new(&atoms, &mut remaining)?;
    let root = bdd.predicate(&predicate)?;
    let guard = bdd.finish(root)?;
    match &guard {
        ConditionalGuard::Decision(graph) if graph.projection_is_exact() && graph.root() > 1 => {
            Some(guard)
        }
        _ => None,
    }
}

fn emit_requirement(
    paths: &mut BTreeMap<ValuesPath, ContractPathAccumulator>,
    terminal_clauses: &mut Vec<GuardConjunction>,
    key: ConsumptionKey,
    folded: FoldedRequirement,
    dependency_roots: &std::collections::BTreeSet<ValuesPath>,
    prefix: &GuardPrefix,
) {
    let FoldedRequirement {
        guard,
        covered,
        absence_covered,
        projection_complete,
        proved_impossible,
    } = folded;
    let absence_covered = absence_covered
        || guard.is_self_truthy_for(&key.path)
        || guard.is_self_presence_for(&key.path);
    let before = match &guard {
        ConditionalGuard::Decision(graph) if graph.root() == 0 => {
            if !proved_impossible && matches!(key.domain, ConsumptionDomain::Opaque { .. }) {
                record_opaque_use_without_guard(paths, &key.path);
            }
            return;
        }
        ConditionalGuard::Decision(graph) if graph.root() == 1 => Vec::new(),
        _ => vec![guard],
    };
    let guards = GuardConjunction::prefixed(before, prefix.clone(), Vec::new());
    match key.domain {
        ConsumptionDomain::Known {
            mut preimage,
            requires_presence,
        } => {
            if let super::FailValueRequirement::MemberHost {
                handled_kinds: _,
                complete_domain,
            } = &mut preimage
            {
                *complete_domain = projection_complete;
            }
            let RequirementTarget {
                path: target_path,
                kind: target,
            } = requirement_target(&key.path);
            let member = !matches!(target, ContractRequirementTarget::Value);
            if !covered {
                path_accumulator(paths, &target_path)
                    .requirement_implications
                    .push(ContractRequirementImplication::from_conjunction(
                        guards.clone(),
                        target,
                        vec![preimage],
                    ));
            }
            if requires_presence
                && !member
                && !dependency_roots.contains(&key.path)
                && !absence_covered
            {
                let mut clause = guards;
                clause.push(ConditionalGuard::Absent { path: key.path });
                terminal_clauses.push(clause);
            }
        }
        ConsumptionDomain::KnownFailure if !covered => terminal_clauses.push(guards),
        ConsumptionDomain::Opaque { .. } => {
            path_accumulator(paths, &key.path)
                .opaque_consumption_guards
                .get_or_insert_with(Vec::new)
                .push(guards);
        }
        ConsumptionDomain::KnownFailure | ConsumptionDomain::KnownAny => {}
    }
}

/// An opaque use whose guard did not fit its allowance. The declared default is replaced as for
/// any opaque use, and the type it would assert under the use's guard is asserted under that
/// guard's narrowest under-approximation, `never`: losing the guard only widens (ADR 0072 §10).
fn record_opaque_use_without_guard(
    paths: &mut BTreeMap<ValuesPath, ContractPathAccumulator>,
    path: &ValuesPath,
) {
    let mut remaining = helm_schema_core::RequirementBdd::WORK_BUDGET;
    let never = helm_schema_core::RequirementBdd::new(&[], &mut remaining)
        .and_then(|bdd| bdd.finish(helm_schema_core::RequirementBdd::never()));
    // An empty decision graph fits any allowance; without one the use cannot be recorded
    // without a guard at all, which would tighten.
    let Some(never) = never else {
        return;
    };
    path_accumulator(paths, path)
        .opaque_consumption_guards
        .get_or_insert_with(Vec::new)
        .push(vec![never].into());
}

/// The consumptions grouped by fold key, and the paths of opaque uses left out of every group.
struct Grouped<'a> {
    groups: BTreeMap<ConsumptionKey, Vec<&'a FrozenConsumption>>,
    /// Groups of consumptions whose ordered key inventory exceeded its allowance, in order of
    /// first occurrence.
    overflow: Vec<(ConsumptionKey, Vec<&'a FrozenConsumption>)>,
    opaque: Vec<&'a ValuesPath>,
}

/// Groups the consumptions by fold key. A consumption whose ordered grouping inventory exceeds
/// its own allowance keeps its demand (ADR 0072 §10): it joins the occurrences with an equal
/// key through a digest and one full comparison, which cost only its own key whatever else the
/// chart records, so complementary choices among them still fold together. An opaque one is
/// kept as a use without a guard.
fn group_consumptions(consumptions: &[FrozenConsumption]) -> Grouped<'_> {
    use std::hash::{Hash as _, Hasher as _};

    let mut groups = BTreeMap::<ConsumptionKey, Vec<&FrozenConsumption>>::new();
    let mut overflow = Vec::<(ConsumptionKey, Vec<&FrozenConsumption>)>::new();
    let mut digests = std::collections::HashMap::<u64, Vec<usize>>::new();
    let mut opaque = Vec::new();
    for consumption in consumptions {
        let mut allowance = MAX_ITEM_WORK;
        if consumption
            .path
            .segments()
            .filter(|segment| segment.is_each_member())
            .count()
            > 1
        {
            continue;
        }
        let key = consumption
            .charge_grouping(&mut allowance, ITEM_COMPARISONS)
            .map(|()| consumption.fold_key());
        let Some(key) = key else {
            let _bound = tracing::debug_span!(
                target: "helm_schema_ir::counters",
                "requirement_grouping_bound"
            )
            .entered();
            if matches!(consumption.domain, ConsumptionDomain::Opaque { .. }) {
                opaque.push(&consumption.path);
                continue;
            }
            let key = consumption.fold_key();
            let mut hasher = std::hash::DefaultHasher::new();
            key.hash(&mut hasher);
            let candidates = digests.entry(hasher.finish()).or_default();
            let existing = candidates.iter().copied().find(|&index| {
                overflow
                    .get(index)
                    .is_some_and(|(existing, _)| existing == &key)
            });
            if let Some((_, members)) = existing.and_then(|index| overflow.get_mut(index)) {
                members.push(consumption);
            } else {
                candidates.push(overflow.len());
                overflow.push((key, vec![consumption]));
            }
            continue;
        };
        groups.entry(key).or_default().push(consumption);
    }
    Grouped {
        groups,
        overflow,
        opaque,
    }
}

/// The dependency values roots; a linear scan of the recorded paths.
fn collect_dependency_roots(
    paths: &BTreeMap<ValuesPath, ContractPathAccumulator>,
) -> std::collections::BTreeSet<ValuesPath> {
    paths
        .iter()
        .filter(|(_, accumulator)| {
            accumulator
                .facts
                .facts
                .accepted_dependency_values_root_fragment
        })
        .map(|(path, _)| path.clone())
        .collect()
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FoldedRequirement {
    pub(super) guard: ConditionalGuard,
    covered: bool,
    absence_covered: bool,
    pub(super) projection_complete: bool,
    /// The local union was false before unresolved choices were projected.
    pub(super) proved_impossible: bool,
}

#[derive(Default)]
pub(super) struct ExistingCoverage {
    body: Vec<helm_schema_core::Predicate>,
    absence: Vec<helm_schema_core::Predicate>,
}

/// Indexes the terminal clauses that cover an absence. A clause whose inventory exceeds its own
/// allowance is left out: it then covers nothing, so the requirement it would cover is emitted.
fn absence_coverage(
    terminal: &[GuardConjunction],
) -> BTreeMap<ValuesPath, Vec<helm_schema_core::Predicate>> {
    let mut indexed = BTreeMap::<ValuesPath, Vec<helm_schema_core::Predicate>>::new();
    for guards in terminal {
        let mut allowance = MAX_ITEM_WORK;
        if guards.len() > 64
            || guards
                .len()
                .checked_mul(ITEM_COMPARISONS)
                .and_then(|cost| allowance.checked_sub(cost))
                .is_none_or(|left| {
                    allowance = left;
                    !guards
                        .iter()
                        .all(|guard| guard.charge_requirement_inventory(&mut allowance).is_some())
                })
        {
            let _bound =
                tracing::debug_span!(target: "helm_schema_ir::counters", "requirement_absence_bound")
                    .entered();
            continue;
        }
        for guard in guards {
            if let ConditionalGuard::Absent { path } = guard {
                indexed
                    .entry(path.clone())
                    .or_default()
                    .push(helm_schema_core::Predicate::all(
                        guards.iter().map(ConditionalGuard::predicate).collect(),
                    ));
            }
        }
    }
    indexed
}

fn existing_coverage(
    paths: &BTreeMap<ValuesPath, ContractPathAccumulator>,
    terminal: &[GuardConjunction],
    absence_by_path: &BTreeMap<ValuesPath, Vec<helm_schema_core::Predicate>>,
    key: &ConsumptionKey,
    remaining: &mut usize,
) -> Option<ExistingCoverage> {
    let mut body = Vec::new();
    match &key.domain {
        ConsumptionDomain::Known {
            preimage,
            requires_presence: _,
        } => {
            if let Some(accumulator) = paths.get(&key.path) {
                *remaining = remaining.checked_sub(accumulator.requirement_implications.len())?;
                for implication in &accumulator.requirement_implications {
                    *remaining = remaining.checked_sub(implication.requirements.len())?;
                    if !matches!(implication.target, ContractRequirementTarget::Value) {
                        continue;
                    }
                    let exact = implication
                        .requirements
                        .iter()
                        .any(|old| same_preimage(old, preimage));
                    let non_nil = implication
                        .requirements
                        .iter()
                        .any(|old| nullable_preimage(old, preimage));
                    if exact || non_nil {
                        body.extend(coverage_predicate(
                            &implication.outer_guards,
                            (!exact).then_some(&key.path),
                            remaining,
                        ));
                    }
                }
            }
        }
        ConsumptionDomain::KnownFailure => {
            *remaining = remaining.checked_sub(terminal.len())?;
            for guards in terminal {
                body.extend(coverage_predicate(guards, None, remaining));
            }
        }
        ConsumptionDomain::KnownAny | ConsumptionDomain::Opaque { .. } => {}
    }
    let absence = absence_by_path
        .get(&key.path)
        .map(Vec::as_slice)
        .unwrap_or_default();
    *remaining = remaining.checked_sub(absence.len())?;
    Some(ExistingCoverage {
        body,
        absence: absence.to_vec(),
    })
}

/// The predicate of one earlier clause that may cover a group. A clause that does not fit the
/// group's allowance covers nothing, so the group's requirement is emitted, possibly redundant.
fn coverage_predicate(
    guards: &GuardConjunction,
    non_nil: Option<&ValuesPath>,
    remaining: &mut usize,
) -> Option<helm_schema_core::Predicate> {
    let skipped = || {
        let _bound =
            tracing::debug_span!(target: "helm_schema_ir::counters", "requirement_clause_bound")
                .entered();
    };
    let Some(allowance) = remaining.checked_sub(guards.len() + 1) else {
        *remaining = 0;
        skipped();
        return None;
    };
    *remaining = allowance;
    if guards.len() > 64
        || !guards
            .iter()
            .all(|guard| guard.charge_requirement_inventory(remaining).is_some())
    {
        skipped();
        return None;
    }
    let mut predicates = guards
        .iter()
        .map(ConditionalGuard::predicate)
        .collect::<Vec<_>>();
    let non_nil = non_nil.filter(|path| !guards.iter().any(|guard| guard.is_self_truthy_for(path)
        || matches!(guard, ConditionalGuard::NotEq { path: tested, value: helm_schema_core::GuardValue::Null } if tested == *path)
        || matches!(guard, ConditionalGuard::TypeIs { path: tested, schema_type } if tested == *path && schema_type != "null")));
    if let Some(path) = non_nil {
        predicates.push(
            helm_schema_core::Predicate::from(super::Guard::Eq {
                path: path.clone(),
                value: helm_schema_core::GuardValue::Null,
            })
            .negated(),
        );
    }
    Some(helm_schema_core::Predicate::all(predicates))
}

fn same_preimage(old: &super::FailValueRequirement, new: &super::FailValueRequirement) -> bool {
    use super::FailValueRequirement;
    old == new
        || match (old, new) {
            (
                FailValueRequirement::MemberHost {
                    handled_kinds: old,
                    complete_domain: _,
                },
                FailValueRequirement::MemberHost {
                    handled_kinds: new,
                    complete_domain: _,
                },
            ) => old.iter().all(|kind| new.contains(kind)),
            (
                FailValueRequirement::SchemaTypeEvenNull(kind),
                FailValueRequirement::MemberHost { .. },
            ) => kind == "object",
            (
                FailValueRequirement::SchemaType(kind),
                FailValueRequirement::MemberHost {
                    handled_kinds,
                    complete_domain: _,
                },
            ) => kind == "object" && handled_kinds.iter().any(|kind| kind == "null"),
            _ => false,
        }
}

fn nullable_preimage(old: &super::FailValueRequirement, new: &super::FailValueRequirement) -> bool {
    use super::FailValueRequirement;
    match (old, new) {
        (FailValueRequirement::SchemaType(old), FailValueRequirement::SchemaTypeEvenNull(new)) => {
            old == new && old != "null"
        }
        (FailValueRequirement::SchemaType(kind), FailValueRequirement::MemberHost { .. }) => {
            kind == "object"
        }
        _ => false,
    }
}

/// Folds one group, and when its coverage by earlier clauses does not fit the allowance, folds it
/// again without that coverage: the requirement is then only possibly redundant, never lost
/// (ADR 0072 §10).
fn fold_or_uncovered(
    occurrences: &[&FrozenConsumption],
    covers: &ExistingCoverage,
    presence: Option<&ValuesPath>,
    identities: &mut crate::value::RequirementIdentities,
    work: usize,
) -> Option<FoldedRequirement> {
    let folded = fold(occurrences, covers, presence, identities, work).or_else(|| {
        if covers.body.is_empty() && covers.absence.is_empty() {
            return None;
        }
        let _bound = tracing::debug_span!(
            target: "helm_schema_ir::counters",
            "requirement_coverage_bound"
        )
        .entered();
        fold(
            occurrences,
            &ExistingCoverage::default(),
            presence,
            identities,
            work,
        )
    });
    if folded.is_none() {
        let _bound = tracing::debug_span!(
            target: "helm_schema_ir::counters",
            "requirement_bound",
            occurrences = occurrences.len()
        )
        .entered();
    }
    folded
}

/// Folds one group on an allowance of its own: whether a group completes never depends on how
/// much the groups before it spent (ADR 0072 §10). Coverage only marks the demand redundant, so
/// a cone that exhausts with it proves no coverage and the demand is folded alone.
fn fold(
    occurrences: &[&FrozenConsumption],
    covers: &ExistingCoverage,
    presence: Option<&ValuesPath>,
    identities: &mut crate::value::RequirementIdentities,
    work: usize,
) -> Option<FoldedRequirement> {
    let mut local = work;
    fold_with_budget(occurrences, covers, presence, &mut local, identities)
}

fn fold_with_budget(
    occurrences: &[&FrozenConsumption],
    covers: &ExistingCoverage,
    presence: Option<&ValuesPath>,
    remaining: &mut usize,
    identities: &mut crate::value::RequirementIdentities,
) -> Option<FoldedRequirement> {
    // A literal true occurrence fixes the raw union before projection.
    // Coverage still needs its usual proof on the same allowance.
    let occurrences = if occurrences.len() > 1
        && let Some(unconditional) = occurrences.iter().find(|item| item.is_raw_unconditional())
    {
        let _proof = tracing::debug_span!(
            target: "helm_schema_ir::counters",
            "requirement_unconditional_union",
            occurrences = occurrences.len(),
        )
        .entered();
        std::slice::from_ref(unconditional)
    } else {
        occurrences
    };
    // One inventory walk per fold: the occurrences share guard nodes, and each node is charged
    // once.
    let mut atoms = std::collections::BTreeSet::new();
    let mut walk = crate::value::GuardWalk::default();
    occurrences.iter().try_for_each(|consumption| {
        consumption.atoms(&mut atoms, remaining, &mut walk, identities)
    })?;
    covers
        .body
        .iter()
        .chain(&covers.absence)
        .try_for_each(|predicate| {
            crate::value::collect_requirement_atoms(predicate, &mut atoms, remaining)
        })?;
    let absence_atom = presence.map(|path| {
        helm_schema_core::RequirementAtom::Value(super::Guard::Absent { path: path.clone() })
    });
    if let Some(atom) = &absence_atom {
        atom.charge_inventory(remaining, 2 + atoms.len().max(1).ilog2() as usize)?;
        atoms.insert(atom.clone());
    }
    let atoms = atoms.into_iter().collect::<Vec<_>>();
    let mut bdd = helm_schema_core::RequirementBdd::new(&atoms, remaining)?;
    let mut root = helm_schema_core::RequirementBdd::never();
    let mut shared = crate::value::GuardImports::default();
    for consumption in occurrences {
        let occurrence = consumption.import(&mut bdd, &mut shared, identities)?;
        root = bdd.or(root, occurrence)?;
    }
    let proved_impossible = helm_schema_core::RequirementBdd::is_never(root);
    let mut old = helm_schema_core::RequirementBdd::never();
    let projected = bdd.project(root)?;
    for predicate in &covers.body {
        let predicate = bdd.predicate(predicate)?;
        old = bdd.or(old, predicate)?;
    }
    let not_old = bdd.not(old)?;
    let uncovered = bdd.and(projected, not_old)?;
    let covered = helm_schema_core::RequirementBdd::is_never(uncovered);
    let absence_covered = if let Some(atom) = &absence_atom {
        let absent = bdd.atom(atom)?;
        let missing = bdd.and(projected, absent)?;
        let mut old = helm_schema_core::RequirementBdd::never();
        for predicate in &covers.absence {
            let predicate = bdd.predicate(predicate)?;
            old = bdd.or(old, predicate)?;
        }
        let not_old = bdd.not(old)?;
        let uncovered = bdd.and(missing, not_old)?;
        helm_schema_core::RequirementBdd::is_never(uncovered)
    } else {
        true
    };
    let guard = bdd.finish(root)?;
    let projection_complete =
        matches!(&guard, ConditionalGuard::Decision(graph) if graph.projection_is_exact());
    Some(FoldedRequirement {
        guard,
        covered,
        absence_covered,
        projection_complete,
        proved_impossible,
    })
}

struct RequirementTarget {
    path: ValuesPath,
    kind: ContractRequirementTarget,
}

fn requirement_target(path: &ValuesPath) -> RequirementTarget {
    let mut parent = ValuesPath::default();
    let mut suffix = Vec::new();
    let mut member = false;
    for segment in path.segments() {
        if segment.is_each_member() {
            member = true;
        } else if member {
            suffix.push(segment.encode_component());
        } else {
            parent = ValuesPath::from_segments(parent.segments().cloned().chain([segment.clone()]));
        }
    }
    if !member {
        return RequirementTarget {
            path: path.clone(),
            kind: ContractRequirementTarget::Value,
        };
    }
    let target = if suffix.is_empty() {
        ContractRequirementTarget::Members {
            allow_integer: true,
        }
    } else {
        ContractRequirementTarget::MembersAt {
            target_path: suffix,
            allow_integer: true,
        }
    };
    RequirementTarget {
        path: parent,
        kind: target,
    }
}

#[cfg(test)]
#[path = "../tests/requirement_export_memory.rs"]
mod tests;
