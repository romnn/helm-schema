use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::helper_binding::HelperBinding;
use crate::helper_summary::{HelperOutputMeta, HelperSummary};
use crate::output_path;
use crate::predicate::Predicate;
use crate::{ValueKind, YamlPath};

pub(crate) fn extend_nested_scalar_render(
    analysis: &mut HelperSummary,
    mut nested: HelperSummary,
    active_output_predicates: &BTreeSet<Predicate>,
) {
    convert_fragment_outputs_to_dependency_outputs(&mut nested);
    for meta in nested.output.values_mut() {
        meta.add_predicates(active_output_predicates.iter().cloned());
    }
    analysis.extend(nested);
}

pub(crate) fn extend_nested_fragment_render(
    analysis: &mut HelperSummary,
    nested: HelperSummary,
    active_output_predicates: &BTreeSet<Predicate>,
    expression_kind: ValueKind,
) {
    for (output, mut meta) in nested.output {
        meta.add_predicates(active_output_predicates.iter().cloned());
        analysis.add_output_meta(output, meta);
    }
    for output in nested.fragment_output {
        analysis.add_fragment_output_use(
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
        analysis.fragment_output_uses.push(output);
    }
    analysis.dependency_paths.extend(
        nested
            .dependency_paths
            .into_iter()
            .filter(|path| !path.trim().is_empty()),
    );
    analysis.add_dependency_meta_map(nested.dependency_meta);
    analysis.guard_paths.extend(nested.guard_paths);
    extend_type_hints(&mut analysis.type_hints, nested.type_hints);
    analysis.suppress_roots.extend(nested.suppress_roots);
    analysis.chart_defaults.extend(nested.chart_defaults);
}

pub(crate) fn convert_fragment_outputs_to_dependency_outputs(analysis: &mut HelperSummary) {
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
    analysis: &mut HelperSummary,
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
    use std::collections::{BTreeSet, HashMap};

    use super::mark_suppressed_roots_for_bound_outputs;
    use crate::helper_binding::HelperBinding;
    use crate::helper_summary::HelperSummary;

    #[test]
    fn suppresses_bound_root_when_helper_outputs_descendant_path() {
        let mut analysis = HelperSummary::default();
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
        let mut analysis = HelperSummary::default();
        analysis.add_output("serviceAccount".to_string(), &BTreeSet::new(), false);
        let bindings = HashMap::from([(
            "config".to_string(),
            HelperBinding::ValuesPath("serviceAccount".to_string()),
        )]);

        mark_suppressed_roots_for_bound_outputs(&mut analysis, &bindings);

        assert!(analysis.suppress_roots.is_empty());
    }
}
