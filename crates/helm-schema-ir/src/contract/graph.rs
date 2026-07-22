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
    /// Input-type hints from literal `default`/`coalesce` fallbacks: they
    /// type only the truthy arm of the path, so lowering must keep the
    /// whole Helm-falsy set open beside them.
    fallback_type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Fallback hints observed under branch predicates: fallback-grade
    /// intent that may type conditional overlays, but never a branch whose
    /// renders all totally format.
    guarded_fallback_type_hints: BTreeMap<String, BTreeSet<String>>,
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
    /// Values subtrees merged in place over the values root; root contracts
    /// project back onto the prefixed spellings.
    values_root_overlay_prefixes: BTreeSet<String>,
    values_program_wrappers: BTreeSet<helm_schema_core::ValuesProgramWrapper>,
    /// Values paths whose nodes must NOT gain a wrapper alternative: a
    /// strict string consumer reads them BEFORE the engine's values-root
    /// rewrite, so a wrapper map there aborts rendering (nats'
    /// `nameOverride` through `fullname | trunc`).
    values_program_wrapper_exclusions: BTreeSet<String>,
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

    /// Records a pathless fragment accepted at a dependency values root.
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
        for (path, schema_types) in other.fallback_type_hints {
            self.fallback_type_hints
                .entry(path)
                .or_default()
                .extend(schema_types);
        }
        for (path, schema_types) in other.guarded_fallback_type_hints {
            self.guarded_fallback_type_hints
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
        self.values_root_overlay_prefixes
            .append(&mut other.values_root_overlay_prefixes);
        self.values_program_wrappers
            .append(&mut other.values_program_wrappers);
        self.values_program_wrapper_exclusions
            .append(&mut other.values_program_wrapper_exclusions);
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
    /// Record that rendering FAILS whenever `condition` holds — an
    /// unconditionally reached `include` whose helper only an inactive
    /// optional dependency defines aborts with "no template". The predicate
    /// lowers through the standard terminal-clause machinery.
    pub fn add_terminal_fail_condition(&mut self, condition: helm_schema_core::Predicate) {
        let capture = crate::eval_effect::FailCapture {
            conjunction: vec![condition],
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::Fail,
        };
        if !self.fail_conditions.contains(&capture) {
            self.fail_conditions.push(capture);
        }
    }

    /// Conjoins activation guards onto every use and terminating failure.
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
            self.values_root_overlay_prefixes.clear();
            // A path-wide runtime string contract is unconditional only
            // within its own chart's rendering: under activation guards the
            // consumer may never run, so the fact must not type the base.
            self.string_contract_value_paths.clear();
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
        let mut fallback_type_hints: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (path, schema_types) in std::mem::take(&mut self.fallback_type_hints) {
            fallback_type_hints
                .entry(map(&path))
                .or_default()
                .extend(schema_types);
        }
        self.fallback_type_hints = fallback_type_hints;
        let mut guarded_fallback_type_hints: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (path, schema_types) in std::mem::take(&mut self.guarded_fallback_type_hints) {
            guarded_fallback_type_hints
                .entry(map(&path))
                .or_default()
                .extend(schema_types);
        }
        self.guarded_fallback_type_hints = guarded_fallback_type_hints;
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
        self.values_root_overlay_prefixes = std::mem::take(&mut self.values_root_overlay_prefixes)
            .into_iter()
            .map(|path| map(&path))
            .collect();
        self.values_program_wrappers = std::mem::take(&mut self.values_program_wrappers)
            .into_iter()
            .map(|wrapper| helm_schema_core::ValuesProgramWrapper {
                scope_path: map(&wrapper.scope_path),
                key: wrapper.key,
                spread: wrapper.spread,
            })
            .collect();
        self.values_program_wrapper_exclusions =
            std::mem::take(&mut self.values_program_wrapper_exclusions)
                .into_iter()
                .map(|path| map(&path))
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
                capture.kind.map_value_paths(&mut map);
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

    pub(crate) fn extend_fallback_type_hints(
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
            self.fallback_type_hints
                .entry(path)
                .or_default()
                .extend(schema_types);
        }
    }

    pub(crate) fn extend_guarded_fallback_type_hints(
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
            self.guarded_fallback_type_hints
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

    pub(crate) fn extend_values_root_overlay_prefixes(
        &mut self,
        prefixes: impl IntoIterator<Item = String>,
    ) {
        self.values_root_overlay_prefixes.extend(prefixes);
    }

    pub(crate) fn extend_values_program_wrappers(
        &mut self,
        wrappers: impl IntoIterator<Item = helm_schema_core::ValuesProgramWrapper>,
    ) {
        self.values_program_wrappers.extend(wrappers);
    }

    pub(crate) fn extend_values_program_wrapper_exclusions(
        &mut self,
        paths: impl IntoIterator<Item = String>,
    ) {
        self.values_program_wrapper_exclusions.extend(paths);
    }

    /// Drop evidence recorded AT a program-wrapper sentinel key: within a
    /// wrapper-engine chart a `$tplYaml`-keyed member is the engine's own
    /// dispatch convention — probed by the recursive walker, replaced
    /// before ordinary consumers read the tree — never an ordinary chart
    /// value, so reads and fail predicates over such paths must not mint
    /// values properties. The wrapper alternatives model those nodes.
    pub(crate) fn scrub_program_wrapper_sentinel_evidence(&mut self) {
        let keys: std::collections::BTreeSet<String> = self
            .values_program_wrappers
            .iter()
            .map(|wrapper| wrapper.key.clone())
            .collect();
        if keys.is_empty() {
            return;
        }
        let touches = |path: &str| {
            helm_schema_core::split_value_path(path)
                .iter()
                .any(|segment| keys.contains(segment))
        };
        self.uses
            .retain(|contract_use| !touches(&contract_use.source_expr));
        self.dependency_uses
            .retain(|contract_use| !touches(&contract_use.source_expr));
        self.fail_conditions.retain(|capture| {
            let mut paths: Vec<String> = capture
                .conjunction
                .iter()
                .flat_map(helm_schema_core::Predicate::value_paths)
                .collect();
            let mut kind = capture.kind.clone();
            kind.map_value_paths(&mut |path: &str| {
                paths.push(path.to_string());
                path.to_string()
            });
            !paths.iter().any(|path| touches(path))
        });
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
    pub fn finalize(mut self) -> FinalizedContract {
        self.scrub_program_wrapper_sentinel_evidence();
        let Self {
            mut uses,
            mut dependency_uses,
            type_hints,
            guarded_type_hints,
            fallback_type_hints,
            guarded_fallback_type_hints,
            shape_erased_value_paths,
            string_contract_value_paths,
            range_modes,
            values_default_sources,
            values_root_overlay_prefixes,
            values_program_wrappers,
            values_program_wrapper_exclusions,
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
            &type_hints,
            &guarded_type_hints,
            &fallback_type_hints,
            &guarded_fallback_type_hints,
            &shape_erased_value_paths,
            &string_contract_value_paths,
            &range_modes,
            values_default_sources,
            values_root_overlay_prefixes,
            values_program_wrappers,
            values_program_wrapper_exclusions,
            &fail_conditions,
            &dependency_values_root_fragments,
        )
    }
}
