use std::collections::{BTreeMap, BTreeSet};

use crate::helper_summary::{HelperOutputMeta, HelperSummary};
use crate::output_path;

pub(crate) fn helper_summary_dependency_paths(summary: &HelperSummary) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = summary
        .output
        .keys()
        .chain(summary.dependency_paths.iter())
        .chain(summary.dependency_meta.keys())
        .chain(summary.guard_paths.iter())
        .chain(summary.fragment_output.iter())
        .chain(summary.type_hints.keys())
        .cloned()
        .collect();
    out.extend(
        summary
            .fragment_output_uses
            .iter()
            .filter(|output| !output.source_expr.trim().is_empty())
            .map(|output| output.source_expr.clone()),
    );
    out.retain(|path| !path.trim().is_empty());
    remove_ancestor_paths(out)
}

pub(crate) fn helper_summary_condition_paths(summary: &HelperSummary) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = summary
        .output
        .keys()
        .chain(summary.dependency_meta.keys())
        .chain(summary.guard_paths.iter())
        .chain(summary.fragment_output.iter())
        .chain(summary.type_hints.keys())
        .cloned()
        .collect();
    out.extend(
        summary
            .fragment_output_uses
            .iter()
            .filter(|output| !output.source_expr.trim().is_empty())
            .map(|output| output.source_expr.clone()),
    );
    out.retain(|path| !path.trim().is_empty());
    remove_ancestor_paths(out)
}

pub(crate) fn helper_output_meta_from_summary(
    summary: &HelperSummary,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut out = summary.output.clone();
    for output in &summary.fragment_output_uses {
        if output.source_expr.trim().is_empty() {
            continue;
        }
        out.entry(output.source_expr.clone())
            .or_default()
            .merge_ref(&output.meta);
    }
    for path in &summary.fragment_output {
        if path.trim().is_empty() {
            continue;
        }
        out.entry(path.clone()).or_default();
    }
    out
}

pub(crate) fn helper_dependency_meta_from_summary(
    summary: &HelperSummary,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut out = summary.dependency_meta.clone();
    for (path, meta) in helper_output_meta_from_summary(summary) {
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
