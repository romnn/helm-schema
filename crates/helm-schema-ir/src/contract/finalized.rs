use super::{ContractDocument, ContractProjection, ContractTypeHint, ContractUse};
use crate::contract_signal_builder::derive_schema_signals_from_contract_parts;
use crate::contract_signals::ContractSchemaSignals;

/// Finalized contract artifact derived from one canonical normalized contract.
///
/// Both inspection projection and schema-lowering signals come from the same
/// normalized contract uses, so downstream callers do not need to re-finalize
/// a [`super::ContractIr`] separately for each artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedContract {
    projection: ContractProjection,
    schema_signals: ContractSchemaSignals,
}

impl FinalizedContract {
    pub(in crate::contract) fn new(
        normalized_uses: Vec<ContractUse>,
        type_hints: Vec<ContractTypeHint>,
    ) -> Self {
        let schema_signals =
            derive_schema_signals_from_contract_parts(&normalized_uses, &type_hints);
        let projection = ContractProjection::from_normalized_uses(normalized_uses);

        Self {
            projection,
            schema_signals,
        }
    }

    #[must_use]
    pub fn projection(&self) -> &ContractProjection {
        &self.projection
    }

    #[must_use]
    pub fn schema_signals(&self) -> &ContractSchemaSignals {
        &self.schema_signals
    }

    #[must_use]
    pub fn document(&self) -> ContractDocument {
        self.projection.clone().into_document()
    }

    #[must_use]
    pub fn into_projection(self) -> ContractProjection {
        self.projection
    }

    #[must_use]
    pub fn into_schema_signals(self) -> ContractSchemaSignals {
        self.schema_signals
    }
}
