use std::collections::{BTreeMap, BTreeSet};

use crate::contract::{ContractDocument, FinalizedContract};
use crate::contract_normalization::{
    canonicalize_contract_uses, drop_default_guard_subsumed_duplicates, normalize_contract_uses,
};
use crate::{ContractUse, Guard, ValueKind, YamlPath};
use helm_schema_core::ContractSchemaSignals;

/// Opaque guarded contract graph for one template interpretation.
///
/// Accumulation, path rebasing, and normalization live behind this
/// contract-layer artifact instead of a raw vector owned by callers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractIr {
    uses: Vec<ContractUse>,
    dependency_uses: Vec<ContractUse>,
    type_hints: BTreeMap<String, BTreeSet<String>>,
    dependency_values_root_fragments: BTreeSet<String>,
}

impl ContractIr {
    /// Build a contract graph from already-structured contract claims.
    ///
    /// This is the contract-layer constructor for tests and expert callers
    /// that already have semantic claims. Schema signals are still derived
    /// through [`ContractIr::into_schema_signals`], so semantic finalization
    /// stays on the contract graph rather than a serialized document.
    #[must_use]
    pub fn from_contract_uses(uses: Vec<ContractUse>) -> Self {
        Self {
            uses,
            ..Self::default()
        }
    }

    pub(crate) fn push(&mut self, contract_use: ContractUse) {
        self.uses.push(contract_use);
    }

    pub(crate) fn push_dependency_use(&mut self, contract_use: ContractUse) {
        self.dependency_uses.push(contract_use);
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

    pub fn push_pathless_dependency_fragment(&mut self, source_expr: impl Into<String>) {
        self.dependency_values_root_fragments
            .insert(source_expr.into());
    }

    /// Move all claims from another contract graph into this graph.
    pub fn append(&mut self, mut other: Self) {
        self.uses.append(&mut other.uses);
        self.dependency_uses.append(&mut other.dependency_uses);
        self.dependency_values_root_fragments
            .append(&mut other.dependency_values_root_fragments);
        for (path, schema_types) in other.type_hints {
            self.type_hints
                .entry(path)
                .or_default()
                .extend(schema_types);
        }
    }

    /// Append guards to every claim in the graph without rewriting any paths.
    ///
    /// This is used for chart-structural activation predicates that apply to
    /// an already-scoped batch of claims, such as dependency `condition:` /
    /// `tags:` liveness from `Chart.yaml`.
    pub fn append_guards_to_all_uses(&mut self, guards: &[Guard]) {
        for contract_use in self.uses.iter_mut().chain(&mut self.dependency_uses) {
            crate::contract_sink::merge_guards(&mut contract_use.guards, guards);
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
        for contract_use in self.uses.iter_mut().chain(&mut self.dependency_uses) {
            contract_use.map_value_paths(&mut map);
        }
        self.dependency_values_root_fragments =
            std::mem::take(&mut self.dependency_values_root_fragments)
                .into_iter()
                .map(|path| map(&path))
                .collect();
        let mut type_hints: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (path, schema_types) in std::mem::take(&mut self.type_hints) {
            type_hints
                .entry(map(&path))
                .or_default()
                .extend(schema_types);
        }
        self.type_hints = type_hints;
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
    pub(crate) fn extend_type_hints(
        &mut self,
        type_hints: impl IntoIterator<Item = (String, BTreeSet<String>)>,
    ) {
        for (path, schema_types) in type_hints {
            if path.trim().is_empty() {
                continue;
            }
            let schema_types = schema_types
                .into_iter()
                .filter(|schema_type| !schema_type.trim().is_empty())
                .collect::<BTreeSet<_>>();
            if schema_types.is_empty() {
                continue;
            }
            self.type_hints
                .entry(path)
                .or_default()
                .extend(schema_types);
        }
    }

    /// Finalize claims and export the stable versioned inspection document.
    #[must_use]
    pub fn document(self) -> ContractDocument {
        self.finalize().document()
    }

    /// Finalize claims and derive the typed schema-generation signals.
    ///
    /// Production schema generation should use this method when it does not
    /// need fixture/inspection rows or the stable export document.
    #[must_use]
    pub fn into_schema_signals(self) -> ContractSchemaSignals {
        self.finalize().into_schema_signals()
    }

    /// Finalize the contract once and derive downstream artifacts from that
    /// one normalized contract representation.
    #[must_use]
    pub fn finalize(self) -> FinalizedContract {
        let Self {
            mut uses,
            mut dependency_uses,
            type_hints,
            dependency_values_root_fragments,
        } = self;
        for source_expr in &dependency_values_root_fragments {
            dependency_uses.push(ContractUse::new(
                source_expr.clone(),
                YamlPath(Vec::new()),
                ValueKind::Fragment,
                Vec::new(),
                None,
            ));
        }
        normalize_contract_uses(&mut uses);
        canonicalize_contract_uses(&mut dependency_uses);
        uses.append(&mut dependency_uses);
        drop_default_guard_subsumed_duplicates(&mut uses);
        canonicalize_contract_uses(&mut uses);
        FinalizedContract::new(uses, type_hints, dependency_values_root_fragments)
    }
}
