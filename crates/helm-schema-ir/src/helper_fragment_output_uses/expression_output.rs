use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::bound_helper_env::BoundHelperEnv;
use crate::fragment_assignment::{
    apply_local_set_mutations_from_exprs, parse_helper_assignment_from_exprs,
};
use crate::helper_output_projection::{
    HelperOutputExprContext, collect_output_uses_from_expr,
    expression_output_use_is_keyed_map_projection, static_yaml_fragment_output_path_from_exprs,
};
use crate::helper_summary::HelperFragmentOutputUse;
use crate::helper_walk_state::FragmentOutputWalkState;
use crate::local_projection::{
    direct_bound_paths_from_exprs_in_context, local_rendered_paths_from_exprs,
};
use crate::output_path;
use crate::predicate::Predicate;
use crate::template_expr_analysis::{
    expr_contains_helper_call, exprs_pipeline_merges_into_var, exprs_start_with_helper_call,
    walk_expr_excluding_helper_call_args,
};
use crate::{ValueKind, YamlPath};

struct FragmentExpressionOutputScope<'a> {
    bindings: &'a HashMap<String, AbstractValue>,
    current_dot: Option<&'a AbstractValue>,
    output_path: &'a YamlPath,
    kind: ValueKind,
    active_output_predicates: &'a BTreeSet<Predicate>,
    fallback_paths: &'a BTreeSet<String>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_bound_fragment_output_uses_from_exprs(
    exprs: &[TemplateExpr],
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    current_dot_fragment: Option<&AbstractValue>,
    relative_path: &YamlPath,
    output_kind: ValueKind,
    active_output_predicates: &BTreeSet<Predicate>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let mut seen_set = HashSet::new();
    if apply_local_set_mutations_from_exprs(
        exprs,
        &mut state.locals.bindings,
        current_dot_fragment,
        state.context,
        &mut seen_set,
    ) {
        return;
    }

    if let Some(assignment) = parse_helper_assignment_from_exprs(exprs) {
        collect_bound_fragment_output_assignment_uses(
            &assignment.variable,
            &assignment.rhs_expr,
            bindings,
            current_dot,
            current_dot_fragment,
            state,
        );
        return;
    }

    let kind = if matches!(output_kind, ValueKind::Fragment)
        || exprs.iter().any(TemplateExpr::renders_yaml_fragment)
    {
        ValueKind::Fragment
    } else {
        ValueKind::Scalar
    };
    let output_path = static_yaml_fragment_output_path_from_exprs(exprs)
        .map(|output_path| output_path::append_relative_path(relative_path, &output_path))
        .unwrap_or_else(|| relative_path.clone());
    let direct_outputs = direct_bound_paths_from_exprs_in_context(exprs, bindings, current_dot);
    let helper_env = BoundHelperEnv::new(bindings, current_dot, state.context);
    let fallback_paths = helper_env.external_default_fallback_paths_in_exprs(exprs);
    let local_outputs = local_rendered_paths_from_exprs(exprs, &state.locals.bindings);
    let handled_outputs: BTreeSet<String> = direct_outputs
        .iter()
        .chain(local_outputs.iter())
        .cloned()
        .collect();

    let mut direct_output_uses = Vec::new();
    for expr in exprs {
        collect_output_uses_from_expr(
            expr,
            HelperOutputExprContext {
                bindings,
                current_dot,
                relative_path: &output_path,
                kind,
                active_output_predicates,
                defaulted_paths: &fallback_paths,
            },
            &mut direct_output_uses,
        );
    }
    state.outputs.extend(direct_output_uses);

    let local_fallback_paths =
        helper_env.local_default_fallback_paths_in_exprs(exprs, &state.locals.default_paths);
    let local_output_uses = local_output_uses_from_exprs(
        exprs,
        &output_path,
        kind,
        active_output_predicates,
        &local_fallback_paths,
        &state.locals.bindings,
    );

    let nested = helper_env.summarize_calls_in_exprs(exprs, &state.locals.bindings, state.seen);
    let mut nested_fragment_outputs = Vec::new();
    let mut nested_scalar_outputs = Vec::new();
    for (path, facts) in nested.path_facts() {
        if let Some(meta) = facts.output_meta() {
            nested_scalar_outputs.push((path.to_string(), meta.clone()));
        }
        nested_fragment_outputs.extend(facts.fragment_output_uses().cloned());
    }
    let nested_structured_sources: BTreeSet<String> = nested_fragment_outputs
        .iter()
        .map(|output| output.source_expr.clone())
        .collect();
    let empty_output_path = YamlPath(Vec::new());
    let nested_descendant_structured_sources: BTreeSet<String> = nested_fragment_outputs
        .iter()
        .filter(|output| expression_output_use_is_keyed_map_projection(output, &empty_output_path))
        .map(|output| output.source_expr.clone())
        .collect();
    let nested_scalar_sources: BTreeSet<String> = nested_scalar_outputs
        .iter()
        .map(|(source_expr, _)| source_expr.clone())
        .collect();
    let nested_has_fragment_outputs = !nested_fragment_outputs.is_empty();

    let expression_output_scope = FragmentExpressionOutputScope {
        bindings,
        current_dot,
        output_path: &output_path,
        kind,
        active_output_predicates,
        fallback_paths: &fallback_paths,
    };
    let expression_output_uses =
        helper_expression_output_uses_from_exprs(exprs, expression_output_scope, state);
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
    for (source_expr, meta) in nested_scalar_outputs {
        if kind == ValueKind::Fragment && nested_has_fragment_outputs {
            continue;
        }
        if nested_structured_sources.contains(&source_expr)
            || expression_descendant_sources.contains(&source_expr)
        {
            continue;
        }
        let meta = meta.with_additional_predicates(active_output_predicates);
        state.outputs.push(HelperFragmentOutputUse::new(
            source_expr,
            relative_path.clone(),
            kind,
            meta,
        ));
    }
    for nested_output in nested_fragment_outputs {
        if kind == ValueKind::Fragment
            && nested_output.relative_path.0.is_empty()
            && (nested_scalar_sources.contains(&nested_output.source_expr)
                || nested_descendant_structured_sources.contains(&nested_output.source_expr)
                || expression_descendant_sources.contains(&nested_output.source_expr))
        {
            continue;
        }
        let meta = nested_output
            .meta
            .with_additional_predicates(active_output_predicates);
        state.outputs.push(HelperFragmentOutputUse::new(
            nested_output.source_expr,
            output_path::append_relative_path(relative_path, &nested_output.relative_path),
            nested_output.kind,
            meta,
        ));
    }
}

