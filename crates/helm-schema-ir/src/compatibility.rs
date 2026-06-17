use serde::{Deserialize, Serialize};

use crate::contract::ContractProjection;
use crate::{Guard, ResourceRef, ValueKind, YamlPath};

/// Serialized inspection row for one observed `.Values.*` path.
///
/// The semantic interpreter produces `ContractIr` / `ContractUse` internally.
/// `ValueUse` is kept as a stable fixture and external-tooling projection
/// format, not as the production contract artifact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ValueUse {
    /// The `.Values.*` sub-path, e.g. `"metrics.enabled"`.
    pub source_expr: String,
    /// The YAML path where this value is placed in the rendered manifest.
    pub path: YamlPath,
    /// Whether this produces a scalar or a YAML fragment.
    pub kind: ValueKind,
    /// Guard conditions (from `if`/`with`/`range`) active when this use appears.
    pub guards: Vec<Guard>,
    /// The Kubernetes resource type detected in context, if any.
    pub resource: Option<ResourceRef>,
}

/// Versioned serialized contract document for stable inspection and tooling.
///
/// This is the semver-facing wire shape for exported contract data. The
/// in-memory `ContractProjection` remains the internal/canonical projection
/// type and is intentionally free to evolve separately from this DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDocumentV1 {
    pub version: u32,
    pub uses: Vec<ValueUse>,
}

impl ContractDocumentV1 {
    pub const VERSION: u32 = 1;

    #[must_use]
    pub fn from_projection(projection: ContractProjection) -> Self {
        Self {
            version: Self::VERSION,
            uses: projection.into_value_uses(),
        }
    }
}
