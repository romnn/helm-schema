use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::TemplateExpr;

use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::fragment_binding::FragmentBinding;
use crate::fragment_binding_projection::{
    fragment_rendered_paths, fragment_source_paths, select_fragment_binding,
};
use crate::helper_analysis::HelperOutputMeta;
use crate::helper_binding::HelperBinding;
use crate::template_expr_analysis::{
    expr_contains_helper_call, walk_expr_excluding_helper_call_args,
};
use crate::template_expr_cache::parse_expr_text;

pub(crate) fn direct_bound_paths_from_text_in_context(
    text: &str,
    bindings: &HashMap<String, HelperBinding>,
    current_dot: Option<&HelperBinding>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let env = EvalEnv::from_helper_context(Some(bindings), current_dot);
    for expr in parse_expr_text(text) {
        walk_expr_excluding_helper_call_args(&expr, &mut |node| {
            if expr_contains_helper_call(node) {
                return;
            }
            if let Some(value) = eval_expr(node, &env).value {
                out.extend(value.paths());
            }
        });
    }
    out
}

pub(crate) fn local_bound_paths_from_text(
    text: &str,
    locals: &HashMap<String, FragmentBinding>,
) -> BTreeSet<String> {
    local_paths_from_text(text, locals, fragment_source_paths)
}

pub(crate) fn local_rendered_paths_from_text(
    text: &str,
    locals: &HashMap<String, FragmentBinding>,
) -> BTreeSet<String> {
    local_paths_from_text(text, locals, fragment_rendered_paths)
}

fn local_paths_from_text(
    text: &str,
    locals: &HashMap<String, FragmentBinding>,
    extract_paths: fn(&FragmentBinding) -> BTreeSet<String>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for expr in parse_expr_text(text) {
        walk_expr_excluding_helper_call_args(&expr, &mut |node| match node {
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
                    && let Some(bound) = select_fragment_binding(binding, path)
                {
                    out.extend(extract_paths(&bound));
                }
            }
            _ => {}
        });
    }
    out
}

pub(crate) fn local_default_paths_from_text(
    text: &str,
    local_default_paths: &HashMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for expr in parse_expr_text(text) {
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

pub(crate) fn local_output_meta_from_text(
    text: &str,
    local_bindings: &HashMap<String, FragmentBinding>,
    local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut out: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
    for expr in parse_expr_text(text) {
        walk_expr_excluding_helper_call_args(&expr, &mut |node| {
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
    local_bindings: &HashMap<String, FragmentBinding>,
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
            let Some(bound) = select_fragment_binding(binding, path) else {
                return BTreeMap::new();
            };
            let selected_paths = fragment_source_paths(&bound);
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