fn collect_bound_fragment_output_assignment_uses(
    var: &str,
    rhs_expr: &TemplateExpr,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    current_dot_fragment: Option<&AbstractValue>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let rhs_exprs = std::slice::from_ref(rhs_expr);
    let helper_env = BoundHelperEnv::new(bindings, current_dot, state.context);
    let mut seen_rhs = HashSet::new();
    let mut binding =
        helper_env.fragment_value_from_expr(rhs_expr, &state.locals.bindings, &mut seen_rhs);
    let mut top_level_helper_dependency_paths = BTreeSet::new();
    if exprs_start_with_helper_call(rhs_exprs) {
        let mut rhs_seen = state.seen.clone();
        let nested =
            helper_env.summarize_calls_in_exprs(rhs_exprs, &state.locals.bindings, &mut rhs_seen);
        let nested_binding = nested.clone().project_fragment_value();
        top_level_helper_dependency_paths = dependency_paths_from_summary(&nested);
        if let Some(nested_binding) = nested_binding {
            binding = match binding {
                Some(binding) => AbstractValue::merge_context_values(vec![binding, nested_binding]),
                None => Some(nested_binding),
            };
        }
    }
    if exprs_pipeline_merges_into_var(rhs_exprs, var)
        && let Some(current_dot_fragment) = current_dot_fragment
        && matches!(
            current_dot_fragment,
            AbstractValue::Dict(_) | AbstractValue::Overlay { .. }
        )
    {
        let current_item_paths = current_dot_fragment.fragment_source_paths();
        let mut internal_item_paths = current_item_paths;
        internal_item_paths.extend(top_level_helper_dependency_paths);
        if !internal_item_paths.is_empty() {
            binding =
                binding.and_then(|binding| binding.remove_fragment_paths(&internal_item_paths));
        }
        binding = match binding {
            Some(binding) => {
                AbstractValue::merge_context_values(vec![binding, current_dot_fragment.clone()])
            }
            None => Some(current_dot_fragment.clone()),
        };
    }
    if let Some(binding) = binding {
        state.locals.bindings.insert(var.to_string(), binding);
    }
    let mut defaulted_paths = helper_env.external_default_fallback_paths_in_exprs(rhs_exprs);
    defaulted_paths.extend(
        helper_env.local_default_fallback_paths_in_exprs(rhs_exprs, &state.locals.default_paths),
    );
    if defaulted_paths.is_empty() {
        state.locals.default_paths.remove(var);
    } else {
        state
            .locals
            .default_paths
            .insert(var.to_string(), defaulted_paths);
    }
}

