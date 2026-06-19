use std::collections::{BTreeMap, BTreeSet};

use crate::ValueKind;
use crate::contract::{ContractTypeHint, ContractUse};
use crate::contract_signals::{
    ConditionalGuard, ConditionalOverlayEvidence, ConditionalPathOverlay,
    ContractPathSchemaEvidence, ContractRequirednessEvidence, ContractSchemaSignals,
    ContractValuePathFacts, MetadataFieldKind,
};
use crate::provider_schema_use::{ProviderSchemaUse, from_contract_use};

use super::classifiers::{
    metadata_field_kind_from_yaml_path, use_is_null_tolerant, use_is_self_guarded,
};
use super::path_signals::ContractPathSignals;
use super::value_path_facts::{
    RenderPathFacts, build_contract_value_path_facts, collect_paths_with_descendants,
};

pub(crate) fn derive_schema_signals_from_contract_parts(
    uses: &[ContractUse],
    type_hints: &[ContractTypeHint],
) -> ContractSchemaSignals {
    let mut builder = ContractSchemaSignalBuilder::default();
    for contract_use in uses {
        builder.record(contract_use);
    }
    for type_hint in type_hints {
        builder.record_declared_type_hint(type_hint);
    }
    builder.finish()
}

#[derive(Default)]
struct ContractSchemaSignalBuilder {
    render_facts_by_path: BTreeMap<String, RenderPathFacts>,
    path_signals: ContractPathSignals,
    provider_schema_uses: Vec<ProviderSchemaUse>,
    requiredness_by_path: BTreeMap<String, ContractRequirednessEvidence>,
    type_hints_by_value_path: BTreeMap<String, BTreeSet<String>>,
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
        self.record_requiredness(contract_use);
        self.record_nullable_path(contract_use);
        self.record_conditional_overlay(contract_use);
    }

    fn finish(mut self) -> ContractSchemaSignals {
        self.path_signals.referenced_value_paths.extend(
            self.type_hints_by_value_path
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
        let conditional_overlays_by_path = self
            .conditional_overlays_by_path
            .into_iter()
            .map(|(target_value_path, accumulator)| {
                let global_facts = value_path_facts
                    .get(&target_value_path)
                    .copied()
                    .unwrap_or_default();
                let type_hints = self
                    .type_hints_by_value_path
                    .get(&target_value_path)
                    .cloned()
                    .unwrap_or_default();
                (
                    target_value_path.clone(),
                    accumulator.finish(global_facts, type_hints),
                )
            })
            .collect();
        let schema_evidence_by_value_path = build_schema_evidence_by_value_path(
            &self.path_signals,
            &self.provider_schema_uses,
            &self.requiredness_by_path,
            &self.type_hints_by_value_path,
            &value_path_facts,
            &conditional_overlays_by_path,
        );

        ContractSchemaSignals::new(schema_evidence_by_value_path)
    }

    fn record_declared_type_hint(&mut self, type_hint: &ContractTypeHint) {
        self.type_hints_by_value_path
            .entry(type_hint.value_path.clone())
            .or_default()
            .extend(type_hint.schema_types.iter().cloned());
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
            self.record_render_use(
                &contract_use.source_expr,
                contract_use.has_self_range_guard(),
                Some(use_is_self_guarded(contract_use)),
            );
        }

        let range_guard_paths = contract_use.top_level_range_guard_paths();
        for path in contract_use.guard_value_paths() {
            if path.trim().is_empty() || path == contract_use.source_expr {
                continue;
            }
            self.render_path_facts(&path);
            if !contract_use.path.0.is_empty() {
                self.record_render_use(&path, range_guard_paths.contains(&path), None);
            }
        }
    }

    fn record_empty_source_render_facts(&mut self, contract_use: &ContractUse) {
        let range_guard_paths = contract_use.top_level_range_guard_paths();
        for path in contract_use.guard_value_paths() {
            if path.trim().is_empty() {
                continue;
            }
            self.render_path_facts(&path);
            if !contract_use.path.0.is_empty() {
                self.record_render_use(&path, range_guard_paths.contains(&path), None);
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
        for path in contract_use.guard_value_paths() {
            if path.trim().is_empty() {
                continue;
            }
            self.path_signals.referenced_value_paths.insert(path);
        }
        for path in contract_use.top_level_range_guard_paths() {
            if path.trim().is_empty() {
                continue;
            }
            self.path_signals.ranged_value_paths.insert(path);
        }
        for predicate in contract_use.conditional_guard_predicates() {
            self.record_guard_predicate(predicate);
        }
    }

    fn record_guard_predicate(&mut self, predicate: ConditionalGuard) {
        let mut paths = BTreeSet::new();
        collect_conditional_guard_paths(&predicate, &mut paths);
        for path in paths {
            if path.trim().is_empty() {
                continue;
            }
            let predicates = self
                .path_signals
                .guard_predicates_by_value_path
                .entry(path)
                .or_default();
            if !predicates.contains(&predicate) {
                predicates.push(predicate.clone());
            }
        }
    }

    fn record_requiredness(&mut self, contract_use: &ContractUse) {
        for path in contract_use.conditionally_optional_paths() {
            self.record_conditionally_optional_path(&path);
        }
        for path in contract_use.default_fallback_paths() {
            self.record_default_fallback_path(&path);
        }

        if contract_use.kind == ValueKind::Scalar
            && contract_use.path.0.is_empty()
            && !contract_use.source_expr.trim().is_empty()
            && contract_use.is_positive_header()
        {
            self.requiredness(&contract_use.source_expr)
                .is_positive_header = true;
        }
    }

    fn record_conditionally_optional_path(&mut self, path: &str) {
        if path.trim().is_empty() {
            return;
        }
        self.requiredness(path).is_conditionally_optional = true;
    }

    fn record_default_fallback_path(&mut self, path: &str) {
        if path.trim().is_empty() {
            return;
        }
        self.requiredness(path).has_default_fallback = true;
    }

    fn requiredness(&mut self, path: &str) -> &mut ContractRequirednessEvidence {
        self.requiredness_by_path
            .entry(path.to_string())
            .or_default()
    }

    fn record_nullable_path(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            return;
        }

        let info = self.nullable_accumulator(&contract_use.source_expr);
        if !contract_use.path.0.is_empty()
            || contract_use.has_self_range_guard()
            || contract_use.kind == ValueKind::Fragment
            || contract_use.has_pathless_self_default_guard()
        {
            info.has_render_use = true;
            info.has_self_guarded_render_use |= use_is_self_guarded(contract_use);
        }
        info.all_uses_nullable &= use_is_null_tolerant(contract_use);

        for path in contract_use.top_level_range_guard_paths() {
            if !path.trim().is_empty() {
                self.nullable_accumulator(&path).has_render_use = true;
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

        let Some(guards) = contract_use.lowerable_conditional_guard_set() else {
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
                let evidence = branch.schema_evidence(global_facts, type_hints.clone());
                ConditionalPathOverlay {
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

        self.has_self_range_guard_render_use |= contract_use.has_self_range_guard();

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
        global_facts: ContractValuePathFacts,
        type_hints: BTreeSet<String>,
    ) -> ConditionalOverlayEvidence {
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
        ConditionalOverlayEvidence {
            facts,
            metadata_field_kinds: self.metadata_field_kinds,
            type_hints,
            provider_schema_uses: self.provider_schema_uses,
        }
    }
}

fn build_schema_evidence_by_value_path(
    path_signals: &ContractPathSignals,
    provider_schema_uses: &[ProviderSchemaUse],
    requiredness_by_path: &BTreeMap<String, ContractRequirednessEvidence>,
    type_hints_by_value_path: &BTreeMap<String, BTreeSet<String>>,
    value_path_facts: &BTreeMap<String, ContractValuePathFacts>,
    conditional_overlays_by_path: &BTreeMap<String, Vec<ConditionalPathOverlay>>,
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
    paths.extend(requiredness_by_path.keys().cloned());
    paths.extend(conditional_overlays_by_path.keys().cloned());

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
                guard_predicates: path_signals
                    .guard_predicates_by_value_path
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
                requiredness: requiredness_by_path
                    .get(&value_path)
                    .copied()
                    .unwrap_or_default(),
                conditional_overlays: conditional_overlays_by_path
                    .get(&value_path)
                    .cloned()
                    .unwrap_or_default(),
            };
            (value_path, evidence)
        })
        .collect()
}

fn collect_conditional_guard_paths(guard: &ConditionalGuard, paths: &mut BTreeSet<String>) {
    match guard {
        ConditionalGuard::Truthy { path }
        | ConditionalGuard::With { path }
        | ConditionalGuard::Eq { path, .. }
        | ConditionalGuard::NotEq { path, .. }
        | ConditionalGuard::Absent { path }
        | ConditionalGuard::TypeIs { path, .. } => {
            paths.insert(path.clone());
        }
        ConditionalGuard::Not(inner) => collect_conditional_guard_paths(inner, paths),
        ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
            for guard in guards {
                collect_conditional_guard_paths(guard, paths);
            }
        }
    }
}
