use std::collections::{BTreeMap, BTreeSet};

use crate::contract::FinalizedContract;
use crate::contract_normalization::{
    canonicalize_contract_uses, drop_default_guard_subsumed_duplicates,
    drop_self_truthy_subsumed_duplicates, normalize_contract_uses,
};
use crate::{ContractUse, Guard, ValueKind, YamlPath};

/// Opaque guarded contract graph for one template interpretation.
///
/// Accumulation, path rebasing, and normalization live behind this
/// contract-layer artifact instead of a raw vector owned by callers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractIr {
    uses: Vec<ContractUse>,
    dependency_uses: Vec<ContractUse>,
    type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Input-type hints observed only under branch predicates: they hold
    /// where those branches render, so they type conditional overlays but
    /// never the unconditional base.
    guarded_type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Paths consumed through total stringifications (`quote`, `toString`,
    /// `join`, `printf`) anywhere in the interpretation: the chart tolerates
    /// any input type at them even when no placed row exists.
    shape_erased_value_paths: BTreeSet<String>,
    /// Paths carrying a real runtime string contract (`trunc`, `b64enc`,
    /// `fromYaml`, a dynamic `printf` format) anywhere.
    string_contract_value_paths: BTreeSet<String>,
    /// The chart's per-path range facts (direct iteration, JSON-decoded
    /// values, key/value destructuring).
    range_modes: crate::range_modes::RangeModes,
    /// Chart value subtrees supplying defaults to effective values subtrees.
    values_default_sources: BTreeSet<crate::ValuesDefaultSource>,
    /// `fail` captures: no valid values document may satisfy one of these
    /// conjunctions.
    fail_conditions: Vec<crate::eval_effect::FailCapture>,
    dependency_values_root_fragments: BTreeSet<String>,
}

