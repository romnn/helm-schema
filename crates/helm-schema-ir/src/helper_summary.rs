use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::{ContractProvenance, Guard, ValueKind, YamlPath};
use helm_schema_core as output_path;
use helm_schema_core::Predicate;

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HelperOutputMeta {
    pub(crate) predicates: BTreeSet<BTreeSet<Predicate>>,
    pub(crate) defaulted: bool,
    pub(crate) provenance: Vec<ContractProvenance>,
    pub(crate) suppress_predicate_paths: BTreeSet<String>,
}

impl HelperOutputMeta {
    pub(crate) fn with_predicates(predicates: &BTreeSet<Predicate>, defaulted: bool) -> Self {
        let mut predicate_branches = BTreeSet::new();
        if !predicates.is_empty() {
            predicate_branches.insert(predicates.clone());
        }
        Self {
            predicates: predicate_branches,
            defaulted,
            provenance: Vec::new(),
            suppress_predicate_paths: BTreeSet::new(),
        }
    }

    pub(crate) fn with_output_site_predicates(
        mut self,
        _source_expr: &str,
        predicates: &BTreeSet<Predicate>,
    ) -> Self {
        let active_predicates = predicates.iter().cloned().collect::<Vec<_>>();
        let base_branches = if self.predicates.is_empty() {
            vec![BTreeSet::new()]
        } else {
            self.predicates.into_iter().collect::<Vec<_>>()
        };
        self.predicates = base_branches
            .into_iter()
            .map(|base_branch| {
                let mut branch = base_branch.clone();
                branch.extend(active_predicates.iter().cloned());
                branch
            })
            .collect();
        self
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.predicates.extend(other.predicates);
        self.defaulted |= other.defaulted;
        self.merge_provenance(other.provenance);
        self.suppress_predicate_paths
            .extend(other.suppress_predicate_paths);
    }

    pub(crate) fn merge_ref(&mut self, other: &Self) {
        self.predicates.extend(other.predicates.iter().cloned());
        self.defaulted |= other.defaulted;
        self.merge_provenance(other.provenance.iter().cloned());
        self.suppress_predicate_paths
            .extend(other.suppress_predicate_paths.iter().cloned());
    }

    pub(crate) fn suppress_predicate_path(&mut self, path: impl Into<String>) {
        let path = path.into();
        if !path.is_empty() {
            self.suppress_predicate_paths.insert(path);
        }
    }

    pub(crate) fn add_provenance_site(&mut self, provenance: ContractProvenance) {
        self.merge_provenance(std::iter::once(provenance));
    }

    fn merge_provenance(&mut self, incoming: impl IntoIterator<Item = ContractProvenance>) {
        for provenance in incoming {
            if !self.provenance.contains(&provenance) {
                self.provenance.push(provenance);
            }
        }
    }

    pub(crate) fn contract_guard_sets(&self, source_expr: &str) -> Vec<Vec<Guard>> {
        let predicate_branches = if self.predicates.is_empty() {
            vec![BTreeSet::new()]
        } else {
            self.predicates.iter().cloned().collect::<Vec<_>>()
        };
        let mut guard_sets = Vec::new();
        for predicate_branch in predicate_branches {
            let predicate_branch = self.prune_suppressed_predicates(predicate_branch, source_expr);
            let mut guards =
                Predicate::contract_guard_stack(&predicate_branch.into_iter().collect::<Vec<_>>());
            if self.defaulted {
                let default_guard = Guard::Default {
                    path: source_expr.to_string(),
                };
                if !guards.contains(&default_guard) {
                    guards.push(default_guard);
                }
            }
            if !guard_sets.contains(&guards) {
                guard_sets.push(guards);
            }
        }
        guard_sets
    }

    fn prune_suppressed_predicates(
        &self,
        predicate_branch: BTreeSet<Predicate>,
        source_expr: &str,
    ) -> BTreeSet<Predicate> {
        let has_source_truthy = predicate_branch
            .iter()
            .any(|predicate| truthy_guard_path(predicate).is_some_and(|path| path == source_expr));
        if !has_source_truthy || self.suppress_predicate_paths.is_empty() {
            return predicate_branch;
        }
        predicate_branch
            .into_iter()
            .filter(|predicate| {
                let Some(path) = truthy_guard_path(predicate) else {
                    return true;
                };
                !self.suppress_predicate_paths.contains(path)
                    || !output_path::values_path_is_descendant(source_expr, path)
            })
            .collect()
    }
}

