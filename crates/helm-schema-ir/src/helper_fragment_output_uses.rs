use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::{HelmAst, TemplateExpr};

use crate::binding::{FragmentBinding, HelperBinding};
use crate::expression_analysis::resolved_default_fallback_paths_for_text;
use crate::fragment_binding_eval::fragment_binding_from_helper_analysis;
use crate::fragment_expr_eval::{
    FragmentEvalContext, fragment_binding_from_text, helper_binding_from_expr_with_fragment_locals,
};
use crate::fragment_scope_eval::{
    apply_local_set_mutations, merge_fragment_locals, parse_helper_assignment,
    range_iterable_binding, range_variable_name,
};
use crate::helper_analysis::{
    BoundHelperAnalysis, HelperFragmentOutputUse, bound_helper_condition_paths,
    bound_helper_dependency_paths, merge_local_default_paths,
};
use crate::helper_output_projection::{
    HelperOutputExprContext, collect_fragment_binding_output_uses,
    collect_helper_binding_output_uses, collect_helper_binding_output_uses_from_expr,
    expression_output_use_is_keyed_map_projection, helper_output_meta_with_guards,
    push_helper_fragment_output, static_yaml_fragment_output_path,
};
use crate::local_projection::{
    direct_bound_paths_from_text_in_context, local_bound_paths_from_text,
    local_default_paths_from_text, local_rendered_paths_from_text,
};
use crate::output_path;
use crate::template_expr_analysis::{
    expr_contains_helper_call, text_pipeline_merges_into_var, text_starts_with_helper_call,
    walk_expr_excluding_helper_call_args,
};
use crate::template_expr_cache::parse_expr_text;
use crate::value_path_context::computed_with_body_dot;
use crate::walker::is_fragment_expr;
use crate::{ValueKind, YamlPath};

pub(crate) type AnalyzeBoundHelperCalls<'context> = fn(
    &str,
    Option<&HashMap<String, HelperBinding>>,
    Option<&HelperBinding>,
    &HashMap<String, FragmentBinding>,
    FragmentEvalContext<'context>,
    &mut HashSet<String>,
) -> BoundHelperAnalysis;

pub(crate) struct FragmentOutputWalkState<'context, 'state> {
    pub(crate) local_bindings: &'state mut HashMap<String, FragmentBinding>,
    pub(crate) local_default_paths: &'state mut HashMap<String, BTreeSet<String>>,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) analyze_bound_helper_calls: AnalyzeBoundHelperCalls<'context>,
    pub(crate) seen: &'state mut HashSet<String>,
    pub(crate) outputs: &'state mut Vec<HelperFragmentOutputUse>,
}

#[derive(Clone, Copy)]
struct FragmentOutputScope<'a> {
    bindings: &'a HashMap<String, HelperBinding>,
    current_dot: Option<&'a HelperBinding>,
    current_dot_fragment: Option<&'a FragmentBinding>,
    relative_path: &'a YamlPath,
    active_output_guards: &'a BTreeSet<String>,
}

struct FragmentExpressionOutputScope<'a> {
    bindings: &'a HashMap<String, HelperBinding>,
    current_dot: Option<&'a HelperBinding>,
    output_path: &'a YamlPath,
    kind: ValueKind,
    active_output_guards: &'a BTreeSet<String>,
    fallback_paths: &'a BTreeSet<String>,
}

pub(crate) fn collect_bound_fragment_output_uses_from_items(
    items: &[HelmAst],
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    current_dot_fragment: Option<&FragmentBinding>,
    relative_path: &YamlPath,
    active_output_guards: &BTreeSet<String>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let mut pending_path: Option<YamlPath> = None;
    for item in items {
        if let Some(path) = output_path::pending_mapping_key_path(item, relative_path) {
            pending_path = Some(path);
            continue;
        }
        let item_path = pending_path.as_ref().unwrap_or(relative_path);
        collect_bound_fragment_output_uses_from_ast(
            item,
            bindings,
            current_dot,
            current_dot_fragment,
            item_path,
            active_output_guards,
            state,
        );
        pending_path = output_path::trailing_pending_mapping_key_path(item, item_path);
    }
}

