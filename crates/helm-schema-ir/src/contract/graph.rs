use std::collections::BTreeSet;

use crate::contract::fact::ContractFact;
use crate::contract::{ContractProjection, ContractTypeHint};
use crate::contract_normalization::normalize_contract_uses;
use crate::contract_signal_builder::derive_schema_signals_from_contract_parts;
use crate::contract_signals::ContractSchemaSignals;
use crate::{ContractUse, Guard, ValueKind, YamlPath};

/// Opaque guarded contract graph for one template interpretation.
///
/// Accumulation, path rebasing, and normalization live behind this
/// contract-layer artifact instead of a raw vector owned by callers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractIr {
    facts: Vec<ContractFact>,
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
            facts: uses.into_iter().map(ContractFact::Use).collect(),
        }
    }

    pub(crate) fn push(&mut self, contract_use: ContractUse) {
        self.facts.push(ContractFact::Use(contract_use));
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
        self.facts.append(&mut other.facts);
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

        for fact in &mut self.facts {
            if let ContractFact::Use(contract_use) = fact {
                for guard in guards {
                    if !contract_use.guards.contains(guard) {
                        contract_use.guards.push(guard.clone());
                    }
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
        for fact in &mut self.facts {
            fact.map_value_paths(&mut map);
        }
    }

    /// Add declared input-type hints for values paths without projecting them
    /// as inspection rows.
    pub fn add_type_hint(&mut self, path: impl Into<String>, schema_type: impl Into<String>) {
        let path = path.into();
        let schema_type = schema_type.into();
        if path.trim().is_empty() || schema_type.trim().is_empty() {
            return;
        }
        if let Some(existing) = self.facts.iter_mut().find_map(|fact| match fact {
            ContractFact::TypeHint(type_hint) if type_hint.value_path == path => Some(type_hint),
            _ => None,
        }) {
            existing.schema_types.insert(schema_type);
            return;
        }

        if let Some(type_hint) = ContractTypeHint::new(path, [schema_type]) {
            self.facts.push(ContractFact::TypeHint(type_hint));
        }
    }

    /// Extend the graph with already-grouped path type hints.
    pub fn extend_type_hints(
        &mut self,
        type_hints: impl IntoIterator<Item = (String, BTreeSet<String>)>,
    ) {
        for (path, schema_types) in type_hints {
            let Some(type_hint) = ContractTypeHint::new(path.clone(), schema_types) else {
                continue;
            };
            if let Some(existing) = self.facts.iter_mut().find_map(|fact| match fact {
                ContractFact::TypeHint(existing) if existing.value_path == path => Some(existing),
                _ => None,
            }) {
                existing.schema_types.extend(type_hint.schema_types);
            } else {
                self.facts.push(ContractFact::TypeHint(type_hint));
            }
        }
    }

    /// Finalize claims and project them to the inspection DTO artifact.
    #[must_use]
    pub fn project(self) -> ContractProjection {
        let (mut uses, _) = self.into_contract_parts();
        normalize_contract_uses(&mut uses);
        ContractProjection::from_normalized_uses(uses)
    }

    /// Finalize claims and derive the typed schema-generation signals.
    ///
    /// Production schema generation should use this method when it does not
    /// need fixture/inspection rows. [`ContractProjection`] remains the
    /// explicit DTO projection boundary.
    #[must_use]
    pub fn into_schema_signals(self) -> ContractSchemaSignals {
        let (mut uses, type_hints) = self.into_contract_parts();
        normalize_contract_uses(&mut uses);
        derive_schema_signals_from_contract_parts(&uses, &type_hints)
    }

    fn into_contract_parts(self) -> (Vec<ContractUse>, Vec<ContractTypeHint>) {
        let mut uses = Vec::new();
        let mut type_hints = Vec::new();

        for fact in self.facts {
            match fact {
                ContractFact::Use(contract_use) => uses.push(contract_use),
                ContractFact::TypeHint(type_hint) => type_hints.push(type_hint),
            }
        }

        (uses, type_hints)
    }
}
