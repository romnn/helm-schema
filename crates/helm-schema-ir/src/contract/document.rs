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
        Self::from_normalized_contract_uses(uses)
    }

    pub(in crate::contract) fn from_normalized_contract_uses(mut uses: Vec<ContractUse>) -> Self {
        // Unconditional reads add no activation context; show placed claims before those summaries.
        uses.sort_by(|left, right| {
            left.source_expr.cmp(&right.source_expr).then_with(|| {
                let left_summary = left.path.0.is_empty() && left.condition.is_unconditional();
                let right_summary = right.path.0.is_empty() && right.condition.is_unconditional();
                left_summary.cmp(&right_summary)
            })
        });
        Self {
            version: Self::VERSION,
            uses,
        }
    }
}

#[cfg(test)]
#[path = "../tests/contract/document.rs"]
mod tests;
