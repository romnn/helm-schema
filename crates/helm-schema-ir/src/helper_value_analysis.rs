use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::HelmAst;

use crate::binding::{FragmentBinding, HelperBinding};
use crate::expression_analysis::{
    resolved_default_fallback_paths_for_text, resolved_string_transform_paths_for_text,
    resolved_type_is_paths_for_text, set_default_chart_paths_for_text,
};
use crate::fragment_expr_eval::{
    FragmentEvalContext, fragment_binding_from_expr,
    fragment_binding_from_text_with_helper_context, helper_binding_from_expr_with_fragment_locals,
};
use crate::fragment_scope_eval::{
    apply_local_set_mutations, merge_fragment_locals, range_iterable_binding,
    range_variable_item_binding, range_variable_name,
};
use crate::helper_analysis::{
    BoundHelperAnalysis, HelperOutputMeta, bound_helper_condition_paths,
    bound_helper_dependency_paths, convert_fragment_outputs_to_dependency_outputs,
    extend_type_hints, helper_dependency_meta_from_analysis, helper_output_meta_from_analysis,
    merge_helper_output_meta_maps, merge_local_default_paths,
};
use crate::helper_output_projection::{
    helper_output_meta_with_predicates, push_helper_fragment_output,
};
use crate::local_projection::{
    direct_bound_paths_from_text_in_context, local_bound_paths_from_text,
    local_default_paths_from_text, local_output_meta_from_text, local_rendered_paths_from_text,
};
use crate::predicate::Predicate;
use crate::template_expr_cache::parse_expr_text;
use crate::value_path_context::computed_with_body_dot;
use crate::walker::is_fragment_expr;
use crate::{ValueKind, YamlPath};

pub(crate) struct HelperValuesWalkState<'context, 'state> {
    pub(crate) local_bindings: &'state mut HashMap<String, FragmentBinding>,
    pub(crate) local_default_paths: &'state mut HashMap<String, BTreeSet<String>>,
    pub(crate) local_output_meta: &'state mut HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'state mut HashSet<String>,
    pub(crate) analysis: &'state mut BoundHelperAnalysis,
}

#[derive(Clone, Copy)]
struct HelperValueScope<'a> {
    bindings: &'a HashMap<String, HelperBinding>,
    current_dot: Option<&'a HelperBinding>,
    active_output_predicates: &'a BTreeSet<Predicate>,
}

/// Walks a helper body collecting the values and effects it contributes to
/// callers that include/template it.
#[tracing::instrument(skip_all)]
pub(crate) fn collect_bound_helper_values_from_ast(
    node: &HelmAst,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    active_output_predicates: &BTreeSet<Predicate>,
    state: &mut HelperValuesWalkState<'_, '_>,
) {
    match node {
        HelmAst::Document { items }
        | HelmAst::Mapping { items }
        | HelmAst::Sequence { items }
        | HelmAst::Define { body: items, .. }
        | HelmAst::Block { body: items, .. } => {
            for item in items {
                collect_bound_helper_values_from_ast(
                    item,
                    bindings,
                    current_dot,
                    active_output_predicates,
                    state,
                );
            }
        }
        HelmAst::Pair { key, value } => {
            collect_bound_helper_values_from_ast(
                key,
                bindings,
                current_dot,
                active_output_predicates,
                state,
            );
            if let Some(value) = value {
                collect_bound_helper_values_from_ast(
                    value,
                    bindings,
                    current_dot,
                    active_output_predicates,
                    state,
                );
            }
        }
        HelmAst::HelmExpr { text } => {
            collect_bound_helper_values_from_expr(
                text,
                bindings,
                current_dot,
                active_output_predicates,
                state,
            );
        }
        HelmAst::If {
            cond,
            then_branch,
            else_branch,
        } => collect_if_bound_helper_values(
            cond,
            then_branch,
            else_branch,
            bindings,
            current_dot,
            active_output_predicates,
            state,
        ),
        HelmAst::Range {
            header,
            body,
            else_branch,
        }
        | HelmAst::With {
            header,
            body,
            else_branch,
        } => {
            let scope = HelperValueScope {
                bindings,
                current_dot,
                active_output_predicates,
            };
            collect_loop_or_with_bound_helper_values(node, header, body, else_branch, scope, state);
        }
        HelmAst::Scalar { .. } | HelmAst::HelmComment { .. } => {}
    }
}

