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
    paths: BTreeMap<String, ContractPathAccumulator>,
}

#[derive(Default)]
struct ContractPathAccumulator {
    referenced: bool,
    ranged: bool,
    used_as_fragment: bool,
    partial_scalar: bool,
    guard_predicates: Vec<ConditionalGuard>,
    evidence: PathSchemaEvidenceAccumulator,
    requiredness: ContractRequirednessEvidence,
    type_hints: BTreeSet<String>,
    conditional_overlays: ConditionalOverlayAccumulator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathUseFactsAccumulator {
    has_render_use: bool,
    has_self_guarded_render_use: bool,
    all_render_uses_self_guarded: bool,
    has_self_range_guard_render_use: bool,
    has_nullable_render_use: bool,
    all_uses_nullable: bool,
}

#[derive(Default)]
struct PathSchemaEvidenceAccumulator {
    metadata_field_kinds: BTreeSet<MetadataFieldKind>,
    provider_schema_uses: Vec<ProviderSchemaUse>,
    uses: PathUseFactsAccumulator,
}

#[derive(Default)]
struct ConditionalOverlayAccumulator {
    branches_by_guards: BTreeMap<Vec<ConditionalGuard>, ConditionalOverlayBranchAccumulator>,
    has_unconditional_peer_use: bool,
    saw_unsupported: bool,
}

#[derive(Default)]
struct ConditionalOverlayBranchAccumulator {
    evidence: PathSchemaEvidenceAccumulator,
    used_as_fragment: bool,
    is_partial_scalar_value_path: bool,
}

impl Default for PathUseFactsAccumulator {
    fn default() -> Self {
        Self {
            has_render_use: false,
            has_self_guarded_render_use: false,
            all_render_uses_self_guarded: true,
            has_self_range_guard_render_use: false,
            has_nullable_render_use: false,
            all_uses_nullable: true,
        }
    }
}

impl PathUseFactsAccumulator {
    fn record_render_use(&mut self, range_guarded: bool, self_guarded: Option<bool>) {
        self.has_render_use = true;
        self.has_self_range_guard_render_use |= range_guarded;
        if let Some(self_guarded) = self_guarded {
            self.has_self_guarded_render_use |= self_guarded;
            self.all_render_uses_self_guarded &= self_guarded;
        }
    }

    fn record_nullable_observation(&mut self, nullable: bool) {
        self.all_uses_nullable &= nullable;
    }

    fn mark_nullable_render_use(&mut self) {
        self.has_nullable_render_use = true;
    }

    fn path_facts(
        &self,
        used_as_fragment: bool,
        is_ranged_source: bool,
        is_partial_scalar_value_path: bool,
        has_referenced_descendants: bool,
    ) -> ContractValuePathFacts {
        ContractValuePathFacts {
            has_referenced_descendants,
            used_as_fragment,
            is_ranged_source,
            is_partial_scalar_value_path,
            has_render_use: self.has_render_use,
            has_self_guarded_render_use: self.has_self_guarded_render_use,
            all_render_uses_self_guarded: self.all_render_uses_self_guarded,
            has_self_range_guard_render_use: self.has_self_range_guard_render_use,
            is_nullable: self.has_nullable_render_use && self.all_uses_nullable,
        }
    }
}

impl PathSchemaEvidenceAccumulator {
    fn record_path_identity(&mut self, contract_use: &ContractUse) {
        if let Some(field_kind) = metadata_field_kind_from_yaml_path(&contract_use.path.0) {
            self.metadata_field_kinds.insert(field_kind);
        }
    }

    fn record_provider_schema_use(&mut self, provider_schema_use: ProviderSchemaUse) {
        if !self.provider_schema_uses.contains(&provider_schema_use) {
            self.provider_schema_uses.push(provider_schema_use);
        }
    }

