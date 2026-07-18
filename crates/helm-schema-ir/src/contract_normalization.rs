use std::collections::{BTreeMap, BTreeSet};

use crate::contract::ContractUse;
use crate::{Guard, ResourceRef, ValueKind, YamlPath};
use helm_schema_core::{self as output_path, Predicate};

/// Apply semantic finalization to claims produced by the interpreter.
///
/// This removes duplicates that are equivalent after chart-default lowering,
/// prefers resource evidence for pathless duplicate roots, and then
/// canonicalizes ordering.
#[tracing::instrument(skip_all)]
pub(crate) fn normalize_contract_uses(uses: &mut Vec<ContractUse>) {
    canonicalize_contract_use_inputs(uses);
    drop_default_guard_subsumed_duplicates(uses);
    drop_self_truthy_subsumed_duplicates(uses);
    merge_pathless_resource_variants(uses);
    drop_self_truthy_subsumed_duplicates(uses);
    canonicalize_contract_uses(uses);
}

/// Canonicalize contract claims without dropping semantically distinct rows.
///
/// Tests and expert callers use this when they provide already-structured
/// claims and need deterministic ordering without losing raw evidence such as
/// one nullable and one non-nullable render site.
#[tracing::instrument(skip_all)]
pub(crate) fn canonicalize_contract_uses(uses: &mut Vec<ContractUse>) {
    canonicalize_contract_use_inputs(uses);
    expand_condition_disjuncts(uses);
    // Deep `GuardDnf` comparisons dominate both sorts below, and conditions
    // repeat heavily across rows. Ranking each DISTINCT condition once and
    // comparing ranks yields the identical order (rank order is condition
    // order) at integer-comparison cost.
    let condition_ranks: Vec<u32> = {
        let distinct: std::collections::BTreeSet<&helm_schema_core::GuardDnf> = uses
            .iter()
            .map(|contract_use| &contract_use.condition)
            .collect();
        let rank_by_condition: std::collections::HashMap<&helm_schema_core::GuardDnf, u32> =
            distinct
                .into_iter()
                .enumerate()
                .map(|(rank, condition)| (condition, u32::try_from(rank).unwrap_or(u32::MAX)))
                .collect();
        uses.iter()
            .map(|contract_use| rank_by_condition[&contract_use.condition])
            .collect()
    };
    let mut rows: Vec<(u32, ContractUse)> = condition_ranks
        .into_iter()
        .zip(std::mem::take(uses))
        .collect();
    rows.sort_by(|(left_rank, left), (right_rank, right)| {
        contract_use_base_cmp(left, right).then_with(|| left_rank.cmp(right_rank))
    });

    let mut semantic_rows: Vec<(u32, ContractUse)> = Vec::with_capacity(rows.len());
    for (rank, contract_use) in rows {
        if let Some((existing_rank, existing)) = semantic_rows.last_mut()
            && *existing_rank == rank
            && contract_use_render_site_cmp(existing, &contract_use).is_eq()
        {
            merge_contract_use_provenance(existing, contract_use.provenance);
            continue;
        }
        semantic_rows.push((rank, contract_use));
    }

    semantic_rows.sort_by(|(left_rank, left), (right_rank, right)| {
        contract_use_render_site_cmp(left, right).then_with(|| left_rank.cmp(right_rank))
    });
    let mut merged_sites: Vec<ContractUse> = Vec::with_capacity(semantic_rows.len());
    for (_, contract_use) in semantic_rows {
        if let Some(existing) = merged_sites.last_mut()
            && contract_use_render_site_cmp(existing, &contract_use).is_eq()
        {
            existing.condition.union_absorbing(contract_use.condition);
            merge_contract_use_provenance(existing, contract_use.provenance);
            continue;
        }
        merged_sites.push(contract_use);
    }
    *uses = merged_sites;
}

fn canonicalize_contract_use_inputs(uses: &mut [ContractUse]) {
    for contract_use in uses {
        contract_use.canonicalize();
    }
}