fn collect_bound_helper_values_from_expr(
    text: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    active_output_predicates: &BTreeSet<Predicate>,
    state: &mut HelperValuesWalkState<'_, '_>,
) {
    if let Some(assignment) = crate::fragment_scope_eval::parse_helper_assignment(text) {
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

    let current_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
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
    let empty_path = YamlPath(Vec::new());
    if expression_kind == ValueKind::Scalar {
        for output in direct_outputs {
            let meta = HelperOutputMeta::with_predicates(
                active_output_predicates,
                fallback_paths.contains(&output),
            );
            state.analysis.add_output_meta(output, meta);
        }
        for output in local_outputs {
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
    let mut nested = state
        .context
        .helper_call_analyzer()
        .analyze_bound_helper_calls(
            text,
            Some(bindings),
            current_dot,
            state.local_bindings,
            state.context,
            state.seen,
        );
    if expression_kind == ValueKind::Fragment {
        for (output, mut meta) in nested.output {
            meta.add_predicates(active_output_predicates.iter().cloned());
            state.analysis.add_output_meta(output, meta);
        }
        for output in nested.fragment_output {
            push_helper_fragment_output(
                &mut state.analysis.fragment_output_uses,
                output,
                &empty_path,
                expression_kind,
                HelperOutputMeta::with_predicates(active_output_predicates, false),
            );
        }
        for mut output in nested.fragment_output_uses {
            output
                .meta
                .add_predicates(active_output_predicates.iter().cloned());
            state.analysis.fragment_output_uses.push(output);
        }
        state.analysis.dependency_paths.extend(
            nested
                .dependency_paths
                .into_iter()
                .filter(|path| !path.trim().is_empty()),
        );
        state
            .analysis
            .add_dependency_meta_map(nested.dependency_meta);
        state.analysis.guard_paths.extend(nested.guard_paths);
        extend_type_hints(&mut state.analysis.type_hints, nested.type_hints);
        state.analysis.suppress_roots.extend(nested.suppress_roots);
        state.analysis.chart_defaults.extend(nested.chart_defaults);
    } else {
        convert_fragment_outputs_to_dependency_outputs(&mut nested);
        for meta in nested.output.values_mut() {
            meta.add_predicates(active_output_predicates.iter().cloned());
        }
        state.analysis.extend(nested);
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
    let current_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
    for expr in parse_expr_text(text) {
        if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
            &expr,
            local_bindings,
            Some(bindings),
            current_dot,
            context,
            seen,
        ) {
            strings.extend(binding.strings());
            continue;
        }
        if let Some(binding) = fragment_binding_from_expr(
            &expr,
            local_bindings,
            current_dot_fragment.as_ref(),
            context,
            seen,
        ) {
            strings.extend(binding.strings());
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

    let current_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
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
    let direct_outputs = direct_bound_paths_from_text_in_context(rhs, bindings, current_dot);
    let local_fallback_paths = local_default_paths_from_text(rhs, state.local_default_paths);
    let local_outputs = local_bound_paths_from_text(rhs, state.local_bindings);
    let local_meta_by_path =
        local_output_meta_from_text(rhs, state.local_bindings, state.local_output_meta);
    let nested = state
        .context
        .helper_call_analyzer()
        .analyze_bound_helper_calls(
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
        .extend(bound_helper_dependency_paths(&nested));
    state
        .analysis
        .add_dependency_meta_map(helper_dependency_meta_from_analysis(&nested));

    let rhs_output_meta = rhs_output_meta(
        &direct_outputs,
        &local_outputs,
        &fallback_paths,
        &local_fallback_paths,
        &local_meta_by_path,
        &nested,
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
    direct_outputs: &BTreeSet<String>,
    local_outputs: &BTreeSet<String>,
    fallback_paths: &BTreeSet<String>,
    local_fallback_paths: &BTreeSet<String>,
    local_meta_by_path: &BTreeMap<String, HelperOutputMeta>,
    nested: &BoundHelperAnalysis,
    active_output_predicates: &BTreeSet<Predicate>,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut rhs_output_meta: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
    for output in direct_outputs {
        let entry = rhs_output_meta.entry(output.clone()).or_default();
        entry.add_predicates(active_output_predicates.iter().cloned());
        entry.defaulted |= fallback_paths.contains(output);
    }
    for output in local_outputs {
        let mut meta = local_meta_by_path.get(output).cloned().unwrap_or_default();
        meta.add_predicates(active_output_predicates.iter().cloned());
        meta.defaulted |= local_fallback_paths.contains(output);
        let entry = rhs_output_meta.entry(output.clone()).or_default();
        entry.predicates.extend(meta.predicates);
        entry.defaulted |= meta.defaulted;
    }
    for (output, meta) in helper_output_meta_from_analysis(nested) {
        let meta = helper_output_meta_with_predicates(meta, active_output_predicates);
        let entry = rhs_output_meta.entry(output).or_default();
        entry.predicates.extend(meta.predicates);
        entry.defaulted |= meta.defaulted;
    }
    rhs_output_meta
}

fn collect_if_bound_helper_values(
    cond: &str,
    then_branch: &[HelmAst],
    else_branch: &[HelmAst],
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    active_output_predicates: &BTreeSet<Predicate>,
    state: &mut HelperValuesWalkState<'_, '_>,
) {
    let mut branch_guard_paths =
        direct_bound_paths_from_text_in_context(cond, bindings, current_dot);
    branch_guard_paths.extend(local_bound_paths_from_text(cond, state.local_bindings));
    let nested = state
        .context
        .helper_call_analyzer()
        .analyze_bound_helper_calls(
            cond,
            Some(bindings),
            current_dot,
            state.local_bindings,
            state.context,
            state.seen,
        );
    branch_guard_paths.extend(bound_helper_condition_paths(&nested));
    state
        .analysis
        .guard_paths
        .extend(branch_guard_paths.iter().cloned());
    let mut then_output_predicates = active_output_predicates.clone();
    then_output_predicates.extend(branch_guard_paths.into_iter().map(Predicate::truthy_path));
    let mut then_bindings = state.local_bindings.clone();
    let mut then_default_paths = state.local_default_paths.clone();
    let mut then_output_meta = state.local_output_meta.clone();
    let mut then_state = HelperValuesWalkState {
        local_bindings: &mut then_bindings,
        local_default_paths: &mut then_default_paths,
        local_output_meta: &mut then_output_meta,
        context: state.context,
        seen: state.seen,
        analysis: state.analysis,
    };
    for item in then_branch {
        collect_bound_helper_values_from_ast(
            item,
            bindings,
            current_dot,
            &then_output_predicates,
            &mut then_state,
        );
    }
    let mut else_bindings = state.local_bindings.clone();
    let mut else_default_paths = state.local_default_paths.clone();
    let mut else_output_meta = state.local_output_meta.clone();
    let mut else_state = HelperValuesWalkState {
        local_bindings: &mut else_bindings,
        local_default_paths: &mut else_default_paths,
        local_output_meta: &mut else_output_meta,
        context: state.context,
        seen: state.seen,
        analysis: state.analysis,
    };
    for item in else_branch {
        collect_bound_helper_values_from_ast(
            item,
            bindings,
            current_dot,
            active_output_predicates,
            &mut else_state,
        );
    }
    *state.local_bindings = merge_fragment_locals(then_bindings, else_bindings);
    *state.local_default_paths = merge_local_default_paths(then_default_paths, else_default_paths);
    *state.local_output_meta = merge_helper_output_meta_maps(then_output_meta, else_output_meta);
}

fn collect_loop_or_with_bound_helper_values(
    node: &HelmAst,
    header: &str,
    body: &[HelmAst],
    else_branch: &[HelmAst],
    scope: HelperValueScope<'_>,
    state: &mut HelperValuesWalkState<'_, '_>,
) {
    let is_with = matches!(node, HelmAst::With { .. });
    let mut branch_guard_paths =
        direct_bound_paths_from_text_in_context(header, scope.bindings, scope.current_dot);
    branch_guard_paths.extend(local_bound_paths_from_text(header, state.local_bindings));
    let nested = state
        .context
        .helper_call_analyzer()
        .analyze_bound_helper_calls(
            header,
            Some(scope.bindings),
            scope.current_dot,
            state.local_bindings,
            state.context,
            state.seen,
        );
    branch_guard_paths.extend(bound_helper_condition_paths(&nested));
    state
        .analysis
        .guard_paths
        .extend(branch_guard_paths.iter().cloned());

    let mut range_fragment_binding = None;
    let mut range_binding = None;
    if !is_with {
        let current_dot_fragment = scope.current_dot.map(HelperBinding::to_fragment_binding);
        let mut seen_range = HashSet::new();
        range_fragment_binding = range_iterable_binding(
            header,
            state.local_bindings,
            current_dot_fragment.as_ref(),
            state.context,
            &mut seen_range,
        );
        range_binding = range_fragment_binding
            .as_ref()
            .and_then(FragmentBinding::to_helper_binding);
    }
    let body_dot = if is_with {
        computed_with_body_dot(header, scope.bindings, scope.current_dot)
    } else {
        range_binding.as_ref().and_then(HelperBinding::item_binding)
    };
    let mut body_output_predicates = scope.active_output_predicates.clone();
    body_output_predicates.extend(branch_guard_paths.into_iter().map(Predicate::truthy_path));
    let mut body_bindings = state.local_bindings.clone();
    let mut body_default_paths = state.local_default_paths.clone();
    let mut body_output_meta = state.local_output_meta.clone();
    if !is_with {
        let header_dot_fragment = scope.current_dot.map(HelperBinding::to_fragment_binding);
        let mut seen_range = HashSet::new();
        if let Some((var, binding)) = range_variable_item_binding(
            header,
            &body_bindings,
            header_dot_fragment.as_ref(),
            state.context,
            &mut seen_range,
        ) {
            body_bindings.insert(var, binding);
        }
    }
    if !is_with && let Some(FragmentBinding::List(items)) = &range_fragment_binding {
        let range_var = range_variable_name(header);
        for item_binding in items {
            if let Some(range_var) = &range_var {
                body_bindings.insert(range_var.clone(), item_binding.clone());
            }
            let item_dot = item_binding.to_helper_binding();
            let mut item_seen = state.seen.clone();
            let mut item_state = HelperValuesWalkState {
                local_bindings: &mut body_bindings,
                local_default_paths: &mut body_default_paths,
                local_output_meta: &mut body_output_meta,
                context: state.context,
                seen: &mut item_seen,
                analysis: state.analysis,
            };
            for item in body {
                collect_bound_helper_values_from_ast(
                    item,
                    scope.bindings,
                    item_dot.as_ref(),
                    &body_output_predicates,
                    &mut item_state,
                );
            }
        }
    } else {
        let mut body_state = HelperValuesWalkState {
            local_bindings: &mut body_bindings,
            local_default_paths: &mut body_default_paths,
            local_output_meta: &mut body_output_meta,
            context: state.context,
            seen: state.seen,
            analysis: state.analysis,
        };
        for item in body {
            collect_bound_helper_values_from_ast(
                item,
                scope.bindings,
                body_dot.as_ref(),
                &body_output_predicates,
                &mut body_state,
            );
        }
    }
    if !is_with
        && range_binding
            .as_ref()
            .is_some_and(HelperBinding::definitely_nonempty_iterable)
    {
        *state.local_bindings = body_bindings;
        *state.local_default_paths = body_default_paths;
        *state.local_output_meta = body_output_meta;
    } else {
        let mut else_bindings = state.local_bindings.clone();
        let mut else_default_paths = state.local_default_paths.clone();
        let mut else_output_meta = state.local_output_meta.clone();
        let mut else_state = HelperValuesWalkState {
            local_bindings: &mut else_bindings,
            local_default_paths: &mut else_default_paths,
            local_output_meta: &mut else_output_meta,
            context: state.context,
            seen: state.seen,
            analysis: state.analysis,
        };
        for item in else_branch {
            collect_bound_helper_values_from_ast(
                item,
                scope.bindings,
                scope.current_dot,
                scope.active_output_predicates,
                &mut else_state,
            );
        }
        *state.local_bindings = merge_fragment_locals(body_bindings, else_bindings);
        *state.local_default_paths =
            merge_local_default_paths(body_default_paths, else_default_paths);
        *state.local_output_meta =
            merge_helper_output_meta_maps(body_output_meta, else_output_meta);
    }
}
