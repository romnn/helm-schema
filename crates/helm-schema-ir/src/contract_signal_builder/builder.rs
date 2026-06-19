use std::collections::{BTreeMap, BTreeSet};

use crate::contract::ContractUse;
use crate::contract_signals::{
    ConditionalGuard, ConditionalPathOverlay, ContractPathSchemaEvidence, ContractPathSignals,
    ContractSchemaSignals, ContractValuePathFacts, MetadataFieldKind,
};
use crate::provider_schema_use::{ProviderSchemaUse, from_contract_use};
use crate::{Guard, ValueKind};

use super::classifiers::{
    guard_constraint_from_guard, metadata_field_kind_from_yaml_path, use_is_null_tolerant,
    use_is_self_guarded,
};
use super::value_path_facts::{
    RenderPathFacts, build_contract_value_path_facts, collect_paths_with_descendants,
};

pub(crate) fn derive_schema_signals_from_uses(
    uses: &[ContractUse],
    type_hints_by_value_path: &BTreeMap<String, BTreeSet<String>>,
) -> ContractSchemaSignals {
    let mut builder = ContractSchemaSignalBuilder::default();
    for contract_use in uses {
        builder.record(contract_use);
    }
    builder.finish(type_hints_by_value_path.clone())
}

#[derive(Default)]
struct ContractSchemaSignalBuilder {
    render_facts_by_path: BTreeMap<String, RenderPathFacts>,
    path_signals: ContractPathSignals,
    provider_schema_uses: Vec<ProviderSchemaUse>,
    nullable_by_path: BTreeMap<String, NullablePathAccumulator>,
    conditional_overlays_by_path: BTreeMap<String, ConditionalOverlayAccumulator>,
}

struct NullablePathAccumulator {
    has_render_use: bool,
    has_self_guarded_render_use: bool,
    all_uses_nullable: bool,
}

#[derive(Default)]
struct ConditionalOverlayAccumulator {
    branches_by_guards: BTreeMap<Vec<ConditionalGuard>, ConditionalOverlayBranchAccumulator>,
    has_unconditional_peer_use: bool,
    saw_unsupported: bool,
}

#[derive(Default)]
struct ConditionalOverlayBranchAccumulator {
    provider_schema_uses: Vec<ProviderSchemaUse>,
    metadata_field_kinds: BTreeSet<MetadataFieldKind>,
    has_render_use: bool,
    has_self_guarded_render_use: bool,
    all_render_uses_self_guarded: bool,
    has_self_range_guard_render_use: bool,
    all_uses_nullable: bool,
    used_as_fragment: bool,
    is_partial_scalar_value_path: bool,
}

impl Default for NullablePathAccumulator {
    fn default() -> Self {
        Self {
            has_render_use: false,
            has_self_guarded_render_use: false,
            all_uses_nullable: true,
        }
    }
}

impl ContractSchemaSignalBuilder {
    fn record(&mut self, contract_use: &ContractUse) {
        self.record_provider_schema_use(contract_use);
        self.record_render_facts(contract_use);
        self.record_path_signals(contract_use);
        self.record_nullable_path(contract_use);
        self.record_conditional_overlay(contract_use);
    }

    fn finish(
        mut self,
        type_hints_by_value_path: BTreeMap<String, BTreeSet<String>>,
    ) -> ContractSchemaSignals {
        self.path_signals.referenced_value_paths.extend(
            type_hints_by_value_path
                .keys()
                .filter(|path| !path.trim().is_empty())
                .cloned(),
        );
        let paths_with_referenced_descendants =
            collect_paths_with_descendants(&self.path_signals.referenced_value_paths);
        let nullable_value_paths = self
            .nullable_by_path
            .into_iter()
            .filter_map(|(path, acc)| (acc.has_render_use && acc.all_uses_nullable).then_some(path))
            .collect();
        let value_path_facts = build_contract_value_path_facts(
            &self.render_facts_by_path,
            &self.path_signals,
            &nullable_value_paths,
            &paths_with_referenced_descendants,
        );
        let schema_evidence_by_value_path = build_schema_evidence_by_value_path(
            &self.path_signals,
            &self.provider_schema_uses,
            &type_hints_by_value_path,
            &value_path_facts,
        );
        let conditional_path_overlays = self
            .conditional_overlays_by_path
            .into_iter()
            .flat_map(|(target_value_path, accumulator)| {
                let global_facts = value_path_facts
                    .get(&target_value_path)
                    .copied()
                    .unwrap_or_default();
                let type_hints = type_hints_by_value_path
                    .get(&target_value_path)
                    .cloned()
                    .unwrap_or_default();
                accumulator.finish(target_value_path, global_facts, type_hints)
            })
            .collect();

        ContractSchemaSignals {
            path_signals: self.path_signals,
            provider_schema_uses: self.provider_schema_uses,
            type_hints_by_value_path,
            nullable_value_paths,
            paths_with_referenced_descendants,
            value_path_facts,
            schema_evidence_by_value_path,
            conditional_path_overlays,
        }
    }

