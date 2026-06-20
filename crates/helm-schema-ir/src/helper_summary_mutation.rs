use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::helper_summary::{HelperOutputMeta, HelperSummary};
use crate::output_path;
use crate::predicate::Predicate;

pub(crate) fn extend_nested_scalar_render(
    analysis: &mut HelperSummary,
    mut nested: HelperSummary,
    active_output_predicates: &BTreeSet<Predicate>,
) {
    convert_fragment_outputs_to_dependency_outputs(&mut nested);
    nested.add_predicates_to_outputs(active_output_predicates);
    analysis.extend(nested);
}

pub(crate) fn extend_nested_fragment_render(
    analysis: &mut HelperSummary,
    nested: HelperSummary,
    active_output_predicates: &BTreeSet<Predicate>,
) {
    for (output, mut meta) in nested.output_path_meta() {
        meta.add_predicates(active_output_predicates.iter().cloned());
        analysis.add_output_meta(output, meta);
    }
    let direct_dependency_paths = nested.direct_dependency_paths();
    let dependency_meta = nested.dependency_path_meta();
    let guard_paths = nested.guard_paths();
    let type_hints = nested.type_hints();
    for mut output in nested.fragment_output_uses {
        output
            .meta
            .add_predicates(active_output_predicates.iter().cloned());
        analysis.fragment_output_uses.push(output);
    }
    for path in direct_dependency_paths {
        analysis.add_dependency_path(path);
    }
    analysis.add_dependency_meta_map(dependency_meta);
    for path in guard_paths {
        analysis.add_guard_path(path);
    }
    analysis.add_type_hints(type_hints);
    analysis.suppress_roots.extend(nested.suppress_roots);
    analysis.chart_defaults.extend(nested.chart_defaults);
}

pub(crate) fn convert_fragment_outputs_to_dependency_outputs(analysis: &mut HelperSummary) {
    let fragment_output_uses = std::mem::take(&mut analysis.fragment_output_uses);
    for output in fragment_output_uses {
        analysis.add_output_meta(output.source_expr, output.meta);
    }
}

pub(crate) fn mark_suppressed_roots_for_bound_outputs(
    analysis: &mut HelperSummary,
    bindings: &HashMap<String, AbstractValue>,
) {
    let rendered_sources: BTreeSet<String> = analysis
        .output_path_meta()
        .into_keys()
        .chain(analysis.guard_paths())
        .collect();
    for binding in bindings.values() {
        let AbstractValue::ValuesPath(root) = binding else {
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

    use super::mark_suppressed_roots_for_bound_outputs;
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

        mark_suppressed_roots_for_bound_outputs(&mut analysis, &bindings);

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

        mark_suppressed_roots_for_bound_outputs(&mut analysis, &bindings);

        assert!(analysis.suppress_roots.is_empty());
    }
}
