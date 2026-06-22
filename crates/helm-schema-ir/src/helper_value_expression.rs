use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ValueKind;
use crate::abstract_value::AbstractValue;
use crate::bound_helper_env::BoundHelperEnv;
use crate::expression_analysis::{
    resolved_string_transform_paths_for_exprs_with_fragment_locals,
    resolved_type_hint_paths_for_exprs_with_fragment_locals, set_default_chart_paths_for_exprs,
};
use crate::fragment_assignment::{
    apply_local_set_mutations_from_exprs, parse_helper_assignment_from_exprs,
};
use crate::helper_summary::{HelperOutputMeta, HelperSummary};
use crate::helper_walk_state::HelperValuesWalkState;
use crate::local_projection::{
    direct_bound_paths_from_exprs_in_context, local_bound_paths_from_expr,
    local_output_meta_from_exprs, local_rendered_paths_from_exprs,
};
use crate::predicate::Predicate;
use helm_schema_ast::TemplateExpr;

enum NestedRenderMode {
    Scalar,
    Fragment,
}

pub(crate) fn collect_helper_value_expression_from_exprs(
    exprs: &[TemplateExpr],
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    active_output_predicates: &BTreeSet<Predicate>,
    state: &mut HelperValuesWalkState<'_, '_>,
) {
    if let Some(assignment) = parse_helper_assignment_from_exprs(exprs) {
        collect_assignment_bound_helper_values(
            &assignment.variable,
            &assignment.rhs_expr,
            exprs,
            bindings,
            current_dot,
            active_output_predicates,
            state,
        );
        return;
    }

    let current_dot_fragment = current_dot.map(AbstractValue::to_context_value);
    let mut seen_set = HashSet::new();
    if apply_local_set_mutations_from_exprs(
        exprs,
        &mut state.locals.bindings,
        current_dot_fragment.as_ref(),
        state.context,
        &mut seen_set,
    ) {
        let set_default_paths =
            set_default_chart_paths_for_exprs(exprs, Some(bindings), current_dot);
        state.analysis.chart_defaults.extend(set_default_paths);
        return;
    }

    let direct_outputs = direct_bound_paths_from_exprs_in_context(exprs, bindings, current_dot);
    let helper_env = BoundHelperEnv::new(bindings, current_dot, state.context);
    let fallback_paths = helper_env.external_default_fallback_paths_in_exprs(exprs);
    state
        .analysis
        .add_type_hints(resolved_type_hint_paths_for_exprs_with_fragment_locals(
            exprs,
            Some(bindings),
            current_dot,
            &state.locals.bindings,
        ));
    state.analysis.add_type_hints(
        resolved_string_transform_paths_for_exprs_with_fragment_locals(
            exprs,
            Some(bindings),
            current_dot,
            &state.locals.bindings,
        ),
    );
    let local_outputs = local_rendered_paths_from_exprs(exprs, &state.locals.bindings);
    let local_fallback_paths =
        helper_env.local_default_fallback_paths_in_exprs(exprs, &state.locals.default_paths);
    let local_meta_by_path =
        local_output_meta_from_exprs(exprs, &state.locals.bindings, state.local_output_meta);
    let expression_kind = if exprs.iter().any(TemplateExpr::renders_yaml_fragment) {
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
        state
            .analysis
            .string_output
            .extend(helper_env.string_outputs_from_exprs(
                exprs,
                &state.locals.bindings,
                state.seen,
            ));
    }
    let nested = helper_env.summarize_calls_in_exprs(exprs, &state.locals.bindings, state.seen);
    if expression_kind == ValueKind::Fragment {
        extend_nested_render(
            state.analysis,
            nested,
            active_output_predicates,
            NestedRenderMode::Fragment,
        );
    } else {
        extend_nested_render(
            state.analysis,
            nested,
            active_output_predicates,
            NestedRenderMode::Scalar,
        );
    }
    let set_default_paths = set_default_chart_paths_for_exprs(exprs, Some(bindings), current_dot);
    state.analysis.chart_defaults.extend(set_default_paths);
}

