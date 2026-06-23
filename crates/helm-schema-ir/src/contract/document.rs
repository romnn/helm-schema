use serde::{Deserialize, Serialize};

use super::ContractUse;
use crate::{ContractProvenance, Guard, ResourceRef, ValueKind, YamlPath};

/// Provenance-aware serialized inspection row for one observed `.Values.*` path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContractDocumentUse {
    pub source_expr: String,
    pub path: YamlPath,
    pub kind: ValueKind,
    pub guards: Vec<Guard>,
    pub resource: Option<ResourceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<ContractProvenance>,
}

impl From<ContractUse> for ContractDocumentUse {
    fn from(contract_use: ContractUse) -> Self {
        let ContractUse {
            source_expr,
            path,
            kind,
            guards,
            resource,
            provenance,
        } = contract_use;

        Self {
            source_expr,
            path,
            kind,
            guards,
            resource,
            provenance,
        }
    }
}

/// Versioned serialized contract document for stable inspection and tooling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDocument {
    pub version: u32,
    pub uses: Vec<ContractDocumentUse>,
}

impl ContractDocument {
    pub const VERSION: u32 = 2;

    #[must_use]
    pub fn from_contract_uses(uses: Vec<ContractUse>) -> Self {
        Self {
            version: Self::VERSION,
            uses: uses.into_iter().map(ContractDocumentUse::from).collect(),
        }
    }
}

#[cfg(test)]
#[path = "../tests/contract/document.rs"]
mod tests;