fn collect_bound_fragment_output_uses_from_ast(
    node: &HelmAst,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    current_dot_fragment: Option<&FragmentBinding>,
    relative_path: &YamlPath,
    active_output_guards: &BTreeSet<String>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    match node {
        HelmAst::Document { items }
        | HelmAst::Mapping { items }
        | HelmAst::Define { body: items, .. }
        | HelmAst::Block { body: items, .. } => {
            collect_bound_fragment_output_uses_from_items(
                items,
                bindings,
                current_dot,
                current_dot_fragment,
                relative_path,
                active_output_guards,
                state,
            );
        }
        HelmAst::Sequence { items } => {
            let item_path = output_path::sequence_item_path(relative_path);
            for item in items {
                collect_bound_fragment_output_uses_from_ast(
                    item,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    &item_path,
                    active_output_guards,
                    state,
                );
            }
        }
        HelmAst::Pair { key, value } => {
            if let Some(segment) = output_path::key_segment(key) {
                let mut value_path = relative_path.clone();
                value_path.0.push(segment);
                if let Some(value) = value {
                    collect_bound_fragment_output_uses_from_ast(
                        value,
                        bindings,
                        current_dot,
                        current_dot_fragment,
                        &value_path,
                        active_output_guards,
                        state,
                    );
                }
                return;
            }

            collect_bound_fragment_output_uses_from_ast(
                key,
                bindings,
                current_dot,
                current_dot_fragment,
                relative_path,
                active_output_guards,
                state,
            );
            if let Some(value) = value {
                collect_bound_fragment_output_uses_from_ast(
                    value,
                    bindings,
                    current_dot,
                    current_dot_fragment,
                    relative_path,
                    active_output_guards,
                    state,
                );
            }
        }
        HelmAst::HelmExpr { text } => {
            collect_bound_fragment_output_uses_from_expr(
                text,
                bindings,
                current_dot,
                current_dot_fragment,
                relative_path,
                active_output_guards,
                state,
            );
        }
        HelmAst::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let scope = FragmentOutputScope {
                bindings,
                current_dot,
                current_dot_fragment,
                relative_path,
                active_output_guards,
            };
            collect_if_fragment_output_uses(cond, then_branch, else_branch, scope, state);
        }
        HelmAst::With {
            header,
            body,
            else_branch,
        } => {
            let scope = FragmentOutputScope {
                bindings,
                current_dot,
                current_dot_fragment,
                relative_path,
                active_output_guards,
            };
            collect_with_fragment_output_uses(header, body, else_branch, scope, state);
        }
        HelmAst::Range {
            header,
            body,
            else_branch,
        } => {
            let scope = FragmentOutputScope {
                bindings,
                current_dot,
                current_dot_fragment,
                relative_path,
                active_output_guards,
            };
            collect_range_fragment_output_uses(header, body, else_branch, scope, state);
        }
        HelmAst::Scalar { .. } | HelmAst::HelmComment { .. } => {}
    }
}

