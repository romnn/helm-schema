use std::collections::{BTreeMap, BTreeSet};

use crate::helper_analysis::{BoundHelperAnalysis, HelperOutputMeta};
use crate::output_path;

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

fn remove_ancestor_paths(paths: BTreeSet<String>) -> BTreeSet<String> {
    paths
        .iter()
        .filter(|path| !output_path::values_path_has_descendant(path, &paths))
        .cloned()
        .collect()
}
