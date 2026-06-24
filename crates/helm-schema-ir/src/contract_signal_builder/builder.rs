use std::collections::{BTreeMap, BTreeSet};

use crate::ProviderSchemaUse;
use crate::contract::{ContractPathObservation, ContractUse, contract_path_observations};
use crate::contract_signals::{
    ConditionalGuard, ConditionalOverlayEvidence, ConditionalPathOverlay,
    ContractPathSchemaEvidence, ContractRequirednessEvidence, ContractSchemaSignals,
    ContractValuePathFacts, MetadataFieldKind,
};

pub(crate) fn derive_schema_signals_from_contract_parts(
    uses: &[ContractUse],
    type_hints: &BTreeMap<String, BTreeSet<String>>,
    dependency_values_root_fragments: &BTreeSet<String>,
) -> ContractSchemaSignals {
    let mut paths = BTreeMap::new();
    for contract_use in uses {
        record_contract_use(&mut paths, contract_use);
    }
    for value_path in dependency_values_root_fragments {
        if let Some((path, observation)) =
            ContractPathObservation::dependency_values_root_fragment(value_path)
        {
            path_accumulator(&mut paths, &path).observe_path_observation(&observation);
        }
    }
    for (value_path, schema_types) in type_hints {
        if let Some((path, observation)) =
            ContractPathObservation::type_hint(value_path, schema_types)
        {
            path_accumulator(&mut paths, &path).observe_path_observation(&observation);
        }
    }
    finish_schema_signals(paths)
}

#[derive(Default)]
struct ContractPathAccumulator {
    referenced: bool,
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
    facts: ContractValuePathFacts,
    all_uses_nullable: bool,
}

impl Default for PathSchemaFactsAccumulator {
    fn default() -> Self {
        Self {
            metadata_field_kinds: BTreeSet::new(),
            provider_schema_uses: Vec::new(),
            facts: ContractValuePathFacts {
                all_render_uses_self_guarded: true,
                ..ContractValuePathFacts::default()
            },
            all_uses_nullable: true,
        }
    }
}

impl PathSchemaFactsAccumulator {
    fn record_nullable_observation(&mut self, nullable: bool) {
        self.all_uses_nullable &= nullable;
    }

    fn record_observation_facts(&mut self, observation: &ContractPathObservation) {
        if let Some(field_kind) = observation.metadata_field_kind {
            self.metadata_field_kinds.insert(field_kind);
        }
        self.facts.used_as_fragment |= observation.facts.used_as_fragment;
        self.facts.used_as_pathless_fragment |= observation.facts.used_as_pathless_fragment;
        self.facts.accepted_values_root_fragment |= observation.facts.accepted_values_root_fragment;
        self.facts.accepted_dependency_values_root_fragment |=
            observation.facts.accepted_dependency_values_root_fragment;
        self.facts.is_ranged_source |= observation.facts.is_ranged_source;
        self.facts.is_partial_scalar_value_path |= observation.facts.is_partial_scalar_value_path;
        self.facts.is_nullable |= observation.facts.is_nullable;
        self.facts.merge_render_use_facts(observation.facts);
    }

    fn record_provider_schema_use(&mut self, provider_schema_use: ProviderSchemaUse) {
        if !self.provider_schema_uses.contains(&provider_schema_use) {
            self.provider_schema_uses.push(provider_schema_use);
        }
    }

    fn facts(&self, has_referenced_descendants: bool) -> ContractValuePathFacts {
        let mut facts = self.facts;
        facts.has_referenced_descendants = has_referenced_descendants;
        facts.is_nullable &= self.all_uses_nullable;
        facts
    }

    fn conditional_overlay_evidence(
        self,
        global_facts: ContractValuePathFacts,
        type_hints: BTreeSet<String>,
    ) -> ConditionalOverlayEvidence {
        let facts = self.facts(global_facts.has_referenced_descendants);
        ConditionalOverlayEvidence {
            facts,
            metadata_field_kinds: self.metadata_field_kinds,
            type_hints,
            provider_schema_uses: self.provider_schema_uses,
        }
    }
}

fn record_contract_use(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    contract_use: &ContractUse,
) {
    for (path, observation) in contract_path_observations(contract_use) {
        path_accumulator(paths, &path).observe_path_observation(&observation);
    }
}

fn finish_schema_signals(
    mut paths: BTreeMap<String, ContractPathAccumulator>,
) -> ContractSchemaSignals {
    let referenced_paths = paths
        .iter()
        .filter_map(|(path, acc)| acc.referenced.then_some(path.clone()))
        .collect();
    let paths_with_referenced_descendants = collect_paths_with_descendants(&referenced_paths);
    for path in &paths_with_referenced_descendants {
        path_accumulator(&mut paths, path);
    }

    let value_path_facts = paths
        .iter()
        .map(|(path, acc)| {
            (
                path.clone(),
                acc.facts(paths_with_referenced_descendants.contains(path)),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let schema_evidence_by_value_path = paths
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

fn path_accumulator<'a>(
    paths: &'a mut BTreeMap<String, ContractPathAccumulator>,
    path: &str,
) -> &'a mut ContractPathAccumulator {
    paths.entry(path.to_string()).or_default()
}

impl ContractPathAccumulator {
    fn observe_path_observation(&mut self, observation: &ContractPathObservation) {
        self.observe_source_use(observation);
        self.referenced |= observation.referenced;
        self.type_hints
            .extend(observation.type_hints.iter().cloned());
        self.facts.record_observation_facts(observation);
        self.requiredness.is_positive_header |= observation.requiredness.is_positive_header;
        self.requiredness.is_conditionally_optional |=
            observation.requiredness.is_conditionally_optional;
        self.requiredness.has_default_fallback |= observation.requiredness.has_default_fallback;
        for predicate in &observation.guard_predicates {
            if !self.guard_predicates.contains(predicate) {
                self.guard_predicates.push(predicate.clone());
            }
        }
    }

    fn observe_source_use(&mut self, observation: &ContractPathObservation) {
        let Some(source_null_tolerant) = observation.source_null_tolerant else {
            return;
        };

        if let Some(provider_use) = observation.provider_schema_use.clone() {
            self.facts.record_provider_schema_use(provider_use);
        }
        self.referenced = true;
        if observation.facts.has_render_use {
            if observation.facts.has_unconditional_render_use {
                self.has_unconditional_overlay_peer = true;
            } else if let Some(guards) = observation.source_lowerable_conditional_guards.clone() {
                let branch = self.conditional_overlay_branches.entry(guards).or_default();
                branch.facts.is_nullable = true;
                branch.record_nullable_observation(source_null_tolerant);
                branch.record_observation_facts(observation);

                if let Some(provider_schema_use) = observation.provider_schema_use.clone() {
                    branch.record_provider_schema_use(provider_schema_use);
                }
            } else {
                self.saw_unsupported_overlay = true;
            }
        }
        self.facts.record_nullable_observation(source_null_tolerant);
    }

    fn facts(&self, has_referenced_descendants: bool) -> ContractValuePathFacts {
        self.facts.facts(has_referenced_descendants)
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
