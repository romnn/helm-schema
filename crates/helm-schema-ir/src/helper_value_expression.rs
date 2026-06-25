use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ValueKind;
use crate::abstract_value::AbstractValue;
use crate::expr_eval::{eval_helper_exprs_direct_effects, eval_helper_exprs_effects};
use crate::fragment_assignment::{
    apply_local_set_mutations_from_exprs, parse_helper_assignment_from_exprs,
};
use crate::fragment_expr_eval::{
    FragmentLocalFacts, helper_result_from_exprs_with_fragment_locals,
};
use crate::helper_summary::{HelperOutputMeta, HelperSummary};
use crate::helper_walk_state::HelperValuesWalkState;
use crate::predicate::Predicate;
use helm_schema_ast::TemplateExpr;

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
        &mut state.locals.fragment_values,
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
    let result = helper_result_from_exprs_with_fragment_locals(
        exprs,
        FragmentLocalFacts::with_output_meta(
            &state.locals.fragment_values,
            &state.locals.default_paths,
            &state.locals.output_meta,
        ),
        Some(bindings),
        current_dot,
        state.context,
        state.seen,
    );
    let fallback_paths = eval_helper_exprs_effects(exprs, bindings, current_dot).defaults;
    let local_effects = &result.effects;
    let expression_kind = if exprs.iter().any(TemplateExpr::renders_yaml_fragment) {
        ValueKind::Fragment
    } else {
        ValueKind::Scalar
    };
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
            meta.defaulted |= local_effects.local_default_paths.contains(&output);
            meta = meta.with_output_site_predicates(&output, active_output_predicates);
            state.analysis.merge_output_meta(output, meta);
        }
        state
            .analysis
            .string_output
            .extend(result.value.iter().flat_map(AbstractValue::strings));
    }
    extend_nested_render(
        state.analysis,
        result.effects.helper_summary,
        active_output_predicates,
        expression_kind == ValueKind::Fragment,
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
        &mut state.locals.fragment_values,
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

    let result = helper_result_from_exprs_with_fragment_locals(
        rhs_exprs,
        FragmentLocalFacts::with_output_meta(
            &state.locals.fragment_values,
            &state.locals.default_paths,
            &state.locals.output_meta,
        ),
        Some(bindings),
        current_dot,
        state.context,
        state.seen,
    );
    let fallback_paths = eval_helper_exprs_effects(rhs_exprs, bindings, current_dot).defaults;
    let mut local_source_paths = result.effects.local_source_paths();
    let local_default_paths = result.effects.local_default_paths.clone();
    let local_output_meta = result.effects.local_output_meta.clone();
    let binding = result.value.clone();
    if let Some(binding) = &binding {
        local_source_paths.extend(binding.paths());
    }
    state
        .analysis
        .add_type_hints(result.effects.schema_type_hints());
    let mut nested_defaulted_output_paths = BTreeSet::new();
    let helper_summary = result.effects.helper_summary;
    let HelperSummary {
        dependency_meta,
        type_hints,
        output_uses,
        chart_defaults,
        ..
    } = helper_summary;
    state.analysis.chart_defaults.extend(chart_defaults);
    state.analysis.add_type_hints(type_hints);
    let nested_output_uses = output_uses.clone();
    for output in output_uses {
        if output.meta.defaulted {
            nested_defaulted_output_paths.insert(output.source_expr.clone());
        }
        state
            .analysis
            .merge_dependency_meta(output.source_expr, output.meta);
    }
    for (path, meta) in dependency_meta {
        if meta.defaulted {
            nested_defaulted_output_paths.insert(path.clone());
        }
        state.analysis.merge_dependency_meta(path, meta);
    }

    let rhs_output_meta = rhs_output_meta(
        &local_source_paths,
        &fallback_paths,
        &local_default_paths,
        &local_output_meta,
        &nested_output_uses,
        active_output_predicates,
    );

    if let Some(binding) = binding {
        state
            .locals
            .fragment_values
            .insert(var.to_string(), binding);
    }
    let mut defaulted_paths = fallback_paths;
    defaulted_paths.extend(local_default_paths);
    defaulted_paths.extend(nested_defaulted_output_paths);
    state.locals.set_default_paths(var, defaulted_paths);
    if rhs_output_meta.is_empty() {
        state.locals.output_meta.remove(var);
    } else {
        state
            .locals
            .output_meta
            .insert(var.to_string(), rhs_output_meta);
    }
}

fn rhs_output_meta(
    local_outputs: &BTreeSet<String>,
    fallback_paths: &BTreeSet<String>,
    local_fallback_paths: &BTreeSet<String>,
    local_meta_by_path: &BTreeMap<String, HelperOutputMeta>,
    nested_outputs: &[crate::helper_summary::HelperFragmentOutputUse],
    active_output_predicates: &BTreeSet<Predicate>,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut rhs_output_meta: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
    for output in nested_outputs {
        let mut meta = output
            .meta
            .clone()
            .with_output_site_predicates(&output.source_expr, active_output_predicates);
        meta.defaulted |= fallback_paths.contains(&output.source_expr);
        rhs_output_meta
            .entry(output.source_expr.clone())
            .or_default()
            .merge(meta);
    }
    for output in local_outputs {
        let mut meta = local_meta_by_path.get(output).cloned().unwrap_or_default();
        meta.defaulted |= local_fallback_paths.contains(output);
        meta = meta.with_output_site_predicates(output, active_output_predicates);
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
    render_as_fragment: bool,
) {
    if !render_as_fragment {
        analysis.string_output.extend(nested.string_output);
    }
    analysis.suppress_roots.extend(nested.suppress_roots);
    analysis.chart_defaults.extend(nested.chart_defaults);
    analysis.add_type_hints(nested.type_hints);

    for (path, meta) in nested.dependency_meta {
        analysis.merge_dependency_meta(path, meta);
    }
    for path in nested.guard_paths {
        analysis.add_guard_path(path);
    }
    for mut output in nested.output_uses {
        output.meta = output
            .meta
            .with_output_site_predicates(&output.source_expr, active_output_predicates);
        if render_as_fragment {
            analysis.add_output_use(output);
        } else {
            analysis.merge_output_meta(output.source_expr, output.meta);
        }
    }
}
