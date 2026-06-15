use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::TemplateExpr;

use crate::fragment_binding::FragmentBinding;
use crate::helper_analysis::HelperOutputMeta;
use crate::template_expr_analysis::walk_expr_excluding_helper_call_args;
use crate::template_expr_cache::parse_expr_text;

use super::ValuePathContext;

impl ValuePathContext<'_> {
    pub(crate) fn local_alias_output_meta_for_text(
        &self,
        text: &str,
    ) -> BTreeMap<String, HelperOutputMeta> {
        let mut out: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        for expr in parse_expr_text(text) {
            walk_expr_excluding_helper_call_args(&expr, &mut |node| {
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
                .map(FragmentBinding::paths)
                .unwrap_or_default(),
            TemplateExpr::Selector { operand, path } => match operand.as_ref() {
                TemplateExpr::Variable(var) if !var.is_empty() => self
                    .template_bindings
                    .get(var)
                    .and_then(|binding| binding.apply_to_binding(path))
                    .map(|binding| FragmentBinding::paths(&binding))
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
                let Some(bound) = binding.apply_to_binding(path) else {
                    return BTreeMap::new();
                };
                let selected_paths = FragmentBinding::paths(&bound);
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
