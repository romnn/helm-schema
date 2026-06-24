use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::output_path;
use crate::predicate::Predicate;
use crate::{ContractProvenance, Guard, ValueKind, YamlPath};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HelperOutputMeta {
    pub(crate) predicates: BTreeSet<BTreeSet<Predicate>>,
    pub(crate) defaulted: bool,
    pub(crate) provenance: Vec<ContractProvenance>,
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
        }
    }

    pub(crate) fn add_predicates(&mut self, predicates: impl IntoIterator<Item = Predicate>) {
        let predicates = predicates.into_iter().collect::<Vec<_>>();
        if predicates.is_empty() {
            return;
        }
        if self.predicates.is_empty() {
            self.predicates.insert(BTreeSet::new());
        }
        let branches = std::mem::take(&mut self.predicates);
        self.predicates = branches
            .into_iter()
            .map(|mut branch| {
                branch.extend(predicates.iter().cloned());
                branch
            })
            .collect();
    }

    pub(crate) fn with_additional_predicates(mut self, predicates: &BTreeSet<Predicate>) -> Self {
        self.add_predicates(predicates.iter().cloned());
        self
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.predicates.extend(other.predicates);
        self.defaulted |= other.defaulted;
        self.merge_provenance(other.provenance);
    }

    pub(crate) fn merge_ref(&mut self, other: &Self) {
        self.predicates.extend(other.predicates.iter().cloned());
        self.defaulted |= other.defaulted;
        self.merge_provenance(other.provenance.iter().cloned());
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
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HelperSummary {
    pub(crate) string_output: BTreeSet<String>,
    pub(crate) scalar_output_meta: BTreeMap<String, HelperOutputMeta>,
    pub(crate) dependency_meta: BTreeMap<String, HelperOutputMeta>,
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(crate) fragment_output_uses: Vec<HelperFragmentOutputUse>,
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
        for (path, meta) in other.scalar_output_meta {
            self.merge_output_meta(path, meta);
        }
        for (path, meta) in other.dependency_meta {
            self.merge_dependency_meta(path, meta);
        }
        self.guard_paths.extend(other.guard_paths);
        for (path, hints) in other.type_hints {
            self.merge_type_hints(path, hints);
        }
        self.fragment_output_uses.extend(other.fragment_output_uses);
        self.string_output.extend(other.string_output);
        self.suppress_roots.extend(other.suppress_roots);
        self.chart_defaults.extend(other.chart_defaults);
    }

    pub(crate) fn merge_output_meta(&mut self, path: String, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        self.scalar_output_meta.entry(path).or_default().merge(meta);
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

    pub(crate) fn add_fragment_output_use(&mut self, output: HelperFragmentOutputUse) {
        if output.source_expr.trim().is_empty() {
            return;
        }
        self.fragment_output_uses.push(output);
    }

    pub(crate) fn add_fragment_output_uses(&mut self, mut outputs: Vec<HelperFragmentOutputUse>) {
        outputs.retain(|output| {
            output.kind == ValueKind::Fragment || !output.relative_path.0.is_empty()
        });
        let structured_sources: BTreeSet<String> = outputs
            .iter()
            .map(|output| output.source_expr.clone())
            .collect();
        for source in &structured_sources {
            self.remove_output_path(source);
        }
        for output in outputs {
            self.add_fragment_output_use(output);
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
        !self.scalar_output_meta.is_empty()
            || !self.dependency_meta.is_empty()
            || !self.guard_paths.is_empty()
            || !self.type_hints.is_empty()
            || !self.fragment_output_uses.is_empty()
    }

    pub(crate) fn add_provenance(&mut self, provenance: ContractProvenance) {
        for meta in self.scalar_output_meta.values_mut() {
            meta.add_provenance_site(provenance.clone());
        }
        for meta in self.dependency_meta.values_mut() {
            meta.add_provenance_site(provenance.clone());
        }
        for output in &mut self.fragment_output_uses {
            output.meta.add_provenance_site(provenance.clone());
        }
    }

    pub(crate) fn remove_output_path(&mut self, path: &str) {
        self.scalar_output_meta.remove(path);
    }

    pub(crate) fn has_structured_fragment_source(&self, path: &str) -> bool {
        self.fragment_output_uses
            .iter()
            .any(|output| output.source_expr == path)
    }

    pub(crate) fn has_rendered_source_descendant(&self, path: &str) -> bool {
        self.scalar_output_meta
            .keys()
            .any(|candidate| output_path::values_path_is_descendant(candidate, path))
            || self
                .fragment_output_uses
                .iter()
                .any(|output| output_path::values_path_is_descendant(&output.source_expr, path))
    }

    pub(crate) fn dependency_relevant_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        paths.extend(self.scalar_output_meta.keys().cloned());
        paths.extend(self.dependency_meta.keys().cloned());
        paths.extend(self.guard_paths.iter().cloned());
        paths.extend(self.type_hints.keys().cloned());
        paths.extend(
            self.fragment_output_uses
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
        let mut rendered_sources: BTreeSet<String> =
            self.scalar_output_meta.keys().cloned().collect();
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
        project_summary_value(self)
            .map(|value| value.to_context_value())
            .and_then(|value| AbstractValue::merge_context_values(vec![value]))
    }
}

fn project_summary_value(analysis: &HelperSummary) -> Option<AbstractValue> {
    let mut values = Vec::new();
    if !analysis.string_output.is_empty() {
        values.push(AbstractValue::StringSet(analysis.string_output.clone()));
    }
    for output in analysis.fragment_output_uses.iter().cloned() {
        values.push(AbstractValue::for_output_path(
            output.source_expr,
            &output.relative_path,
            output.meta,
        ));
    }
    for (path, meta) in &analysis.scalar_output_meta {
        if !analysis.has_structured_fragment_source(path)
            && !analysis.has_rendered_source_descendant(path)
        {
            values.push(AbstractValue::OutputSet(
                [(path.clone(), meta.clone())].into_iter().collect(),
            ));
        }
    }
    AbstractValue::merge_all(values)
}

#[cfg(test)]
#[path = "tests/helper_summary.rs"]
mod tests;
