use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::contract_sink::EmissionWitness;
use crate::eval_effect::Effects;
use crate::{ContractProvenance, Guard, ValueKind, YamlPath};
use helm_schema_core as output_path;
use helm_schema_core::Predicate;

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HelperOutputMeta {
    pub(crate) predicates: BTreeSet<BTreeSet<Predicate>>,
    pub(crate) defaulted: bool,
    pub(crate) provenance: Vec<ContractProvenance>,
    pub(crate) suppress_predicate_paths: BTreeSet<String>,
    pub(crate) related_sources: BTreeSet<String>,
    pub(crate) sibling_sources: BTreeSet<String>,
    pub(crate) require_sibling_guards: bool,
}

impl HelperOutputMeta {
    pub(crate) fn with_output_site_predicates(mut self, predicates: &BTreeSet<Predicate>) -> Self {
        if predicates.is_empty() {
            return self;
        }
        let active_predicates = predicates.iter().cloned().collect::<Vec<_>>();
        self.predicates = branches_or_unconditional(self.predicates)
            .into_iter()
            .map(|mut branch| {
                branch.extend(active_predicates.iter().cloned());
                branch
            })
            .collect();
        self
    }

    pub(crate) fn merge(&mut self, other: &Self) {
        self.predicates.extend(other.predicates.iter().cloned());
        self.defaulted |= other.defaulted;
        merge_provenance_sites(&mut self.provenance, &other.provenance);
        self.suppress_predicate_paths
            .extend(other.suppress_predicate_paths.iter().cloned());
        self.related_sources
            .extend(other.related_sources.iter().cloned());
        self.sibling_sources
            .extend(other.sibling_sources.iter().cloned());
        self.require_sibling_guards |= other.require_sibling_guards;
    }

    pub(crate) fn suppress_predicate_path(&mut self, path: impl Into<String>) {
        let path = path.into();
        if !path.is_empty() {
            self.suppress_predicate_paths.insert(path);
        }
    }

    pub(crate) fn add_provenance_site(&mut self, provenance: ContractProvenance) {
        merge_provenance_sites(&mut self.provenance, std::slice::from_ref(&provenance));
    }

    pub(crate) fn relate_sources(&mut self, sources: &BTreeSet<String>) {
        self.related_sources
            .extend(sources.iter().filter(|source| !source.is_empty()).cloned());
    }

    pub(crate) fn relate_source_relations(&mut self, relations: &[BTreeSet<String>]) {
        for sources in relations {
            self.relate_sources(sources);
        }
    }

    pub(crate) fn note_sibling_sources(&mut self, source_expr: &str, sources: &BTreeSet<String>) {
        if self.predicates.is_empty() && !self.defaulted {
            return;
        }
        self.sibling_sources.extend(
            sources
                .iter()
                .filter(|source| source.as_str() != source_expr && !source.is_empty())
                .cloned(),
        );
    }

    pub(crate) fn require_sibling_guards(&mut self) {
        self.require_sibling_guards = true;
    }

    /// Applies one pruning rule to every predicate branch. The branch set is
    /// detached while the rule runs, so the rule may consult the rest of the
    /// meta (`related_sources`, `defaulted`, ...) through the `&Self` view.
    fn rewrite_predicate_branches(&mut self, rule: impl Fn(&Self, &mut BTreeSet<Predicate>)) {
        let predicate_branches = std::mem::take(&mut self.predicates);
        self.predicates = predicate_branches
            .into_iter()
            .map(|mut predicate_branch| {
                rule(self, &mut predicate_branch);
                predicate_branch
            })
            .collect();
    }

    pub(crate) fn prune_source_not_for_sibling_truthy(
        &mut self,
        source_expr: &str,
        sources: &BTreeSet<String>,
    ) {
        self.rewrite_predicate_branches(|meta, predicate_branch| {
            let has_truthy_sibling = predicate_branch.iter().any(|predicate| {
                predicate_truthy_path(predicate)
                    .is_some_and(|path| unrelated_source_sibling(source_expr, path, meta, sources))
            });
            if has_truthy_sibling {
                predicate_branch.retain(|predicate| {
                    predicate_not_truthy_path(predicate).is_none_or(|path| path != source_expr)
                });
            }
        });
    }

