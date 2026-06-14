use serde::{Deserialize, Serialize};

use crate::contract_normalization::{canonicalize_contract_uses, normalize_contract_uses};
use crate::contract_signal_builder::derive_schema_signals_from_uses;
use crate::contract_signals::ContractSchemaSignals;
use crate::{Guard, ResourceRef, ValueKind, ValueUse, YamlPath};

/// A contract claim for one observed values path.
///
/// This is still the migration-era claim shape, but it is owned by the
/// contract layer. [`ValueUse`] remains the serialized fixture DTO at the
/// inspection boundary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContractUse {
    pub source_expr: String,
    pub path: YamlPath,
    pub kind: ValueKind,
    pub guards: Vec<Guard>,
    pub resource: Option<ResourceRef>,
}

impl ContractUse {
    pub(crate) fn new(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
    ) -> Self {
        Self {
            source_expr,
            path,
            kind,
            guards,
            resource,
        }
    }

    fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(&str) -> String,
    {
        self.source_expr = map(&self.source_expr);
        self.guards = std::mem::take(&mut self.guards)
            .into_iter()
            .map(|guard| guard.map_value_paths(map))
            .collect();
    }
}

impl From<ContractUse> for ValueUse {
    fn from(contract_use: ContractUse) -> Self {
        Self {
            source_expr: contract_use.source_expr,
            path: contract_use.path,
            kind: contract_use.kind,
            guards: contract_use.guards,
            resource: contract_use.resource,
        }
    }
}

/// Canonical inspection projection of a contract graph.
///
/// Fixture and external tooling code use this boundary when they need
/// inspection rows. Production schema generation consumes
/// [`ContractSchemaSignals`] directly from [`ContractIr::into_schema_signals`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContractProjection {
    uses: Vec<ContractUse>,
}

impl ContractProjection {
    /// Build a canonical projection from already-projected contract claims.
    ///
    /// This constructor is for tests and external tooling that construct
    /// inspection rows directly. Interpreter output should normally flow
    /// through [`ContractIr::project`], which applies semantic finalization
    /// before creating this projection.
    #[must_use]
    pub fn from_contract_uses(mut uses: Vec<ContractUse>) -> Self {
        canonicalize_contract_uses(&mut uses);
        Self { uses }
    }

    /// Borrow the canonicalized contract claims.
    #[must_use]
    pub fn uses(&self) -> &[ContractUse] {
        &self.uses
    }

    /// Consume the projection and return the compatibility DTOs.
    #[must_use]
    pub fn into_value_uses(self) -> Vec<ValueUse> {
        self.uses.into_iter().map(ValueUse::from).collect()
    }
}

/// Opaque guarded contract graph for one template interpretation.
///
/// Accumulation, path rebasing, and normalization live behind this
/// contract-layer artifact instead of a raw vector owned by callers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractIr {
    uses: Vec<ContractUse>,
}

impl ContractIr {
    /// Build a contract graph from already-structured contract claims.
    ///
    /// This is the contract-layer constructor for tests and expert callers
    /// that already have semantic claims. Schema signals are still derived
    /// through [`ContractIr::into_schema_signals`], so semantic finalization
    /// stays on the contract graph rather than the inspection projection.
    #[must_use]
    pub fn from_contract_uses(uses: Vec<ContractUse>) -> Self {
        Self { uses }
    }

    pub(crate) fn push(&mut self, contract_use: ContractUse) {
        self.uses.push(contract_use);
    }

