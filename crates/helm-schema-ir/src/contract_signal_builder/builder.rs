use std::collections::{BTreeMap, BTreeSet};

use crate::ProviderSchemaUse;
use crate::contract::{
    ContractPathObservation, ContractSourceObservation, ContractTypeHint, ContractUse,
    ContractUseObservation,
};
use crate::contract_signals::{
    ConditionalGuard, ConditionalOverlayEvidence, ConditionalPathOverlay,
    ContractPathSchemaEvidence, ContractRequirednessEvidence, ContractSchemaSignals,
    ContractValuePathFacts, MetadataFieldKind,
};

pub(crate) fn derive_schema_signals_from_contract_parts(
    uses: &[ContractUse],
    type_hints: &[ContractTypeHint],
    dependency_values_root_fragments: &BTreeSet<String>,
) -> ContractSchemaSignals {
    let mut builder = ContractSchemaSignalBuilder::default();
    for contract_use in uses {
        builder.record(contract_use);
    }
    for value_path in dependency_values_root_fragments {
        builder.record_dependency_values_root_fragment(value_path);
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
    guard_predicates: Vec<ConditionalGuard>,
    facts: PathSchemaFactsAccumulator,
    requiredness: ContractRequirednessEvidence,
    type_hints: BTreeSet<String>,
    conditional_overlay_branches: BTreeMap<Vec<ConditionalGuard>, PathSchemaFactsAccumulator>,
    has_unconditional_overlay_peer: bool,
    saw_unsupported_overlay: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathSchemaFactsAccumulator {
    metadata_field_kinds: BTreeSet<MetadataFieldKind>,
    provider_schema_uses: Vec<ProviderSchemaUse>,
    has_render_use: bool,
    has_unconditional_render_use: bool,
    has_self_guarded_render_use: bool,
    all_render_uses_self_guarded: bool,
    has_self_range_guard_render_use: bool,
    has_nullable_render_use: bool,
    all_uses_nullable: bool,
    used_as_fragment: bool,
    used_as_pathless_fragment: bool,
    accepted_values_root_fragment: bool,
    accepted_dependency_values_root_fragment: bool,
    is_partial_scalar_value_path: bool,
}

impl Default for PathSchemaFactsAccumulator {
    fn default() -> Self {
        Self {
            metadata_field_kinds: BTreeSet::new(),
            provider_schema_uses: Vec::new(),
            has_render_use: false,
            has_unconditional_render_use: false,
            has_self_guarded_render_use: false,
            all_render_uses_self_guarded: true,
            has_self_range_guard_render_use: false,
            has_nullable_render_use: false,
            all_uses_nullable: true,
            used_as_fragment: false,
            used_as_pathless_fragment: false,
            accepted_values_root_fragment: false,
            accepted_dependency_values_root_fragment: false,
            is_partial_scalar_value_path: false,
        }
    }
}

impl PathSchemaFactsAccumulator {
    fn record_render_use(&mut self, range_guarded: bool, self_guarded: Option<bool>) {
        self.has_render_use = true;
        self.has_self_range_guard_render_use |= range_guarded;
        if let Some(self_guarded) = self_guarded {
            self.has_self_guarded_render_use |= self_guarded;
            self.all_render_uses_self_guarded &= self_guarded;
        }
    }

    fn mark_unconditional_render_use(&mut self) {
        self.has_unconditional_render_use = true;
    }

    fn record_nullable_observation(&mut self, nullable: bool) {
        self.all_uses_nullable &= nullable;
    }

    fn mark_nullable_render_use(&mut self) {
        self.has_nullable_render_use = true;
    }

    fn record_source_facts(&mut self, source: &ContractSourceObservation) {
        if let Some(field_kind) = source.metadata_field_kind {
            self.metadata_field_kinds.insert(field_kind);
        }
        self.used_as_fragment |= source.used_as_fragment;
        self.used_as_pathless_fragment |= source.used_as_pathless_fragment;
        self.is_partial_scalar_value_path |= source.is_partial_scalar_value_path;
    }

    fn mark_dependency_values_root_fragment(&mut self) {
        self.accepted_values_root_fragment = true;
        self.accepted_dependency_values_root_fragment = true;
    }

    fn record_provider_schema_use(&mut self, provider_schema_use: ProviderSchemaUse) {
        if !self.provider_schema_uses.contains(&provider_schema_use) {
            self.provider_schema_uses.push(provider_schema_use);
        }
    }

    fn facts(
        &self,
        is_ranged_source: bool,
        has_referenced_descendants: bool,
    ) -> ContractValuePathFacts {
        ContractValuePathFacts {
            has_referenced_descendants,
            used_as_fragment: self.used_as_fragment,
            used_as_pathless_fragment: self.used_as_pathless_fragment,
            accepted_values_root_fragment: self.accepted_values_root_fragment,
            accepted_dependency_values_root_fragment: self.accepted_dependency_values_root_fragment,
            is_ranged_source,
            is_partial_scalar_value_path: self.is_partial_scalar_value_path,
            has_render_use: self.has_render_use,
            has_unconditional_render_use: self.has_unconditional_render_use,
            has_self_guarded_render_use: self.has_self_guarded_render_use,
            all_render_uses_self_guarded: self.all_render_uses_self_guarded,
            has_self_range_guard_render_use: self.has_self_range_guard_render_use,
            is_nullable: self.has_nullable_render_use && self.all_uses_nullable,
        }
    }

    fn conditional_overlay_evidence(
        self,
        global_facts: ContractValuePathFacts,
        type_hints: BTreeSet<String>,
    ) -> ConditionalOverlayEvidence {
        let facts = self.facts(false, global_facts.has_referenced_descendants);
        ConditionalOverlayEvidence {
            facts,
            metadata_field_kinds: self.metadata_field_kinds,
            type_hints,
            provider_schema_uses: self.provider_schema_uses,
        }
    }
}

impl ContractSchemaSignalBuilder {
    fn record(&mut self, contract_use: &ContractUse) {
        let observation = ContractUseObservation::new(contract_use);

        for (path, path_observation) in &observation.path_observations {
            self.path(path).observe_path_observation(path_observation);
        }
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
                let evidence = acc.into_schema_evidence(value_path.clone(), facts);
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

    fn record_dependency_values_root_fragment(&mut self, value_path: &str) {
        if value_path.trim().is_empty() {
            return;
        }
        let acc = self.path(value_path);
        acc.referenced = true;
        acc.facts.mark_dependency_values_root_fragment();
    }

    fn path(&mut self, path: &str) -> &mut ContractPathAccumulator {
        self.paths.entry(path.to_string()).or_default()
    }
}

impl ContractPathAccumulator {
    fn observe_path_observation(&mut self, observation: &ContractPathObservation) {
        if let Some(source) = observation.source_use.as_ref() {
            self.observe_source_use(source);
        }
        self.referenced |= observation.referenced;
        self.ranged |= observation.ranged;
        self.requiredness.is_positive_header |= observation.requiredness.is_positive_header;
        self.requiredness.is_conditionally_optional |=
            observation.requiredness.is_conditionally_optional;
        self.requiredness.has_default_fallback |= observation.requiredness.has_default_fallback;
        for predicate in &observation.guard_predicates {
            if !self.guard_predicates.contains(predicate) {
                self.guard_predicates.push(predicate.clone());
            }
        }
        if let Some(render_use) = observation.guard_render_use {
            self.facts
                .record_render_use(render_use.range_guarded, render_use.self_guarded);
        }
        if observation.nullable_render_use {
            self.facts.mark_nullable_render_use();
        }
    }

    fn observe_source_use(&mut self, source: &ContractSourceObservation) {
        if let Some(provider_use) = source.provider_schema_use.clone() {
            self.facts.record_provider_schema_use(provider_use);
        }
        self.referenced = true;
        self.facts.record_source_facts(source);
        if let Some(render_use) = source.render_use {
            self.facts
                .record_render_use(render_use.range_guarded, render_use.self_guarded);
            if source.guards_empty {
                self.facts.mark_unconditional_render_use();
                self.has_unconditional_overlay_peer = true;
            } else if let Some(guards) = source.lowerable_conditional_guards.clone() {
                let branch = self.conditional_overlay_branches.entry(guards).or_default();
                branch.record_render_use(render_use.range_guarded, render_use.self_guarded);
                branch.mark_nullable_render_use();
                branch.record_nullable_observation(source.null_tolerant);
                branch.record_source_facts(source);

                if let Some(provider_schema_use) = source.provider_schema_use.clone() {
                    branch.record_provider_schema_use(provider_schema_use);
                }
            } else {
                self.saw_unsupported_overlay = true;
            }
        }
        self.facts.record_nullable_observation(source.null_tolerant);
    }

    fn facts(&self, has_referenced_descendants: bool) -> ContractValuePathFacts {
        self.facts.facts(self.ranged, has_referenced_descendants)
    }

    fn into_schema_evidence(
        self,
        value_path: String,
        facts: ContractValuePathFacts,
    ) -> ContractPathSchemaEvidence {
        let ContractPathAccumulator {
            referenced,
            guard_predicates,
            facts: path_facts,
            requiredness,
            type_hints,
            conditional_overlay_branches,
            has_unconditional_overlay_peer,
            saw_unsupported_overlay,
            ..
        } = self;
        let conditional_overlays = conditional_overlays(
            conditional_overlay_branches,
            has_unconditional_overlay_peer,
            saw_unsupported_overlay,
            &type_hints,
            facts,
        );
        ContractPathSchemaEvidence {
            value_path,
            is_referenced_value_path: referenced,
            facts,
            guard_predicates,
            metadata_field_kinds: path_facts.metadata_field_kinds,
            type_hints,
            provider_schema_uses: path_facts.provider_schema_uses,
            requiredness,
            conditional_overlays,
        }
    }
}

fn conditional_overlays(
    branches: BTreeMap<Vec<ConditionalGuard>, PathSchemaFactsAccumulator>,
    preserve_base_schema: bool,
    saw_unsupported_overlay: bool,
    type_hints: &BTreeSet<String>,
    global_facts: ContractValuePathFacts,
) -> Vec<ConditionalPathOverlay> {
    if saw_unsupported_overlay {
        return Vec::new();
    }
    branches
        .into_iter()
        .map(|(guards, branch)| ConditionalPathOverlay {
            guards,
            evidence: branch.conditional_overlay_evidence(global_facts, type_hints.clone()),
            preserve_base_schema,
        })
        .collect()
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
