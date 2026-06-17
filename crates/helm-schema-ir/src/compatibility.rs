use serde::{Deserialize, Serialize};

use crate::contract::ContractProjection;
use crate::{ContractProvenance, ContractUse, Guard, ResourceRef, SourceSpan, ValueKind, YamlPath};

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

/// Serialized source span in the versioned contract export.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceSpanV2 {
    pub start: usize,
    pub end: usize,
}

impl From<SourceSpan> for SourceSpanV2 {
    fn from(span: SourceSpan) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

/// Serialized provenance row for one observed contract use site.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContractProvenanceV2 {
    pub template_path: String,
    pub span: SourceSpanV2,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub helper_chain: Vec<String>,
}

impl From<ContractProvenance> for ContractProvenanceV2 {
    fn from(provenance: ContractProvenance) -> Self {
        Self {
            template_path: provenance.template_path,
            span: provenance.span.into(),
            helper_chain: provenance.helper_chain,
        }
    }
}

/// Provenance-aware serialized inspection row for one observed `.Values.*` path.
///
/// This is the preferred Ring-2 DTO going forward: it preserves the stable
/// `ValueUse` projection fields while also exporting the normalized source
/// provenance collected during interpretation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContractUseV2 {
    #[serde(flatten)]
    pub value_use: ValueUse,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<ContractProvenanceV2>,
}

impl From<ContractUse> for ContractUseV2 {
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
            value_use: ValueUse {
                source_expr,
                path,
                kind,
                guards,
                resource,
            },
            provenance: provenance
                .into_iter()
                .map(ContractProvenanceV2::from)
                .collect(),
        }
    }
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

/// Versioned serialized contract document that preserves source provenance.
///
/// `ContractDocumentV1` remains supported for legacy fixtures and older
/// tooling. `ContractDocumentV2` is the richer inspection format intended for
/// new consumers because it carries normalized provenance without exposing the
/// in-memory contract graph directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDocumentV2 {
    pub version: u32,
    pub uses: Vec<ContractUseV2>,
}

impl ContractDocumentV2 {
    pub const VERSION: u32 = 2;

    #[must_use]
    pub fn from_projection(projection: ContractProjection) -> Self {
        Self {
            version: Self::VERSION,
            uses: projection
                .into_contract_uses()
                .into_iter()
                .map(ContractUseV2::from)
                .collect(),
        }
    }
}