    fn record_provider_schema_use(&mut self, contract_use: &ContractUse) {
        if let Some(provider_use) = from_contract_use(contract_use) {
            self.provider_schema_uses.push(provider_use);
        }
    }

    fn record_render_facts(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            self.record_empty_source_render_facts(contract_use);
            return;
        }

        self.render_path_facts(&contract_use.source_expr);
        if !contract_use.path.0.is_empty() {
            let self_range_guarded = contract_use.guards.iter().any(
                |guard| matches!(guard, Guard::Range { path } if path == &contract_use.source_expr),
            );
            self.record_render_use(
                &contract_use.source_expr,
                self_range_guarded,
                Some(use_is_self_guarded(contract_use)),
            );
        }

        for guard in &contract_use.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() || path == contract_use.source_expr {
                    continue;
                }
                self.render_path_facts(path);
                if !contract_use.path.0.is_empty() {
                    self.record_render_use(path, matches!(guard, Guard::Range { .. }), None);
                }
            }
        }
    }

    fn record_empty_source_render_facts(&mut self, contract_use: &ContractUse) {
        for guard in &contract_use.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() {
                    continue;
                }
                self.render_path_facts(path);
                if !contract_use.path.0.is_empty() {
                    self.record_render_use(path, matches!(guard, Guard::Range { .. }), None);
                }
            }
        }
    }

    fn render_path_facts(&mut self, path: &str) -> &mut RenderPathFacts {
        self.render_facts_by_path
            .entry(path.to_string())
            .or_default()
    }

    fn record_render_use(&mut self, path: &str, range_guarded: bool, self_guarded: Option<bool>) {
        let acc = self.render_path_facts(path);
        acc.has_render_use = true;
        acc.has_self_range_guard_render_use |= range_guarded;
        if let Some(self_guarded) = self_guarded {
            acc.has_self_guarded_render_use |= self_guarded;
            acc.all_render_uses_self_guarded &= self_guarded;
        }
    }

    fn record_path_signals(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            return;
        }

        self.path_signals
            .referenced_value_paths
            .insert(contract_use.source_expr.clone());
        if contract_use.kind == ValueKind::Fragment {
            self.path_signals
                .value_paths_used_as_fragment
                .insert(contract_use.source_expr.clone());
        }
        if contract_use.kind == ValueKind::PartialScalar && !contract_use.path.0.is_empty() {
            self.path_signals
                .partial_scalar_value_paths
                .insert(contract_use.source_expr.clone());
        }
        if let Some(field_kind) = metadata_field_kind_from_yaml_path(&contract_use.path.0) {
            self.path_signals
                .metadata_fields_by_value_path
                .entry(contract_use.source_expr.clone())
                .or_default()
                .insert(field_kind);
        }
        for guard in &contract_use.guards {
            for path in guard.value_paths() {
                if path.trim().is_empty() {
                    continue;
                }
                self.path_signals
                    .referenced_value_paths
                    .insert(path.to_string());
                if matches!(guard, Guard::Range { .. }) {
                    self.path_signals
                        .ranged_value_paths
                        .insert(path.to_string());
                }

                if let Some(constraint) = guard_constraint_from_guard(guard) {
                    self.path_signals
                        .guard_constraints_by_value_path
                        .entry(path.to_string())
                        .or_default()
                        .push(constraint);
                }
            }
        }
    }

    fn record_nullable_path(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            return;
        }

        let has_self_range_guard = contract_use.guards.iter().any(
            |guard| matches!(guard, Guard::Range { path } if path == &contract_use.source_expr),
        );
        let has_pathless_self_default_guard = contract_use.path.0.is_empty()
            && contract_use
                .guards
                .iter()
                .any(|guard| matches!(guard, Guard::Default { path } if path == &contract_use.source_expr));
        let info = self.nullable_accumulator(&contract_use.source_expr);
        if !contract_use.path.0.is_empty()
            || has_self_range_guard
            || contract_use.kind == ValueKind::Fragment
            || has_pathless_self_default_guard
        {
            info.has_render_use = true;
            info.has_self_guarded_render_use |= use_is_self_guarded(contract_use);
        }
        info.all_uses_nullable &= use_is_null_tolerant(contract_use);

        for guard in &contract_use.guards {
            if let Guard::Range { path } = guard
                && !path.trim().is_empty()
            {
                self.nullable_accumulator(path).has_render_use = true;
            }
        }
    }

    fn nullable_accumulator(&mut self, path: &str) -> &mut NullablePathAccumulator {
        self.nullable_by_path.entry(path.to_string()).or_default()
    }

    fn record_conditional_overlay(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() || contract_use.path.0.is_empty() {
            return;
        }

        let accumulator = self
            .conditional_overlays_by_path
            .entry(contract_use.source_expr.clone())
            .or_default();

        if contract_use.guards.is_empty() {
            accumulator.has_unconditional_peer_use = true;
            return;
        }

        let Some(guards) = lowerable_guard_set(contract_use) else {
            accumulator.saw_unsupported = true;
            return;
        };

        let branch = accumulator
            .branches_by_guards
            .entry(guards)
            .or_insert_with(ConditionalOverlayBranchAccumulator::new);
        branch.record_use(contract_use);
    }
}

