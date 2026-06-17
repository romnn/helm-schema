use crate::contract::{ContractDocument, ContractUse};
use crate::contract_normalization::canonicalize_contract_uses;

/// Canonical inspection projection of a contract graph.
///
/// Fixture and external tooling code use this boundary when they need
/// inspection rows. Production schema generation consumes
/// [`crate::ContractSchemaSignals`] directly from
/// [`crate::ContractIr::into_schema_signals`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractProjection {
    uses: Vec<ContractUse>,
}

impl ContractProjection {
    /// Build a canonical projection from already-projected contract claims.
    ///
    /// This constructor is for tests and external tooling that construct
    /// inspection rows directly. Interpreter output should normally flow
    /// through [`crate::ContractIr::project`], which applies semantic
    /// finalization before creating this projection.
    #[must_use]
    pub fn from_contract_uses(mut uses: Vec<ContractUse>) -> Self {
        canonicalize_contract_uses(&mut uses);
        Self { uses }
    }

    pub(in crate::contract) fn from_normalized_uses(uses: Vec<ContractUse>) -> Self {
        Self { uses }
    }

    /// Borrow the canonicalized contract claims.
    #[must_use]
    pub fn uses(&self) -> &[ContractUse] {
        &self.uses
    }

    pub(crate) fn into_contract_uses(self) -> Vec<ContractUse> {
        self.uses
    }

    /// Consume the projection and export the stable versioned wire format.
    #[must_use]
    pub fn into_document(self) -> ContractDocument {
        ContractDocument::from_projection(self)
    }
}
