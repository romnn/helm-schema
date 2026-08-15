use std::collections::BTreeSet;

use super::{ContractDocument, ContractUse};
use crate::contract_signal_builder::derive_schema_signals_from_contract_parts;
use crate::observed_facts::ObservedFacts;
use helm_schema_core::ContractSchemaSignals;

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
        values_program_wrapper_exclusions: BTreeSet<String>,
        dependency_values_root_fragments: &BTreeSet<String>,
    ) -> Self {
        let schema_signals = derive_schema_signals_from_contract_parts(
            &normalized_uses,
            observed_facts,
            dependency_values_root_fragments,
        )
        .with_values_default_sources(observed_facts.values_default_sources.clone())
        .with_root_overlay_fail_implications(observed_facts.values_root_overlay_prefixes.clone())
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
