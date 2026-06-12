use std::collections::{BTreeMap, BTreeSet};

use crate::{Guard, ResourceRef, ValueKind, ValueUse, YamlPath};

/// Internal compatibility-era contract use.
///
/// The symbolic interpreter builds these as semantic claims. The public
/// `ValueUse` type is only produced after contract normalization, so later
/// phases can grow richer contract witnesses without teaching the interpreter
/// to construct fixture DTOs directly.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ContractUse {
    pub(crate) source_expr: String,
    pub(crate) path: YamlPath,
    pub(crate) kind: ValueKind,
    pub(crate) guards: Vec<Guard>,
    pub(crate) resource: Option<ResourceRef>,
}

impl ContractUse {
    pub(crate) fn new(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
    ) -> Self {
        Self {
            source_expr,
            path,
            kind,
            guards,
            resource,
        }
    }

    fn into_value_use(self) -> ValueUse {
        ValueUse {
            source_expr: self.source_expr,
            path: self.path,
            kind: self.kind,
            guards: self.guards,
            resource: self.resource,
        }
    }
}

/// Receives contract claims from node/action interpretation.
///
/// Some helper-summary passes intentionally implement this as a no-op because
/// they collect local helper facts rather than root chart contract claims.
pub(crate) trait ContractUseSink {
    fn emit_contract_use(&mut self, source_expr: String, path: YamlPath, kind: ValueKind);

    fn emit_contract_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    );
}

pub(crate) fn finalize_contract_uses(mut uses: Vec<ContractUse>) -> Vec<ValueUse> {
    normalize_contract_uses(&mut uses);
    uses.into_iter().map(ContractUse::into_value_use).collect()
}

fn normalize_contract_uses(uses: &mut Vec<ContractUse>) {
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
