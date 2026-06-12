use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::binding::HelperBinding;
use crate::output_path;
use crate::predicate::Predicate;
use crate::{Guard, ValueKind, YamlPath};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HelperOutputMeta {
    pub(crate) predicates: BTreeSet<Predicate>,
    pub(crate) defaulted: bool,
}

impl HelperOutputMeta {
    pub(crate) fn with_predicates(predicates: &BTreeSet<Predicate>, defaulted: bool) -> Self {
        Self {
            predicates: predicates.clone(),
            defaulted,
        }
    }

    pub(crate) fn add_predicates(&mut self, predicates: impl IntoIterator<Item = Predicate>) {
        self.predicates.extend(predicates);
    }

    pub(crate) fn compatibility_guards(&self, source_expr: &str) -> Vec<Guard> {
        let mut guards = Vec::new();
        for predicate in &self.predicates {
            for guard in predicate.compatibility_guards() {
                if !guards.contains(&guard) {
                    guards.push(guard);
                }
            }
        }
        if self.defaulted {
            let default_guard = Guard::Default {
                path: source_expr.to_string(),
            };
            if !guards.contains(&default_guard) {
                guards.push(default_guard);
            }
        }
        guards
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HelperFragmentOutputUse {
    pub(crate) source_expr: String,
    pub(crate) relative_path: YamlPath,
    pub(crate) kind: ValueKind,
    pub(crate) meta: HelperOutputMeta,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BoundHelperAnalysis {
    pub(crate) output: BTreeMap<String, HelperOutputMeta>,
    pub(crate) fragment_output: BTreeSet<String>,
    pub(crate) fragment_output_uses: Vec<HelperFragmentOutputUse>,
    pub(crate) string_output: BTreeSet<String>,
    pub(crate) dependency_paths: BTreeSet<String>,
    pub(crate) dependency_meta: BTreeMap<String, HelperOutputMeta>,
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
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

impl BoundHelperAnalysis {
    pub(crate) fn extend(&mut self, other: Self) {
        for (path, meta) in other.output {
            self.add_output_meta(path, meta);
        }
        self.fragment_output.extend(
            other
                .fragment_output
                .into_iter()
                .filter(|path| !path.trim().is_empty()),
        );
        self.fragment_output_uses.extend(
            other
                .fragment_output_uses
                .into_iter()
                .filter(|output| !output.source_expr.trim().is_empty()),
        );
        self.string_output.extend(other.string_output);
        self.dependency_paths.extend(
            other
                .dependency_paths
                .into_iter()
                .filter(|path| !path.trim().is_empty()),
        );
        self.add_dependency_meta_map(other.dependency_meta);
        self.guard_paths.extend(
            other
                .guard_paths
                .into_iter()
                .filter(|path| !path.trim().is_empty()),
        );
        for (path, schema_types) in other.type_hints {
            self.type_hints
                .entry(path)
                .or_default()
                .extend(schema_types);
        }
        self.suppress_roots.extend(other.suppress_roots);
        self.chart_defaults.extend(other.chart_defaults);
    }

    pub(crate) fn add_output(
        &mut self,
        path: String,
        predicates: &BTreeSet<Predicate>,
        defaulted: bool,
    ) {
        if path.trim().is_empty() {
            return;
        }
        let entry = self.output.entry(path).or_default();
        entry.predicates.extend(predicates.iter().cloned());
        entry.defaulted |= defaulted;
    }

    pub(crate) fn add_output_meta(&mut self, path: String, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        let entry = self.output.entry(path).or_default();
        entry.predicates.extend(meta.predicates);
        entry.defaulted |= meta.defaulted;
    }

    pub(crate) fn add_dependency_meta_map(
        &mut self,
        meta_by_path: BTreeMap<String, HelperOutputMeta>,
    ) {
        for (path, meta) in meta_by_path {
            if path.trim().is_empty() {
                continue;
            }
            self.dependency_paths.insert(path.clone());
            let entry = self.dependency_meta.entry(path).or_default();
            entry.predicates.extend(meta.predicates);
            entry.defaulted |= meta.defaulted;
        }
    }
}

pub(crate) fn bound_helper_dependency_paths(analysis: &BoundHelperAnalysis) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = analysis
        .output
        .keys()
        .chain(analysis.dependency_paths.iter())
        .chain(analysis.dependency_meta.keys())
        .chain(analysis.guard_paths.iter())
        .chain(analysis.fragment_output.iter())
        .chain(analysis.type_hints.keys())
        .cloned()
        .collect();
    out.extend(
        analysis
            .fragment_output_uses
            .iter()
            .filter(|output| !output.source_expr.trim().is_empty())
            .map(|output| output.source_expr.clone()),
    );
    out.retain(|path| !path.trim().is_empty());
    remove_ancestor_paths(out)
}

pub(crate) fn bound_helper_condition_paths(analysis: &BoundHelperAnalysis) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = analysis
        .output
        .keys()
        .chain(analysis.dependency_meta.keys())
        .chain(analysis.guard_paths.iter())
        .chain(analysis.fragment_output.iter())
        .chain(analysis.type_hints.keys())
        .cloned()
        .collect();
    out.extend(
        analysis
            .fragment_output_uses
            .iter()
            .filter(|output| !output.source_expr.trim().is_empty())
            .map(|output| output.source_expr.clone()),
    );
    out.retain(|path| !path.trim().is_empty());
    remove_ancestor_paths(out)
}

fn remove_ancestor_paths(paths: BTreeSet<String>) -> BTreeSet<String> {
    paths
        .iter()
        .filter(|path| !output_path::values_path_has_descendant(path, &paths))
        .cloned()
        .collect()
}

pub(crate) fn helper_output_meta_from_analysis(
    analysis: &BoundHelperAnalysis,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut out = analysis.output.clone();
    for output in &analysis.fragment_output_uses {
        if output.source_expr.trim().is_empty() {
            continue;
        }
        let entry = out.entry(output.source_expr.clone()).or_default();
        entry
            .predicates
            .extend(output.meta.predicates.iter().cloned());
        entry.defaulted |= output.meta.defaulted;
    }
    for path in &analysis.fragment_output {
        if path.trim().is_empty() {
            continue;
        }
        out.entry(path.clone()).or_default();
    }
    out
}

pub(crate) fn helper_dependency_meta_from_analysis(
    analysis: &BoundHelperAnalysis,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut out = analysis.dependency_meta.clone();
    for (path, meta) in helper_output_meta_from_analysis(analysis) {
        let entry = out.entry(path).or_default();
        entry.predicates.extend(meta.predicates);
        entry.defaulted |= meta.defaulted;
    }
    out
}

pub(crate) fn convert_fragment_outputs_to_dependency_outputs(analysis: &mut BoundHelperAnalysis) {
    let fragment_output = std::mem::take(&mut analysis.fragment_output);
    for source_expr in fragment_output {
        analysis.add_output(source_expr, &BTreeSet::new(), false);
    }

    let fragment_output_uses = std::mem::take(&mut analysis.fragment_output_uses);
    for output in fragment_output_uses {
        let entry = analysis.output.entry(output.source_expr).or_default();
        entry.predicates.extend(output.meta.predicates);
        entry.defaulted |= output.meta.defaulted;
    }
}

pub(crate) fn mark_suppressed_roots_for_bound_outputs(
    analysis: &mut BoundHelperAnalysis,
    bindings: &HashMap<String, HelperBinding>,
) {
    let rendered_sources: BTreeSet<String> = analysis
        .output
        .keys()
        .chain(analysis.guard_paths.iter())
        .cloned()
        .collect();
    for binding in bindings.values() {
        let HelperBinding::ValuesPath(root) = binding else {
            continue;
        };
        if output_path::values_path_has_descendant(root, &rendered_sources) {
            analysis.suppress_roots.insert(root.clone());
        }
    }
}

pub(crate) fn merge_local_default_paths(
    mut base: HashMap<String, BTreeSet<String>>,
    other: HashMap<String, BTreeSet<String>>,
) -> HashMap<String, BTreeSet<String>> {
    for (key, paths) in other {
        base.entry(key).or_default().extend(paths);
    }
    base
}

pub(crate) fn extend_type_hints(
    target: &mut BTreeMap<String, BTreeSet<String>>,
    hints: BTreeMap<String, BTreeSet<String>>,
) {
    for (path, schema_types) in hints {
        target.entry(path).or_default().extend(schema_types);
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

pub(crate) fn merge_helper_output_meta_maps(
    mut base: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    other: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
) -> HashMap<String, BTreeMap<String, HelperOutputMeta>> {
    for (var, meta_by_path) in other {
        let entry = base.entry(var).or_default();
        for (path, meta) in meta_by_path {
            let path_entry = entry.entry(path).or_default();
            path_entry.predicates.extend(meta.predicates);
            path_entry.defaulted |= meta.defaulted;
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use super::{BoundHelperAnalysis, HelperOutputMeta, mark_suppressed_roots_for_bound_outputs};
    use crate::Guard;
    use crate::binding::HelperBinding;
    use crate::predicate::{Predicate, PredicateAtom};

    #[test]
    fn helper_output_meta_projects_predicates_at_compatibility_boundary() {
        let meta = HelperOutputMeta {
            predicates: BTreeSet::from([Predicate::Not(Box::new(Predicate::Atom(
                PredicateAtom::Truthy {
                    path: "feature.enabled".to_string(),
                },
            )))]),
            defaulted: true,
        };

        assert_eq!(
            meta.compatibility_guards("serviceAccount.name"),
            vec![
                Guard::Not {
                    path: "feature.enabled".to_string(),
                },
                Guard::Default {
                    path: "serviceAccount.name".to_string(),
                },
            ]
        );
    }

    #[test]
    fn suppresses_bound_root_when_helper_outputs_descendant_path() {
        let mut analysis = BoundHelperAnalysis::default();
        analysis.add_output("serviceAccount.name".to_string(), &BTreeSet::new(), false);
        let bindings = HashMap::from([(
            "config".to_string(),
            HelperBinding::ValuesPath("serviceAccount".to_string()),
        )]);

        mark_suppressed_roots_for_bound_outputs(&mut analysis, &bindings);

        assert_eq!(
            analysis.suppress_roots,
            BTreeSet::from(["serviceAccount".to_string()])
        );
    }

    #[test]
    fn does_not_suppress_bound_root_for_exact_root_output() {
        let mut analysis = BoundHelperAnalysis::default();
        analysis.add_output("serviceAccount".to_string(), &BTreeSet::new(), false);
        let bindings = HashMap::from([(
            "config".to_string(),
            HelperBinding::ValuesPath("serviceAccount".to_string()),
        )]);

        mark_suppressed_roots_for_bound_outputs(&mut analysis, &bindings);

        assert!(analysis.suppress_roots.is_empty());
    }
}
