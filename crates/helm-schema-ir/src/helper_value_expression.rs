use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ValueKind;
use crate::abstract_value::AbstractValue;
use crate::expr_eval::{
    eval_helper_exprs_direct_effects, eval_helper_exprs_effects, eval_local_exprs_effects,
};
use crate::fragment_assignment::{
    apply_local_set_mutations_from_exprs, parse_helper_assignment_from_exprs,
};
use crate::fragment_expr_eval::{
    helper_result_from_expr_with_fragment_locals, helper_result_from_exprs_with_fragment_locals,
};
use crate::helper_summary::{HelperOutputMeta, HelperSummary};
use crate::helper_walk_state::HelperValuesWalkState;
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
    let mut seen_set = state.seen.clone();
    if apply_local_set_mutations_from_exprs(
        exprs,
        &mut state.locals.bindings,
        current_dot_fragment.as_ref(),
        state.context,
        &mut seen_set,
    ) {
        let effects = eval_helper_exprs_direct_effects(exprs, bindings, current_dot);
        state
            .analysis
            .chart_defaults
            .extend(effects.chart_default_paths);
        return;
    }

    let direct_effects = eval_helper_exprs_direct_effects(exprs, bindings, current_dot);
    let direct_outputs = direct_effects.reads.clone();
    let fallback_paths = eval_helper_exprs_effects(exprs, bindings, current_dot).defaults;
    let local_effects = eval_local_exprs_effects(
        exprs,
        &state.locals.bindings,
        &state.locals.default_paths,
        state.local_output_meta,
    );
    let expression_kind = if exprs.iter().any(TemplateExpr::renders_yaml_fragment) {
        ValueKind::Fragment
    } else {
        ValueKind::Scalar
    };
    let result = helper_result_from_exprs_with_fragment_locals(
        exprs,
        &state.locals.bindings,
        Some(bindings),
        current_dot,
        state.context,
        state.seen,
    );
    state
        .analysis
        .add_type_hints(result.effects.schema_type_hints());
    if expression_kind == ValueKind::Scalar {
        for output in direct_outputs {
            let meta = HelperOutputMeta::with_predicates(
                active_output_predicates,
                fallback_paths.contains(&output),
            );
            state.analysis.merge_output_meta(output, meta);
        }
        for output in local_effects.local_output_sources() {
            let mut meta = local_effects
                .local_output_meta
                .get(&output)
                .cloned()
                .unwrap_or_default();
            meta.add_predicates(active_output_predicates.iter().cloned());
            meta.defaulted |= local_effects.local_default_paths.contains(&output);
            state.analysis.merge_output_meta(output, meta);
        }
        state
            .analysis
            .string_output
            .extend(result.value.iter().flat_map(AbstractValue::strings));
    }
    let nested_render_mode = if expression_kind == ValueKind::Fragment {
        NestedRenderMode::Fragment
    } else {
        NestedRenderMode::Scalar
    };
    extend_nested_render(
        state.analysis,
        result.effects.helper_summary,
        active_output_predicates,
        nested_render_mode,
    );
    state
        .analysis
        .chart_defaults
        .extend(direct_effects.chart_default_paths);
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
    let full_effects = eval_helper_exprs_direct_effects(full_exprs, bindings, current_dot);
    state
        .analysis
        .chart_defaults
        .extend(full_effects.chart_default_paths);

    let current_dot_fragment = current_dot.map(AbstractValue::to_context_value);
    let mut seen_set = state.seen.clone();
    if apply_local_set_mutations_from_exprs(
        full_exprs,
        &mut state.locals.bindings,
        current_dot_fragment.as_ref(),
        state.context,
        &mut seen_set,
    ) {
        let defaulted_dependencies =
            eval_helper_exprs_effects(rhs_exprs, bindings, current_dot).defaults;
        for path in defaulted_dependencies {
            state.analysis.merge_dependency_meta(
                path,
                HelperOutputMeta::with_predicates(active_output_predicates, true),
            );
        }
        return;
    }

    let fallback_paths = eval_helper_exprs_effects(rhs_exprs, bindings, current_dot).defaults;
    let local_effects = eval_local_exprs_effects(
        rhs_exprs,
        &state.locals.bindings,
        &state.locals.default_paths,
        state.local_output_meta,
    );
    let result = helper_result_from_exprs_with_fragment_locals(
        rhs_exprs,
        &state.locals.bindings,
        Some(bindings),
        current_dot,
        state.context,
        state.seen,
    );
    state
        .analysis
        .add_type_hints(result.effects.schema_type_hints());
    let mut nested_defaulted_output_paths = BTreeSet::new();
    state
        .analysis
        .chart_defaults
        .extend(result.effects.helper_summary.chart_defaults.iter().cloned());
    for (path, facts) in result.effects.helper_summary.path_facts() {
        if let Some(output_meta) = facts.output_meta.clone() {
            if output_meta.defaulted {
                nested_defaulted_output_paths.insert(path.to_string());
            }
            state
                .analysis
                .merge_dependency_meta(path.to_string(), output_meta);
        }
        if let Some(dependency_meta) = facts.dependency_meta.clone() {
            state
                .analysis
                .merge_dependency_meta(path.to_string(), dependency_meta);
        }
        if !facts.type_hints.is_empty() {
            state
                .analysis
                .merge_type_hints(path.to_string(), facts.type_hints.clone());
        }
        for output in facts.fragment_output_uses.iter().cloned() {
            if output.meta.defaulted {
                nested_defaulted_output_paths.insert(output.source_expr.clone());
            }
            state
                .analysis
                .merge_dependency_meta(output.source_expr, output.meta);
        }
    }

    let rhs_output_meta = rhs_output_meta(
        &local_effects.local_source_paths,
        &fallback_paths,
        &local_effects.local_default_paths,
        &local_effects.local_output_meta,
        &result
            .value
            .as_ref()
            .map(AbstractValue::output_meta)
            .unwrap_or_default(),
        active_output_predicates,
    );

    let mut seen_rhs = state.seen.clone();
    if let Some(binding) = helper_result_from_expr_with_fragment_locals(
        rhs_expr,
        &state.locals.bindings,
        Some(bindings),
        current_dot,
        state.context,
        &mut seen_rhs,
    )
    .value
    {
        state.locals.bindings.insert(var.to_string(), binding);
    }
    let mut defaulted_paths = fallback_paths;
    defaulted_paths.extend(local_effects.local_default_paths);
    defaulted_paths.extend(nested_defaulted_output_paths);
    state.locals.set_default_paths(var, defaulted_paths);
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
        if let Some(mut meta) = facts.output_meta.clone() {
            meta.add_predicates(active_output_predicates.iter().cloned());
            analysis.merge_output_meta(path.to_string(), meta);
        }
        if let Some(meta) = facts.dependency_meta.clone() {
            analysis.merge_dependency_meta(path.to_string(), meta);
        }
        if facts.guard {
            analysis.add_guard_path(path.to_string());
        }
        if !facts.type_hints.is_empty() {
            analysis.merge_type_hints(path.to_string(), facts.type_hints.clone());
        }
        for mut output in facts.fragment_output_uses.iter().cloned() {
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