impl ContractIr {
    /// Build a contract graph from already-structured contract claims.
    ///
    /// This is the contract-layer constructor for tests and expert callers
    /// that already have semantic claims. Schema signals are still derived
    /// through [`ContractIr::finalize`], so semantic finalization stays on
    /// the contract graph rather than a serialized document.
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
        for (path, schema_types) in other.guarded_type_hints {
            self.guarded_type_hints
                .entry(path)
                .or_default()
                .extend(schema_types);
        }
        self.shape_erased_value_paths
            .append(&mut other.shape_erased_value_paths);
        self.string_contract_value_paths
            .append(&mut other.string_contract_value_paths);
        self.range_modes.merge(&other.range_modes);
        self.values_default_sources
            .append(&mut other.values_default_sources);
        for condition in std::mem::take(&mut other.fail_conditions) {
            if !self.fail_conditions.contains(&condition) {
                self.fail_conditions.push(condition);
            }
        }
    }

    /// Append guards to every claim in the graph without rewriting any paths.
    ///
    /// This is used for chart-structural activation predicates that apply to
    /// an already-scoped batch of claims, such as dependency `condition:` /
    /// `tags:` liveness from `Chart.yaml`.
    pub fn append_guards_to_all_uses(&mut self, guards: &[Guard]) {
        for contract_use in self.uses.iter_mut().chain(&mut self.dependency_uses) {
            contract_use.condition = contract_use
                .condition
                .conjoined_with_guards(guards.iter().cloned());
        }
        // Fail captures are claims too: a `fail` inside a dependency gated
        // off by `condition:` / `tags:` cannot abort rendering, so its
        // conjunction must carry the activation predicate like every row.
        for capture in &mut self.fail_conditions {
            capture.conjunction.splice(
                0..0,
                guards
                    .iter()
                    .cloned()
                    .map(helm_schema_core::Predicate::from),
            );
        }
        // A conditionally active chart cannot contribute unconditional
        // effective defaults. Conditional default overlays are not yet part
        // of the schema-signal vocabulary, so abstain instead of leaking them.
        if !guards.is_empty() {
            self.values_default_sources.clear();
        }
    }

    /// Mark rendered claims as textual output rather than structured YAML
    /// placements. Runtime operand contracts and terminal effects remain
    /// unchanged.
    pub fn mark_rendered_output_textual(&mut self) {
        for contract_use in self.uses.iter_mut().chain(&mut self.dependency_uses) {
            contract_use.kind = ValueKind::Serialized;
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
        let mut guarded_type_hints: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (path, schema_types) in std::mem::take(&mut self.guarded_type_hints) {
            guarded_type_hints
                .entry(map(&path))
                .or_default()
                .extend(schema_types);
        }
        self.guarded_type_hints = guarded_type_hints;
        self.shape_erased_value_paths = std::mem::take(&mut self.shape_erased_value_paths)
            .into_iter()
            .map(|path| map(&path))
            .collect();
        self.string_contract_value_paths = std::mem::take(&mut self.string_contract_value_paths)
            .into_iter()
            .map(|path| map(&path))
            .collect();
        self.range_modes.map_value_paths(&mut map);
        self.values_default_sources = std::mem::take(&mut self.values_default_sources)
            .into_iter()
            .map(|source| crate::ValuesDefaultSource {
                target_path: map(&source.target_path),
                source_path: map(&source.source_path),
            })
            .collect();
        self.fail_conditions = std::mem::take(&mut self.fail_conditions)
            .into_iter()
            .map(|mut capture| {
                capture.conjunction = capture
                    .conjunction
                    .into_iter()
                    .map(|predicate| predicate.map_value_paths(&mut map))
                    .collect();
                capture.ranged.map_value_paths(&mut map);
                capture
            })
            .collect();
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

    pub(crate) fn extend_guarded_type_hints(
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
            self.guarded_type_hints
                .entry(path)
                .or_default()
                .extend(schema_types);
        }
    }

    pub(crate) fn extend_shape_erased_value_paths(
        &mut self,
        paths: impl IntoIterator<Item = String>,
    ) {
        self.shape_erased_value_paths
            .extend(paths.into_iter().filter(|path| !path.trim().is_empty()));
    }

    pub(crate) fn extend_string_contract_value_paths(
        &mut self,
        paths: impl IntoIterator<Item = String>,
    ) {
        self.string_contract_value_paths
            .extend(paths.into_iter().filter(|path| !path.trim().is_empty()));
    }

    pub(crate) fn merge_range_modes(&mut self, range_modes: &crate::range_modes::RangeModes) {
        self.range_modes.merge(range_modes);
    }

    pub(crate) fn extend_values_default_sources(
        &mut self,
        sources: impl IntoIterator<Item = crate::ValuesDefaultSource>,
    ) {
        self.values_default_sources.extend(sources);
    }

    pub(crate) fn extend_fail_conditions(
        &mut self,
        conditions: impl IntoIterator<Item = crate::eval_effect::FailCapture>,
    ) {
        for conjunction in conditions {
            if !self.fail_conditions.contains(&conjunction) {
                self.fail_conditions.push(conjunction);
            }
        }
    }

    /// Finalize the contract once and derive downstream artifacts from that
    /// one normalized contract representation.
    #[must_use]
    #[tracing::instrument(skip_all)]
    pub fn finalize(self) -> FinalizedContract {
        let Self {
            mut uses,
            mut dependency_uses,
            type_hints,
            guarded_type_hints,
            shape_erased_value_paths,
            string_contract_value_paths,
            range_modes,
            values_default_sources,
            mut fail_conditions,
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
        drop_self_truthy_subsumed_duplicates(&mut dependency_uses);
        canonicalize_contract_uses(&mut dependency_uses);
        uses.append(&mut dependency_uses);
        drop_default_guard_subsumed_duplicates(&mut uses);
        drop_self_truthy_subsumed_duplicates(&mut uses);
        canonicalize_contract_uses(&mut uses);
        fail_conditions.sort();
        fail_conditions.dedup();
        FinalizedContract::new(
            uses,
            type_hints,
            guarded_type_hints,
            shape_erased_value_paths,
            string_contract_value_paths,
            range_modes,
            values_default_sources,
            fail_conditions,
            dependency_values_root_fragments,
        )
    }
}
