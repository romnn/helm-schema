use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::{Literal, TemplateExpr, parse_yaml_key};

use crate::abstract_value::AbstractValue;
use crate::expr_eval::expr_starts_with_helper_call;
use crate::fragment_assignment::{
    apply_local_set_mutations_from_exprs, parse_helper_assignment_from_exprs,
};
use crate::fragment_expr_eval::{
    FragmentLocalFacts, helper_result_from_expr_with_fragment_locals,
    helper_result_from_exprs_with_fragment_locals,
};
use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta};
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
    let result = helper_result_from_exprs_with_fragment_locals(
        exprs,
        FragmentLocalFacts::without_output_meta(
            &state.locals.fragment_values,
            &state.locals.default_paths,
        ),
        Some(bindings),
        current_dot,
        state.context,
        state.seen,
    );
    let fallback_paths = result.effects.defaults.clone();
    let local_effects = &result.effects;

    let handled_outputs: BTreeSet<String> = local_effects.local_rendered_paths();

    let local_output_uses =
        local_effects.local_output_uses(&output_path, kind, active_output_predicates);

    let mut expression_output_uses = Vec::new();
    let mut expression_default_paths = fallback_paths.clone();
    expression_default_paths.extend(local_effects.local_default_paths.iter().cloned());
    if let Some(binding) = &result.value {
        binding.collect_output_uses_with_encoding(
            &mut expression_output_uses,
            &output_path,
            kind,
            &local_effects.encoded_paths,
            active_output_predicates,
            &expression_default_paths,
            true,
        );
    }
    if !exprs.iter().any(expr_contains_helper_call)
        && let Some(value) = AbstractValue::path_choices(local_effects.output_paths.clone())
    {
        let existing = expression_output_uses
            .iter()
            .map(|output| (output.source_expr.clone(), output.relative_path.clone()))
            .collect::<BTreeSet<_>>();
        let mut effect_output_uses = Vec::new();
        value.collect_output_uses_with_encoding(
            &mut effect_output_uses,
            &output_path,
            kind,
            &local_effects.encoded_paths,
            active_output_predicates,
            &expression_default_paths,
            true,
        );
        for output in &mut effect_output_uses {
            if kind == ValueKind::Scalar
                && output.source_expr.ends_with(".*")
                && output.relative_path == output_path
                && !existing.contains(&(output.source_expr.clone(), output.relative_path.clone()))
            {
                output.relative_path = output_path::sequence_item_path(&output.relative_path);
            }
        }
        expression_output_uses.extend(effect_output_uses.into_iter().filter(|output| {
            !existing.contains(&(output.source_expr.clone(), output.relative_path.clone()))
        }));
    }
    let nested_outputs = result.effects.helper_summary.output_uses;
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
    for nested_output in nested_scalar_outputs {
        if kind == ValueKind::Fragment && nested_has_structured_outputs {
            continue;
        }
        if nested_structured_sources.contains(&nested_output.source_expr)
            || expression_descendant_sources.contains(&nested_output.source_expr)
        {
            continue;
        }
        let meta = nested_output
            .meta
            .with_output_site_predicates(&nested_output.source_expr, active_output_predicates);
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
        let meta = nested_output
            .meta
            .with_output_site_predicates(&nested_output.source_expr, active_output_predicates);
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
    let mut binding = result.value;
    let local_default_paths = result.effects.local_default_paths.clone();
    let mut output_meta = result.effects.local_output_meta.clone();
    let mut top_level_helper_dependency_paths = BTreeSet::new();
    if exprs_start_with_helper_call(rhs_exprs) {
        let nested = result.effects.helper_summary;
        let nested_binding = nested.project_value();
        top_level_helper_dependency_paths = nested.dependency_relevant_paths();
        merge_output_use_meta(&mut output_meta, &nested.output_uses);
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
        state
            .locals
            .fragment_values
            .insert(var.to_string(), binding);
    }
    let mut defaulted_paths = result.effects.defaults.clone();
    defaulted_paths.extend(local_default_paths);
    state.locals.set_default_paths(var, defaulted_paths);
    state.locals.set_output_meta(var.to_string(), output_meta);
}

fn merge_output_use_meta(
    output_meta: &mut BTreeMap<String, HelperOutputMeta>,
    outputs: &[HelperFragmentOutputUse],
) {
    for output in outputs {
        output_meta
            .entry(output.source_expr.clone())
            .or_default()
            .merge_ref(&output.meta);
    }
}

fn expr_contains_helper_call(expr: &TemplateExpr) -> bool {
    let mut found = false;
    expr.walk(|node| {
        if let TemplateExpr::Call { function, .. } = node
            && matches!(function.as_str(), "include" | "template")
        {
            found = true;
        }
    });
    found
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

fn exprs_pipeline_merges_into_var(exprs: &[TemplateExpr], var: &str) -> bool {
    let [TemplateExpr::Pipeline(stages)] = exprs else {
        return false;
    };
    stages.iter().skip(1).any(|stage| {
        let TemplateExpr::Call { function, args } = stage else {
            return false;
        };
        matches!(
            function.as_str(),
            "merge" | "mustMerge" | "mergeOverwrite" | "mustMergeOverwrite"
        ) && args
            .iter()
            .any(|arg| matches!(arg, TemplateExpr::Variable(name) if name == var))
    })
}