fn dependency_paths_from_summary(
    summary: &crate::helper_summary::HelperSummary,
) -> BTreeSet<String> {
    let paths = summary
        .path_facts()
        .filter(|(_path, facts)| facts.is_dependency_relevant())
        .map(|(path, _facts)| path.to_string())
        .filter(|path| !path.trim().is_empty())
        .collect();
    remove_ancestor_paths(paths)
}

fn remove_ancestor_paths(paths: BTreeSet<String>) -> BTreeSet<String> {
    paths
        .iter()
        .filter(|path| !output_path::values_path_has_descendant(path, &paths))
        .cloned()
        .collect()
}

fn local_output_uses_from_exprs(
    exprs: &[TemplateExpr],
    output_path: &YamlPath,
    kind: ValueKind,
    active_output_predicates: &BTreeSet<Predicate>,
    local_fallback_paths: &BTreeSet<String>,
    local_bindings: &HashMap<String, AbstractValue>,
) -> Vec<HelperFragmentOutputUse> {
    let mut local_output_uses = Vec::new();
    for expr in exprs {
        walk_expr_excluding_helper_call_args(expr, &mut |node| {
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
                        .and_then(|binding| binding.select_fragment_path(path))
                }
                _ => None,
            };
            if let Some(binding) = binding {
                binding.collect_fragment_output_uses(
                    &mut local_output_uses,
                    output_path,
                    kind,
                    active_output_predicates,
                    local_fallback_paths,
                );
            }
        });
    }
    local_output_uses
}

fn helper_expression_output_uses_from_exprs(
    exprs: &[TemplateExpr],
    scope: FragmentExpressionOutputScope<'_>,
    state: &mut FragmentOutputWalkState<'_, '_>,
) -> Vec<HelperFragmentOutputUse> {
    let mut expression_output_uses = Vec::new();
    let mut expression_seen = state.seen.clone();
    let helper_env = BoundHelperEnv::new(scope.bindings, scope.current_dot, state.context);
    for expr in exprs {
        if !expr_contains_helper_call(expr) {
            continue;
        }
        if let Some(binding) =
            helper_env.helper_value_from_expr(expr, &state.locals.bindings, &mut expression_seen)
        {
            binding.collect_output_uses(
                &mut expression_output_uses,
                scope.output_path,
                scope.kind,
                scope.active_output_predicates,
                scope.fallback_paths,
            );
        }
    }
    expression_output_uses
        .retain(|output| expression_output_use_is_keyed_map_projection(output, scope.output_path));
    expression_output_uses
}
