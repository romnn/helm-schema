use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::helper_summary::{HelperOutputMeta, HelperSummary};
use crate::predicate::Predicate;

pub(crate) fn extend_nested_scalar_render(
    analysis: &mut HelperSummary,
    nested: HelperSummary,
    active_output_predicates: &BTreeSet<Predicate>,
) {
    analysis
        .string_output
        .extend(nested.string_output.iter().cloned());
    analysis
        .suppress_roots
        .extend(nested.suppress_roots.iter().cloned());
    analysis
        .chart_defaults
        .extend(nested.chart_defaults.iter().cloned());

    for (path, mut facts) in nested.into_path_facts() {
        if let Some(mut meta) = facts.output.take() {
            meta.add_predicates(active_output_predicates.iter().cloned());
            analysis.merge_output_meta(path.clone(), meta);
        }
        if let Some(meta) = facts.dependency.take() {
            analysis.merge_dependency_meta(path.clone(), meta);
        }
        if facts.guard {
            analysis.add_guard_path(path.clone());
        }
        let type_hints = std::mem::take(&mut facts.type_hints);
        if !type_hints.is_empty() {
            analysis.merge_type_hints(path.clone(), type_hints);
        }
        for mut output in facts.take_fragment_output_uses(&path) {
            output
                .meta
                .add_predicates(active_output_predicates.iter().cloned());
            analysis.merge_output_meta(output.source_expr, output.meta);
        }
    }
}

pub(crate) fn extend_nested_fragment_render(
    analysis: &mut HelperSummary,
    nested: HelperSummary,
    active_output_predicates: &BTreeSet<Predicate>,
) {
    analysis
        .suppress_roots
        .extend(nested.suppress_roots.iter().cloned());
    analysis
        .chart_defaults
        .extend(nested.chart_defaults.iter().cloned());

    for (path, mut facts) in nested.into_path_facts() {
        if let Some(mut meta) = facts.output.take() {
            meta.add_predicates(active_output_predicates.iter().cloned());
            analysis.merge_output_meta(path.clone(), meta);
        }
        if facts.dependency.is_some() {
            analysis.add_dependency_path(path.clone());
        }
        if let Some(meta) = facts.dependency.take() {
            analysis.merge_dependency_meta(path.clone(), meta);
        }
        if facts.guard {
            analysis.add_guard_path(path.clone());
        }
        let type_hints = std::mem::take(&mut facts.type_hints);
        if !type_hints.is_empty() {
            analysis.merge_type_hints(path.clone(), type_hints);
        }
        for mut output in facts.take_fragment_output_uses(&path) {
            output
                .meta
                .add_predicates(active_output_predicates.iter().cloned());
            analysis.add_fragment_output_use(output);
        }
    }
}

pub(crate) fn merge_local_default_paths(
    mut base: HashMap<String, BTreeSet<String>>,
    other: HashMap<String, BTreeSet<String>>,
) -> HashMap<String, BTreeSet<String>> {
    base.retain(|key, base_paths| {
        let Some(other_paths) = other.get(key) else {
            return false;
        };
        base_paths.extend(other_paths.iter().cloned());
        true
    });
    base
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
    use std::collections::{BTreeSet, HashMap};
    use test_util::prelude::sim_assert_eq;

    use crate::abstract_value::AbstractValue;
    use crate::helper_summary::{HelperOutputMeta, HelperSummary};

    #[test]
    fn suppresses_bound_root_when_helper_outputs_descendant_path() {
        let mut analysis = HelperSummary::default();
        analysis.add_output_meta(
            "serviceAccount.name".to_string(),
            HelperOutputMeta::default(),
        );
        let bindings = HashMap::from([(
            "config".to_string(),
            AbstractValue::ValuesPath("serviceAccount".to_string()),
        )]);

        analysis.mark_suppressed_roots_for_bound_outputs(&bindings);

        sim_assert_eq!(
            have: analysis.suppress_roots,
            want: BTreeSet::from(["serviceAccount".to_string()])
        );
    }

    #[test]
    fn does_not_suppress_bound_root_for_exact_root_output() {
        let mut analysis = HelperSummary::default();
        analysis.add_output_meta("serviceAccount".to_string(), HelperOutputMeta::default());
        let bindings = HashMap::from([(
            "config".to_string(),
            AbstractValue::ValuesPath("serviceAccount".to_string()),
        )]);

        analysis.mark_suppressed_roots_for_bound_outputs(&bindings);

        assert!(analysis.suppress_roots.is_empty());
    }

    #[test]
    fn merge_local_default_paths_intersects_branch_presence() {
        let base = HashMap::from([
            (
                "serviceAccount".to_string(),
                BTreeSet::from(["left.default".to_string()]),
            ),
            (
                "leftOnly".to_string(),
                BTreeSet::from(["left.only".to_string()]),
            ),
        ]);
        let other = HashMap::from([
            (
                "serviceAccount".to_string(),
                BTreeSet::from(["right.default".to_string()]),
            ),
            (
                "rightOnly".to_string(),
                BTreeSet::from(["right.only".to_string()]),
            ),
        ]);

        let merged = super::merge_local_default_paths(base, other);

        sim_assert_eq!(
            have: merged,
            want: HashMap::from([(
                "serviceAccount".to_string(),
                BTreeSet::from(["left.default".to_string(), "right.default".to_string()])
            )])
        );
    }
}
