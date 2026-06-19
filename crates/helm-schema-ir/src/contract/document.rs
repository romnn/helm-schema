use serde::{Deserialize, Serialize};

use super::{ContractProjection, ContractUse};
use crate::{ContractProvenance, Guard, GuardValue, ResourceRef, SourceSpan, ValueKind, YamlPath};

/// Stable serialized guard row in the versioned contract document.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContractDocumentGuard {
    Truthy {
        path: String,
    },
    Not {
        path: String,
    },
    Eq {
        path: String,
        value: GuardValue,
    },
    NotEq {
        path: String,
        value: GuardValue,
    },
    Absent {
        path: String,
    },
    Or {
        paths: Vec<String>,
    },
    AnyOf {
        alternatives: Vec<Vec<ContractDocumentGuard>>,
    },
    Range {
        path: String,
    },
    With {
        path: String,
    },
    Default {
        path: String,
    },
    TypeIs {
        path: String,
        schema_type: String,
    },
}

impl From<Guard> for ContractDocumentGuard {
    fn from(guard: Guard) -> Self {
        match guard {
            Guard::Truthy { path } => Self::Truthy { path },
            Guard::Not { path } => Self::Not { path },
            Guard::Eq { path, value } => Self::Eq { path, value },
            Guard::NotEq { path, value } => Self::NotEq { path, value },
            Guard::Absent { path } => Self::Absent { path },
            Guard::Or { paths } => Self::Or { paths },
            Guard::AnyOf { alternatives } => Self::AnyOf {
                alternatives: alternatives
                    .into_iter()
                    .map(|alternative| {
                        alternative
                            .into_iter()
                            .map(ContractDocumentGuard::from)
                            .collect()
                    })
                    .collect(),
            },
            Guard::Range { path } => Self::Range { path },
            Guard::With { path } => Self::With { path },
            Guard::Default { path } => Self::Default { path },
            Guard::TypeIs { path, schema_type } => Self::TypeIs { path, schema_type },
        }
    }
}

impl From<ContractDocumentGuard> for Guard {
    fn from(guard: ContractDocumentGuard) -> Self {
        match guard {
            ContractDocumentGuard::Truthy { path } => Self::Truthy { path },
            ContractDocumentGuard::Not { path } => Self::Not { path },
            ContractDocumentGuard::Eq { path, value } => Self::Eq { path, value },
            ContractDocumentGuard::NotEq { path, value } => Self::NotEq { path, value },
            ContractDocumentGuard::Absent { path } => Self::Absent { path },
            ContractDocumentGuard::Or { paths } => Self::Or { paths },
            ContractDocumentGuard::AnyOf { alternatives } => Self::AnyOf {
                alternatives: alternatives
                    .into_iter()
                    .map(|alternative| alternative.into_iter().map(Guard::from).collect())
                    .collect(),
            },
            ContractDocumentGuard::Range { path } => Self::Range { path },
            ContractDocumentGuard::With { path } => Self::With { path },
            ContractDocumentGuard::Default { path } => Self::Default { path },
            ContractDocumentGuard::TypeIs { path, schema_type } => {
                Self::TypeIs { path, schema_type }
            }
        }
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContractDocumentUse {
    pub source_expr: String,
    pub path: YamlPath,
    pub kind: ValueKind,
    pub guards: Vec<ContractDocumentGuard>,
    pub resource: Option<ResourceRef>,
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
            source_expr,
            path,
            kind,
            guards: guards
                .into_iter()
                .map(ContractDocumentGuard::from)
                .collect(),
            resource,
            provenance: provenance
                .into_iter()
                .map(ContractDocumentProvenance::from)
                .collect(),
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

    use super::{ContractDocument, ContractDocumentGuard, ContractDocumentUse};
    use crate::{ResourceRef, ValueKind, YamlPath};

    #[test]
    fn contract_document_serializes_stable_guard_shape() {
        let value_use = ContractDocumentUse {
            source_expr: "kid.enabled".to_string(),
            path: YamlPath(vec!["data".to_string(), "enabled".to_string()]),
            kind: ValueKind::Scalar,
            guards: vec![
                ContractDocumentGuard::Or {
                    paths: vec![
                        "global.kidEnabled".to_string(),
                        "kid.enabled".to_string(),
                        "tags.observability".to_string(),
                    ],
                },
                ContractDocumentGuard::AnyOf {
                    alternatives: vec![
                        vec![ContractDocumentGuard::Truthy {
                            path: "kid.enabled".to_string(),
                        }],
                        vec![ContractDocumentGuard::Eq {
                            path: "kid.mode".to_string(),
                            value: crate::GuardValue::string("prod"),
                        }],
                    ],
                },
            ],
            resource: Some(ResourceRef {
                api_version: "v1".to_string(),
                kind: "ConfigMap".to_string(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            }),
            provenance: Vec::new(),
        };

        let actual = serde_json::to_value(ContractDocument {
            version: ContractDocument::VERSION,
            uses: vec![value_use.clone()],
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
                    }, {
                        "type": "any_of",
                        "alternatives": [[{
                            "type": "truthy",
                            "path": "kid.enabled"
                        }], [{
                            "type": "eq",
                            "path": "kid.mode",
                            "value": "prod"
                        }]]
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
        assert_eq!(decoded.uses, vec![value_use]);
    }
}