fn collect_bound_fragment_output_uses_from_expr(
    text: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    current_dot_fragment: Option<&FragmentBinding>,
    relative_path: &YamlPath,
    active_output_guards: &BTreeSet<String>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let mut seen_set = HashSet::new();
    if apply_local_set_mutations(
        text,
        state.local_bindings,
        current_dot_fragment,
        state.context,
        &mut seen_set,
    ) {
        return;
    }

    if let Some((var, _declares, rhs)) = parse_helper_assignment(text) {
        collect_bound_fragment_output_assignment_uses(
            &var,
            &rhs,
            bindings,
            current_dot,
            current_dot_fragment,
            state,
        );
        return;
    }

    let kind = if is_fragment_expr(text) {
        ValueKind::Fragment
    } else {
        ValueKind::Scalar
    };
    let output_path = static_yaml_fragment_output_path(text)
        .map(|output_path| output_path::append_relative_path(relative_path, &output_path))
        .unwrap_or_else(|| relative_path.clone());
    let direct_outputs = direct_bound_paths_from_text_in_context(text, bindings, current_dot);
    let fallback_paths =
        resolved_default_fallback_paths_for_text(text, Some(bindings), current_dot);
    let local_outputs = local_rendered_paths_from_text(text, state.local_bindings);
    let handled_outputs: BTreeSet<String> = direct_outputs
        .iter()
        .chain(local_outputs.iter())
        .cloned()
        .collect();

    let mut direct_output_uses = Vec::new();
    for expr in parse_expr_text(text) {
        collect_helper_binding_output_uses_from_expr(
            &expr,
            HelperOutputExprContext {
                bindings,
                current_dot,
                relative_path: &output_path,
                kind,
                active_output_guards,
                defaulted_paths: &fallback_paths,
            },
            &mut direct_output_uses,
        );
    }
    state.outputs.extend(direct_output_uses);

    let local_fallback_paths = local_default_paths_from_text(text, state.local_default_paths);
    let local_output_uses = local_output_uses_from_text(
        text,
        &output_path,
        kind,
        active_output_guards,
        &local_fallback_paths,
        state.local_bindings,
    );

    let mut nested = (state.analyze_bound_helper_calls)(
        text,
        Some(bindings),
        current_dot,
        state.local_bindings,
        state.context,
        state.seen,
    );
    let nested_structured_sources: BTreeSet<String> = nested
        .fragment_output_uses
        .iter()
        .map(|output| output.source_expr.clone())
        .collect();
    let empty_output_path = YamlPath(Vec::new());
    let nested_descendant_structured_sources: BTreeSet<String> = nested
        .fragment_output_uses
        .iter()
        .filter(|output| expression_output_use_is_keyed_map_projection(output, &empty_output_path))
        .map(|output| output.source_expr.clone())
        .collect();
    let nested_scalar_sources: BTreeSet<String> = nested.output.keys().cloned().collect();
    let nested_has_fragment_outputs =
        !nested.fragment_output.is_empty() || !nested.fragment_output_uses.is_empty();

    let expression_output_scope = FragmentExpressionOutputScope {
        bindings,
        current_dot,
        output_path: &output_path,
        kind,
        active_output_guards,
        fallback_paths: &fallback_paths,
    };
    let expression_output_uses =
        helper_expression_output_uses_from_text(text, expression_output_scope, state);
    let expression_descendant_sources: BTreeSet<String> = expression_output_uses
        .iter()
        .filter(|output| !output.relative_path.0.is_empty())
        .map(|output| output.source_expr.clone())
        .collect();

    state.outputs.extend(local_output_uses);
    for output in expression_output_uses {
        if output.relative_path.0.is_empty()
            && (handled_outputs.contains(&output.source_expr)
                || nested_structured_sources.contains(&output.source_expr)
                || nested_scalar_sources.contains(&output.source_expr))
        {
            continue;
        }
        state.outputs.push(output);
    }
    for (source_expr, meta) in nested.output {
        if kind == ValueKind::Fragment && nested_has_fragment_outputs {
            continue;
        }
        if nested_structured_sources.contains(&source_expr)
            || expression_descendant_sources.contains(&source_expr)
        {
            continue;
        }
        let meta = helper_output_meta_with_guards(meta, active_output_guards);
        push_helper_fragment_output(state.outputs, source_expr, relative_path, kind, meta);
    }
    for nested_output in nested.fragment_output_uses.drain(..) {
        if kind == ValueKind::Fragment
            && nested_output.relative_path.0.is_empty()
            && (nested_scalar_sources.contains(&nested_output.source_expr)
                || nested_descendant_structured_sources.contains(&nested_output.source_expr)
                || expression_descendant_sources.contains(&nested_output.source_expr))
        {
            continue;
        }
        let meta = helper_output_meta_with_guards(nested_output.meta, active_output_guards);
        push_helper_fragment_output(
            state.outputs,
            nested_output.source_expr,
            &output_path::append_relative_path(relative_path, &nested_output.relative_path),
            nested_output.kind,
            meta,
        );
    }
}

