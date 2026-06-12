use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::binding::{FragmentBinding, HelperBinding};
use crate::helper_analysis::BoundHelperAnalysis;
use crate::helper_binding_eval::binding_from_expr;
use crate::output_path;
use crate::walker::values_path_from_expr;

pub(crate) fn fragment_binding_from_helper_analysis(
    mut analysis: BoundHelperAnalysis,
) -> Option<FragmentBinding> {
    let structured_sources: BTreeSet<String> = analysis
        .fragment_output_uses
        .iter()
        .map(|output| output.source_expr.clone())
        .collect();
    let mut rendered_sources = structured_sources.clone();
    rendered_sources.extend(analysis.fragment_output.iter().cloned());
    rendered_sources.extend(analysis.output.keys().cloned());
    let mut bindings = Vec::new();
    if !analysis.string_output.is_empty() {
        bindings.push(FragmentBinding::StringSet(analysis.string_output.clone()));
    }
    for output in analysis.fragment_output_uses.drain(..) {
        bindings.push(FragmentBinding::for_output_path(
            output.source_expr,
            &output.relative_path,
        ));
    }
    for source in analysis.fragment_output {
        if !structured_sources.contains(&source)
            && !output_path::values_path_has_descendant(&source, &rendered_sources)
        {
            bindings.push(FragmentBinding::OutputSet([source].into_iter().collect()));
        }
    }
    for source in analysis.output.into_keys() {
        if !structured_sources.contains(&source)
            && !output_path::values_path_has_descendant(&source, &rendered_sources)
        {
            bindings.push(FragmentBinding::OutputSet([source].into_iter().collect()));
        }
    }
    FragmentBinding::merge_all(bindings)
}

pub(crate) fn helper_binding_from_helper_analysis(
    mut analysis: BoundHelperAnalysis,
) -> Option<HelperBinding> {
    let structured_sources: BTreeSet<String> = analysis
        .fragment_output_uses
        .iter()
        .map(|output| output.source_expr.clone())
        .collect();
    let mut rendered_sources = structured_sources.clone();
    rendered_sources.extend(analysis.fragment_output.iter().cloned());
    rendered_sources.extend(analysis.output.keys().cloned());

    let mut bindings = Vec::new();
    if !analysis.string_output.is_empty() {
        bindings.push(HelperBinding::StringSet(analysis.string_output.clone()));
    }
    for output in analysis.fragment_output_uses.drain(..) {
        bindings.push(HelperBinding::for_output_path(
            output.source_expr,
            &output.relative_path,
            output.meta,
        ));
    }
    for source in analysis.fragment_output {
        if !structured_sources.contains(&source)
            && !output_path::values_path_has_descendant(&source, &rendered_sources)
        {
            bindings.push(HelperBinding::PathSet([source].into_iter().collect()));
        }
    }
    for (source, meta) in analysis.output {
        if !structured_sources.contains(&source)
            && !output_path::values_path_has_descendant(&source, &rendered_sources)
        {
            bindings.push(HelperBinding::OutputSet(
                [(source, meta)].into_iter().collect(),
            ));
        }
    }
    HelperBinding::merge_all(bindings)
}

pub(crate) fn fragment_binding_from_outer_expr(
    expr: &TemplateExpr,
    outer_locals: Option<&HashMap<String, FragmentBinding>>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
) -> Option<FragmentBinding> {
    if let Some(path) = values_path_from_expr(expr) {
        return Some(FragmentBinding::ValuesPath(path));
    }

    match expr {
        TemplateExpr::Literal(Literal::String(value)) => Some(FragmentBinding::StringSet(
            [value.clone()].into_iter().collect(),
        )),
        TemplateExpr::Parenthesized(inner) => {
            fragment_binding_from_outer_expr(inner, outer_locals, outer, current_dot)
        }
        TemplateExpr::Field(path) if path.is_empty() => {
            if let Some(bindings) = outer {
                return Some(FragmentBinding::Dict(
                    bindings
                        .iter()
                        .map(|(key, binding)| (key.clone(), binding.to_fragment_binding()))
                        .collect(),
                ));
            }
            current_dot
                .map(HelperBinding::to_fragment_binding)
                .or(Some(FragmentBinding::RootContext))
        }
        TemplateExpr::Field(path) if path.first().is_some_and(|segment| segment == "Values") => {
            Some(FragmentBinding::ValuesPath(path[1..].join(".")))
        }
        TemplateExpr::Variable(var) if var.is_empty() => {
            if let Some(bindings) = outer {
                return Some(FragmentBinding::Dict(
                    bindings
                        .iter()
                        .map(|(key, binding)| (key.clone(), binding.to_fragment_binding()))
                        .collect(),
                ));
            }
            Some(FragmentBinding::RootContext)
        }
        TemplateExpr::Variable(var) if !var.is_empty() => {
            outer_locals.and_then(|locals| locals.get(var).cloned())
        }
        TemplateExpr::Call { function, args } if matches!(function.as_str(), "list" | "tuple") => {
            let mut items = Vec::new();
            for arg in args {
                items.push(
                    fragment_binding_from_outer_expr(arg, outer_locals, outer, current_dot)
                        .unwrap_or(FragmentBinding::Unknown),
                );
            }
            Some(FragmentBinding::List(items))
        }
        TemplateExpr::Call { function, args } if function == "dict" => {
            let mut map = BTreeMap::new();
            let mut index = 0usize;
            while index + 1 < args.len() {
                let TemplateExpr::Literal(Literal::String(key)) = &args[index] else {
                    index += 1;
                    continue;
                };
                if let Some(binding) = fragment_binding_from_outer_expr(
                    &args[index + 1],
                    outer_locals,
                    outer,
                    current_dot,
                ) {
                    map.insert(key.clone(), binding);
                }
                index += 2;
            }
            Some(FragmentBinding::Dict(map))
        }
        TemplateExpr::Call { function, args } if function == "coalesce" => {
            let mut choices = Vec::new();
            for arg in args {
                if let Some(binding) =
                    fragment_binding_from_outer_expr(arg, outer_locals, outer, current_dot)
                {
                    choices.push(binding);
                }
            }
            FragmentBinding::choice(choices)
        }
        TemplateExpr::Call { function, args } if function == "ternary" => {
            let mut choices = Vec::new();
            for arg in args.iter().take(2) {
                if let Some(binding) =
                    fragment_binding_from_outer_expr(arg, outer_locals, outer, current_dot)
                {
                    choices.push(binding);
                }
            }
            FragmentBinding::choice(choices)
        }
        _ => {
            binding_from_expr(expr, outer, current_dot).map(|binding| binding.to_fragment_binding())
        }
    }
}
