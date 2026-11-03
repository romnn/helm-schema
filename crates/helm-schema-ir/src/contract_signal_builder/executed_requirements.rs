//! Export identical input preimages from captured runtime guards.

use super::{
    BTreeMap, ConditionalGuard, ContractPathAccumulator, ContractRequirementImplication,
    ContractRequirementTarget, ValuesPath, path_accumulator,
};
use crate::value::{ConsumptionDomain, ConsumptionKey, FrozenConsumption};

const MAX_CHART_WORK: usize = 8_388_608;

#[tracing::instrument(skip_all)]
pub(super) fn record_executed_requirements(
    paths: &mut BTreeMap<ValuesPath, ContractPathAccumulator>,
    terminal_clauses: &mut Vec<Vec<ConditionalGuard>>,
    consumptions: &[FrozenConsumption],
) {
    if consumptions.is_empty() {
        return;
    }
    let Some(mut remaining) = grouping_allowance(consumptions) else {
        return;
    };
    let Some(absence_by_path) = absence_coverage(terminal_clauses, &mut remaining) else {
        return;
    };
    let Some(dependency_roots) = collect_dependency_roots(paths, &mut remaining) else {
        return;
    };
    let mut groups = BTreeMap::<ConsumptionKey, Vec<&FrozenConsumption>>::new();
    let mut identities = crate::value::RequirementIdentities::default();
    for consumption in consumptions {
        if consumption
            .path
            .segments()
            .filter(|segment| segment.is_each_member())
            .count()
            > 1
        {
            continue;
        }
        let Some(key) = consumption.fold_key(&mut identities) else {
            return;
        };
        groups.entry(key).or_default().push(consumption);
    }
    for (key, occurrences) in groups {
        if matches!(key.domain, ConsumptionDomain::KnownAny) {
            continue;
        }
        let Some(covers) = existing_coverage(
            paths,
            terminal_clauses,
            &absence_by_path,
            &key,
            &mut remaining,
        ) else {
            continue;
        };
        let presence = matches!(
            &key.domain,
            ConsumptionDomain::Known {
                requires_presence: true,
                ..
            }
        ) && key.path.item_parent().is_none()
            && !dependency_roots.contains(&key.path);
        let Some(folded) = fold(
            &occurrences,
            &covers,
            presence.then_some(&key.path),
            &mut remaining,
            &mut identities,
        ) else {
            continue;
        };
        emit_requirement(paths, terminal_clauses, key, folded, &dependency_roots);
    }
}

