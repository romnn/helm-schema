use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::abstract_value::AbstractValue;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_body_analysis::{
    ResolveBoundHelperCallParams, interpret_bound_helper_body, resolve_bound_helper_call,
};
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
    path_facts: BTreeMap<String, HelperPathFacts>,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HelperPathFacts {
    pub(crate) output_meta: Option<HelperOutputMeta>,
    pub(crate) dependency_meta: Option<HelperOutputMeta>,
    pub(crate) guard: bool,
    pub(crate) type_hints: BTreeSet<String>,
    pub(crate) fragment_output_uses: Vec<HelperFragmentOutputUse>,
}

impl HelperPathFacts {
    fn merge(&mut self, other: Self) {
        merge_optional_meta(&mut self.output_meta, other.output_meta);
        merge_optional_meta(&mut self.dependency_meta, other.dependency_meta);
        self.guard |= other.guard;
        self.type_hints.extend(other.type_hints);
        self.fragment_output_uses.extend(other.fragment_output_uses);
    }

    pub(crate) fn is_dependency_relevant(&self) -> bool {
        self.output_meta.is_some()
            || self.dependency_meta.is_some()
            || self.guard
            || !self.type_hints.is_empty()
            || !self.fragment_output_uses.is_empty()
    }

    fn merge_output_meta(&mut self, meta: HelperOutputMeta) {
        merge_optional_meta(&mut self.output_meta, Some(meta));
    }

    fn merge_dependency_meta(&mut self, meta: HelperOutputMeta) {
        merge_optional_meta(&mut self.dependency_meta, Some(meta));
    }

    fn ensure_dependency_meta(&mut self) {
        self.dependency_meta
            .get_or_insert_with(HelperOutputMeta::default);
    }
}

fn merge_optional_meta(target: &mut Option<HelperOutputMeta>, incoming: Option<HelperOutputMeta>) {
    let Some(incoming) = incoming else {
        return;
    };
    match target {
        Some(target) => target.merge(incoming),
        None => *target = Some(incoming),
    }
}

impl HelperSummary {
    pub(crate) fn extend(&mut self, other: Self) {
        self.merge_path_facts(other.path_facts);
        self.string_output.extend(other.string_output);
        self.suppress_roots.extend(other.suppress_roots);
        self.chart_defaults.extend(other.chart_defaults);
    }

    pub(crate) fn add_output_meta(&mut self, path: String, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        self.merge_output_meta(path, meta);
    }

    pub(crate) fn merge_output_meta(&mut self, path: String, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        self.path_facts
            .entry(path)
            .or_default()
            .merge_output_meta(meta);
    }

    pub(crate) fn add_dependency_path(&mut self, path: String) {
        if !path.trim().is_empty() {
            self.path_facts
                .entry(path)
                .or_default()
                .ensure_dependency_meta();
        }
    }

    pub(crate) fn merge_dependency_meta(&mut self, path: String, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        self.path_facts
            .entry(path)
            .or_default()
            .merge_dependency_meta(meta);
    }

    pub(crate) fn add_guard_path(&mut self, path: String) {
        if !path.trim().is_empty() {
            self.path_facts.entry(path).or_default().guard = true;
        }
    }

    pub(crate) fn add_fragment_output_use(&mut self, output: HelperFragmentOutputUse) {
        if output.source_expr.trim().is_empty() {
            return;
        }
        self.path_facts
            .entry(output.source_expr.clone())
            .or_default()
            .fragment_output_uses
            .push(output);
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
        self.path_facts
            .entry(path)
            .or_default()
            .type_hints
            .extend(schema_types);
    }

    pub(crate) fn has_document_value_facts(&self) -> bool {
        self.path_facts
            .values()
            .any(HelperPathFacts::is_dependency_relevant)
    }

    pub(crate) fn add_provenance_to_outputs(&mut self, provenance: ContractProvenance) {
        for facts in self.path_facts.values_mut() {
            if let Some(meta) = facts.output_meta.as_mut() {
                meta.add_provenance_site(provenance.clone());
            }
        }
    }

    pub(crate) fn add_provenance_to_dependencies(&mut self, provenance: ContractProvenance) {
        for facts in self.path_facts.values_mut() {
            if let Some(meta) = facts.dependency_meta.as_mut() {
                meta.add_provenance_site(provenance.clone());
            }
        }
    }

    pub(crate) fn add_provenance_to_fragment_outputs(&mut self, provenance: ContractProvenance) {
        for facts in self.path_facts.values_mut() {
            for output in &mut facts.fragment_output_uses {
                output.meta.add_provenance_site(provenance.clone());
            }
        }
    }

    pub(crate) fn remove_output_path(&mut self, path: &str) {
        if let Some(facts) = self.path_facts.get_mut(path) {
            facts.output_meta = None;
        }
    }

    pub(crate) fn path_facts(&self) -> impl Iterator<Item = (&str, &HelperPathFacts)> {
        self.path_facts
            .iter()
            .map(|(path, facts)| (path.as_str(), facts))
    }

    pub(crate) fn structured_fragment_sources(&self) -> BTreeSet<String> {
        self.path_facts()
            .filter(|(_path, facts)| !facts.fragment_output_uses.is_empty())
            .map(|(path, _facts)| path.to_string())
            .collect()
    }

    pub(crate) fn rendered_sources(&self) -> BTreeSet<String> {
        let mut rendered_sources = self.structured_fragment_sources();
        rendered_sources.extend(
            self.path_facts()
                .filter(|(_path, facts)| facts.output_meta.is_some())
                .map(|(path, _facts)| path.to_string()),
        );
        rendered_sources
    }

