use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::binding::{FragmentBinding, HelperBinding};
use crate::expression_analysis::{
    resolve_expr_to_values_path, resolved_default_fallback_paths_for_text,
};
use crate::fragment_expr_eval::{FragmentEvalContext, fragment_binding_from_outer_expr};
use crate::template_expr_analysis::{
    expr_contains_helper_call, walk_expr_excluding_helper_call_args,
};
use crate::template_expr_cache::parse_expr_text;
use crate::value_path_extraction::values_path_from_expr;

use super::ValuePathContext;

/// Resolves a `with` header's value to the fragment binding walkers should use
/// as `current_dot` while interpreting the body.
pub(crate) fn computed_with_body_fragment_binding(
    header: &str,
    root_bindings: &HashMap<String, HelperBinding>,
    template_bindings: &HashMap<String, FragmentBinding>,
    fragment_context: FragmentEvalContext<'_>,
    current_dot_fragment: Option<&FragmentBinding>,
    current_dot_binding: Option<&HelperBinding>,
) -> Option<FragmentBinding> {
    let exprs = parse_expr_text(header);
    let [expr] = exprs.as_slice() else {
        return None;
    };

    let mut locals = template_bindings.clone();
    for (key, value) in root_bindings {
        locals.insert(key.clone(), value.to_fragment_binding());
    }

    fragment_binding_from_outer_expr(
        expr,
        Some(&locals),
        Some(root_bindings),
        current_dot_binding,
    )
    .or_else(|| {
        fragment_context.fragment_binding_from_expr(
            expr,
            template_bindings,
            current_dot_fragment,
            &mut HashSet::new(),
        )
    })
}

impl ValuePathContext<'_> {
    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    pub(crate) fn resolved_values_paths(&self, text: &str) -> Vec<String> {
        let exprs = parse_expr_text(text);
        let mut paths = direct_values_paths_from_exprs(&exprs);

        if !self.root_bindings.is_empty() {
            for expr in &exprs {
                walk_expr_excluding_helper_call_args(expr, &mut |node| {
                    if let Some(path) =
                        resolve_expr_to_values_path(node, Some(self.root_bindings), None)
                    {
                        paths.insert(path);
                    }
                });
            }
        }

        if !self.template_bindings.is_empty() {
            for expr in &exprs {
                walk_expr_excluding_helper_call_args(expr, &mut |node| {
                    paths.extend(self.local_alias_paths_for_expr(node));
                });
            }
        }

        paths.extend(self.resolved_values_paths_in_expr_tree(text));

        paths.into_iter().collect()
    }

    pub(crate) fn resolved_default_fallback_paths(&self, text: &str) -> BTreeSet<String> {
        let exprs = parse_expr_text(text);
        let mut paths = resolved_default_fallback_paths_for_text(
            text,
            Some(self.root_bindings),
            self.current_dot_binding.as_ref(),
        );
        for expr in &exprs {
            paths.extend(self.resolved_default_fallback_paths_for_expr(expr));
        }
        if !self.template_default_paths.is_empty() {
            for expr in &exprs {
                expr.walk(|node| {
                    paths.extend(self.local_alias_default_paths_for_expr(node));
                });
            }
        }
        paths
    }

    fn resolved_default_fallback_paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        expr.walk(|node| match node {
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                out.extend(self.resolve_expr_to_values_paths(&args[1]));
            }
            TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
                for window in stages.windows(2) {
                    let TemplateExpr::Call { function, .. } = &window[1] else {
                        continue;
                    };
                    if function != "default" {
                        continue;
                    }
                    out.extend(self.resolve_expr_to_values_paths(&window[0]));
                }
            }
            _ => {}
        });
        out
    }

    pub(crate) fn resolve_expr_to_values_paths(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        if let Some(path) = values_path_from_expr(expr) {
            return [path].into_iter().collect();
        }

        let mut locals = self.template_bindings.clone();
        for (key, value) in self.root_bindings {
            locals.insert(key.clone(), value.to_fragment_binding());
        }

        let outer_binding = fragment_binding_from_outer_expr(
            expr,
            Some(&locals),
            Some(self.root_bindings),
            self.current_dot_binding.as_ref(),
        );
        let fragment_binding =
            self.fragment_binding_from_expr(expr, self.current_dot_fragment.as_ref());

        outer_binding
            .into_iter()
            .chain(fragment_binding)
            .flat_map(|binding| FragmentBinding::paths(&binding))
            .filter(|path| !path.trim().is_empty())
            .collect()
    }

    pub(crate) fn with_body_fragment_binding(&self, header: &str) -> Option<FragmentBinding> {
        computed_with_body_fragment_binding(
            header,
            self.root_bindings,
            self.template_bindings,
            self.fragment_context,
            self.current_dot_fragment.as_ref(),
            self.current_dot_binding.as_ref(),
        )
    }

    pub(crate) fn single_resolved_values_path(&self, text: &str) -> Option<String> {
        let mut paths = self.resolved_values_paths(text);
        if paths.len() == 1 { paths.pop() } else { None }
    }

    pub(crate) fn single_direct_iterable_range_path(&self, text: &str) -> Option<String> {
        let exprs = parse_expr_text(text);
        if exprs.len() != 1 || !is_direct_path_expr(&exprs[0], self.root_bindings) {
            return None;
        }
        self.single_resolved_values_path(text)
    }

    fn fragment_binding_from_expr(
        &self,
        expr: &TemplateExpr,
        current_dot: Option<&FragmentBinding>,
    ) -> Option<FragmentBinding> {
        let mut seen = HashSet::new();
        self.fragment_context.fragment_binding_from_expr(
            expr,
            self.template_bindings,
            current_dot,
            &mut seen,
        )
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    pub(super) fn resolved_values_paths_in_expr_tree(&self, text: &str) -> BTreeSet<String> {
        let mut locals = self.template_bindings.clone();
        for (key, value) in self.root_bindings {
            locals.insert(key.clone(), value.to_fragment_binding());
        }

        let mut paths = BTreeSet::new();
        for expr in parse_expr_text(text) {
            walk_expr_excluding_helper_call_args(&expr, &mut |node| {
                if expr_contains_helper_call(node) {
                    return;
                }
                let outer_binding = fragment_binding_from_outer_expr(
                    node,
                    Some(&locals),
                    Some(self.root_bindings),
                    self.current_dot_binding.as_ref(),
                );
                let fragment_binding =
                    self.fragment_binding_from_expr(node, self.current_dot_fragment.as_ref());
                paths.extend(
                    outer_binding
                        .into_iter()
                        .chain(fragment_binding)
                        .flat_map(|binding| FragmentBinding::paths(&binding))
                        .filter(|path| !path.trim().is_empty()),
                );
            });
        }
        paths
    }
}

fn direct_values_paths_from_exprs(exprs: &[TemplateExpr]) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for expr in exprs {
        walk_expr_excluding_helper_call_args(expr, &mut |node| {
            if let Some(path) = values_path_from_expr(node) {
                paths.insert(path);
            }
        });
    }
    paths
}

fn is_direct_path_expr(expr: &TemplateExpr, bindings: &HashMap<String, HelperBinding>) -> bool {
    match expr {
        TemplateExpr::Parenthesized(inner) => is_direct_path_expr(inner, bindings),
        TemplateExpr::Field(_) => true,
        TemplateExpr::Selector { .. } => {
            resolve_expr_to_values_path(expr, Some(bindings), None).is_some()
        }
        _ => false,
    }
}
