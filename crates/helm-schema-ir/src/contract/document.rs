use serde::{Deserialize, Serialize};

use super::ContractUse;
use crate::contract_normalization::canonicalize_contract_uses;

/// Versioned serialized contract document for stable inspection and tooling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDocument {
    /// Serialized contract format version.
    pub version: u32,
    /// Canonical contract uses carried by the document.
    pub uses: Vec<ContractUse>,
}

impl ContractDocument {
    /// Current serialized contract format version.
    pub const VERSION: u32 = 3;

    /// Canonicalizes contract uses and wraps them in the current format.
    #[must_use]
    pub fn from_contract_uses(mut uses: Vec<ContractUse>) -> Self {
        canonicalize_contract_uses(&mut uses);
        Self {
            version: Self::VERSION,
            uses,
        }
    }
}

#[cfg(test)]
#[path = "../tests/contract/document.rs"]
mod tests;