impl ConditionalOverlayAccumulator {
    fn finish(
        self,
        target_value_path: String,
        global_facts: ContractValuePathFacts,
        type_hints: BTreeSet<String>,
    ) -> Vec<ConditionalPathOverlay> {
        if self.saw_unsupported {
            return Vec::new();
        }
        let preserve_base_schema = self.has_unconditional_peer_use;
        self.branches_by_guards
            .into_iter()
            .map(|(guards, branch)| {
                let evidence = branch.schema_evidence(
                    target_value_path.clone(),
                    global_facts,
                    type_hints.clone(),
                );
                ConditionalPathOverlay {
                    target_value_path: target_value_path.clone(),
                    guards,
                    evidence,
                    preserve_base_schema,
                }
            })
            .collect()
    }
}

impl ConditionalOverlayBranchAccumulator {
    fn new() -> Self {
        Self {
            provider_schema_uses: Vec::new(),
            metadata_field_kinds: BTreeSet::new(),
            has_render_use: false,
            has_self_guarded_render_use: false,
            all_render_uses_self_guarded: true,
            has_self_range_guard_render_use: false,
            all_uses_nullable: true,
            used_as_fragment: false,
            is_partial_scalar_value_path: false,
        }
    }

    fn record_use(&mut self, contract_use: &ContractUse) {
        self.has_render_use = true;
        self.has_self_guarded_render_use |= use_is_self_guarded(contract_use);
        self.all_render_uses_self_guarded &= use_is_self_guarded(contract_use);
        self.all_uses_nullable &= use_is_null_tolerant(contract_use);
        self.used_as_fragment |= contract_use.kind == ValueKind::Fragment;
        self.is_partial_scalar_value_path |= contract_use.kind == ValueKind::PartialScalar;

        let has_self_range_guard = contract_use.guards.iter().any(
            |guard| matches!(guard, Guard::Range { path } if path == &contract_use.source_expr),
        );
        self.has_self_range_guard_render_use |= has_self_range_guard;

        if let Some(field_kind) = metadata_field_kind_from_yaml_path(&contract_use.path.0) {
            self.metadata_field_kinds.insert(field_kind);
        }

        if let Some(provider_schema_use) = from_contract_use(contract_use)
            && !self.provider_schema_uses.contains(&provider_schema_use)
        {
            self.provider_schema_uses.push(provider_schema_use);
        }
    }

    fn schema_evidence(
        self,
        value_path: String,
        global_facts: ContractValuePathFacts,
        type_hints: BTreeSet<String>,
    ) -> ContractPathSchemaEvidence {
        let facts = ContractValuePathFacts {
            has_referenced_descendants: global_facts.has_referenced_descendants,
            used_as_fragment: self.used_as_fragment,
            is_ranged_source: false,
            is_partial_scalar_value_path: self.is_partial_scalar_value_path,
            has_render_use: self.has_render_use,
            has_self_guarded_render_use: self.has_self_guarded_render_use,
            all_render_uses_self_guarded: self.all_render_uses_self_guarded,
            has_self_range_guard_render_use: self.has_self_range_guard_render_use,
            is_nullable: self.has_render_use && self.all_uses_nullable,
        };
        ContractPathSchemaEvidence {
            value_path,
            is_referenced_value_path: true,
            facts,
            guard_constraints: Vec::new(),
            metadata_field_kinds: self.metadata_field_kinds,
            type_hints,
            provider_schema_uses: self.provider_schema_uses,
        }
    }
}