    pub(crate) fn prune_truthy_ancestors_of_source(&mut self, source_expr: &str) {
        self.rewrite_predicate_branches(|_meta, predicate_branch| {
            let has_source_truthy = predicate_branch.iter().any(|predicate| {
                predicate_truthy_path(predicate).is_some_and(|path| path == source_expr)
            });
            if has_source_truthy {
                predicate_branch.retain(|predicate| {
                    !predicate_truthy_path(predicate).is_some_and(|path| {
                        path != source_expr
                            && output_path::values_path_is_descendant(source_expr, path)
                    })
                });
            }
        });
    }

    fn record_unconditional_branch(&mut self) {
        if self.predicates.is_empty() {
            self.predicates.insert(BTreeSet::new());
        }
    }

    pub(crate) fn contract_guard_sets(&self, source_expr: &str) -> Vec<Vec<Guard>> {
        let predicate_branches = branches_or_unconditional(self.predicates.clone());
        let mut guard_sets = Vec::new();
        for predicate_branch in predicate_branches {
            let predicate_branch = self.prune_suppressed_predicates(predicate_branch, source_expr);
            let mut guards =
                Predicate::contract_guard_stack(&predicate_branch.into_iter().collect::<Vec<_>>());
            drop_redundant_not_eq_guards(&mut guards);
            if self.defaulted {
                let default_guard = Guard::Default {
                    path: source_expr.to_string(),
                };
                if !guards.contains(&default_guard) {
                    guards.push(default_guard);
                }
                self.prune_defaulted_sibling_guards(&mut guards, source_expr);
            }
            if !guard_sets.contains(&guards) {
                guard_sets.push(guards);
            }
        }
        guard_sets
    }

    /// Lowers this meta to the witness for one emitted row: one guard set
    /// per predicate branch plus this meta's provenance sites.
    pub(crate) fn emission_witness(
        &self,
        source_expr: &str,
        emit_path: Option<YamlPath>,
        kind: ValueKind,
    ) -> EmissionWitness {
        EmissionWitness {
            source_expr: source_expr.to_string(),
            emit_path,
            kind,
            guard_sets: self.contract_guard_sets(source_expr),
            provenance: self.provenance.clone(),
            dependency: false,
        }
    }

    fn prune_defaulted_sibling_guards(&self, guards: &mut Vec<Guard>, source_expr: &str) {
        let unrelated_sibling_truthy = |guard: &Guard| {
            let Guard::Truthy { path } = guard else {
                return false;
            };
            unrelated_source_sibling(source_expr, path, self, &self.sibling_sources)
        };
        let has_source_truthy = guards
            .iter()
            .any(|guard| matches!(guard, Guard::Truthy { path } if path == source_expr));
        let has_unrelated_sibling_truthy = guards.iter().any(unrelated_sibling_truthy);
        if has_source_truthy {
            guards.retain(|guard| !unrelated_sibling_truthy(guard));
        }
        if has_source_truthy || has_unrelated_sibling_truthy {
            guards.retain(|guard| !matches!(guard, Guard::Not { path } if path == source_expr));
        }
    }

    fn prune_suppressed_predicates(
        &self,
        predicate_branch: BTreeSet<Predicate>,
        source_expr: &str,
    ) -> BTreeSet<Predicate> {
        let has_source_truthy = predicate_branch.iter().any(|predicate| {
            predicate_truthy_path(predicate).is_some_and(|path| path == source_expr)
        });
        if !has_source_truthy || self.suppress_predicate_paths.is_empty() {
            return predicate_branch;
        }
        predicate_branch
            .into_iter()
            .filter(|predicate| {
                let Some(path) = predicate_truthy_path(predicate) else {
                    return true;
                };
                !self.suppress_predicate_paths.contains(path)
                    || !output_path::values_path_is_descendant(source_expr, path)
            })
            .collect()
    }
}

