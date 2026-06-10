use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::{ValueKind, YamlPath};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HelperOutputMeta {
    pub(crate) guards: BTreeSet<String>,
    pub(crate) defaulted: bool,
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
            let entry = self.output.entry(path).or_default();
            entry.guards.extend(meta.guards);
            entry.defaulted |= meta.defaulted;
        }
        self.fragment_output.extend(other.fragment_output);
        self.fragment_output_uses.extend(other.fragment_output_uses);
        self.dependency_paths.extend(other.dependency_paths);
        self.add_dependency_meta_map(other.dependency_meta);
        self.guard_paths.extend(other.guard_paths);
        for (path, schema_types) in other.type_hints {
            self.type_hints
                .entry(path)
                .or_default()
                .extend(schema_types);
        }
        self.suppress_roots.extend(other.suppress_roots);
        self.chart_defaults.extend(other.chart_defaults);
    }

    pub(crate) fn add_output(&mut self, path: String, guards: &BTreeSet<String>, defaulted: bool) {
        let entry = self.output.entry(path).or_default();
        entry.guards.extend(guards.iter().cloned());
        entry.defaulted |= defaulted;
    }

    pub(crate) fn add_output_meta(&mut self, path: String, meta: HelperOutputMeta) {
        let entry = self.output.entry(path).or_default();
        entry.guards.extend(meta.guards);
        entry.defaulted |= meta.defaulted;
    }

    pub(crate) fn add_dependency_meta_map(
        &mut self,
        meta_by_path: BTreeMap<String, HelperOutputMeta>,
    ) {
        for (path, meta) in meta_by_path {
            self.dependency_paths.insert(path.clone());
            let entry = self.dependency_meta.entry(path).or_default();
            entry.guards.extend(meta.guards);
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
            .map(|output| output.source_expr.clone()),
    );
    out
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
            .map(|output| output.source_expr.clone()),
    );
    out
}

pub(crate) fn helper_output_meta_from_analysis(
    analysis: &BoundHelperAnalysis,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut out = analysis.output.clone();
    for output in &analysis.fragment_output_uses {
        let entry = out.entry(output.source_expr.clone()).or_default();
        entry.guards.extend(output.meta.guards.iter().cloned());
        entry.defaulted |= output.meta.defaulted;
    }
    for path in &analysis.fragment_output {
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
        entry.guards.extend(meta.guards);
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
        entry.guards.extend(output.meta.guards);
        entry.defaulted |= output.meta.defaulted;
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
            path_entry.guards.extend(meta.guards);
            path_entry.defaulted |= meta.defaulted;
        }
    }
    base
}