fn build_schema_evidence_by_value_path(
    path_signals: &ContractPathSignals,
    provider_schema_uses: &[ProviderSchemaUse],
    type_hints_by_value_path: &BTreeMap<String, BTreeSet<String>>,
    value_path_facts: &BTreeMap<String, ContractValuePathFacts>,
) -> BTreeMap<String, ContractPathSchemaEvidence> {
    let mut provider_uses_by_path: BTreeMap<String, Vec<ProviderSchemaUse>> = BTreeMap::new();
    for provider_use in provider_schema_uses {
        provider_uses_by_path
            .entry(provider_use.value_path.clone())
            .or_default()
            .push(provider_use.clone());
    }

    let mut paths = BTreeSet::new();
    paths.extend(value_path_facts.keys().cloned());
    paths.extend(type_hints_by_value_path.keys().cloned());
    paths.extend(provider_uses_by_path.keys().cloned());

    paths
        .into_iter()
        .map(|value_path| {
            let evidence = ContractPathSchemaEvidence {
                value_path: value_path.clone(),
                is_referenced_value_path: path_signals.referenced_value_paths.contains(&value_path),
                facts: value_path_facts
                    .get(&value_path)
                    .copied()
                    .unwrap_or_default(),
                guard_constraints: path_signals
                    .guard_constraints_by_value_path
                    .get(&value_path)
                    .cloned()
                    .unwrap_or_default(),
                metadata_field_kinds: path_signals
                    .metadata_fields_by_value_path
                    .get(&value_path)
                    .cloned()
                    .unwrap_or_default(),
                type_hints: type_hints_by_value_path
                    .get(&value_path)
                    .cloned()
                    .unwrap_or_default(),
                provider_schema_uses: provider_uses_by_path
                    .get(&value_path)
                    .cloned()
                    .unwrap_or_default(),
            };
            (value_path, evidence)
        })
        .collect()
}

fn lowerable_guard_set(contract_use: &ContractUse) -> Option<Vec<ConditionalGuard>> {
    if path_contains_wildcard(&contract_use.source_expr) {
        return None;
    }

    let mut guards = Vec::new();
    for guard in &contract_use.guards {
        extend_lowerable_guard(guard, &contract_use.source_expr, &mut guards)?;
    }

    guards.sort();
    guards.dedup();
    Some(guards)
}

fn lowerable_guard_alternative(
    alternative: &[Guard],
    target_value_path: &str,
) -> Option<ConditionalGuard> {
    let mut guards = Vec::new();
    for guard in alternative {
        extend_lowerable_guard(guard, target_value_path, &mut guards)?;
    }
    guards.sort();
    guards.dedup();
    match guards.as_slice() {
        [] => None,
        [guard] => Some(guard.clone()),
        _ => Some(ConditionalGuard::AllOf(guards)),
    }
}

fn extend_lowerable_guard(
    guard: &Guard,
    target_value_path: &str,
    guards: &mut Vec<ConditionalGuard>,
) -> Option<()> {
    match guard {
        Guard::With { .. } => {}
        Guard::Truthy { path } => guards.push(ConditionalGuard::Truthy {
            path: lowerable_guard_path(path, target_value_path)?,
        }),
        Guard::Eq { path, value } => guards.push(ConditionalGuard::Eq {
            path: lowerable_guard_path(path, target_value_path)?,
            value: value.clone(),
        }),
        Guard::NotEq { path, value } => guards.push(ConditionalGuard::NotEq {
            path: lowerable_guard_path(path, target_value_path)?,
            value: value.clone(),
        }),
        Guard::Absent { path } => guards.push(ConditionalGuard::Absent {
            path: lowerable_guard_path(path, target_value_path)?,
        }),
        Guard::TypeIs { path, schema_type } => guards.push(ConditionalGuard::TypeIs {
            path: lowerable_guard_path(path, target_value_path)?,
            schema_type: schema_type.clone(),
        }),
        Guard::Not { path } => {
            guards.push(ConditionalGuard::Not(Box::new(ConditionalGuard::Truthy {
                path: lowerable_guard_path(path, target_value_path)?,
            })));
        }
        Guard::Or { paths } => {
            let mut any_of = paths
                .iter()
                .map(|path| {
                    Some(ConditionalGuard::Truthy {
                        path: lowerable_guard_path(path, target_value_path)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            any_of.sort();
            any_of.dedup();
            guards.push(ConditionalGuard::AnyOf(any_of));
        }
        Guard::AnyOf { alternatives } => {
            let mut any_of = alternatives
                .iter()
                .map(|alternative| lowerable_guard_alternative(alternative, target_value_path))
                .collect::<Option<Vec<_>>>()?;
            any_of.sort();
            any_of.dedup();
            guards.push(ConditionalGuard::AnyOf(any_of));
        }
        Guard::Range { .. } => return None,
        Guard::Default { path } if path == target_value_path => {}
        Guard::Default { .. } => return None,
    }
    Some(())
}

fn lowerable_guard_path(path: &str, target_value_path: &str) -> Option<String> {
    (!path_contains_wildcard(path) && path != target_value_path).then(|| path.to_string())
}

fn path_contains_wildcard(path: &str) -> bool {
    path.split('.').any(|segment| segment == "*")
}