    fn facts(
        &self,
        used_as_fragment: bool,
        is_ranged_source: bool,
        is_partial_scalar_value_path: bool,
        has_referenced_descendants: bool,
    ) -> ContractValuePathFacts {
        self.uses.path_facts(
            used_as_fragment,
            is_ranged_source,
            is_partial_scalar_value_path,
            has_referenced_descendants,
        )
    }
}

impl ContractSchemaSignalBuilder {
    fn record(&mut self, contract_use: &ContractUse) {
        self.record_provider_schema_use(contract_use);
        self.record_render_facts(contract_use);
        self.record_path_identity(contract_use);
        self.record_requiredness(contract_use);
        self.record_nullable_path(contract_use);
        self.record_conditional_overlay(contract_use);
    }

    fn finish(mut self) -> ContractSchemaSignals {
        let referenced_paths = self
            .paths
            .iter()
            .filter_map(|(path, acc)| acc.referenced.then_some(path.clone()))
            .collect();
        let paths_with_referenced_descendants = collect_paths_with_descendants(&referenced_paths);
        for path in &paths_with_referenced_descendants {
            self.path(path);
        }

        let value_path_facts = self
            .paths
            .iter()
            .map(|(path, acc)| {
                (
                    path.clone(),
                    acc.facts(paths_with_referenced_descendants.contains(path)),
                )
            })
            .collect::<BTreeMap<_, _>>();

        let schema_evidence_by_value_path = self
            .paths
            .into_iter()
            .map(|(value_path, acc)| {
                let facts = value_path_facts
                    .get(&value_path)
                    .copied()
                    .unwrap_or_default();
                let conditional_overlays = acc
                    .conditional_overlays
                    .finish(facts, acc.type_hints.clone());
                let evidence = ContractPathSchemaEvidence {
                    value_path: value_path.clone(),
                    is_referenced_value_path: acc.referenced,
                    facts,
                    guard_predicates: acc.guard_predicates,
                    metadata_field_kinds: acc.evidence.metadata_field_kinds,
                    type_hints: acc.type_hints,
                    provider_schema_uses: acc.evidence.provider_schema_uses,
                    requiredness: acc.requiredness,
                    conditional_overlays,
                };
                (value_path, evidence)
            })
            .collect();
        ContractSchemaSignals::new(schema_evidence_by_value_path)
    }

    fn record_declared_type_hint(&mut self, type_hint: &ContractTypeHint) {
        let acc = self.path(&type_hint.value_path);
        acc.type_hints
            .extend(type_hint.schema_types.iter().cloned());
        if !type_hint.value_path.trim().is_empty() {
            acc.referenced = true;
        }
    }

    fn path(&mut self, path: &str) -> &mut ContractPathAccumulator {
        self.paths.entry(path.to_string()).or_default()
    }

    fn record_provider_schema_use(&mut self, contract_use: &ContractUse) {
        if let Some(provider_use) = from_contract_use(contract_use) {
            self.path(&provider_use.value_path)
                .evidence
                .record_provider_schema_use(provider_use);
        }
    }

    fn record_render_facts(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            self.record_empty_source_render_facts(contract_use);
            return;
        }

