use crate::contract::ContractProjection;
use crate::contract_normalization::normalize_contract_uses;
use crate::contract_signal_builder::derive_schema_signals_from_uses;
use crate::contract_signals::ContractSchemaSignals;
use crate::{ContractUse, ValueKind, YamlPath};

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
        ContractProjection::from_normalized_uses(self.uses)
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