/// Appends `extra` provenance sites onto `target`, preserving first-seen
/// order and skipping sites already present. Every provenance merge in the
/// contract pipeline uses this discipline so emitted site lists stay
/// deterministic.
pub(crate) fn merge_provenance_sites(
    target: &mut Vec<ContractProvenance>,
    extra: &[ContractProvenance],
) {
    for site in extra {
        if !target.contains(site) {
            target.push(site.clone());
        }
    }
}

/// A meta with no recorded predicates means one unconditional branch, not
/// zero branches.
fn branches_or_unconditional(branches: BTreeSet<BTreeSet<Predicate>>) -> Vec<BTreeSet<Predicate>> {
    if branches.is_empty() {
        vec![BTreeSet::new()]
    } else {
        branches.into_iter().collect()
    }
}

fn predicate_truthy_path(predicate: &Predicate) -> Option<&str> {
    match predicate {
        Predicate::Guard(Guard::Truthy { path }) => Some(path),
        _ => None,
    }
}

fn predicate_not_truthy_path(predicate: &Predicate) -> Option<&str> {
    match predicate {
        Predicate::Guard(Guard::Not { path }) => Some(path),
        Predicate::Not(inner) => predicate_truthy_path(inner),
        _ => None,
    }
}

fn predicate_truthiness(predicate: &Predicate) -> Option<(&str, bool)> {
    if let Some(path) = predicate_truthy_path(predicate) {
        return Some((path, true));
    }
    predicate_not_truthy_path(predicate).map(|path| (path, false))
}

/// The active output-site predicates that may apply to one nested row. A
/// predicate about another nested source is dropped unless the paths are
/// related (it describes a sibling's branch, not this row's).
fn nested_output_site_predicates(
    source_expr: &str,
    active_output_predicates: &BTreeSet<Predicate>,
    sibling_sources: &BTreeSet<String>,
) -> BTreeSet<Predicate> {
    active_output_predicates
        .iter()
        .filter(|predicate| {
            let Some((path, _)) = predicate_truthiness(predicate) else {
                return true;
            };
            !sibling_sources.contains(path)
                || path == source_expr
                || values_paths_are_related(path, source_expr)
        })
        .cloned()
        .collect()
}