fn truthy_guard_path(predicate: &Predicate) -> Option<&str> {
    match predicate {
        Predicate::Guard(Guard::Truthy { path }) => Some(path),
        _ => None,
    }
}

pub(crate) fn insert_type_hint(
    hints: &mut BTreeMap<String, BTreeSet<String>>,
    path: String,
    schema_type: &str,
) {
    if path.trim().is_empty() {
        return;
    }
    hints
        .entry(path)
        .or_default()
        .insert(schema_type.to_string());
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HelperFragmentOutputUse {
    pub(crate) source_expr: String,
    pub(crate) relative_path: YamlPath,
    pub(crate) kind: ValueKind,
    pub(crate) encoded: bool,
    pub(crate) meta: HelperOutputMeta,
}

impl HelperFragmentOutputUse {
    pub(crate) fn new(
        source_expr: String,
        relative_path: YamlPath,
        kind: ValueKind,
        meta: HelperOutputMeta,
    ) -> Self {
        Self {
            source_expr,
            relative_path,
            kind,
            encoded: false,
            meta,
        }
    }

    pub(crate) fn with_encoding(
        source_expr: String,
        relative_path: YamlPath,
        kind: ValueKind,
        encoded: bool,
        meta: HelperOutputMeta,
    ) -> Self {
        Self {
            source_expr,
            relative_path,
            kind,
            encoded,
            meta,
        }
    }

    pub(crate) fn is_scalar_summary_output(&self) -> bool {
        self.relative_path.0.is_empty() && self.kind == ValueKind::Scalar && !self.encoded
    }

    pub(crate) fn is_structured_output(&self) -> bool {
        !self.is_scalar_summary_output()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HelperSummary {
    pub(crate) string_output: BTreeSet<String>,
    pub(crate) dependency_meta: BTreeMap<String, HelperOutputMeta>,
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(crate) output_uses: Vec<HelperFragmentOutputUse>,
    pub(crate) suppress_roots: BTreeSet<String>,
    /// Values-rooted paths that a helper body structurally declares as
    /// null-tolerant via a `set OPERAND "KEY" (OPERAND.KEY | default V)`
    /// mutation. Distinct from `defaulted`, which represents local
    /// `(X | default V)` expressions including condition fallbacks.
    ///
    /// Only explicit set-mutation defaults count here, because that is
    /// the chart writer asserting that this path gets normalized before
    /// later reads in the same render flow.
    pub(crate) chart_defaults: BTreeSet<String>,
}

impl HelperSummary {
    pub(crate) fn extend(&mut self, other: Self) {
        for (path, meta) in other.dependency_meta {
            self.merge_dependency_meta(path, meta);
        }
        self.guard_paths.extend(other.guard_paths);
        for (path, hints) in other.type_hints {
            self.merge_type_hints(path, hints);
        }
        self.add_output_uses(other.output_uses);
        self.string_output.extend(other.string_output);
        self.suppress_roots.extend(other.suppress_roots);
        self.chart_defaults.extend(other.chart_defaults);
    }

    pub(crate) fn merge_output_meta(&mut self, path: String, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        self.add_output_use(HelperFragmentOutputUse::new(
            path,
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            meta,
        ));
    }

    pub(crate) fn merge_dependency_meta(&mut self, path: String, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        self.dependency_meta.entry(path).or_default().merge(meta);
    }

    pub(crate) fn add_guard_path(&mut self, path: String) {
        if !path.trim().is_empty() {
            self.guard_paths.insert(path);
        }
    }

    pub(crate) fn add_output_use(&mut self, output: HelperFragmentOutputUse) {
        if output.source_expr.trim().is_empty() {
            return;
        }
        if output.is_scalar_summary_output()
            && let Some(existing) = self.output_uses.iter_mut().find(|existing| {
                existing.is_scalar_summary_output() && existing.source_expr == output.source_expr
            })
        {
            existing.meta.merge(output.meta);
            return;
        }
        self.output_uses.push(output);
    }

    pub(crate) fn add_output_uses(&mut self, outputs: Vec<HelperFragmentOutputUse>) {
        for output in outputs {
            self.add_output_use(output);
        }
    }

    pub(crate) fn add_type_hints(&mut self, hints: BTreeMap<String, BTreeSet<String>>) {
        for (path, schema_types) in hints {
            self.merge_type_hints(path, schema_types);
        }
    }

    pub(crate) fn merge_type_hints(&mut self, path: String, schema_types: BTreeSet<String>) {
        if path.trim().is_empty() {
            return;
        }
        self.type_hints
            .entry(path)
            .or_default()
            .extend(schema_types);
    }

    pub(crate) fn has_document_value_facts(&self) -> bool {
        !self.output_uses.is_empty()
            || !self.dependency_meta.is_empty()
            || !self.guard_paths.is_empty()
            || !self.type_hints.is_empty()
    }

    pub(crate) fn add_provenance(&mut self, provenance: ContractProvenance) {
        for meta in self.dependency_meta.values_mut() {
            meta.add_provenance_site(provenance.clone());
        }
        for output in &mut self.output_uses {
            output.meta.add_provenance_site(provenance.clone());
        }
    }

    pub(crate) fn remove_output_path(&mut self, path: &str) {
        self.output_uses
            .retain(|output| !(output.is_scalar_summary_output() && output.source_expr == path));
    }

    pub(crate) fn has_structured_fragment_source(&self, path: &str) -> bool {
        self.output_uses
            .iter()
            .any(|output| output.is_structured_output() && output.source_expr == path)
    }

    pub(crate) fn has_rendered_source_descendant(&self, path: &str) -> bool {
        self.output_uses
            .iter()
            .any(|output| output_path::values_path_is_descendant(&output.source_expr, path))
    }

    pub(crate) fn dependency_relevant_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        paths.extend(self.dependency_meta.keys().cloned());
        paths.extend(self.guard_paths.iter().cloned());
        paths.extend(self.type_hints.keys().cloned());
        paths.extend(
            self.output_uses
                .iter()
                .map(|output| output.source_expr.clone()),
        );
        paths
            .iter()
            .filter(|path| !output_path::values_path_has_descendant(path, &paths))
            .cloned()
            .collect()
    }

    pub(crate) fn take_chart_value_defaults(&mut self) -> BTreeSet<String> {
        std::mem::take(&mut self.chart_defaults)
    }

    pub(crate) fn mark_suppressed_roots_for_bound_outputs(
        &mut self,
        bindings: &HashMap<String, AbstractValue>,
    ) {
        let mut rendered_sources: BTreeSet<String> = self
            .output_uses
            .iter()
            .filter(|output| output.is_scalar_summary_output())
            .map(|output| output.source_expr.clone())
            .collect();
        rendered_sources.extend(self.guard_paths.iter().cloned());
        for binding in bindings.values() {
            let AbstractValue::ValuesPath(root) = binding else {
                continue;
            };
            if output_path::values_path_has_descendant(root, &rendered_sources) {
                self.suppress_roots.insert(root.clone());
            }
        }
    }

    pub(crate) fn project_value(&self) -> Option<AbstractValue> {
        let mut values = Vec::new();
        if !self.string_output.is_empty() {
            values.push(AbstractValue::StringSet(self.string_output.clone()));
        }
        for output in self.output_uses.iter().cloned() {
            if output.is_scalar_summary_output()
                && (self.has_structured_fragment_source(&output.source_expr)
                    || self.has_rendered_source_descendant(&output.source_expr))
            {
                continue;
            }
            values.push(AbstractValue::for_output_path(
                output.source_expr,
                &output.relative_path,
                output.meta,
            ));
        }
        AbstractValue::merge_all(values)
            .map(|value| value.to_context_value())
            .and_then(|value| AbstractValue::merge_context_values(vec![value]))
    }
}

#[cfg(test)]
#[path = "tests/helper_summary.rs"]
mod tests;