    pub(crate) fn dependency_relevant_paths(&self) -> BTreeSet<String> {
        let paths = self
            .path_facts()
            .filter(|(_path, facts)| facts.is_dependency_relevant())
            .map(|(path, _facts)| path.to_string())
            .filter(|path| !path.trim().is_empty())
            .collect::<BTreeSet<_>>();
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
        let rendered_sources: BTreeSet<String> = self
            .path_facts
            .iter()
            .filter_map(|(path, facts)| {
                (facts.output_meta.is_some() || facts.guard).then_some(path.clone())
            })
            .collect();
        for binding in bindings.values() {
            let AbstractValue::ValuesPath(root) = binding else {
                continue;
            };
            if output_path::values_path_has_descendant(root, &rendered_sources) {
                self.suppress_roots.insert(root.clone());
            }
        }
    }

    fn merge_path_facts(&mut self, path_facts: BTreeMap<String, HelperPathFacts>) {
        for (path, facts) in path_facts {
            if path.trim().is_empty() {
                continue;
            }
            self.path_facts.entry(path).or_default().merge(facts);
        }
    }

    pub(crate) fn project_helper_value(self) -> Option<AbstractValue> {
        project_summary_value(self).map(|value| value.to_context_value())
    }

    pub(crate) fn project_fragment_value(self) -> Option<AbstractValue> {
        project_summary_value(self)
            .map(|value| value.to_context_value())
            .and_then(|value| AbstractValue::merge_context_values(vec![value]))
    }
}

fn project_summary_value(analysis: HelperSummary) -> Option<AbstractValue> {
    let structured_sources = analysis.structured_fragment_sources();
    let rendered_sources = analysis.rendered_sources();

    let mut values = Vec::new();
    if !analysis.string_output.is_empty() {
        values.push(AbstractValue::StringSet(analysis.string_output.clone()));
    }
    for (path, facts) in analysis.path_facts() {
        for output in facts.fragment_output_uses.iter().cloned() {
            values.push(AbstractValue::for_output_path(
                output.source_expr,
                &output.relative_path,
                output.meta,
            ));
        }
        if let Some(meta) = facts.output_meta.as_ref()
            && !structured_sources.contains(path)
            && !output_path::values_path_has_descendant(path, &rendered_sources)
        {
            values.push(AbstractValue::OutputSet(
                [(path.to_string(), meta.clone())].into_iter().collect(),
            ));
        }
    }
    AbstractValue::merge_all(values)
}

pub(crate) struct HelperSummaryCache {
    bound_helper_call: RefCell<BTreeMap<BoundHelperCallCacheKey, HelperSummary>>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BoundHelperCallCacheKey {
    name: String,
    arg: String,
    current_dot: Option<AbstractValue>,
    outer_bindings: BTreeMap<String, AbstractValue>,
    fragment_locals: BTreeMap<String, AbstractValue>,
    seen: BTreeSet<String>,
}

#[tracing::instrument(skip_all, fields(helper = name))]
fn analyze_bound_helper_call_with_fragment_locals(
    name: &str,
    arg: Option<&helm_schema_ast::TemplateExpr>,
    outer_bindings: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
    fragment_locals: &HashMap<String, AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HelperSummary {
    if !seen.insert(name.to_string()) {
        return HelperSummary::default();
    }

    let resolution = resolve_bound_helper_call(ResolveBoundHelperCallParams {
        helper_name: name,
        arg,
        outer_bindings,
        current_dot,
        fragment_locals,
        context,
        seen,
    });
    let mut analysis = interpret_bound_helper_body(name, &resolution, context, seen);
    analysis.mark_suppressed_roots_for_bound_outputs(&resolution.bindings);

    seen.remove(name);
    analysis
}

impl HelperSummaryCache {
    pub(crate) fn new() -> Self {
        Self {
            bound_helper_call: RefCell::new(BTreeMap::new()),
        }
    }

    #[tracing::instrument(skip_all, fields(helper = name))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn summarize_bound_helper_call(
        &self,
        name: &str,
        arg: Option<&helm_schema_ast::TemplateExpr>,
        outer_bindings: Option<&HashMap<String, AbstractValue>>,
        current_dot: Option<&AbstractValue>,
        fragment_locals: &HashMap<String, AbstractValue>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> HelperSummary {
        let outer_bindings_key: BTreeMap<String, AbstractValue> = outer_bindings
            .into_iter()
            .flatten()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let fragment_locals_key: BTreeMap<String, AbstractValue> = fragment_locals
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let key = BoundHelperCallCacheKey {
            name: name.to_string(),
            arg: format!("{arg:?}"),
            current_dot: current_dot.cloned(),
            outer_bindings: outer_bindings_key,
            fragment_locals: fragment_locals_key,
            seen: seen.iter().cloned().collect(),
        };

        if let Some(cached) = self.bound_helper_call.borrow().get(&key) {
            return cached.clone();
        }

        let summary = analyze_bound_helper_call_with_fragment_locals(
            name,
            arg,
            outer_bindings,
            current_dot,
            fragment_locals,
            context,
            seen,
        );
        self.bound_helper_call
            .borrow_mut()
            .insert(key, summary.clone());
        summary
    }
}

#[cfg(test)]
#[path = "tests/helper_summary.rs"]
mod tests;
