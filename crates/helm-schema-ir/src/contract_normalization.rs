use std::collections::{BTreeMap, BTreeSet};

use crate::contract::ContractUse;
use crate::{Guard, ResourceRef, ValueKind, YamlPath};
use helm_schema_core as output_path;

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
    uses.sort_by(contract_use_semantic_cmp);

    let mut merged = Vec::with_capacity(uses.len());
    for contract_use in std::mem::take(uses) {
        if let Some(existing) = merged.last_mut()
            && contract_use_semantic_cmp(existing, &contract_use).is_eq()
        {
            merge_contract_use_provenance(existing, contract_use.provenance);
            continue;
        }
        merged.push(contract_use);
    }
    *uses = merged;
}

fn canonicalize_contract_use_inputs(uses: &mut [ContractUse]) {
    for contract_use in uses {
        contract_use.canonicalize();
    }
}

#[tracing::instrument(skip_all)]
fn merge_pathless_resource_variants(uses: &mut Vec<ContractUse>) {
    let mut merged: Vec<ContractUse> = Vec::with_capacity(uses.len());
    let mut pathless_index_by_identity: BTreeMap<(String, ValueKind, Vec<Guard>), usize> =
        BTreeMap::new();

    for mut contract_use in std::mem::take(uses) {
        if contract_use.path.0.is_empty() {
            let key = (
                contract_use.source_expr.clone(),
                contract_use.kind,
                contract_use.guards.clone(),
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
fn drop_self_truthy_subsumed_duplicates(uses: &mut Vec<ContractUse>) {
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
    for indices in buckets.values() {
        if indices.len() < 2 {
            continue;
        }
        for &index in indices {
            let Some(contract_use) = uses.get(index) else {
                continue;
            };
            let has_self_truthy = contract_use.guards.iter().any(
                |guard| matches!(guard, Guard::Truthy { path } if path == &contract_use.source_expr),
            );
            if contract_use.guards.iter().any(
                |guard| matches!(guard, Guard::Default { path } if path == &contract_use.source_expr),
            ) {
                continue;
            }
            let subsumed = indices
                .iter()
                .filter_map(|&other_index| uses.get(other_index))
                .any(|other| {
                    !other.provenance.is_empty()
                        && ((contract_use.provenance.is_empty()
                            && contract_use.resource.is_some())
                            || other.provenance == contract_use.provenance)
                        && other.guards.len() > contract_use.guards.len()
                        && contract_use
                            .guards
                            .iter()
                            .all(|guard| other.guards.contains(guard))
                        && ((!has_self_truthy
                            && other.guards.iter().any(|guard| {
                                matches!(guard, Guard::Truthy { path } if path == &contract_use.source_expr)
                            }))
                            || extra_guards_are_truthy_parents(contract_use, other))
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

fn extra_guards_are_truthy_parents(contract_use: &ContractUse, other: &ContractUse) -> bool {
    other
        .guards
        .iter()
        .filter(|guard| !contract_use.guards.contains(guard))
        .all(|guard| {
            let Guard::Truthy { path: parent } = guard else {
                return false;
            };
            contract_use.guards.iter().any(|existing| {
                matches!(
                    existing,
                    Guard::Truthy { path: child }
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
    contract_use
        .guards
        .iter()
        .any(|guard| matches!(guard, Guard::Default { path } if path == &contract_use.source_expr))
}

fn contract_use_semantic_cmp(left: &ContractUse, right: &ContractUse) -> std::cmp::Ordering {
    left.source_expr
        .cmp(&right.source_expr)
        .then_with(|| left.path.0.cmp(&right.path.0))
        .then_with(|| (left.kind as u8).cmp(&(right.kind as u8)))
        .then_with(|| left.resource.cmp(&right.resource))
        .then_with(|| left.guards.cmp(&right.guards))
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
