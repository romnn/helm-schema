use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ValueKind;
use crate::expression_analysis::{
    resolved_default_fallback_paths_for_text, resolved_string_transform_paths_for_text,
    resolved_type_is_paths_for_text, set_default_chart_paths_for_text,
};
use crate::fragment_assignment::{apply_local_set_mutations, parse_helper_assignment};
use crate::fragment_binding::FragmentBinding;
use crate::fragment_binding_projection::fragment_strings;
use crate::fragment_classification::is_fragment_expr;
use crate::fragment_expr_eval::{
    FragmentEvalContext, fragment_binding_from_expr,
    fragment_binding_from_text_with_helper_context, helper_binding_from_expr_with_fragment_locals,
};
use crate::helper_binding::HelperBinding;
use crate::helper_binding_projection::{helper_strings, helper_to_fragment_binding};
use crate::helper_output_projection::helper_binding_output_meta;
use crate::helper_summary::HelperOutputMeta;
use crate::helper_summary_mutation::{
    extend_nested_fragment_render, extend_nested_scalar_render, extend_type_hints,
};
use crate::helper_summary_projection::{
    helper_dependency_meta_from_summary, helper_summary_dependency_paths,
};
use crate::helper_walk_state::HelperValuesWalkState;
use crate::local_projection::{
    direct_bound_paths_from_text_in_context, local_bound_paths_from_text,
    local_default_paths_from_text, local_output_meta_from_text, local_rendered_paths_from_text,
};
use crate::predicate::Predicate;
use crate::template_expr_cache::parse_expr_text;

pub(crate) fn collect_helper_value_expression(
    text: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    active_output_predicates: &BTreeSet<Predicate>,
    state: &mut HelperValuesWalkState<'_, '_>,
) {
    if let Some(assignment) = parse_helper_assignment(text) {
        collect_assignment_bound_helper_values(
            &assignment.variable,
            &assignment.rhs,
            text,
            bindings,
            current_dot,
            active_output_predicates,
            state,
        );
        return;
    }

    let current_dot_fragment = current_dot.map(helper_to_fragment_binding);
    let mut seen_set = HashSet::new();
    if apply_local_set_mutations(
        text,
        state.local_bindings,
        current_dot_fragment.as_ref(),
        state.context,
        &mut seen_set,
    ) {
        let set_default_paths = set_default_chart_paths_for_text(text, Some(bindings), current_dot);
        state.analysis.chart_defaults.extend(set_default_paths);
        return;
    }

    let direct_outputs = direct_bound_paths_from_text_in_context(text, bindings, current_dot);
    let fallback_paths =
        resolved_default_fallback_paths_for_text(text, Some(bindings), current_dot);
    extend_type_hints(
        &mut state.analysis.type_hints,
        resolved_type_is_paths_for_text(text, Some(bindings), current_dot),
    );
    extend_type_hints(
        &mut state.analysis.type_hints,
        resolved_string_transform_paths_for_text(text, Some(bindings), current_dot),
    );
    let local_outputs = local_rendered_paths_from_text(text, state.local_bindings);
    let local_fallback_paths = local_default_paths_from_text(text, state.local_default_paths);
    let local_meta_by_path =
        local_output_meta_from_text(text, state.local_bindings, state.local_output_meta);
    let expression_kind = if is_fragment_expr(text) {
        ValueKind::Fragment
    } else {
        ValueKind::Scalar
    };
    if expression_kind == ValueKind::Scalar {
        for output in direct_outputs {
            let meta = HelperOutputMeta::with_predicates(
                active_output_predicates,
                fallback_paths.contains(&output),
            );
            state.analysis.add_output_meta(output, meta);
        }
        let mut local_output_sources = local_outputs;
        local_output_sources.extend(local_meta_by_path.keys().cloned());
        for output in local_output_sources {
            let mut meta = local_meta_by_path.get(&output).cloned().unwrap_or_default();
            meta.add_predicates(active_output_predicates.iter().cloned());
            meta.defaulted |= local_fallback_paths.contains(&output);
            state.analysis.add_output_meta(output, meta);
        }
        let mut string_seen = state.seen.clone();
        state
            .analysis
            .string_output
            .extend(string_outputs_from_text(
                text,
                bindings,
                current_dot,
                state.local_bindings,
                state.context,
                &mut string_seen,
            ));
    }
    let nested = state
        .context
        .helper_summaries()
        .summarize_bound_helper_calls(
            text,
            Some(bindings),
            current_dot,
            state.local_bindings,
            state.context,
            state.seen,
        );
    if expression_kind == ValueKind::Fragment {
        extend_nested_fragment_render(
            state.analysis,
            nested,
            active_output_predicates,
            expression_kind,
        );
    } else {
        extend_nested_scalar_render(state.analysis, nested, active_output_predicates);
    }
    let set_default_paths = set_default_chart_paths_for_text(text, Some(bindings), current_dot);
    state.analysis.chart_defaults.extend(set_default_paths);
}

fn string_outputs_from_text(
    text: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    local_bindings: &HashMap<String, FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> BTreeSet<String> {
    let mut strings = BTreeSet::new();
    let current_dot_fragment = current_dot.map(helper_to_fragment_binding);
    for expr in parse_expr_text(text) {
        if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
            &expr,
            local_bindings,
            Some(bindings),
            current_dot,
            context,
            seen,
        ) {
            strings.extend(helper_strings(&binding));
            continue;
        }
        if let Some(binding) = fragment_binding_from_expr(
            &expr,
            local_bindings,
            current_dot_fragment.as_ref(),
            context,
            seen,
        ) {
            strings.extend(fragment_strings(&binding));
        }
    }
    strings
}