fn collect_assignment_bound_helper_values(
    var: &str,
    rhs_expr: &helm_schema_ast::TemplateExpr,
    full_exprs: &[helm_schema_ast::TemplateExpr],
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    active_output_predicates: &BTreeSet<Predicate>,
    state: &mut HelperValuesWalkState<'_, '_>,
) {
    let rhs_exprs = std::slice::from_ref(rhs_expr);
    let set_default_paths =
        set_default_chart_paths_for_exprs(full_exprs, Some(bindings), current_dot);
    state.analysis.chart_defaults.extend(set_default_paths);

    let current_dot_fragment = current_dot.map(AbstractValue::to_context_value);
    let mut seen_set = HashSet::new();
    if apply_local_set_mutations_from_exprs(
        full_exprs,
        &mut state.locals.bindings,
        current_dot_fragment.as_ref(),
        state.context,
        &mut seen_set,
    ) {
        let helper_env = BoundHelperEnv::new(bindings, current_dot, state.context);
        let defaulted_dependencies = helper_env.external_default_fallback_paths_in_exprs(rhs_exprs);
        state.analysis.add_dependency_meta_map(
            defaulted_dependencies
                .into_iter()
                .map(|path| {
                    (
                        path,
                        HelperOutputMeta::with_predicates(active_output_predicates, true),
                    )
                })
                .collect(),
        );
        return;
    }

    state
        .analysis
        .add_type_hints(resolved_type_hint_paths_for_exprs_with_fragment_locals(
            rhs_exprs,
            Some(bindings),
            current_dot,
            &state.locals.bindings,
        ));
    state.analysis.add_type_hints(
        resolved_string_transform_paths_for_exprs_with_fragment_locals(
            rhs_exprs,
            Some(bindings),
            current_dot,
            &state.locals.bindings,
        ),
    );

    let helper_env = BoundHelperEnv::new(bindings, current_dot, state.context);
    let fallback_paths = helper_env.external_default_fallback_paths_in_exprs(rhs_exprs);
    let local_fallback_paths =
        helper_env.local_default_fallback_paths_in_exprs(rhs_exprs, &state.locals.default_paths);
    let local_outputs = local_bound_paths_from_expr(rhs_expr, &state.locals.bindings);
    let local_meta_by_path =
        local_output_meta_from_exprs(rhs_exprs, &state.locals.bindings, state.local_output_meta);
    let result_meta_by_path =
        helper_env.output_meta_from_exprs(rhs_exprs, &state.locals.bindings, state.seen);
    let nested = helper_env.summarize_calls_in_exprs(rhs_exprs, &state.locals.bindings, state.seen);
    let mut nested_defaulted_output_paths = BTreeSet::new();
    state
        .analysis
        .chart_defaults
        .extend(nested.chart_defaults.iter().cloned());
    for (path, facts) in nested.path_facts() {
        if let Some(output_meta) = facts.output_meta().cloned() {
            if output_meta.defaulted {
                nested_defaulted_output_paths.insert(path.to_string());
            }
            state
                .analysis
                .merge_dependency_meta(path.to_string(), output_meta);
        }
        if let Some(dependency_meta) = facts.dependency_meta().cloned() {
            state.analysis.add_dependency_path(path.to_string());
            state
                .analysis
                .merge_dependency_meta(path.to_string(), dependency_meta);
        }
        if !facts.type_hints().is_empty() {
            state
                .analysis
                .merge_type_hints(path.to_string(), facts.type_hints().clone());
        }
        for output in facts.fragment_output_uses(path) {
            if output.meta.defaulted {
                nested_defaulted_output_paths.insert(output.source_expr.clone());
            }
            state
                .analysis
                .merge_dependency_meta(output.source_expr, output.meta);
        }
    }

    let rhs_output_meta = rhs_output_meta(
        &local_outputs,
        &fallback_paths,
        &local_fallback_paths,
        &local_meta_by_path,
        &result_meta_by_path,
        active_output_predicates,
    );

    let mut seen_rhs = HashSet::new();
    if let Some(binding) =
        helper_env.fragment_value_from_expr(rhs_expr, &state.locals.bindings, &mut seen_rhs)
    {
        state.locals.bindings.insert(var.to_string(), binding);
    }
    let mut defaulted_paths = fallback_paths;
    defaulted_paths.extend(local_fallback_paths);
    defaulted_paths.extend(nested_defaulted_output_paths);
    if defaulted_paths.is_empty() {
        state.locals.default_paths.remove(var);
    } else {
        state
            .locals
            .default_paths
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

fn extend_nested_render(
    analysis: &mut HelperSummary,
    nested: HelperSummary,
    active_output_predicates: &BTreeSet<Predicate>,
    mode: NestedRenderMode,
) {
    if matches!(mode, NestedRenderMode::Scalar) {
        analysis
            .string_output
            .extend(nested.string_output.iter().cloned());
    }
    analysis
        .suppress_roots
        .extend(nested.suppress_roots.iter().cloned());
    analysis
        .chart_defaults
        .extend(nested.chart_defaults.iter().cloned());

    for (path, facts) in nested.path_facts() {
        if let Some(mut meta) = facts.output_meta().cloned() {
            meta.add_predicates(active_output_predicates.iter().cloned());
            analysis.merge_output_meta(path.to_string(), meta);
        }
        if let Some(meta) = facts.dependency_meta().cloned() {
            analysis.merge_dependency_meta(path.to_string(), meta);
        }
        if facts.is_guard() {
            analysis.add_guard_path(path.to_string());
        }
        if !facts.type_hints().is_empty() {
            analysis.merge_type_hints(path.to_string(), facts.type_hints().clone());
        }
        for mut output in facts.fragment_output_uses(path) {
            output
                .meta
                .add_predicates(active_output_predicates.iter().cloned());
            match mode {
                NestedRenderMode::Scalar => {
                    analysis.merge_output_meta(output.source_expr, output.meta);
                }
                NestedRenderMode::Fragment => {
                    analysis.add_fragment_output_use(output);
                }
            }
        }
    }
}