    /// Add a pathless scalar claim for a value path.
    ///
    /// Pathless claims make a value path visible to downstream schema
    /// generation without asserting any rendered Kubernetes field shape.
    pub fn push_pathless_scalar(&mut self, source_expr: impl Into<String>) {
        self.push(ContractUse::new(
            source_expr.into(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ));
    }

    /// Move all claims from another contract graph into this graph.
    pub fn append(&mut self, mut other: Self) {
        self.uses.append(&mut other.uses);
    }

    /// Rewrite all referenced values paths while preserving rendered YAML paths.
    ///
    /// This is used at chart boundaries where a dependency's `.Values.foo`
    /// contract becomes `.Values.subchart.foo`, while rendered manifest paths
    /// such as `metadata.name` stay unchanged.
    pub fn map_value_paths<F>(&mut self, mut map: F)
    where
        F: FnMut(&str) -> String,
    {
        for contract_use in &mut self.uses {
            contract_use.map_value_paths(&mut map);
        }
    }

    /// Finalize claims and project them to the inspection DTO artifact.
    #[must_use]
    pub fn project(mut self) -> ContractProjection {
        self.normalize();
        ContractProjection { uses: self.uses }
    }

    /// Finalize claims and derive the typed schema-generation signals.
    ///
    /// Production schema generation should use this method when it does not
    /// need fixture/inspection rows. [`ContractProjection`] remains the
    /// explicit DTO projection boundary.
    #[must_use]
    pub fn into_schema_signals(mut self) -> ContractSchemaSignals {
        self.normalize();
        derive_schema_signals_from_uses(&self.uses)
    }

    fn normalize(&mut self) {
        normalize_contract_uses(&mut self.uses);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_ir_finalization_keeps_default_guarded_render_site_over_bare_duplicate() {
        let mut contract = ContractIr::default();
        contract.push(ContractUse::new(
            "serviceAccount.name".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ));
        contract.push(ContractUse::new(
            "serviceAccount.name".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Default {
                path: "serviceAccount.name".to_string(),
            }],
            None,
        ));

        let value_uses = contract.project().into_value_uses();

        assert_eq!(value_uses.len(), 1);
        assert_eq!(
            value_uses.first().map(|value_use| &value_use.guards),
            Some(&vec![Guard::Default {
                path: "serviceAccount.name".to_string(),
            }])
        );
    }

    #[test]
    fn contract_ir_finalization_prefers_resource_claim_for_pathless_duplicate() {
        let mut contract = ContractIr::default();
        contract.push(ContractUse::new(
            "nameOverride".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ));
        contract.push(ContractUse::new(
            "nameOverride".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            Vec::new(),
            Some(ResourceRef {
                api_version: "v1".to_string(),
                kind: "Service".to_string(),
                api_version_candidates: Vec::new(),
                api_version_branches: Vec::new(),
            }),
        ));

        let value_uses = contract.project().into_value_uses();

        assert_eq!(value_uses.len(), 1);
        assert_eq!(
            value_uses
                .first()
                .and_then(|value_use| value_use.resource.as_ref())
                .map(|resource| (resource.api_version.as_str(), resource.kind.as_str())),
            Some(("v1", "Service"))
        );
    }

    #[test]
    fn contract_ir_maps_value_paths_without_touching_rendered_yaml_path() {
        let mut contract = ContractIr::default();
        contract.push(ContractUse::new(
            "serviceAccount.name".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            vec![
                Guard::Truthy {
                    path: "serviceAccount.enabled".to_string(),
                },
                Guard::Or {
                    paths: vec!["pod.enabled".to_string(), "global.enabled".to_string()],
                },
            ],
            None,
        ));

        contract.map_value_paths(|path| {
            if path.starts_with("global.") {
                path.to_string()
            } else {
                format!("subchart.{path}")
            }
        });

        let value_uses = contract.project().into_value_uses();
        let value_use = value_uses.first().expect("mapped value use");

        assert_eq!(value_use.source_expr, "subchart.serviceAccount.name");
        assert_eq!(
            value_use.path,
            YamlPath(vec!["metadata".to_string(), "name".to_string()])
        );
        assert_eq!(
            value_use.guards,
            vec![
                Guard::Truthy {
                    path: "subchart.serviceAccount.enabled".to_string(),
                },
                Guard::Or {
                    paths: vec![
                        "subchart.pod.enabled".to_string(),
                        "global.enabled".to_string()
                    ],
                },
            ]
        );
    }

    #[test]
    fn contract_ir_pathless_scalar_seed_projects_without_rendered_path() {
        let mut contract = ContractIr::default();

        contract.push_pathless_scalar("extraConfig");

        let projection = contract.project();
        let value_uses = projection.uses();
        assert_eq!(value_uses.len(), 1);
        assert_eq!(value_uses[0].source_expr, "extraConfig");
        assert_eq!(value_uses[0].path, YamlPath(Vec::new()));
        assert_eq!(value_uses[0].kind, ValueKind::Scalar);
        assert!(value_uses[0].guards.is_empty());
        assert!(value_uses[0].resource.is_none());
    }
}
