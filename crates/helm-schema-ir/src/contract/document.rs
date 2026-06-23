use serde::{Deserialize, Serialize};

use super::ContractUse;

/// Versioned serialized contract document for stable inspection and tooling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDocument {
    pub version: u32,
    pub uses: Vec<ContractUse>,
}

impl ContractDocument {
    pub const VERSION: u32 = 2;

    #[must_use]
    pub fn from_contract_uses(uses: Vec<ContractUse>) -> Self {
        Self {
            version: Self::VERSION,
            uses,
        }
    }
}

#[cfg(test)]
#[path = "../tests/contract/document.rs"]
mod tests;
