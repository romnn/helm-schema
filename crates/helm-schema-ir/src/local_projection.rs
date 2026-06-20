use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::helper_summary::HelperOutputMeta;
use crate::template_expr_analysis::{
    expr_contains_helper_call, walk_expr_excluding_helper_call_args,
};

pub(crate) fn direct_bound_paths_from_exprs_in_context(
    exprs: &[TemplateExpr],
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
) -> BTreeSet<String> {
    let env = EvalEnv::from_helper_context(Some(bindings), current_dot);
    exprs
        .iter()
        .flat_map(|expr| direct_bound_paths_from_expr_in_context(expr, &env))
        .collect()
}

pub(crate) fn direct_bound_paths_from_expr_in_context(
    expr: &TemplateExpr,
    env: &EvalEnv,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    walk_expr_excluding_helper_call_args(expr, &mut |node| {
        if expr_contains_helper_call(node) {
            return;
        }
        if let Some(value) = eval_expr(node, env).value {
            out.extend(value.paths());
        }
    });
    out
}

pub(crate) fn local_rendered_paths_from_exprs(
    exprs: &[TemplateExpr],
    locals: &HashMap<String, AbstractValue>,
) -> BTreeSet<String> {
    local_paths_from_exprs(exprs, locals, AbstractValue::fragment_rendered_paths)
}

fn local_paths_from_exprs(
    exprs: &[TemplateExpr],
    locals: &HashMap<String, AbstractValue>,
    extract_paths: fn(&AbstractValue) -> BTreeSet<String>,
) -> BTreeSet<String> {
    exprs
        .iter()
        .flat_map(|expr| local_paths_from_expr(expr, locals, extract_paths))
        .collect()
}

pub(crate) fn local_bound_paths_from_expr(
    expr: &TemplateExpr,
    locals: &HashMap<String, AbstractValue>,
) -> BTreeSet<String> {
    local_paths_from_expr(expr, locals, AbstractValue::fragment_source_paths)
}

fn local_paths_from_expr(
    expr: &TemplateExpr,
    locals: &HashMap<String, AbstractValue>,
    extract_paths: fn(&AbstractValue) -> BTreeSet<String>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    walk_expr_excluding_helper_call_args(expr, &mut |node| match node {
        TemplateExpr::Variable(var) if !var.is_empty() => {
            if let Some(binding) = locals.get(var) {
                out.extend(extract_paths(binding));
            }
        }
        TemplateExpr::Selector { operand, path } => {
            let TemplateExpr::Variable(var) = operand.as_ref() else {
                return;
            };
            if var.is_empty() {
                return;
            }
            if let Some(binding) = locals.get(var)
                && let Some(bound) = binding.select_fragment_path(path)
            {
                out.extend(extract_paths(&bound));
            }
        }
        _ => {}
    });
    out
}

pub(crate) fn local_default_paths_from_exprs(
    exprs: &[TemplateExpr],
    local_default_paths: &HashMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for expr in exprs {
        expr.walk(|node| {
            let TemplateExpr::Variable(var) = node else {
                return;
            };
            if var.is_empty() {
                return;
            }
            if let Some(paths) = local_default_paths.get(var) {
                out.extend(paths.iter().cloned());
            }
        });
    }
    out
}

pub(crate) fn local_output_meta_from_exprs(
    exprs: &[TemplateExpr],
    local_bindings: &HashMap<String, AbstractValue>,
    local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut out: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
    for expr in exprs {
        walk_expr_excluding_helper_call_args(expr, &mut |node| {
            for (path, meta) in local_output_meta_from_expr(node, local_bindings, local_output_meta)
            {
                out.entry(path).or_default().merge(meta);
            }
        });
    }
    out
}

fn local_output_meta_from_expr(
    expr: &TemplateExpr,
    local_bindings: &HashMap<String, AbstractValue>,
    local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
) -> BTreeMap<String, HelperOutputMeta> {
    match expr {
        TemplateExpr::Variable(var) if !var.is_empty() => {
            local_output_meta.get(var).cloned().unwrap_or_default()
        }
        TemplateExpr::Selector { operand, path } => {
            let TemplateExpr::Variable(var) = operand.as_ref() else {
                return BTreeMap::new();
            };
            if var.is_empty() {
                return BTreeMap::new();
            }
            let Some(binding) = local_bindings.get(var) else {
                return BTreeMap::new();
            };
            let Some(bound) = binding.select_fragment_path(path) else {
                return BTreeMap::new();
            };
            let selected_paths = bound.fragment_source_paths();
            local_output_meta
                .get(var)
                .into_iter()
                .flat_map(|meta_by_path| meta_by_path.iter())
                .filter(|(path, _meta)| selected_paths.contains(*path))
                .map(|(path, meta)| (path.clone(), meta.clone()))
                .collect()
        }
        _ => BTreeMap::new(),
    }
}
