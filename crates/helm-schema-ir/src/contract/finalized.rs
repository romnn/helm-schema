use std::collections::{BTreeMap, BTreeSet};

use super::{ContractDocument, ContractUse};
use crate::contract_signal_builder::derive_schema_signals_from_contract_parts;
use crate::observed_facts::ObservedFacts;
use helm_schema_core::{ConditionalGuard, ContractSchemaSignals, GuardedValuesDefaultSource};

fn activation_guards(guards: &[crate::Guard]) -> Option<Vec<ConditionalGuard>> {
    let mut lowered = guards
        .iter()
        .map(ConditionalGuard::try_from)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    lowered.sort();
    lowered.dedup();
    Some(lowered)
}

fn activation_guard_disjunction(guard_sets: Vec<Vec<ConditionalGuard>>) -> Vec<ConditionalGuard> {
    let mut guard_sets =
        helm_schema_core::GuardDnf::normalize_conditional_guard_disjunction(guard_sets);
    match guard_sets.as_mut_slice() {
        [] => Vec::new(),
        [guards] => std::mem::take(guards),
        _ => vec![ConditionalGuard::AnyOf(
            guard_sets
                .into_iter()
                .map(|mut guards| match guards.len() {
                    1 => guards.remove(0),
                    _ => ConditionalGuard::AllOf(guards),
                })
                .collect(),
        )],
    }
}

/// Finalized contract artifact derived from one canonical normalized contract.
///
/// Stable inspection rows and schema-lowering signals come from the same
/// normalized contract uses, so downstream callers do not need to re-finalize
/// a [`super::ContractIr`] separately or hop through another wrapper type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedContract {
    uses: Vec<ContractUse>,
    schema_signals: ContractSchemaSignals,
}

impl FinalizedContract {
    pub(in crate::contract) fn new(
        normalized_uses: Vec<ContractUse>,
        observed_facts: &ObservedFacts,
        values_program_wrappers: BTreeSet<helm_schema_core::ValuesProgramWrapper>,
        values_program_wrapper_exclusions: BTreeSet<helm_schema_core::ValuesPath>,
        dependency_values_root_fragments: &BTreeSet<String>,
    ) -> Self {
        let mut default_guard_sets = BTreeMap::<_, Vec<Vec<ConditionalGuard>>>::new();
        for fact in &observed_facts.activated_values_default_sources {
            if let Some(guards) = activation_guards(&fact.guards) {
                default_guard_sets
                    .entry(fact.source.clone())
                    .or_default()
                    .push(guards);
            }
        }
        let guarded_values_default_sources = default_guard_sets
            .into_iter()
            .map(|(source, guard_sets)| GuardedValuesDefaultSource {
                outer_guards: activation_guard_disjunction(guard_sets),
                source,
            })
            .collect::<Vec<_>>();
        let mut overlay_guard_sets = BTreeMap::<_, Vec<Vec<ConditionalGuard>>>::new();
        for fact in &observed_facts.activated_values_root_overlays {
            if let Some(guards) = activation_guards(&fact.guards) {
                overlay_guard_sets
                    .entry((fact.target_path.clone(), fact.source_path.clone()))
                    .or_default()
                    .push(guards);
            }
        }
        let guarded_root_overlays = overlay_guard_sets
            .into_iter()
            .map(|((target_path, source_path), guard_sets)| {
                (
                    activation_guard_disjunction(guard_sets),
                    target_path,
                    source_path,
                )
            })
            .chain(observed_facts.values_root_overlays.iter().map(|fact| {
                (
                    Vec::new(),
                    fact.target_path.clone(),
                    fact.source_path.clone(),
                )
            }))
            .collect::<Vec<_>>();
        let schema_signals = derive_schema_signals_from_contract_parts(
            &normalized_uses,
            observed_facts,
            dependency_values_root_fragments,
        )
        .with_values_default_sources(observed_facts.values_default_sources.clone())
        .with_guarded_values_default_sources(guarded_values_default_sources)
        .with_scoped_guarded_root_overlay_requirement_implications(guarded_root_overlays)
        .with_values_program_wrappers(values_program_wrappers)
        .with_values_program_wrapper_exclusions(values_program_wrapper_exclusions);

        Self {
            uses: normalized_uses,
            schema_signals,
        }
    }

    /// Returns normalized contract uses in stable inspection order.
    #[must_use]
    pub fn uses(&self) -> &[ContractUse] {
        &self.uses
    }

    /// Returns path-local facts prepared for schema lowering.
    #[must_use]
    pub fn schema_signals(&self) -> &ContractSchemaSignals {
        &self.schema_signals
    }

    /// Builds the versioned inspection document for this contract.
    #[must_use]
    pub fn document(&self) -> ContractDocument {
        ContractDocument::from_contract_uses(self.uses.clone())
    }

    /// Consumes the contract and returns its schema-lowering signals.
    #[must_use]
    pub fn into_schema_signals(self) -> ContractSchemaSignals {
        self.schema_signals
    }
}