        self.path(&contract_use.source_expr);
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
            self.path(&path);
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
            self.path(&path);
            if !contract_use.path.0.is_empty() {
                self.record_render_use(&path, range_guard_paths.contains(&path), None);
            }
        }
    }

    fn record_render_use(&mut self, path: &str, range_guarded: bool, self_guarded: Option<bool>) {
        self.path(path)
            .evidence
            .uses
            .record_render_use(range_guarded, self_guarded);
    }

    fn record_path_identity(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            return;
        }

        let source_acc = self.path(&contract_use.source_expr);
        source_acc.referenced = true;
        if contract_use.kind == ValueKind::Fragment {
            source_acc.used_as_fragment = true;
        }
        if contract_use.kind == ValueKind::PartialScalar && !contract_use.path.0.is_empty() {
            source_acc.partial_scalar = true;
        }
        source_acc.evidence.record_path_identity(contract_use);
        for path in contract_use.guard_value_paths() {
            if path.trim().is_empty() {
                continue;
            }
            self.path(&path).referenced = true;
        }
        for path in contract_use.top_level_range_guard_paths() {
            if path.trim().is_empty() {
                continue;
            }
            self.path(&path).ranged = true;
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
            let predicates = &mut self.path(&path).guard_predicates;
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
            self.path(&contract_use.source_expr)
                .requiredness
                .is_positive_header = true;
        }
    }

    fn record_conditionally_optional_path(&mut self, path: &str) {
        if path.trim().is_empty() {
            return;
        }
        self.path(path).requiredness.is_conditionally_optional = true;
    }

    fn record_default_fallback_path(&mut self, path: &str) {
        if path.trim().is_empty() {
            return;
        }
        self.path(path).requiredness.has_default_fallback = true;
    }

    fn record_nullable_path(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() {
            return;
        }

        let info = &mut self.path(&contract_use.source_expr).evidence.uses;
        if !contract_use.path.0.is_empty()
            || contract_use.has_self_range_guard()
            || contract_use.kind == ValueKind::Fragment
            || contract_use.has_pathless_self_default_guard()
        {
            info.mark_nullable_render_use();
        }
        info.record_nullable_observation(use_is_null_tolerant(contract_use));

        for path in contract_use.top_level_range_guard_paths() {
            if !path.trim().is_empty() {
                self.path(&path).evidence.uses.mark_nullable_render_use();
            }
        }
    }

    fn record_conditional_overlay(&mut self, contract_use: &ContractUse) {
        if contract_use.source_expr.trim().is_empty() || contract_use.path.0.is_empty() {
            return;
        }

        let accumulator = &mut self.path(&contract_use.source_expr).conditional_overlays;

        if contract_use.guards.is_empty() {
            accumulator.has_unconditional_peer_use = true;
            return;
        }

        let Some(guards) = contract_use.lowerable_conditional_guard_set() else {
            accumulator.saw_unsupported = true;
            return;
        };

        let branch = accumulator.branches_by_guards.entry(guards).or_default();
        branch.record_use(contract_use);
    }
}

impl ContractPathAccumulator {
    fn facts(&self, has_referenced_descendants: bool) -> ContractValuePathFacts {
        self.evidence.facts(
            self.used_as_fragment,
            self.ranged,
            self.partial_scalar,
            has_referenced_descendants,
        )
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
    fn record_use(&mut self, contract_use: &ContractUse) {
        self.evidence.uses.record_render_use(
            contract_use.has_self_range_guard(),
            Some(use_is_self_guarded(contract_use)),
        );
        self.evidence.uses.mark_nullable_render_use();
        self.evidence
            .uses
            .record_nullable_observation(use_is_null_tolerant(contract_use));
        self.used_as_fragment |= contract_use.kind == ValueKind::Fragment;
        self.is_partial_scalar_value_path |= contract_use.kind == ValueKind::PartialScalar;
        self.evidence.record_path_identity(contract_use);

        if let Some(provider_schema_use) = from_contract_use(contract_use) {
            self.evidence
                .record_provider_schema_use(provider_schema_use);
        }
    }

    fn schema_evidence(
        self,
        global_facts: ContractValuePathFacts,
        type_hints: BTreeSet<String>,
    ) -> ConditionalOverlayEvidence {
        let facts = self.evidence.facts(
            self.used_as_fragment,
            false,
            self.is_partial_scalar_value_path,
            global_facts.has_referenced_descendants,
        );
        ConditionalOverlayEvidence {
            facts,
            metadata_field_kinds: self.evidence.metadata_field_kinds,
            type_hints,
            provider_schema_uses: self.evidence.provider_schema_uses,
        }
    }
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

fn collect_paths_with_descendants(paths: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for path in paths {
        let mut segments: Vec<&str> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        while segments.len() > 1 {
            segments.pop();
            out.insert(segments.join("."));
        }
    }
    out
}
