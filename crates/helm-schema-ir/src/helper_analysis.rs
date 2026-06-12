use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::binding::{FragmentBinding, HelperBinding};
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

    pub(crate) fn merge(&mut self, other: Self) {
        self.predicates.extend(other.predicates);
        self.defaulted |= other.defaulted;
    }

    pub(crate) fn merge_ref(&mut self, other: &Self) {
        self.predicates.extend(other.predicates.iter().cloned());
        self.defaulted |= other.defaulted;
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

pub(crate) struct BoundHelperOutputProjection {
    pub(crate) output_values: BTreeMap<String, HelperOutputMeta>,
    pub(crate) fragment_output_values: Vec<String>,
    pub(crate) fragment_output_uses: Vec<HelperFragmentOutputUse>,
    pub(crate) dependency_values: BTreeMap<String, HelperOutputMeta>,
    pub(crate) guard_values: BTreeSet<String>,
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(crate) suppress_roots: BTreeSet<String>,
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
        self.add_output_meta(
            path,
            HelperOutputMeta::with_predicates(predicates, defaulted),
        );
    }

    pub(crate) fn add_output_meta(&mut self, path: String, meta: HelperOutputMeta) {
        if path.trim().is_empty() {
            return;
        }
        self.output.entry(path).or_default().merge(meta);
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
            self.dependency_meta.entry(path).or_default().merge(meta);
        }
    }

    pub(crate) fn extend_nested_scalar_render(
        &mut self,
        mut nested: Self,
        active_output_predicates: &BTreeSet<Predicate>,
    ) {
        convert_fragment_outputs_to_dependency_outputs(&mut nested);
        for meta in nested.output.values_mut() {
            meta.add_predicates(active_output_predicates.iter().cloned());
        }
        self.extend(nested);
    }

    pub(crate) fn extend_nested_fragment_render(
        &mut self,
        nested: Self,
        active_output_predicates: &BTreeSet<Predicate>,
        expression_kind: ValueKind,
    ) {
        for (output, mut meta) in nested.output {
            meta.add_predicates(active_output_predicates.iter().cloned());
            self.add_output_meta(output, meta);
        }
        for output in nested.fragment_output {
            self.add_fragment_output_use(
                output,
                YamlPath(Vec::new()),
                expression_kind,
                HelperOutputMeta::with_predicates(active_output_predicates, false),
            );
        }
        for mut output in nested.fragment_output_uses {
            output
                .meta
                .add_predicates(active_output_predicates.iter().cloned());
            self.fragment_output_uses.push(output);
        }
        self.dependency_paths.extend(
            nested
                .dependency_paths
                .into_iter()
                .filter(|path| !path.trim().is_empty()),
        );
        self.add_dependency_meta_map(nested.dependency_meta);
        self.guard_paths.extend(nested.guard_paths);
        extend_type_hints(&mut self.type_hints, nested.type_hints);
        self.suppress_roots.extend(nested.suppress_roots);
        self.chart_defaults.extend(nested.chart_defaults);
    }

    pub(crate) fn add_fragment_output_use(
        &mut self,
        source_expr: String,
        relative_path: YamlPath,
        kind: ValueKind,
        meta: HelperOutputMeta,
    ) {
        self.fragment_output_uses.push(HelperFragmentOutputUse {
            source_expr,
            relative_path,
            kind,
            meta,
        });
    }

    pub(crate) fn into_output_projection(
        self,
        output_kind: ValueKind,
    ) -> BoundHelperOutputProjection {
        let mut dependency_values: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        for path in self.dependency_paths {
            dependency_values.entry(path).or_default();
        }
        for (path, meta) in self.dependency_meta {
            dependency_values.entry(path).or_default().merge(meta);
        }

        let mut fragment_output_values = Vec::new();
        if output_kind == ValueKind::Fragment {
            fragment_output_values.extend(self.fragment_output);
            fragment_output_values.sort();
            fragment_output_values.dedup();
        }

        BoundHelperOutputProjection {
            output_values: self.output,
            fragment_output_values,
            fragment_output_uses: self.fragment_output_uses,
            dependency_values,
            guard_values: self.guard_paths,
            type_hints: self.type_hints,
            suppress_roots: self.suppress_roots,
            chart_defaults: self.chart_defaults,
        }
    }

    pub(crate) fn into_fragment_binding(mut self) -> Option<FragmentBinding> {
        let structured_sources = self.structured_fragment_sources();
        let rendered_sources = self.rendered_sources(&structured_sources);

        let mut bindings = Vec::new();
        if !self.string_output.is_empty() {
            bindings.push(FragmentBinding::StringSet(self.string_output.clone()));
        }
        for output in self.fragment_output_uses.drain(..) {
            bindings.push(FragmentBinding::for_output_path(
                output.source_expr,
                &output.relative_path,
            ));
        }
        for source in self.fragment_output {
            if !structured_sources.contains(&source)
                && !output_path::values_path_has_descendant(&source, &rendered_sources)
            {
                bindings.push(FragmentBinding::OutputSet([source].into_iter().collect()));
            }
        }
        for source in self.output.into_keys() {
            if !structured_sources.contains(&source)
                && !output_path::values_path_has_descendant(&source, &rendered_sources)
            {
                bindings.push(FragmentBinding::OutputSet([source].into_iter().collect()));
            }
        }
        FragmentBinding::merge_all(bindings)
    }

    pub(crate) fn into_helper_binding(mut self) -> Option<HelperBinding> {
        let structured_sources = self.structured_fragment_sources();
        let rendered_sources = self.rendered_sources(&structured_sources);

        let mut bindings = Vec::new();
        if !self.string_output.is_empty() {
            bindings.push(HelperBinding::StringSet(self.string_output.clone()));
        }
        for output in self.fragment_output_uses.drain(..) {
            bindings.push(HelperBinding::for_output_path(
                output.source_expr,
                &output.relative_path,
                output.meta,
            ));
        }
        for source in self.fragment_output {
            if !structured_sources.contains(&source)
                && !output_path::values_path_has_descendant(&source, &rendered_sources)
            {
                bindings.push(HelperBinding::PathSet([source].into_iter().collect()));
            }
        }
        for (source, meta) in self.output {
            if !structured_sources.contains(&source)
                && !output_path::values_path_has_descendant(&source, &rendered_sources)
            {
                bindings.push(HelperBinding::OutputSet(
                    [(source, meta)].into_iter().collect(),
                ));
            }
        }
        HelperBinding::merge_all(bindings)
    }

    fn structured_fragment_sources(&self) -> BTreeSet<String> {
        self.fragment_output_uses
            .iter()
            .map(|output| output.source_expr.clone())
            .collect()
    }

    fn rendered_sources(&self, structured_sources: &BTreeSet<String>) -> BTreeSet<String> {
        let mut rendered_sources = structured_sources.clone();
        rendered_sources.extend(self.fragment_output.iter().cloned());
        rendered_sources.extend(self.output.keys().cloned());
        rendered_sources
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
        out.entry(output.source_expr.clone())
            .or_default()
            .merge_ref(&output.meta);
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
        out.entry(path).or_default().merge(meta);
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
        analysis
            .output
            .entry(output.source_expr)
            .or_default()
            .merge(output.meta);
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
            entry.entry(path).or_default().merge(meta);
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    use super::{BoundHelperAnalysis, HelperOutputMeta, mark_suppressed_roots_for_bound_outputs};
    use crate::binding::{FragmentBinding, HelperBinding};
    use crate::predicate::{Predicate, PredicateAtom};
    use crate::{Guard, ValueKind, YamlPath};

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

    #[test]
    fn output_projection_preserves_helper_summary_fields() {
        let mut analysis = BoundHelperAnalysis::default();
        let meta = HelperOutputMeta {
            predicates: BTreeSet::from([Predicate::truthy_path("enabled".to_string())]),
            defaulted: true,
        };
        analysis.add_output_meta("image.tag".to_string(), meta.clone());
        analysis.fragment_output.insert("extraEnv".to_string());
        analysis.dependency_paths.insert("global".to_string());
        analysis
            .dependency_meta
            .insert("global.image.tag".to_string(), meta.clone());
        analysis.guard_paths.insert("service.enabled".to_string());
        analysis
            .type_hints
            .entry("image.tag".to_string())
            .or_default()
            .insert("string".to_string());
        analysis.suppress_roots.insert("image".to_string());
        analysis.chart_defaults.insert("nameOverride".to_string());

        let projection = analysis.into_output_projection(ValueKind::Fragment);

        assert_eq!(
            projection.output_values,
            BTreeMap::from([("image.tag".to_string(), meta.clone())])
        );
        assert_eq!(
            projection.fragment_output_values,
            vec!["extraEnv".to_string()]
        );
        assert_eq!(
            projection.dependency_values,
            BTreeMap::from([
                ("global".to_string(), HelperOutputMeta::default()),
                ("global.image.tag".to_string(), meta),
            ])
        );
        assert_eq!(
            projection.guard_values,
            BTreeSet::from(["service.enabled".to_string()])
        );
        assert_eq!(
            projection.type_hints,
            BTreeMap::from([(
                "image.tag".to_string(),
                BTreeSet::from(["string".to_string()])
            )])
        );
        assert_eq!(
            projection.suppress_roots,
            BTreeSet::from(["image".to_string()])
        );
        assert_eq!(
            projection.chart_defaults,
            BTreeSet::from(["nameOverride".to_string()])
        );
    }

    #[test]
    fn helper_binding_projection_preserves_structured_output_metadata() {
        let meta = HelperOutputMeta {
            predicates: BTreeSet::from([Predicate::truthy_path("enabled".to_string())]),
            defaulted: true,
        };
        let mut analysis = BoundHelperAnalysis::default();
        analysis.add_fragment_output_use(
            "podLabels".to_string(),
            YamlPath(vec!["app".to_string()]),
            ValueKind::Fragment,
            meta.clone(),
        );

        assert_eq!(
            analysis.into_helper_binding(),
            Some(HelperBinding::Dict(BTreeMap::from([(
                "app".to_string(),
                HelperBinding::OutputSet(BTreeMap::from([("podLabels".to_string(), meta)])),
            )])))
        );
    }

    #[test]
    fn fragment_binding_projection_preserves_structured_output_path() {
        let mut analysis = BoundHelperAnalysis::default();
        analysis.add_fragment_output_use(
            "podLabels".to_string(),
            YamlPath(vec!["app".to_string()]),
            ValueKind::Fragment,
            HelperOutputMeta::default(),
        );

        assert_eq!(
            analysis.into_fragment_binding(),
            Some(FragmentBinding::Dict(BTreeMap::from([(
                "app".to_string(),
                FragmentBinding::OutputSet(BTreeSet::from(["podLabels".to_string()])),
            )])))
        );
    }
}