fn emit_requirement(
    paths: &mut BTreeMap<ValuesPath, ContractPathAccumulator>,
    terminal_clauses: &mut Vec<Vec<ConditionalGuard>>,
    key: ConsumptionKey,
    folded: FoldedRequirement,
    dependency_roots: &std::collections::BTreeSet<ValuesPath>,
) {
    let FoldedRequirement {
        guard,
        covered,
        absence_covered,
    } = folded;
    let projection_exact =
        matches!(&guard, ConditionalGuard::Decision(graph) if graph.projection_is_exact());
    let absence_covered = absence_covered
        || guard.is_self_truthy_for(&key.path)
        || guard.is_self_presence_for(&key.path);
    let guards = match &guard {
        ConditionalGuard::Decision(graph) if graph.root() == 0 => return,
        ConditionalGuard::Decision(graph) if graph.root() == 1 => Vec::new(),
        _ => vec![guard],
    };
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
                *complete_domain = projection_exact;
            }
            let RequirementTarget {
                path: target_path,
                kind: target,
            } = requirement_target(&key.path);
            let member = !matches!(target, ContractRequirementTarget::Value);
            if !covered {
                path_accumulator(paths, &target_path)
                    .requirement_implications
                    .push(ContractRequirementImplication::new(
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

fn grouping_allowance(consumptions: &[FrozenConsumption]) -> Option<usize> {
    let comparisons = 2 + consumptions.len().max(1).ilog2() as usize;
    let mut remaining = MAX_CHART_WORK.checked_sub(consumptions.len())?;
    for consumption in consumptions {
        consumption.charge_grouping(&mut remaining, comparisons)?;
    }
    Some(remaining)
}

fn collect_dependency_roots(
    paths: &BTreeMap<ValuesPath, ContractPathAccumulator>,
    remaining: &mut usize,
) -> Option<std::collections::BTreeSet<ValuesPath>> {
    *remaining = remaining.checked_sub(paths.len())?;
    let comparisons = 2 + paths.len().max(1).ilog2() as usize;
    let mut roots = std::collections::BTreeSet::new();
    for (path, accumulator) in paths {
        if accumulator
            .facts
            .facts
            .accepted_dependency_values_root_fragment
        {
            helm_schema_core::RequirementAtom::charge_path_inventory(path, remaining, comparisons)?;
            roots.insert(path.clone());
        }
    }
    Some(roots)
}

struct FoldedRequirement {
    guard: ConditionalGuard,
    covered: bool,
    absence_covered: bool,
}

struct ExistingCoverage {
    body: Vec<helm_schema_core::Predicate>,
    absence: Vec<helm_schema_core::Predicate>,
}

fn absence_coverage(
    terminal: &[Vec<ConditionalGuard>],
    remaining: &mut usize,
) -> Option<BTreeMap<ValuesPath, Vec<helm_schema_core::Predicate>>> {
    let mut indexed = BTreeMap::<ValuesPath, Vec<helm_schema_core::Predicate>>::new();
    let lookup_cost = 2 + terminal.len().max(1).ilog2() as usize;
    for guards in terminal {
        *remaining = remaining.checked_sub(guards.len().checked_mul(lookup_cost)?)?;
        if guards.len() > 64
            || !guards
                .iter()
                .all(|guard| guard.charge_requirement_inventory(remaining).is_some())
        {
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
    Some(indexed)
}

fn existing_coverage(
    paths: &BTreeMap<ValuesPath, ContractPathAccumulator>,
    terminal: &[Vec<ConditionalGuard>],
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
                    if (exact || non_nil)
                        && let Some(predicate) = coverage_predicate(
                            &implication.outer_guards,
                            (!exact).then_some(&key.path),
                            remaining,
                        )
                    {
                        body.push(predicate);
                    }
                }
            }
        }
        ConsumptionDomain::KnownFailure => {
            *remaining = remaining.checked_sub(terminal.len())?;
            for guards in terminal {
                if let Some(predicate) = coverage_predicate(guards, None, remaining) {
                    body.push(predicate);
                }
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

fn coverage_predicate(
    guards: &[ConditionalGuard],
    non_nil: Option<&ValuesPath>,
    remaining: &mut usize,
) -> Option<helm_schema_core::Predicate> {
    let Some(allowance) = remaining.checked_sub(guards.len() + 1) else {
        *remaining = 0;
        return None;
    };
    *remaining = allowance;
    if guards.len() > 64
        || !guards
            .iter()
            .all(|guard| guard.charge_requirement_inventory(remaining).is_some())
    {
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

fn fold(
    occurrences: &[&FrozenConsumption],
    covers: &ExistingCoverage,
    presence: Option<&ValuesPath>,
    remaining: &mut usize,
    identities: &mut crate::value::RequirementIdentities,
) -> Option<FoldedRequirement> {
    let mut local = (*remaining).min(helm_schema_core::RequirementBdd::WORK_BUDGET);
    let before = local;
    let folded = fold_with_budget(occurrences, covers, presence, &mut local, identities);
    *remaining = remaining.saturating_sub(before - local);
    folded
}

fn fold_with_budget(
    occurrences: &[&FrozenConsumption],
    covers: &ExistingCoverage,
    presence: Option<&ValuesPath>,
    remaining: &mut usize,
    identities: &mut crate::value::RequirementIdentities,
) -> Option<FoldedRequirement> {
    let mut atoms = std::collections::BTreeSet::new();
    occurrences
        .iter()
        .try_for_each(|consumption| consumption.atoms(&mut atoms, remaining, identities))?;
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
    for consumption in occurrences {
        let occurrence = consumption.import(&mut bdd, identities)?;
        root = bdd.or(root, occurrence)?;
    }
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
    Some(FoldedRequirement {
        guard: bdd.finish(root)?,
        covered,
        absence_covered,
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