#[tracing::instrument(skip_all)]
fn merge_pathless_resource_variants(uses: &mut Vec<ContractUse>) {
    let mut merged: Vec<ContractUse> = Vec::with_capacity(uses.len());
    let mut pathless_index_by_identity: BTreeMap<(String, ValueKind, BTreeSet<Predicate>), usize> =
        BTreeMap::new();

    for mut contract_use in std::mem::take(uses) {
        if contract_use.path.0.is_empty() {
            let key = (
                contract_use.source_expr.clone(),
                contract_use.kind,
                contract_predicates(&contract_use),
            );
            if let Some(existing) = pathless_index_by_identity
                .get(&key)
                .and_then(|index| merged.get_mut(*index))
                && (existing.resource.is_none()
                    || contract_use.resource.is_none()
                    || existing.resource == contract_use.resource)
            {
                if existing.resource.is_none() {
                    existing.resource = contract_use.resource.take();
                }
                merge_contract_use_provenance(existing, contract_use.provenance);
                continue;
            }
            pathless_index_by_identity.insert(key, merged.len());
        }
        merged.push(contract_use);
    }

    *uses = merged;
}

#[tracing::instrument(skip_all)]
pub(crate) fn drop_default_guard_subsumed_duplicates(uses: &mut Vec<ContractUse>) {
    expand_condition_disjuncts(uses);
    let defaulted_render_sites: BTreeSet<_> = uses
        .iter()
        .filter(|contract_use| has_self_default_guard(contract_use))
        .map(render_site)
        .collect();

    uses.retain(|contract_use| {
        if has_self_default_guard(contract_use) {
            return true;
        }
        !defaulted_render_sites.contains(&render_site(contract_use))
    });
}

#[tracing::instrument(skip_all)]
pub(crate) fn drop_self_truthy_subsumed_duplicates(uses: &mut Vec<ContractUse>) {
    expand_condition_disjuncts(uses);
    // The subsumption scan only ever compares rows sharing one render site
    // (source, path, kind, resource), so group indices once and keep the
    // quadratic candidate scan inside those buckets instead of over all rows.
    let mut buckets: BTreeMap<(&String, &YamlPath, ValueKind, Option<&ResourceRef>), Vec<usize>> =
        BTreeMap::new();
    for (index, contract_use) in uses.iter().enumerate() {
        buckets
            .entry((
                &contract_use.source_expr,
                &contract_use.path,
                contract_use.kind,
                contract_use.resource.as_ref(),
            ))
            .or_default()
            .push(index);
    }

    let mut keep = vec![true; uses.len()];
    let predicates_by_index = uses.iter().map(contract_predicates).collect::<Vec<_>>();
    for indices in buckets.values() {
        if indices.len() < 2 {
            continue;
        }
        for &index in indices {
            let Some(contract_use) = uses.get(index) else {
                continue;
            };
            let predicates = predicates_by_index.get(index).cloned().unwrap_or_default();
            let has_self_truthy = predicates.iter().any(
                |predicate| matches!(predicate, Predicate::Guard(Guard::Truthy { path }) if path == &contract_use.source_expr),
            );
            if predicates.iter().any(
                |predicate| matches!(predicate, Predicate::Guard(Guard::Default { path }) if path == &contract_use.source_expr),
            ) {
                continue;
            }
            let subsumed = indices
                .iter()
                .filter_map(|&other_index| {
                    uses.get(other_index)
                        .zip(predicates_by_index.get(other_index))
                })
                // Cheapest discriminant first: a subsuming row must carry
                // strictly MORE predicates, so length filters out most of the
                // bucket before any set or provenance comparison runs.
                .filter(|(_, other_predicates)| other_predicates.len() > predicates.len())
                .any(|(other, other_predicates)| {
                    !other.provenance.is_empty()
                        && ((contract_use.provenance.is_empty()
                            && contract_use.resource.is_some())
                            || other.provenance == contract_use.provenance)
                        && predicates.is_subset(other_predicates)
                        && ((!has_self_truthy
                            && other_predicates.iter().any(|predicate| {
                                matches!(predicate, Predicate::Guard(Guard::Truthy { path }) if path == &contract_use.source_expr)
                            }))
                            || extra_predicates_are_truthy_parents(
                                &predicates,
                                other_predicates,
                            ))
                });
            if subsumed && let Some(flag) = keep.get_mut(index) {
                *flag = false;
            }
        }
    }

    let mut index = 0;
    uses.retain(|_| {
        let kept = keep.get(index).copied().unwrap_or(true);
        index += 1;
        kept
    });
}

