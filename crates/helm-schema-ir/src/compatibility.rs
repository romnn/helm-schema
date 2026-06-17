use serde::{Deserialize, Serialize};

use crate::contract::ContractProjection;
use crate::{ContractProvenance, ContractUse, Guard, ResourceRef, SourceSpan, ValueKind, YamlPath};

/// Serialized inspection row for one observed `.Values.*` path.
///
/// The semantic interpreter produces `ContractIr` / `ContractUse` internally.
/// `ValueUse` is kept as a stable fixture and external-tooling projection
/// format, not as the production contract artifact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum CompatGuard {
    Truthy { path: String },
    Not { path: String },
    Eq { path: String, value: String },
    Or { paths: Vec<String> },
    Range { path: String },
    With { path: String },
    Default { path: String },
    TypeIs { path: String, schema_type: String },
}

impl From<Guard> for CompatGuard {
    fn from(guard: Guard) -> Self {
        match guard {
            Guard::Truthy { path } => Self::Truthy { path },
            Guard::Not { path } => Self::Not { path },
            Guard::Eq { path, value } => Self::Eq { path, value },
            Guard::Or { paths } => Self::Or { paths },
            Guard::Range { path } => Self::Range { path },
            Guard::With { path } => Self::With { path },
            Guard::Default { path } => Self::Default { path },
            Guard::TypeIs { path, schema_type } => Self::TypeIs { path, schema_type },
        }
    }
}

impl From<CompatGuard> for Guard {
    fn from(guard: CompatGuard) -> Self {
        match guard {
            CompatGuard::Truthy { path } => Self::Truthy { path },
            CompatGuard::Not { path } => Self::Not { path },
            CompatGuard::Eq { path, value } => Self::Eq { path, value },
            CompatGuard::Or { paths } => Self::Or { paths },
            CompatGuard::Range { path } => Self::Range { path },
            CompatGuard::With { path } => Self::With { path },
            CompatGuard::Default { path } => Self::Default { path },
            CompatGuard::TypeIs { path, schema_type } => Self::TypeIs { path, schema_type },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ValueUseSerde {
    source_expr: String,
    path: YamlPath,
    kind: ValueKind,
    guards: Vec<CompatGuard>,
    resource: Option<ResourceRef>,
}

impl From<ValueUse> for ValueUseSerde {
    fn from(value_use: ValueUse) -> Self {
        Self {
            source_expr: value_use.source_expr,
            path: value_use.path,
            kind: value_use.kind,
            guards: value_use
                .guards
                .into_iter()
                .map(CompatGuard::from)
                .collect(),
            resource: value_use.resource,
        }
    }
}

impl From<ValueUseSerde> for ValueUse {
    fn from(value_use: ValueUseSerde) -> Self {
        Self {
            source_expr: value_use.source_expr,
            path: value_use.path,
            kind: value_use.kind,
            guards: value_use.guards.into_iter().map(Guard::from).collect(),
            resource: value_use.resource,
        }
    }
}

impl Serialize for ValueUse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ValueUseSerde::from(self.clone()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ValueUse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        ValueUseSerde::deserialize(deserializer).map(ValueUse::from)
    }
}

/// Serialized source span in the versioned contract export.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContractDocumentSpan {
    pub start: usize,
    pub end: usize,
}

impl From<SourceSpan> for ContractDocumentSpan {
    fn from(span: SourceSpan) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

/// Serialized provenance row for one observed contract use site.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContractDocumentProvenance {
    pub template_path: String,
    pub span: ContractDocumentSpan,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub helper_chain: Vec<String>,
}

impl From<ContractProvenance> for ContractDocumentProvenance {
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
/// This is the Ring-2 DTO: it preserves the stable `ValueUse` projection
/// fields while also exporting the normalized source provenance collected
/// during interpretation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContractDocumentUse {
    #[serde(flatten)]
    pub value_use: ValueUse,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<ContractDocumentProvenance>,
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
            value_use: ValueUse {
                source_expr,
                path,
                kind,
                guards,
                resource,
            },
            provenance: provenance
                .into_iter()
                .map(ContractDocumentProvenance::from)
                .collect(),
        }
    }
}

/// Versioned serialized contract document for stable inspection and tooling.
///
/// This is the current Ring-2 export surface. The in-memory
/// `ContractProjection` remains the internal/canonical projection type and is
/// intentionally free to evolve separately from this DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDocument {
    pub version: u32,
    pub uses: Vec<ContractDocumentUse>,
}

impl ContractDocument {
    pub const VERSION: u32 = 2;

    #[must_use]
    pub fn from_projection(projection: ContractProjection) -> Self {
        Self {
            version: Self::VERSION,
            uses: projection
                .into_contract_uses()
                .into_iter()
                .map(ContractDocumentUse::from)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ContractDocument, ContractDocumentUse, ValueUse};
    use crate::{Guard, ResourceRef, ValueKind, YamlPath};

    #[test]
    fn contract_document_serializes_stable_guard_shape_without_guard_serde_derives() {
        let value_use = ValueUse {
            source_expr: "kid.enabled".to_string(),
            path: YamlPath(vec!["data".to_string(), "enabled".to_string()]),
            kind: ValueKind::Scalar,
            guards: vec![Guard::Or {
                paths: vec![
                    "global.kidEnabled".to_string(),
                    "kid.enabled".to_string(),
                    "tags.observability".to_string(),
                ],
            }],
            resource: Some(ResourceRef {
                api_version: "v1".to_string(),
                kind: "ConfigMap".to_string(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            }),
        };

        let actual = serde_json::to_value(ContractDocument {
            version: ContractDocument::VERSION,
            uses: vec![ContractDocumentUse {
                value_use: value_use.clone(),
                provenance: Vec::new(),
            }],
        })
        .expect("serialize contract document");

        assert_eq!(
            actual,
            json!({
                "version": 2,
                "uses": [{
                    "source_expr": "kid.enabled",
                    "path": ["data", "enabled"],
                    "kind": "Scalar",
                    "guards": [{
                        "type": "or",
                        "paths": ["global.kidEnabled", "kid.enabled", "tags.observability"]
                    }],
                    "resource": {
                        "api_version": "v1",
                        "kind": "ConfigMap"
                    }
                }]
            })
        );

        let decoded: ContractDocument =
            serde_json::from_value(actual).expect("deserialize contract document");
        assert_eq!(decoded.uses.len(), 1);
        assert_eq!(decoded.uses[0].value_use, value_use);
        assert!(decoded.uses[0].provenance.is_empty());
    }
}
