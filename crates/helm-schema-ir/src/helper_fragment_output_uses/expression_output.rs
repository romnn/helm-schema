use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::is_merge_function;
use helm_schema_ast::{Literal, TemplateExpr, parse_yaml_key};

use crate::abstract_value::{AbstractValue, OutputProjectionScope};
use crate::expr_eval::{eval_helper_exprs_direct_effects, expr_starts_with_helper_call};
use crate::fragment_assignment::{
    AssignmentKind, apply_local_set_mutations_from_exprs, parse_helper_assignment_from_exprs,
};
use crate::fragment_expr_eval::{
    FragmentLocalFacts, helper_result_from_expr_with_fragment_locals,
    helper_result_from_exprs_with_fragment_locals,
};
use crate::helper_summary::{
    HelperFragmentOutputUse, HelperOutputMeta, NestedDependencyRows, relate_outputs_to_sources,
};
use crate::helper_walk_state::FragmentOutputWalkState;
use crate::{ValueKind, YamlPath};
use helm_schema_core as output_path;
use helm_schema_core::Predicate;

#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_bound_fragment_output_uses_from_exprs(
    exprs: &[TemplateExpr],
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    current_dot_fragment: Option<&AbstractValue>,
    relative_path: &YamlPath,
    output_kind: ValueKind,
    active_output_predicates: &BTreeSet<Predicate>,
    active_source_relations: &[BTreeSet<String>],
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let mut seen_set = state.seen.clone();
    if apply_local_set_mutations_from_exprs(
        exprs,
        &mut state.locals.fragment_values,
        current_dot_fragment,
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

    if let Some(assignment) = parse_helper_assignment_from_exprs(exprs) {
        collect_bound_fragment_output_assignment_uses(
            &assignment.variable,
            assignment.kind,
            &assignment.rhs_expr,
            bindings,
            current_dot,
            current_dot_fragment,
            active_output_predicates,
            active_source_relations,
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
    let local_effects = &result.effects;
    state
        .analysis
        .chart_defaults
        .extend(local_effects.chart_default_paths.iter().cloned());
    state
        .analysis
        .add_type_hints(local_effects.type_hints.clone());
    if kind == ValueKind::Scalar && relative_path.0.is_empty() {
        state
            .analysis
            .string_output
            .extend(result.value.iter().flat_map(AbstractValue::strings));
    }

    let mut expression_output_uses = Vec::new();
    let expression_default_paths = local_effects.default_paths_with_local();
    if let Some(binding) = &result.value {
        binding.collect_output_uses(
            &mut expression_output_uses,
            &output_path,
            kind,
            &OutputProjectionScope {
                root: &output_path,
                encoded_paths: &local_effects.encoded_paths,
                active_output_predicates,
                defaulted_paths: &expression_default_paths,
                path_meta: &local_effects.local_output_meta,
                local_rendered_paths: &local_effects.local_rendered_paths,
                local_defaulted_paths: &local_effects.local_default_paths,
            },
        );
        relate_outputs_to_active_sources(&mut expression_output_uses, active_source_relations);
    }
    let mut expression_sources = rendered_sources(expression_output_uses.iter());
    expression_sources.extend(local_effects.local_rendered_paths.iter().cloned());
    note_outputs_sibling_sources(&mut expression_output_uses, &expression_sources);
    let nested_summary = result.effects.helper_summary;
    if kind == ValueKind::Scalar {
        state
            .analysis
            .string_output
            .extend(nested_summary.string_output.iter().cloned());
    }
    state.analysis.absorb_nested_dependencies(
        &nested_summary,
        NestedDependencyRows::DependenciesOnly,
        active_output_predicates,
        active_source_relations,
    );
    let nested_outputs = nested_summary
        .output_uses
        .into_iter()
        .filter(|output| output.is_rendered())
        .collect::<Vec<_>>();
    let nested_structured_outputs = nested_outputs
        .iter()
        .filter(|output| output.is_structured_output())
        .cloned()
        .collect::<Vec<_>>();
    let nested_scalar_outputs = nested_outputs
        .iter()
        .filter(|output| output.is_scalar_summary_output())
        .cloned()
        .collect::<Vec<_>>();
    let nested_structured_sources: BTreeSet<String> = nested_structured_outputs
        .iter()
        .map(|output| output.source_expr.clone())
        .collect();
    let empty_output_path = YamlPath(Vec::new());
    let nested_descendant_structured_sources: BTreeSet<String> = nested_structured_outputs
        .iter()
        .filter(|output| expression_output_use_is_keyed_map_projection(output, &empty_output_path))
        .map(|output| output.source_expr.clone())
        .collect();
    let nested_scalar_sources: BTreeSet<String> = nested_scalar_outputs
        .iter()
        .map(|output| output.source_expr.clone())
        .collect();
    let nested_has_structured_outputs = !nested_structured_outputs.is_empty();
    let sequence_output_path = output_path::sequence_item_path(&output_path);

    expression_output_uses.retain(|output| {
        (kind == ValueKind::Fragment && output.relative_path.0.is_empty())
            || (kind == ValueKind::Scalar
                && output_path.0.is_empty()
                && output.relative_path.0.is_empty())
            || (!output_path.0.is_empty() && output.relative_path == output_path)
            || (kind == ValueKind::Scalar
                && !sequence_output_path.0.is_empty()
                && output.relative_path == sequence_output_path)
            || expression_output_use_is_keyed_map_projection(output, &output_path)
    });
    let expression_descendant_sources: BTreeSet<String> = expression_output_uses
        .iter()
        .filter(|output| !output.relative_path.0.is_empty())
        .map(|output| output.source_expr.clone())
        .collect();

    for output in expression_output_uses {
        if output.relative_path.0.is_empty()
            && (nested_structured_sources.contains(&output.source_expr)
                || nested_scalar_sources.contains(&output.source_expr))
        {
            continue;
        }
        state.outputs.push(output);
    }
    for nested_output in nested_scalar_outputs {
        if kind == ValueKind::Fragment && nested_has_structured_outputs {
            continue;
        }
        if nested_structured_sources.contains(&nested_output.source_expr)
            || expression_descendant_sources.contains(&nested_output.source_expr)
        {
            continue;
        }
        let mut meta = nested_output
            .meta
            .with_output_site_predicates(active_output_predicates);
        meta.relate_source_relations(active_source_relations);
        state.outputs.push(HelperFragmentOutputUse::new(
            nested_output.source_expr,
            relative_path.clone(),
            kind,
            meta,
        ));
    }
    for nested_output in nested_structured_outputs {
        if kind == ValueKind::Fragment
            && nested_output.relative_path.0.is_empty()
            && (nested_scalar_sources.contains(&nested_output.source_expr)
                || nested_descendant_structured_sources.contains(&nested_output.source_expr)
                || expression_descendant_sources.contains(&nested_output.source_expr))
        {
            continue;
        }
        let mut meta = nested_output
            .meta
            .with_output_site_predicates(active_output_predicates);
        meta.relate_source_relations(active_source_relations);
        if yaml_path_contains_sequence(relative_path) && !nested_output.relative_path.0.is_empty() {
            state.outputs.push(HelperFragmentOutputUse::new(
                nested_output.source_expr.clone(),
                nested_output.relative_path.clone(),
                nested_output.kind,
                meta.clone(),
            ));
        }
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
    assignment_kind: AssignmentKind,
    rhs_expr: &TemplateExpr,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    current_dot_fragment: Option<&AbstractValue>,
    active_output_predicates: &BTreeSet<Predicate>,
    active_source_relations: &[BTreeSet<String>],
    state: &mut FragmentOutputWalkState<'_, '_>,
) {
    let rhs_exprs = std::slice::from_ref(rhs_expr);
    let mut seen_rhs = state.seen.clone();
    let result = helper_result_from_expr_with_fragment_locals(
        rhs_expr,
        FragmentLocalFacts::without_output_meta(
            &state.locals.fragment_values,
            &state.locals.default_paths,
        ),
        Some(bindings),
        current_dot,
        state.context,
        &mut seen_rhs,
    );
    let mut binding = result.value.and_then(AbstractValue::without_widened);
    let local_default_paths = result.effects.local_default_paths.clone();
    let mut output_meta = result.effects.local_output_meta.clone();
    if assignment_kind == AssignmentKind::Assignment && !active_output_predicates.is_empty() {
        output_meta = output_meta
            .into_iter()
            .map(|(path, meta)| {
                let meta = meta.with_output_site_predicates(active_output_predicates);
                (path, meta)
            })
            .collect();
    }
    let rhs_carries_local_output_meta = !result.effects.local_output_meta.is_empty();
    let rhs_starts_with_fragment_local =
        exprs_start_with_local_binding(rhs_exprs, &state.locals.fragment_values);
    let carried_output_meta = state
        .locals
        .output_meta
        .get(var)
        .cloned()
        .unwrap_or_default();
    state
        .analysis
        .chart_defaults
        .extend(result.effects.chart_default_paths.clone());
    state
        .analysis
        .add_type_hints(result.effects.type_hints.clone());
    let mut top_level_helper_dependency_paths = BTreeSet::new();
    let nested = result.effects.helper_summary;
    let direct_helper_assignment = exprs_start_with_helper_call(rhs_exprs);
    let rhs_merges_into_var = exprs_merge_into_var(rhs_exprs, var);
    let emit_nested_dependencies =
        direct_helper_assignment || binding.is_none() || rhs_merges_into_var;
    if nested.has_document_value_facts() {
        if emit_nested_dependencies {
            top_level_helper_dependency_paths = nested.dependency_relevant_paths();
        }
        if direct_helper_assignment {
            merge_output_use_meta(&mut output_meta, &nested.output_uses);
        }
        if emit_nested_dependencies {
            state.analysis.absorb_nested_dependencies(
                &nested,
                NestedDependencyRows::AllRows,
                active_output_predicates,
                active_source_relations,
            );
        }
    }
    let mut merged_current_item_paths = BTreeSet::new();
    if rhs_merges_into_var
        && let Some(current_dot_fragment) = current_dot_fragment
        && matches!(
            current_dot_fragment,
            AbstractValue::Dict(_) | AbstractValue::Overlay { .. }
        )
    {
        let current_item_paths = current_dot_fragment.fragment_rendered_paths();
        merged_current_item_paths = current_item_paths.clone();
        let mut internal_item_paths = current_item_paths.clone();
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
        for (path, meta) in carried_output_meta {
            if current_item_paths.contains(&path) {
                output_meta.insert(path, meta);
            }
        }
    }
    let mut defaulted_paths = result.effects.defaults.clone();
    defaulted_paths.extend(local_default_paths);
    let branch_predicates_apply_to_assignment =
        assignment_kind == AssignmentKind::Assignment && !active_output_predicates.is_empty();
    if (rhs_carries_local_output_meta
        || rhs_starts_with_fragment_local
        || branch_predicates_apply_to_assignment)
        && let Some(binding) = &binding
    {
        let mut assigned_outputs = Vec::new();
        binding.collect_output_uses(
            &mut assigned_outputs,
            &YamlPath(Vec::new()),
            ValueKind::Scalar,
            &OutputProjectionScope {
                root: &YamlPath(Vec::new()),
                encoded_paths: &result.effects.encoded_paths,
                active_output_predicates,
                defaulted_paths: &defaulted_paths,
                path_meta: &BTreeMap::new(),
                local_rendered_paths: &BTreeSet::new(),
                local_defaulted_paths: &BTreeSet::new(),
            },
        );
        relate_outputs_to_active_sources(&mut assigned_outputs, active_source_relations);
        merge_output_use_meta(&mut output_meta, &assigned_outputs);
    }
    if rhs_merges_into_var && let Some(binding) = &binding {
        let sibling_sources = binding.fragment_rendered_paths();
        if sibling_sources.len() > 1 {
            for (path, meta) in &mut output_meta {
                if !merged_current_item_paths.contains(path) {
                    continue;
                }
                meta.note_sibling_sources(path, &sibling_sources);
                meta.require_sibling_guards();
            }
        }
    }
    if let Some(binding) = binding {
        state
            .locals
            .fragment_values
            .insert(var.to_string(), binding);
    }
    state.locals.set_default_paths(var, defaulted_paths);
    state.locals.set_output_meta(var.to_string(), output_meta);
}

fn merge_output_use_meta(
    output_meta: &mut BTreeMap<String, HelperOutputMeta>,
    outputs: &[HelperFragmentOutputUse],
) {
    for output in outputs {
        if output.is_dependency() {
            continue;
        }
        output_meta
            .entry(output.source_expr.clone())
            .or_default()
            .merge_ref(&output.meta);
    }
}

fn relate_outputs_to_active_sources(
    outputs: &mut [HelperFragmentOutputUse],
    active_source_relations: &[BTreeSet<String>],
) {
    for sources in active_source_relations {
        relate_outputs_to_sources(outputs, sources);
    }
}

fn rendered_sources<'a>(
    outputs: impl Iterator<Item = &'a HelperFragmentOutputUse>,
) -> BTreeSet<String> {
    outputs
        .filter(|output| output.is_rendered())
        .map(|output| output.source_expr.clone())
        .collect()
}

fn note_outputs_sibling_sources(
    outputs: &mut [HelperFragmentOutputUse],
    sources: &BTreeSet<String>,
) {
    if sources.len() < 2 {
        return;
    }
    for output in outputs.iter_mut().filter(|output| output.is_rendered()) {
        output
            .meta
            .note_sibling_sources(&output.source_expr, &sources);
    }
}

fn expression_output_use_is_keyed_map_projection(
    output: &HelperFragmentOutputUse,
    expression_base: &YamlPath,
) -> bool {
    let suffix = if output.relative_path.0.starts_with(&expression_base.0) {
        &output.relative_path.0[expression_base.0.len()..]
    } else {
        output.relative_path.0.as_slice()
    };
    !suffix.is_empty() && suffix.iter().all(|segment| !segment.ends_with("[*]"))
}

fn yaml_path_contains_sequence(path: &YamlPath) -> bool {
    path.0.iter().any(|segment| segment.ends_with("[*]"))
}

fn static_yaml_fragment_output_path_from_exprs(exprs: &[TemplateExpr]) -> Option<YamlPath> {
    fn printf_format(expr: &TemplateExpr) -> Option<&str> {
        match expr {
            TemplateExpr::Parenthesized(inner) => printf_format(inner),
            TemplateExpr::Call { function, args } if function == "printf" => {
                let TemplateExpr::Literal(Literal::String(format) | Literal::RawString(format)) =
                    args.first()?
                else {
                    return None;
                };
                Some(format)
            }
            TemplateExpr::Pipeline(stages) => stages.first().and_then(printf_format),
            _ => None,
        }
    }

    let [expr] = exprs else {
        return None;
    };
    let format = printf_format(expr)?;
    let key = parse_yaml_key(format.trim_start())?.into_key();
    Some(YamlPath(vec![key]))
}

fn exprs_start_with_helper_call(exprs: &[TemplateExpr]) -> bool {
    matches!(exprs, [expr] if expr_starts_with_helper_call(expr))
}

fn exprs_start_with_local_binding(
    exprs: &[TemplateExpr],
    locals: &HashMap<String, AbstractValue>,
) -> bool {
    matches!(exprs, [expr] if crate::expr_eval::expr_leading_variable(expr).is_some_and(|name| locals.contains_key(name)))
}

fn exprs_merge_into_var(exprs: &[TemplateExpr], var: &str) -> bool {
    exprs
        .iter()
        .any(|expr| expr_merge_call_targets_var(expr, var))
}

fn expr_merge_call_targets_var(expr: &TemplateExpr, var: &str) -> bool {
    match expr.deparen() {
        TemplateExpr::Call { function, args } if is_merge_function(function) => args
            .iter()
            .any(|arg| expr_references_assignment_target(arg, var)),
        TemplateExpr::Pipeline(stages) => stages
            .iter()
            .any(|stage| expr_merge_call_targets_var(stage, var)),
        _ => false,
    }
}

fn expr_references_assignment_target(expr: &TemplateExpr, var: &str) -> bool {
    let target = var.trim_start_matches('$');
    match expr.deparen() {
        TemplateExpr::Variable(name) => name == var || name == target,
        TemplateExpr::Field(path) => path.last().is_some_and(|segment| segment == target),
        TemplateExpr::Selector { operand, path } => {
            expr_references_assignment_target(operand, var)
                || path.last().is_some_and(|segment| segment == target)
        }
        TemplateExpr::Pipeline(stages) => stages
            .iter()
            .any(|stage| expr_references_assignment_target(stage, var)),
        TemplateExpr::Call { args, .. } => args
            .iter()
            .any(|arg| expr_references_assignment_target(arg, var)),
        TemplateExpr::Parenthesized(inner) => expr_references_assignment_target(inner, var),
        _ => false,
    }
}
