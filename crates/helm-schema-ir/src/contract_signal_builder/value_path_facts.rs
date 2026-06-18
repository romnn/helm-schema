use std::collections::{BTreeMap, BTreeSet};

use crate::contract_signals::{ContractPathSignals, ContractValuePathFacts};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RenderPathFacts {
    pub has_render_use: bool,
    pub has_self_guarded_render_use: bool,
    pub all_render_uses_self_guarded: bool,
    pub has_self_range_guard_render_use: bool,
}

impl Default for RenderPathFacts {
    fn default() -> Self {
        Self {
            has_render_use: false,
            has_self_guarded_render_use: false,
            all_render_uses_self_guarded: true,
            has_self_range_guard_render_use: false,
        }
    }
}

pub(super) fn build_contract_value_path_facts(
    render_facts: &BTreeMap<String, RenderPathFacts>,
    path_signals: &ContractPathSignals,
    nullable_value_paths: &BTreeSet<String>,
    paths_with_referenced_descendants: &BTreeSet<String>,
) -> BTreeMap<String, ContractValuePathFacts> {
    let mut paths = BTreeSet::new();
    paths.extend(render_facts.keys().cloned());
    paths.extend(path_signals.referenced_value_paths.iter().cloned());
    paths.extend(path_signals.ranged_value_paths.iter().cloned());
    paths.extend(path_signals.value_paths_used_as_fragment.iter().cloned());
    paths.extend(path_signals.partial_scalar_value_paths.iter().cloned());
    paths.extend(path_signals.guard_constraints_by_value_path.keys().cloned());
    paths.extend(path_signals.metadata_fields_by_value_path.keys().cloned());
    paths.extend(nullable_value_paths.iter().cloned());
    paths.extend(paths_with_referenced_descendants.iter().cloned());

    paths
        .into_iter()
        .map(|path| {
            let render_fact = render_facts.get(&path).cloned().unwrap_or_default();
            (
                path.clone(),
                ContractValuePathFacts {
                    has_referenced_descendants: paths_with_referenced_descendants.contains(&path),
                    used_as_fragment: path_signals.value_paths_used_as_fragment.contains(&path),
                    is_ranged_source: path_signals.ranged_value_paths.contains(&path),
                    is_partial_scalar_value_path: path_signals
                        .partial_scalar_value_paths
                        .contains(&path),
                    has_render_use: render_fact.has_render_use,
                    has_self_guarded_render_use: render_fact.has_self_guarded_render_use,
                    all_render_uses_self_guarded: render_fact.all_render_uses_self_guarded,
                    has_self_range_guard_render_use: render_fact.has_self_range_guard_render_use,
                    is_nullable: nullable_value_paths.contains(&path),
                },
            )
        })
        .collect()
}

pub(super) fn collect_paths_with_descendants(paths: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for path in paths {
        let mut segments: Vec<&str> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        while segments.len() > 1 {
            segments.pop();
            out.insert(segments.join("."));
        }
    }
    out
}
