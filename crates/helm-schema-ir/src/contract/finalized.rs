use std::collections::{BTreeMap, BTreeSet};

use super::{ContractDocument, ContractUse};
use crate::contract_signal_builder::derive_schema_signals_from_contract_parts;
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
    #[expect(
        clippy::too_many_arguments,
        reason = "each parameter is one interpreter fact channel; a struct would                   mirror the same fields without adding an invariant"
    )]
    pub(in crate::contract) fn new(
        normalized_uses: Vec<ContractUse>,
        type_hints: BTreeMap<String, BTreeSet<String>>,
        guarded_type_hints: BTreeMap<String, BTreeSet<String>>,
        fallback_type_hints: BTreeMap<String, BTreeSet<String>>,
        guarded_fallback_type_hints: BTreeMap<String, BTreeSet<String>>,
        shape_erased_value_paths: BTreeSet<String>,
        string_contract_value_paths: BTreeSet<String>,
        range_modes: crate::range_modes::RangeModes,
        values_default_sources: BTreeSet<crate::ValuesDefaultSource>,
        values_program_wrappers: BTreeSet<helm_schema_core::ValuesProgramWrapper>,
        fail_conditions: Vec<crate::eval_effect::FailCapture>,
        dependency_values_root_fragments: BTreeSet<String>,
    ) -> Self {
        let schema_signals = derive_schema_signals_from_contract_parts(
            &normalized_uses,
            &type_hints,
            &guarded_type_hints,
            &fallback_type_hints,
            &guarded_fallback_type_hints,
            &shape_erased_value_paths,
            &string_contract_value_paths,
            &range_modes,
            &fail_conditions,
            &dependency_values_root_fragments,
        )
        .with_values_default_sources(values_default_sources)
        .with_values_program_wrappers(values_program_wrappers);

        Self {
            uses: normalized_uses,
            schema_signals,
        }
    }

    #[must_use]
    pub fn uses(&self) -> &[ContractUse] {
        &self.uses
    }

    #[must_use]
    pub fn schema_signals(&self) -> &ContractSchemaSignals {
        &self.schema_signals
    }

    #[must_use]
    pub fn document(&self) -> ContractDocument {
        ContractDocument::from_contract_uses(self.uses.clone())
    }

    #[must_use]
    pub fn into_schema_signals(self) -> ContractSchemaSignals {
        self.schema_signals
    }
}
