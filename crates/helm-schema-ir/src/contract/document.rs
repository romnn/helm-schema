use serde::{Deserialize, Serialize};

use super::ContractUse;
use crate::contract_normalization::canonicalize_contract_uses;

/// Versioned serialized contract document for stable inspection and tooling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDocument {
    pub version: u32,
    pub uses: Vec<ContractUse>,
}

impl ContractDocument {
    pub const VERSION: u32 = 2;

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