fn collect_bound_fragment_output_assignment_uses(
    var: &str,
    rhs: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
    current_dot_fragment: Option<&FragmentBinding>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let mut seen_rhs = HashSet::new();
    let mut binding = fragment_binding_from_text(
        rhs,
        state.local_bindings,
        current_dot_fragment,
        state.context,
        &mut seen_rhs,
    );
    let mut top_level_helper_dependency_paths = BTreeSet::new();
    if text_starts_with_helper_call(rhs) {
        let mut rhs_seen = state.seen.clone();
        let nested = (state.analyze_bound_helper_calls)(
            rhs,
            Some(bindings),
            current_dot,
            state.local_bindings,
            state.context,
            &mut rhs_seen,
        );
        top_level_helper_dependency_paths = bound_helper_dependency_paths(&nested);
        if let Some(nested_binding) = fragment_binding_from_helper_analysis(nested) {
            binding = match binding {
                Some(binding) => FragmentBinding::merge_all(vec![binding, nested_binding]),
                None => Some(nested_binding),
            };
        }
    }
    if text_pipeline_merges_into_var(rhs, var)
        && let Some(current_dot_fragment) = current_dot_fragment
        && matches!(
            current_dot_fragment,
            FragmentBinding::Dict(_) | FragmentBinding::Overlay { .. }
        )
    {
        let current_item_paths = FragmentBinding::paths(current_dot_fragment);
        let mut internal_item_paths = current_item_paths;
        internal_item_paths.extend(top_level_helper_dependency_paths);
        if !internal_item_paths.is_empty() {
            binding = binding.and_then(|binding| binding.remove_paths(&internal_item_paths));
        }
        binding = match binding {
            Some(binding) => {
                FragmentBinding::merge_all(vec![binding, current_dot_fragment.clone()])
            }
            None => Some(current_dot_fragment.clone()),
        };
    }
    if let Some(binding) = binding {
        state.local_bindings.insert(var.to_string(), binding);
    }
    let mut defaulted_paths =
        resolved_default_fallback_paths_for_text(rhs, Some(bindings), current_dot);
    defaulted_paths.extend(local_default_paths_from_text(
        rhs,
        state.local_default_paths,
    ));
    if defaulted_paths.is_empty() {
        state.local_default_paths.remove(var);
    } else {
        state
            .local_default_paths
            .insert(var.to_string(), defaulted_paths);
    }
}

fn local_output_uses_from_text(
    text: &str,
    output_path: &YamlPath,
    kind: ValueKind,
    active_output_guards: &BTreeSet<String>,
    local_fallback_paths: &BTreeSet<String>,
    local_bindings: &HashMap<String, FragmentBinding>,
) -> Vec<HelperFragmentOutputUse> {
    let mut local_output_uses = Vec::new();
    for expr in parse_expr_text(text) {
        walk_expr_excluding_helper_call_args(&expr, &mut |node| {
            let binding = match node {
                TemplateExpr::Variable(var) if !var.is_empty() => local_bindings.get(var).cloned(),
                TemplateExpr::Selector { operand, path } => {
                    let TemplateExpr::Variable(var) = operand.as_ref() else {
                        return;
                    };
                    if var.is_empty() {
                        return;
                    }
                    local_bindings
                        .get(var)
                        .and_then(|binding| binding.apply_to_binding(path))
                }
                _ => None,
            };
            if let Some(binding) = binding {
                collect_fragment_binding_output_uses(
                    &mut local_output_uses,
                    &binding,
                    output_path,
                    kind,
                    active_output_guards,
                    local_fallback_paths,
                );
            }
        });
    }
    local_output_uses
}

fn helper_expression_output_uses_from_text(
    text: &str,
    scope: FragmentExpressionOutputScope<'_>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) -> Vec<HelperFragmentOutputUse> {
    let mut expression_output_uses = Vec::new();
    let mut expression_seen = state.seen.clone();
    for expr in parse_expr_text(text) {
        if !expr_contains_helper_call(&expr) {
            continue;
        }
        if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
            &expr,
            state.local_bindings,
            Some(scope.bindings),
            scope.current_dot,
            state.context,
            &mut expression_seen,
        ) {
            collect_helper_binding_output_uses(
                &mut expression_output_uses,
                &binding,
                scope.output_path,
                scope.kind,
                scope.active_output_guards,
                scope.fallback_paths,
            );
        }
    }
    expression_output_uses
        .retain(|output| expression_output_use_is_keyed_map_projection(output, scope.output_path));
    expression_output_uses
}