fn collect_assignment_bound_helper_values(
    var: &str,
    rhs: &str,
    text: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    active_output_predicates: &BTreeSet<Predicate>,
    state: &mut HelperValuesWalkState<'_, '_>,
) {
    let set_default_paths = set_default_chart_paths_for_text(text, Some(bindings), current_dot);
    state.analysis.chart_defaults.extend(set_default_paths);
    extend_type_hints(
        &mut state.analysis.type_hints,
        resolved_type_is_paths_for_text(rhs, Some(bindings), current_dot),
    );
    extend_type_hints(
        &mut state.analysis.type_hints,
        resolved_string_transform_paths_for_text(rhs, Some(bindings), current_dot),
    );

    let current_dot_fragment = current_dot.map(helper_to_fragment_binding);
    let mut seen_set = HashSet::new();
    if apply_local_set_mutations(
        text,
        state.local_bindings,
        current_dot_fragment.as_ref(),
        state.context,
        &mut seen_set,
    ) {
        return;
    }

    let fallback_paths = resolved_default_fallback_paths_for_text(rhs, Some(bindings), current_dot);
    let local_fallback_paths = local_default_paths_from_text(rhs, state.local_default_paths);
    let local_outputs = local_bound_paths_from_text(rhs, state.local_bindings);
    let local_meta_by_path =
        local_output_meta_from_text(rhs, state.local_bindings, state.local_output_meta);
    let mut result_seen = state.seen.clone();
    let result_meta_by_path = helper_binding_output_meta_from_text(
        rhs,
        state.local_bindings,
        bindings,
        current_dot,
        state.context,
        &mut result_seen,
    );
    let nested = state
        .context
        .helper_summaries()
        .summarize_bound_helper_calls(
            rhs,
            Some(bindings),
            current_dot,
            state.local_bindings,
            state.context,
            state.seen,
        );
    state
        .analysis
        .chart_defaults
        .extend(nested.chart_defaults.clone());
    extend_type_hints(&mut state.analysis.type_hints, nested.type_hints.clone());
    state
        .analysis
        .dependency_paths
        .extend(helper_summary_dependency_paths(&nested));
    state
        .analysis
        .add_dependency_meta_map(helper_dependency_meta_from_summary(&nested));

    let rhs_output_meta = rhs_output_meta(
        &local_outputs,
        &fallback_paths,
        &local_fallback_paths,
        &local_meta_by_path,
        &result_meta_by_path,
        active_output_predicates,
    );

    let mut seen_rhs = HashSet::new();
    if let Some(binding) = fragment_binding_from_text_with_helper_context(
        rhs,
        state.local_bindings,
        Some(bindings),
        current_dot,
        state.context,
        &mut seen_rhs,
    ) {
        state.local_bindings.insert(var.to_string(), binding);
    }
    let mut defaulted_paths = fallback_paths;
    defaulted_paths.extend(local_fallback_paths);
    defaulted_paths.extend(
        nested
            .output
            .iter()
            .filter(|(_path, meta)| meta.defaulted)
            .map(|(path, _meta)| path.clone()),
    );
    defaulted_paths.extend(
        nested
            .fragment_output_uses
            .iter()
            .filter(|output| output.meta.defaulted)
            .map(|output| output.source_expr.clone()),
    );
    if defaulted_paths.is_empty() {
        state.local_default_paths.remove(var);
    } else {
        state
            .local_default_paths
            .insert(var.to_string(), defaulted_paths);
    }
    if rhs_output_meta.is_empty() {
        state.local_output_meta.remove(var);
    } else {
        state
            .local_output_meta
            .insert(var.to_string(), rhs_output_meta);
    }
}

fn rhs_output_meta(
    local_outputs: &BTreeSet<String>,
    fallback_paths: &BTreeSet<String>,
    local_fallback_paths: &BTreeSet<String>,
    local_meta_by_path: &BTreeMap<String, HelperOutputMeta>,
    result_meta_by_path: &BTreeMap<String, HelperOutputMeta>,
    active_output_predicates: &BTreeSet<Predicate>,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut rhs_output_meta: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
    for (output, meta) in result_meta_by_path {
        let mut meta = meta.clone();
        meta.add_predicates(active_output_predicates.iter().cloned());
        meta.defaulted |= fallback_paths.contains(output);
        rhs_output_meta
            .entry(output.clone())
            .or_default()
            .merge(meta);
    }
    for output in local_outputs {
        let mut meta = local_meta_by_path.get(output).cloned().unwrap_or_default();
        meta.add_predicates(active_output_predicates.iter().cloned());
        meta.defaulted |= local_fallback_paths.contains(output);
        rhs_output_meta
            .entry(output.clone())
            .or_default()
            .merge(meta);
    }
    rhs_output_meta
}

fn helper_binding_output_meta_from_text(
    text: &str,
    local_bindings: &HashMap<String, FragmentBinding>,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut out: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
    for expr in parse_expr_text(text) {
        if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
            &expr,
            local_bindings,
            Some(bindings),
            current_dot,
            context,
            seen,
        ) {
            for (path, meta) in helper_binding_output_meta(&binding) {
                out.entry(path).or_default().merge(meta);
            }
        }
    }
    out
}
