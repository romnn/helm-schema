use std::collections::{BTreeSet, HashMap};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::Effects;
use crate::expr_eval::{
    eval_helper_exprs_effects, eval_non_helper_output_exprs_effects, expr_starts_with_helper_call,
};
use crate::fragment_assignment::{
    apply_local_set_mutations_from_exprs, parse_helper_assignment_from_exprs,
};
use crate::fragment_expr_eval::{
    FragmentLocalFacts, helper_result_from_expr_with_fragment_locals,
    helper_result_from_exprs_with_fragment_locals,
};
use crate::helper_summary::HelperFragmentOutputUse;
use crate::helper_walk_state::FragmentOutputWalkState;
use crate::output_path;
use crate::predicate::Predicate;
use crate::yaml_syntax::parse_yaml_key;
use crate::{ValueKind, YamlPath};

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
    let fallback_paths = eval_helper_exprs_effects(exprs, bindings, current_dot).defaults;
    let result = helper_result_from_exprs_with_fragment_locals(
        exprs,
        FragmentLocalFacts::without_output_meta(
            &state.locals.bindings,
            &state.locals.default_paths,
        ),
        Some(bindings),
        current_dot,
        state.context,
        state.seen,
    );
    let local_effects = &result.effects;

    let direct_output_effects = eval_non_helper_output_exprs_effects(exprs, bindings, current_dot);
    let direct_output_uses = output_uses_from_rendered_effects(
        &direct_output_effects,
        &output_path,
        kind,
        active_output_predicates,
        &fallback_paths,
    );
    let handled_outputs: BTreeSet<String> = direct_output_uses
        .iter()
        .map(|output| output.source_expr.clone())
        .chain(local_effects.local_rendered_paths.iter().cloned())
        .collect();
    state.outputs.extend(direct_output_uses);

    let local_output_uses = local_output_uses_from_effects(
        &local_effects,
        &output_path,
        kind,
        active_output_predicates,
        &local_effects.local_default_paths,
    );

    let mut expression_output_uses = Vec::new();
    if let Some(binding) = &result.value {
        binding.collect_output_uses(
            &mut expression_output_uses,
            &output_path,
            kind,
            active_output_predicates,
            &fallback_paths,
        );
    }
    let nested = result.effects.helper_summary;
    let nested_scalar_outputs = nested.scalar_output_meta.into_iter().collect::<Vec<_>>();
    let nested_fragment_outputs = nested.fragment_output_uses;
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

    expression_output_uses
        .retain(|output| expression_output_use_is_keyed_map_projection(output, &output_path));
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
    let mut seen_rhs = state.seen.clone();
    let result = helper_result_from_expr_with_fragment_locals(
        rhs_expr,
        FragmentLocalFacts::without_output_meta(
            &state.locals.bindings,
            &state.locals.default_paths,
        ),
        Some(bindings),
        current_dot,
        state.context,
        &mut seen_rhs,
    );
    let mut binding = result.value;
    let local_default_paths = result.effects.local_default_paths.clone();
    let mut top_level_helper_dependency_paths = BTreeSet::new();
    if exprs_start_with_helper_call(rhs_exprs) {
        let nested = result.effects.helper_summary;
        let nested_binding = nested.project_value();
        top_level_helper_dependency_paths = nested.dependency_relevant_paths();
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
    let mut defaulted_paths = eval_helper_exprs_effects(rhs_exprs, bindings, current_dot).defaults;
    defaulted_paths.extend(local_default_paths);
    state.locals.set_default_paths(var, defaulted_paths);
}

fn local_output_uses_from_effects(
    effects: &Effects,
    output_path: &YamlPath,
    kind: ValueKind,
    active_output_predicates: &BTreeSet<Predicate>,
    local_fallback_paths: &BTreeSet<String>,
) -> Vec<HelperFragmentOutputUse> {
    let mut local_output_uses = Vec::new();
    for binding in &effects.local_output_values {
        binding.collect_fragment_output_uses(
            &mut local_output_uses,
            output_path,
            kind,
            active_output_predicates,
            local_fallback_paths,
        );
    }
    local_output_uses
}

fn output_uses_from_rendered_effects(
    effects: &Effects,
    output_path: &YamlPath,
    kind: ValueKind,
    active_output_predicates: &BTreeSet<Predicate>,
    fallback_paths: &BTreeSet<String>,
) -> Vec<HelperFragmentOutputUse> {
    let mut outputs = Vec::new();
    for value in &effects.rendered_output_values {
        value.collect_output_uses_with_encoding(
            &mut outputs,
            output_path,
            kind,
            &effects.encoded_paths,
            active_output_predicates,
            fallback_paths,
        );
    }
    outputs
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
