use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::TemplateExpr;

use crate::fragment_binding_projection::{fragment_source_paths, select_fragment_binding};
use crate::helper_summary::HelperOutputMeta;
use crate::template_expr_analysis::walk_expr_excluding_helper_call_args;

use super::ValuePathContext;

impl ValuePathContext<'_> {
    pub(crate) fn local_alias_output_meta_for_exprs(
        &self,
        exprs: &[TemplateExpr],
    ) -> BTreeMap<String, HelperOutputMeta> {
        let mut out: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        for expr in exprs {
            walk_expr_excluding_helper_call_args(expr, &mut |node| {
                for (path, meta) in self.local_alias_output_meta_for_expr(node) {
                    out.entry(path).or_default().merge(meta);
                }
            });
        }
        out
    }

    pub(super) fn local_alias_paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => self
                .template_bindings
                .get(var)
                .map(fragment_source_paths)
                .unwrap_or_default(),
            TemplateExpr::Selector { operand, path } => match operand.as_ref() {
                TemplateExpr::Variable(var) if !var.is_empty() => self
                    .template_bindings
                    .get(var)
                    .and_then(|binding| select_fragment_binding(binding, path))
                    .map(|binding| fragment_source_paths(&binding))
                    .unwrap_or_default(),
                _ => BTreeSet::new(),
            },
            _ => BTreeSet::new(),
        }
    }

    pub(super) fn local_alias_default_paths_for_expr(
        &self,
        expr: &TemplateExpr,
    ) -> BTreeSet<String> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => self
                .template_default_paths
                .get(var)
                .cloned()
                .unwrap_or_default(),
            _ => BTreeSet::new(),
        }
    }

    fn local_alias_output_meta_for_expr(
        &self,
        expr: &TemplateExpr,
    ) -> BTreeMap<String, HelperOutputMeta> {
        match expr {
            TemplateExpr::Variable(var) if !var.is_empty() => self
                .template_output_meta
                .get(var)
                .cloned()
                .unwrap_or_default(),
            TemplateExpr::Selector { operand, path } => {
                let TemplateExpr::Variable(var) = operand.as_ref() else {
                    return BTreeMap::new();
                };
                if var.is_empty() {
                    return BTreeMap::new();
                }
                let Some(binding) = self.template_bindings.get(var) else {
                    return BTreeMap::new();
                };
                let Some(bound) = select_fragment_binding(binding, path) else {
                    return BTreeMap::new();
                };
                let selected_paths = fragment_source_paths(&bound);
                self.template_output_meta
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

    pub(super) fn paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        let mut paths = self.resolve_expr_to_values_paths(expr);
        paths.extend(self.local_alias_paths_for_expr(expr));
        paths
            .into_iter()
            .filter(|path| !path.trim().is_empty())
            .collect()
    }
}
