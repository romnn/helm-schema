use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::helper_summary::HelperOutputMeta;
use crate::template_expr_analysis::{
    expr_contains_helper_call, walk_expr_excluding_helper_call_args,
};

#[derive(Default)]
pub(crate) struct LocalExpressionFacts {
    pub(crate) source_paths: BTreeSet<String>,
    pub(crate) rendered_paths: BTreeSet<String>,
    pub(crate) default_paths: BTreeSet<String>,
    pub(crate) output_meta: BTreeMap<String, HelperOutputMeta>,
}

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

pub(crate) fn local_expression_facts_from_exprs(
    exprs: &[TemplateExpr],
    local_bindings: &HashMap<String, AbstractValue>,
    local_default_paths: &HashMap<String, BTreeSet<String>>,
    local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
) -> LocalExpressionFacts {
    let mut facts = LocalExpressionFacts::default();
    for expr in exprs {
        collect_local_binding_facts(expr, local_bindings, local_output_meta, &mut facts);
        collect_local_default_paths(expr, local_default_paths, &mut facts.default_paths);
    }
    facts
}

fn collect_local_binding_facts(
    expr: &TemplateExpr,
    local_bindings: &HashMap<String, AbstractValue>,
    local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    facts: &mut LocalExpressionFacts,
) {
    walk_expr_excluding_helper_call_args(expr, &mut |node| match node {
        TemplateExpr::Variable(var) if !var.is_empty() => {
            let Some(binding) = local_bindings.get(var) else {
                return;
            };
            facts.source_paths.extend(binding.fragment_source_paths());
            facts
                .rendered_paths
                .extend(binding.fragment_rendered_paths());
            if let Some(meta_by_path) = local_output_meta.get(var) {
                merge_output_meta(&mut facts.output_meta, meta_by_path.iter());
            }
        }
        TemplateExpr::Selector { operand, path } => {
            let TemplateExpr::Variable(var) = operand.as_ref() else {
                return;
            };
            if var.is_empty() {
                return;
            }
            let Some(binding) = local_bindings.get(var) else {
                return;
            };
            let Some(bound) = binding.select_fragment_path(path) else {
                return;
            };
            let selected_paths = bound.fragment_source_paths();
            facts.source_paths.extend(selected_paths.iter().cloned());
            facts.rendered_paths.extend(bound.fragment_rendered_paths());
            if let Some(meta_by_path) = local_output_meta.get(var) {
                merge_output_meta(
                    &mut facts.output_meta,
                    meta_by_path
                        .iter()
                        .filter(|(path, _meta)| selected_paths.contains(*path)),
                );
            }
        }
        _ => {}
    });
}

fn collect_local_default_paths(
    expr: &TemplateExpr,
    local_default_paths: &HashMap<String, BTreeSet<String>>,
    out: &mut BTreeSet<String>,
) {
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

fn merge_output_meta<'a>(
    out: &mut BTreeMap<String, HelperOutputMeta>,
    meta: impl IntoIterator<Item = (&'a String, &'a HelperOutputMeta)>,
) {
    for (path, meta) in meta {
        out.entry(path.clone()).or_default().merge(meta.clone());
    }
}
