use std::collections::{BTreeMap, BTreeSet};

use crate::{Guard, ValueKind, ValueUse};

pub(crate) fn postprocess_value_uses(uses: &mut Vec<ValueUse>) {
    drop_default_guard_subsumed_duplicates(uses);
    drop_values_list_envelope_duplicates(uses);
    merge_pathless_resource_variants(uses);
    uses.sort_by(|a, b| {
        a.source_expr
            .cmp(&b.source_expr)
            .then_with(|| a.path.0.cmp(&b.path.0))
            .then_with(|| (a.kind as u8).cmp(&(b.kind as u8)))
            .then_with(|| a.resource.cmp(&b.resource))
            .then_with(|| a.guards.cmp(&b.guards))
    });
    uses.dedup();
}

fn merge_pathless_resource_variants(uses: &mut Vec<ValueUse>) {
    let mut merged = Vec::with_capacity(uses.len());
    let mut pathless_index_by_identity: BTreeMap<(String, ValueKind, Vec<Guard>), usize> =
        BTreeMap::new();

    for value_use in std::mem::take(uses) {
        if value_use.path.0.is_empty() {
            let key = (
                value_use.source_expr.clone(),
                value_use.kind,
                value_use.guards.clone(),
            );
            if let Some(index) = pathless_index_by_identity.get(&key).copied() {
                let existing_resource = merged
                    .get(index)
                    .and_then(|existing: &ValueUse| existing.resource.clone());
                match (existing_resource, &value_use.resource) {
                    (None, Some(_)) => {
                        if let Some(existing) = merged.get_mut(index) {
                            existing.resource = value_use.resource;
                        }
                        continue;
                    }
                    (None, None) => continue,
                    (Some(existing), Some(resource)) if existing == *resource => continue,
                    (Some(_), None) => continue,
                    (Some(_), Some(_)) => {}
                }
            }
            pathless_index_by_identity.insert(key, merged.len());
        }
        merged.push(value_use);
    }

    *uses = merged;
}

fn drop_default_guard_subsumed_duplicates(uses: &mut Vec<ValueUse>) {
    let defaulted_render_sites: BTreeSet<_> = uses
        .iter()
        .filter(|value_use| {
            value_use.guards.iter().any(
                |guard| matches!(guard, Guard::Default { path } if path == &value_use.source_expr),
            )
        })
        .map(|value_use| {
            (
                value_use.source_expr.clone(),
                value_use.path.clone(),
                value_use.kind,
                value_use.resource.clone(),
            )
        })
        .collect();

    uses.retain(|value_use| {
        if value_use
            .guards
            .iter()
            .any(|guard| matches!(guard, Guard::Default { path } if path == &value_use.source_expr))
        {
            return true;
        }
        !defaulted_render_sites.contains(&(
            value_use.source_expr.clone(),
            value_use.path.clone(),
            value_use.kind,
            value_use.resource.clone(),
        ))
    });
}

fn drop_values_list_envelope_duplicates(uses: &mut Vec<ValueUse>) {
    let render_sites: BTreeSet<_> = uses
        .iter()
        .map(|value_use| {
            (
                value_use.source_expr.clone(),
                value_use.path.clone(),
                value_use.kind,
                value_use.resource.clone(),
            )
        })
        .collect();

    uses.retain(|value_use| {
        let Some(index) = value_use
            .path
            .0
            .iter()
            .position(|segment| segment == "values[*]")
        else {
            return true;
        };
        let mut collapsed_path = value_use.path.clone();
        collapsed_path.0.remove(index);
        !render_sites.contains(&(
            value_use.source_expr.clone(),
            collapsed_path,
            value_use.kind,
            value_use.resource.clone(),
        ))
    });
}