fn collect_if_fragment_output_uses(
    cond: &str,
    then_branch: &[HelmAst],
    else_branch: &[HelmAst],
    scope: FragmentOutputScope<'_>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let mut branch_guard_paths =
        direct_bound_paths_from_text_in_context(cond, scope.bindings, scope.current_dot);
    branch_guard_paths.extend(local_bound_paths_from_text(cond, state.local_bindings));
    let nested = (state.analyze_bound_helper_calls)(
        cond,
        Some(scope.bindings),
        scope.current_dot,
        state.local_bindings,
        state.context,
        state.seen,
    );
    branch_guard_paths.extend(bound_helper_condition_paths(&nested));

    let mut then_guards = scope.active_output_guards.clone();
    then_guards.extend(branch_guard_paths);
    let mut then_bindings = state.local_bindings.clone();
    let mut then_defaults = state.local_default_paths.clone();
    let mut then_state = FragmentOutputWalkState {
        local_bindings: &mut then_bindings,
        local_default_paths: &mut then_defaults,
        context: state.context,
        analyze_bound_helper_calls: state.analyze_bound_helper_calls,
        seen: state.seen,
        outputs: state.outputs,
    };
    collect_bound_fragment_output_uses_from_items(
        then_branch,
        scope.bindings,
        scope.current_dot,
        scope.current_dot_fragment,
        scope.relative_path,
        &then_guards,
        &mut then_state,
    );

    let mut else_bindings = state.local_bindings.clone();
    let mut else_defaults = state.local_default_paths.clone();
    let mut else_state = FragmentOutputWalkState {
        local_bindings: &mut else_bindings,
        local_default_paths: &mut else_defaults,
        context: state.context,
        analyze_bound_helper_calls: state.analyze_bound_helper_calls,
        seen: state.seen,
        outputs: state.outputs,
    };
    collect_bound_fragment_output_uses_from_items(
        else_branch,
        scope.bindings,
        scope.current_dot,
        scope.current_dot_fragment,
        scope.relative_path,
        scope.active_output_guards,
        &mut else_state,
    );
    *state.local_bindings = merge_fragment_locals(then_bindings, else_bindings);
    *state.local_default_paths = merge_local_default_paths(then_defaults, else_defaults);
}

fn collect_with_fragment_output_uses(
    header: &str,
    body: &[HelmAst],
    else_branch: &[HelmAst],
    scope: FragmentOutputScope<'_>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let mut branch_guard_paths =
        direct_bound_paths_from_text_in_context(header, scope.bindings, scope.current_dot);
    branch_guard_paths.extend(local_bound_paths_from_text(header, state.local_bindings));
    let nested = (state.analyze_bound_helper_calls)(
        header,
        Some(scope.bindings),
        scope.current_dot,
        state.local_bindings,
        state.context,
        state.seen,
    );
    branch_guard_paths.extend(bound_helper_condition_paths(&nested));
    let body_dot = computed_with_body_dot(header, scope.bindings, scope.current_dot);

    let mut body_guards = scope.active_output_guards.clone();
    body_guards.extend(branch_guard_paths);
    let mut body_bindings = state.local_bindings.clone();
    let mut body_defaults = state.local_default_paths.clone();
    let body_dot_fragment = body_dot.as_ref().map(HelperBinding::to_fragment_binding);
    let mut body_state = FragmentOutputWalkState {
        local_bindings: &mut body_bindings,
        local_default_paths: &mut body_defaults,
        context: state.context,
        analyze_bound_helper_calls: state.analyze_bound_helper_calls,
        seen: state.seen,
        outputs: state.outputs,
    };
    collect_bound_fragment_output_uses_from_items(
        body,
        scope.bindings,
        body_dot.as_ref(),
        body_dot_fragment.as_ref(),
        scope.relative_path,
        &body_guards,
        &mut body_state,
    );

    let mut else_bindings = state.local_bindings.clone();
    let mut else_defaults = state.local_default_paths.clone();
    let mut else_state = FragmentOutputWalkState {
        local_bindings: &mut else_bindings,
        local_default_paths: &mut else_defaults,
        context: state.context,
        analyze_bound_helper_calls: state.analyze_bound_helper_calls,
        seen: state.seen,
        outputs: state.outputs,
    };
    collect_bound_fragment_output_uses_from_items(
        else_branch,
        scope.bindings,
        scope.current_dot,
        scope.current_dot_fragment,
        scope.relative_path,
        scope.active_output_guards,
        &mut else_state,
    );
    *state.local_bindings = merge_fragment_locals(body_bindings, else_bindings);
    *state.local_default_paths = merge_local_default_paths(body_defaults, else_defaults);
}