fn drop_redundant_not_eq_guards(guards: &mut Vec<Guard>) {
    let eq_guards = guards
        .iter()
        .filter_map(|guard| match guard {
            Guard::Eq { path, value } => Some((path.clone(), value.clone())),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if eq_guards.is_empty() {
        return;
    }
    guards.retain(|guard| match guard {
        Guard::NotEq { path, value } => !eq_guards
            .iter()
            .any(|(eq_path, eq_value)| eq_path == path && eq_value != value),
        _ => true,
    });
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
    pub(crate) rendered: bool,
    pub(crate) meta: HelperOutputMeta,
}

impl HelperFragmentOutputUse {
    pub(crate) fn new(
        source_expr: String,
        relative_path: YamlPath,
        kind: ValueKind,
        meta: HelperOutputMeta,
    ) -> Self {
        Self::with_encoding(source_expr, relative_path, kind, false, meta)
    }

    pub(crate) fn with_encoding(
        source_expr: String,
        relative_path: YamlPath,
        kind: ValueKind,
        encoded: bool,
        mut meta: HelperOutputMeta,
    ) -> Self {
        meta.record_unconditional_branch();
        Self {
            source_expr,
            relative_path,
            kind,
            encoded,
            rendered: true,
            meta,
        }
    }

    pub(crate) fn dependency(source_expr: String, meta: HelperOutputMeta) -> Self {
        Self {
            source_expr,
            relative_path: YamlPath(Vec::new()),
            kind: ValueKind::Scalar,
            encoded: false,
            rendered: false,
            meta,
        }
    }

    pub(crate) fn is_rendered(&self) -> bool {
        self.rendered
    }

    pub(crate) fn is_dependency(&self) -> bool {
        !self.rendered
    }

    pub(crate) fn is_scalar_summary_output(&self) -> bool {
        self.rendered
            && self.relative_path.0.is_empty()
            && self.kind == ValueKind::Scalar
            && !self.encoded
    }

    pub(crate) fn is_structured_output(&self) -> bool {
        self.rendered && !self.is_scalar_summary_output()
    }
}

/// Merges the meta of every rendered row into a per-source meta map. The
/// fragment-output assignment path and the symbolic walker's assignment-fact
/// refresh both carry helper output meta on local bindings this way.
pub(crate) fn merge_output_use_meta(
    output_meta: &mut BTreeMap<String, HelperOutputMeta>,
    outputs: &[HelperFragmentOutputUse],
) {
    for output in outputs {
        if output.is_dependency() {
            continue;
        }
        output_meta
            .entry(output.source_expr.clone())
            .or_default()
            .merge(&output.meta);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HelperSummary {
    pub(crate) string_output: BTreeSet<String>,
    pub(crate) guard_path_meta: BTreeMap<String, HelperOutputMeta>,
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

/// Which rows of a nested helper call's summary land in the calling
/// collector's summary as dependency rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NestedDependencyRows {
    /// A direct output site keeps nested rendered rows as rendered output (it
    /// re-bases and re-pushes them itself), so only rows that already were
    /// dependencies transfer.
    DependenciesOnly,
    /// An assignment site captures rendered output into a local binding
    /// instead of emitting it, so every nested row demotes to a dependency.
    AllRows,
}

impl HelperSummary {
    pub(crate) fn extend(&mut self, other: Self) {
        for (path, meta) in other.guard_path_meta {
            self.merge_guard_path_meta(path, meta);
        }
        self.add_type_hints(other.type_hints);
        self.add_output_uses(other.output_uses);
        self.string_output.extend(other.string_output);
        self.suppress_roots.extend(other.suppress_roots);
        self.chart_defaults.extend(other.chart_defaults);
    }

    /// Absorbs a nested helper call's read facts: guard-path meta rows first,
    /// then the selected output rows as dependency rows. Each dependency row
    /// keeps the nested meta, gains the active output-site predicates that
    /// survive nested sibling-source filtering, and records the active source
    /// relations. Every nested source counts as a sibling for that filter,
    /// even when only the dependency rows land.
    pub(crate) fn absorb_nested_dependencies(
        &mut self,
        nested: &HelperSummary,
        rows: NestedDependencyRows,
        active_output_predicates: &BTreeSet<Predicate>,
        active_source_relations: &[BTreeSet<String>],
    ) {
        for (path, meta) in &nested.guard_path_meta {
            self.merge_guard_path_meta(path.clone(), meta.clone());
        }
        let nested_site_sources: BTreeSet<String> = nested
            .output_uses
            .iter()
            .map(|output| output.source_expr.clone())
            .collect();
        for output in &nested.output_uses {
            if rows == NestedDependencyRows::DependenciesOnly && !output.is_dependency() {
                continue;
            }
            let output_site_predicates = nested_output_site_predicates(
                &output.source_expr,
                active_output_predicates,
                &nested_site_sources,
            );
            let mut meta = output
                .meta
                .clone()
                .with_output_site_predicates(&output_site_predicates);
            meta.relate_source_relations(active_source_relations);
            self.merge_dependency_meta(output.source_expr.clone(), meta);
        }
    }

    pub(crate) fn merge_dependency_meta(&mut self, path: String, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        self.add_output_use(HelperFragmentOutputUse::dependency(path, meta));
    }

    pub(crate) fn add_guard_path(&mut self, path: String) {
        self.merge_guard_path_meta(path, HelperOutputMeta::default());
    }

    pub(crate) fn merge_guard_path_meta(&mut self, path: String, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        self.guard_path_meta.entry(path).or_default().merge(&meta);
    }

    pub(crate) fn add_output_use(&mut self, output: HelperFragmentOutputUse) {
        self.insert_output_use(output);
        self.prune_pathless_summary_sibling_predicates();
    }

    /// Inserts one row without re-normalizing sibling predicates. The prune is
    /// a function of the whole row set, so batch entry points run it once after
    /// all rows land instead of once per row (which grows quadratically).
    fn insert_output_use(&mut self, mut output: HelperFragmentOutputUse) {
        if output.source_expr.trim().is_empty() {
            return;
        }
        if output.is_structured_output()
            && output.meta.defaulted
            && !output.meta.sibling_sources.is_empty()
        {
            output.meta.require_sibling_guards();
        }
        if let Some(existing) = self
            .output_uses
            .iter_mut()
            .find(|existing| helper_output_use_identity_matches(existing, &output))
        {
            existing.meta.merge(&output.meta);
            return;
        }
        self.output_uses.push(output);
    }

    pub(crate) fn add_output_uses(&mut self, outputs: Vec<HelperFragmentOutputUse>) {
        for output in outputs {
            self.insert_output_use(output);
        }
        self.prune_pathless_summary_sibling_predicates();
    }

    fn prune_pathless_summary_sibling_predicates(&mut self) {
        let pathless_sources: BTreeSet<String> = self
            .output_uses
            .iter()
            .filter(|output| output.is_scalar_summary_output() || output.is_dependency())
            .map(|output| output.source_expr.clone())
            .collect();
        if pathless_sources.len() < 2 {
            return;
        }
        for output in &mut self.output_uses {
            if !pathless_sources.contains(&output.source_expr) {
                continue;
            }
            let record_sibling_sources = !output.is_structured_output();
            prune_pathless_sibling_predicates_for_meta(
                &output.source_expr,
                output.relative_path.0.is_empty(),
                &mut output.meta,
                &pathless_sources,
                record_sibling_sources,
            );
        }
        for (source_expr, meta) in &mut self.guard_path_meta {
            if pathless_sources.contains(source_expr) {
                prune_pathless_sibling_predicates_for_meta(
                    source_expr,
                    true,
                    meta,
                    &pathless_sources,
                    true,
                );
            }
        }
    }

    pub(crate) fn add_type_hints(&mut self, hints: BTreeMap<String, BTreeSet<String>>) {
        for (path, schema_types) in hints {
            if path.trim().is_empty() {
                continue;
            }
            self.type_hints
                .entry(path)
                .or_default()
                .extend(schema_types);
        }
    }

    /// Expression-level effects contribute their chart-default paths and
    /// type hints to the enclosing summary; all other effect fields stay
    /// with the expression site.
    pub(crate) fn absorb_effect_hints(&mut self, effects: &Effects) {
        self.chart_defaults
            .extend(effects.chart_default_paths.iter().cloned());
        self.add_type_hints(effects.type_hints.clone());
    }

    pub(crate) fn has_document_value_facts(&self) -> bool {
        !self.output_uses.is_empty()
            || !self.guard_path_meta.is_empty()
            || !self.type_hints.is_empty()
    }

    pub(crate) fn add_provenance(&mut self, provenance: ContractProvenance) {
        for output in &mut self.output_uses {
            output.meta.add_provenance_site(provenance.clone());
        }
        for meta in self.guard_path_meta.values_mut() {
            meta.add_provenance_site(provenance.clone());
        }
    }

    pub(crate) fn has_rendered_source_descendant(&self, path: &str) -> bool {
        self.output_uses.iter().any(|output| {
            output.is_rendered()
                && output_path::values_path_is_descendant(&output.source_expr, path)
        })
    }

    pub(crate) fn dependency_relevant_paths(&self) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();
        paths.extend(self.guard_path_meta.keys().cloned());
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
        rendered_sources.extend(self.guard_path_meta.keys().cloned());
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
        for output in self
            .output_uses
            .iter()
            .filter(|output| output.is_rendered())
            .cloned()
        {
            values.push(AbstractValue::for_output_path(
                output.source_expr,
                &output.relative_path,
                projected_output_meta(output.meta),
            ));
        }
        AbstractValue::merge_all(values)
            .map(|value| value.to_context_value())
            .and_then(|value| AbstractValue::merge_context_values(vec![value]))
    }
}

fn projected_output_meta(mut meta: HelperOutputMeta) -> HelperOutputMeta {
    if meta.predicates.len() == 1
        && meta
            .predicates
            .iter()
            .next()
            .is_some_and(BTreeSet::is_empty)
    {
        meta.predicates.clear();
    }
    if meta.predicates.is_empty() && !meta.defaulted && !meta.require_sibling_guards {
        meta.related_sources.clear();
        meta.sibling_sources.clear();
    }
    meta
}

fn helper_output_use_identity_matches(
    existing: &HelperFragmentOutputUse,
    output: &HelperFragmentOutputUse,
) -> bool {
    existing.rendered == output.rendered
        && existing.source_expr == output.source_expr
        && existing.meta.sibling_sources == output.meta.sibling_sources
        && existing.meta.require_sibling_guards == output.meta.require_sibling_guards
        && (!existing.is_structured_output() || existing.meta.provenance == output.meta.provenance)
        && (!existing.rendered
            || (existing.relative_path == output.relative_path
                && existing.kind == output.kind
                && existing.encoded == output.encoded))
}

fn prune_pathless_sibling_predicates_for_meta(
    source_expr: &str,
    relative_path_empty: bool,
    meta: &mut HelperOutputMeta,
    pathless_sources: &BTreeSet<String>,
    record_sibling_sources: bool,
) {
    if record_sibling_sources {
        meta.note_sibling_sources(source_expr, pathless_sources);
    }
    meta.rewrite_predicate_branches(|meta, predicate_branch| {
        let has_truthy_sibling = predicate_branch.iter().any(|predicate| {
            predicate_truthy_path(predicate).is_some_and(|path| {
                unrelated_source_sibling(source_expr, path, meta, pathless_sources)
            })
        });
        if !meta.defaulted || relative_path_empty {
            predicate_branch.retain(|predicate| {
                !predicate_truthy_path(predicate).is_some_and(|path| {
                    unrelated_source_sibling(source_expr, path, meta, pathless_sources)
                })
            });
        }
        let has_source_truthy = predicate_branch.iter().any(|predicate| {
            predicate_truthy_path(predicate).is_some_and(|path| path == source_expr)
        });
        if meta.defaulted && (has_source_truthy || has_truthy_sibling || !relative_path_empty) {
            predicate_branch.retain(|predicate| {
                predicate_not_truthy_path(predicate).is_none_or(|path| path != source_expr)
            });
        }
    });
}

fn unrelated_source_sibling(
    source_expr: &str,
    predicate_path: &str,
    meta: &HelperOutputMeta,
    sibling_sources: &BTreeSet<String>,
) -> bool {
    if meta.related_sources.contains(predicate_path) {
        return false;
    }
    predicate_path != source_expr
        && sibling_sources.contains(predicate_path)
        && !values_paths_are_related(predicate_path, source_expr)
}

pub(crate) fn values_paths_are_related(left: &str, right: &str) -> bool {
    values_path_root(left) == values_path_root(right)
        || output_path::values_path_is_descendant(left, right)
        || output_path::values_path_is_descendant(right, left)
}

fn values_path_root(path: &str) -> &str {
    path.split('.').next().unwrap_or(path)
}

#[cfg(test)]
#[path = "tests/helper_summary.rs"]
mod tests;
