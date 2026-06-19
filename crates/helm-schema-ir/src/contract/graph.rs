use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::contract::ContractProjection;
use crate::contract_normalization::normalize_contract_uses;
use crate::contract_signal_builder::derive_schema_signals_from_uses;
use crate::contract_signals::ContractSchemaSignals;
use crate::{ContractUse, Guard, ValueKind, YamlPath};

/// Opaque guarded contract graph for one template interpretation.
///
/// Accumulation, path rebasing, and normalization live behind this
/// contract-layer artifact instead of a raw vector owned by callers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractIr {
    uses: Vec<ContractUse>,
    type_hints: BTreeMap<String, BTreeSet<String>>,
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
        Self {
            uses,
            type_hints: BTreeMap::new(),
        }
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
        self.extend_type_hints(std::mem::take(&mut other.type_hints));
    }

    /// Append guards to every claim in the graph without rewriting any paths.
    ///
    /// This is used for chart-structural activation predicates that apply to
    /// an already-scoped batch of claims, such as dependency `condition:` /
    /// `tags:` liveness from `Chart.yaml`.
    pub fn append_guards_to_all_uses(&mut self, guards: &[Guard]) {
        if guards.is_empty() {
            return;
        }

        for contract_use in &mut self.uses {
            for guard in guards {
                if !contract_use.guards.contains(guard) {
                    contract_use.guards.push(guard.clone());
                }
            }
        }
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
        let mut mapped = BTreeMap::new();
        for (path, schema_types) in std::mem::take(&mut self.type_hints) {
            mapped
                .entry(map(&path))
                .or_insert_with(BTreeSet::new)
                .extend(schema_types);
        }
        self.type_hints = mapped;
    }

    /// Add declared input-type hints for values paths without projecting them
    /// as inspection rows.
    pub fn add_type_hint(&mut self, path: impl Into<String>, schema_type: impl Into<String>) {
        let path = path.into();
        let schema_type = schema_type.into();
        if path.trim().is_empty() || schema_type.trim().is_empty() {
            return;
        }
        self.type_hints.entry(path).or_default().insert(schema_type);
    }

    /// Extend the graph with already-grouped path type hints.
    pub fn extend_type_hints(
        &mut self,
        type_hints: impl IntoIterator<Item = (String, BTreeSet<String>)>,
    ) {
        for (path, schema_types) in type_hints {
            if path.trim().is_empty() {
                continue;
            }
            self.type_hints.entry(path).or_default().extend(
                schema_types
                    .into_iter()
                    .filter(|schema_type| !schema_type.trim().is_empty()),
            );
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
        derive_schema_signals_from_uses(&self.uses, &self.type_hints)
    }

    fn normalize(&mut self) {
        normalize_contract_uses(&mut self.uses);
    }
}
