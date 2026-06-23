use std::collections::{BTreeMap, BTreeSet};

use crate::contract::ContractUse;
use crate::{Guard, ValueKind};

/// Apply semantic finalization to claims produced by the interpreter.
///
/// This removes duplicates that are equivalent after chart-default lowering,
/// collapses transparent `kind: List` envelope projections, prefers resource
/// evidence for pathless duplicate roots, and then canonicalizes ordering.
pub(crate) fn normalize_contract_uses(uses: &mut Vec<ContractUse>) {
    drop_default_guard_subsumed_duplicates(uses);
    drop_values_list_envelope_duplicates(uses);
    merge_pathless_resource_variants(uses);
    canonicalize_contract_uses(uses);
}

/// Canonicalize contract claims without dropping semantically distinct rows.
///
/// Tests and expert callers use this when they provide already-structured
/// claims and need deterministic ordering without losing raw evidence such as
/// one nullable and one non-nullable render site.
pub(crate) fn canonicalize_contract_uses(uses: &mut Vec<ContractUse>) {
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

fn merge_pathless_resource_variants(uses: &mut Vec<ContractUse>) {
    let mut merged = Vec::with_capacity(uses.len());
    let mut pathless_index_by_identity: BTreeMap<(String, ValueKind, Vec<Guard>), usize> =
        BTreeMap::new();

    for contract_use in std::mem::take(uses) {
        if contract_use.path.0.is_empty() {
            let key = (
                contract_use.source_expr.clone(),
                contract_use.kind,
                contract_use.guards.clone(),
            );
            if let Some(index) = pathless_index_by_identity.get(&key).copied() {
                let existing_resource = merged
                    .get(index)
                    .and_then(|existing: &ContractUse| existing.resource.clone());
                match (existing_resource, &contract_use.resource) {
                    (None, Some(_)) => {
                        if let Some(existing) = merged.get_mut(index) {
                            existing.resource = contract_use.resource;
                            merge_contract_use_provenance(existing, contract_use.provenance);
                        }
                        continue;
                    }
                    (None, None) => {
                        if let Some(existing) = merged.get_mut(index) {
                            merge_contract_use_provenance(existing, contract_use.provenance);
                        }
                        continue;
                    }
                    (Some(existing), Some(resource)) if existing == *resource => {
                        if let Some(existing) = merged.get_mut(index) {
                            merge_contract_use_provenance(existing, contract_use.provenance);
                        }
                        continue;
                    }
                    (Some(_), None) => {
                        if let Some(existing) = merged.get_mut(index) {
                            merge_contract_use_provenance(existing, contract_use.provenance);
                        }
                        continue;
                    }
                    (Some(_), Some(_)) => {}
                }
            }
            pathless_index_by_identity.insert(key, merged.len());
        }
        merged.push(contract_use);
    }

    *uses = merged;
}

fn drop_default_guard_subsumed_duplicates(uses: &mut Vec<ContractUse>) {
    let defaulted_render_sites: BTreeSet<_> = uses
        .iter()
        .filter(|contract_use| {
            contract_use.guards.iter().any(
                |guard| matches!(guard, Guard::Default { path } if path == &contract_use.source_expr),
            )
        })
        .map(|contract_use| {
            (
                contract_use.source_expr.clone(),
                contract_use.path.clone(),
                contract_use.kind,
                contract_use.resource.clone(),
            )
        })
        .collect();

    uses.retain(|contract_use| {
        if contract_use.guards.iter().any(
            |guard| matches!(guard, Guard::Default { path } if path == &contract_use.source_expr),
        ) {
            return true;
        }
        !defaulted_render_sites.contains(&(
            contract_use.source_expr.clone(),
            contract_use.path.clone(),
            contract_use.kind,
            contract_use.resource.clone(),
        ))
    });
}

fn drop_values_list_envelope_duplicates(uses: &mut Vec<ContractUse>) {
    let render_sites: BTreeSet<_> = uses
        .iter()
        .map(|contract_use| {
            (
                contract_use.source_expr.clone(),
                contract_use.path.clone(),
                contract_use.kind,
                contract_use.resource.clone(),
            )
        })
        .collect();

    uses.retain(|contract_use| {
        let Some(index) = contract_use
            .path
            .0
            .iter()
            .position(|segment| segment == "values[*]")
        else {
            return true;
        };
        let mut collapsed_path = contract_use.path.clone();
        collapsed_path.0.remove(index);
        !render_sites.contains(&(
            contract_use.source_expr.clone(),
            collapsed_path,
            contract_use.kind,
            contract_use.resource.clone(),
        ))
    });
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
    for provenance in incoming {
        if !target.provenance.contains(&provenance) {
            target.provenance.push(provenance);
        }
    }
}

#[cfg(test)]
#[path = "tests/contract_normalization.rs"]
mod tests;