fn extra_predicates_are_truthy_parents(
    predicates: &BTreeSet<Predicate>,
    other_predicates: &BTreeSet<Predicate>,
) -> bool {
    other_predicates
        .iter()
        .filter(|predicate| !predicates.contains(predicate))
        .all(|predicate| {
            let Predicate::Guard(Guard::Truthy { path: parent }) = predicate else {
                return false;
            };
            predicates.iter().any(|existing| {
                matches!(
                    existing,
                    Predicate::Guard(Guard::Truthy { path: child })
                        if output_path::values_path_is_descendant(child, parent)
                )
            })
        })
}

type RenderSite = (String, YamlPath, ValueKind, Option<ResourceRef>);

fn render_site(contract_use: &ContractUse) -> RenderSite {
    (
        contract_use.source_expr.clone(),
        contract_use.path.clone(),
        contract_use.kind,
        contract_use.resource.clone(),
    )
}

fn has_self_default_guard(contract_use: &ContractUse) -> bool {
    contract_predicates(contract_use)
        .iter()
        .any(|predicate| matches!(predicate, Predicate::Guard(Guard::Default { path }) if path == &contract_use.source_expr))
}

fn contract_predicates(contract_use: &ContractUse) -> BTreeSet<Predicate> {
    contract_use
        .condition
        .disjuncts()
        .iter()
        .next()
        .cloned()
        .unwrap_or_default()
}

fn expand_condition_disjuncts(uses: &mut Vec<ContractUse>) {
    let mut expanded = Vec::new();
    for contract_use in std::mem::take(uses) {
        // The single-disjunct row (the common case after the first expansion)
        // moves through unchanged instead of being re-cloned per pass.
        if contract_use.condition.disjuncts().len() <= 1 {
            expanded.push(contract_use);
            continue;
        }
        for conjunction in contract_use.condition.disjuncts() {
            let mut branch = contract_use.clone();
            branch.condition =
                helm_schema_core::GuardDnf::from_conjunction(conjunction.iter().cloned());
            expanded.push(branch);
        }
    }
    // Unstable sort: equal rows are fully interchangeable (dedup keeps one
    // of an identical run), so stability buys nothing here.
    expanded.sort_unstable();
    expanded.dedup();
    *uses = expanded;
}

fn contract_use_render_site_cmp(left: &ContractUse, right: &ContractUse) -> std::cmp::Ordering {
    contract_use_base_cmp(left, right)
}

fn contract_use_base_cmp(left: &ContractUse, right: &ContractUse) -> std::cmp::Ordering {
    left.source_expr
        .cmp(&right.source_expr)
        .then_with(|| left.path.0.cmp(&right.path.0))
        .then_with(|| (left.kind as u8).cmp(&(right.kind as u8)))
        .then_with(|| left.resource.cmp(&right.resource))
        .then_with(|| left.has_string_contract.cmp(&right.has_string_contract))
        // A merge-layer or digest row carries row-scoped semantics
        // (per-layer shadowing, branch-only serialized tolerance), so it
        // must not fold into a plain row at the same site: the fold keeps
        // one row's marker for ALL unioned disjuncts and mis-attributes
        // the other's (airflow's otel `mustMerge` labels beside the
        // pod-template `with` renders).
        .then_with(|| left.merge_layers.cmp(&right.merge_layers))
        .then_with(|| left.digest.cmp(&right.digest))
}

fn merge_contract_use_provenance(
    target: &mut ContractUse,
    incoming: Vec<crate::ContractProvenance>,
) {
    target.provenance.extend(incoming);
    target.provenance.sort();
    target.provenance.dedup();
}

#[cfg(test)]
#[path = "tests/contract_normalization.rs"]
mod tests;