fn collect_range_fragment_output_uses(
    header: &str,
    body: &[HelmAst],
    else_branch: &[HelmAst],
    scope: FragmentOutputScope<'_>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let mut branch_guard_paths =
        direct_bound_paths_from_text_in_context(header, scope.bindings, scope.current_dot);
    branch_guard_paths.extend(local_bound_paths_from_text(header, state.local_bindings));
    let nested = (state.analyze_bound_helper_calls)(
        header,
        Some(scope.bindings),
        scope.current_dot,
        state.local_bindings,
        state.context,
        state.seen,
    );
    branch_guard_paths.extend(bound_helper_condition_paths(&nested));
    let mut seen_range_binding = HashSet::new();
    let range_binding = range_iterable_binding(
        header,
        state.local_bindings,
        scope.current_dot_fragment,
        state.context,
        &mut seen_range_binding,
    );
    let body_dot_fragment = range_binding
        .as_ref()
        .and_then(FragmentBinding::item_binding);
    let body_dot = body_dot_fragment
        .as_ref()
        .and_then(FragmentBinding::to_helper_binding);

    let mut body_guards = scope.active_output_guards.clone();
    body_guards.extend(branch_guard_paths);
    let mut body_bindings = state.local_bindings.clone();
    let mut body_defaults = state.local_default_paths.clone();
    if let Some(FragmentBinding::List(items)) = &range_binding {
        let range_var = range_variable_name(header);
        for item_binding in items {
            if let Some(range_var) = &range_var {
                body_bindings.insert(range_var.clone(), item_binding.clone());
            }
            let item_dot = item_binding.to_helper_binding();
            let mut item_seen = state.seen.clone();
            let mut item_state = FragmentOutputWalkState {
                local_bindings: &mut body_bindings,
                local_default_paths: &mut body_defaults,
                context: state.context,
                analyze_bound_helper_calls: state.analyze_bound_helper_calls,
                seen: &mut item_seen,
                outputs: state.outputs,
            };
            collect_bound_fragment_output_uses_from_items(
                body,
                scope.bindings,
                item_dot.as_ref(),
                Some(item_binding),
                scope.relative_path,
                &body_guards,
                &mut item_state,
            );
        }
    } else {
        let mut body_state = FragmentOutputWalkState {
            local_bindings: &mut body_bindings,
            local_default_paths: &mut body_defaults,
            context: state.context,
            analyze_bound_helper_calls: state.analyze_bound_helper_calls,
            seen: state.seen,
            outputs: state.outputs,
        };
        collect_bound_fragment_output_uses_from_items(
            body,
            scope.bindings,
            body_dot.as_ref(),
            body_dot_fragment.as_ref(),
            scope.relative_path,
            &body_guards,
            &mut body_state,
        );
    }

    if range_binding
        .as_ref()
        .is_some_and(FragmentBinding::definitely_nonempty_iterable)
    {
        *state.local_bindings = body_bindings;
        *state.local_default_paths = body_defaults;
    } else {
        let mut else_bindings = state.local_bindings.clone();
        let mut else_defaults = state.local_default_paths.clone();
        let mut else_state = FragmentOutputWalkState {
            local_bindings: &mut else_bindings,
            local_default_paths: &mut else_defaults,
            context: state.context,
            analyze_bound_helper_calls: state.analyze_bound_helper_calls,
            seen: state.seen,
            outputs: state.outputs,
        };
        collect_bound_fragment_output_uses_from_items(
            else_branch,
            scope.bindings,
            scope.current_dot,
            scope.current_dot_fragment,
            scope.relative_path,
            scope.active_output_guards,
            &mut else_state,
        );
        *state.local_bindings = merge_fragment_locals(body_bindings, else_bindings);
        *state.local_default_paths = merge_local_default_paths(body_defaults, else_defaults);
    }
}
