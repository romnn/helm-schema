use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::expression_analysis::{
    resolve_expr_to_values_path, resolved_default_fallback_paths_for_exprs,
};
use crate::fragment_expr_eval::{FragmentEvalContext, fragment_binding_from_outer_expr};
use crate::template_expr_analysis::expr_contains_helper_call;
use crate::value_path_extraction::values_path_from_expr;

use super::ValuePathContext;

pub(crate) fn computed_with_body_fragment_binding_expr(
    expr: &TemplateExpr,
    root_bindings: &HashMap<String, AbstractValue>,
    template_bindings: &HashMap<String, AbstractValue>,
    fragment_context: FragmentEvalContext<'_>,
    current_dot_fragment: Option<&AbstractValue>,
    current_dot_binding: Option<&AbstractValue>,
) -> Option<AbstractValue> {
    let mut locals = template_bindings.clone();
    for (key, value) in root_bindings {
        locals.insert(key.clone(), value.to_context_value());
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
    pub(crate) fn resolved_values_paths_in_exprs(&self, exprs: &[TemplateExpr]) -> Vec<String> {
        let mut paths = BTreeSet::new();
        for expr in exprs {
            paths.extend(self.resolved_values_paths_from_expr(&expr));
        }
        paths.into_iter().collect()
    }

    pub(crate) fn resolved_values_paths_from_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        let mut paths = BTreeSet::new();

        walk_expr_maximal_paths_excluding_helper_call_args(expr, &mut |node| {
            if let Some(path) = values_path_from_expr(node) {
                paths.insert(path);
                return true;
            }
            false
        });

        if !self.root_bindings.is_empty() {
            walk_expr_maximal_paths_excluding_helper_call_args(expr, &mut |node| {
                if let Some(path) =
                    resolve_expr_to_values_path(node, Some(self.root_bindings), None)
                {
                    paths.insert(path);
                    return true;
                }
                false
            });
        }

        if !self.template_bindings.is_empty() {
            walk_expr_maximal_paths_excluding_helper_call_args(expr, &mut |node| {
                let local_paths = self.local_alias_paths_for_expr(node);
                if local_paths.is_empty() {
                    return false;
                }
                paths.extend(local_paths);
                true
            });
        }

        paths.extend(self.resolved_values_paths_in_expr_tree(expr));
        paths
    }

    pub(crate) fn resolved_default_fallback_paths_in_exprs(
        &self,
        exprs: &[TemplateExpr],
    ) -> BTreeSet<String> {
        let mut paths = resolved_default_fallback_paths_for_exprs(
            exprs,
            Some(self.root_bindings),
            self.current_dot_binding.as_ref(),
        );
        for expr in exprs {
            paths.extend(self.resolved_default_fallback_paths_for_expr(expr));
        }
        if !self.template_default_paths.is_empty() {
            for expr in exprs {
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
            locals.insert(key.clone(), value.to_context_value());
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
            .flat_map(|binding| binding.fragment_source_paths())
            .filter(|path| !path.trim().is_empty())
            .collect()
    }

    pub(crate) fn with_body_fragment_binding_expr(
        &self,
        expr: &TemplateExpr,
    ) -> Option<AbstractValue> {
        computed_with_body_fragment_binding_expr(
            expr,
            self.root_bindings,
            self.template_bindings,
            self.fragment_context,
            self.current_dot_fragment.as_ref(),
            self.current_dot_binding.as_ref(),
        )
    }

    pub(crate) fn single_resolved_values_path_expr(&self, expr: &TemplateExpr) -> Option<String> {
        let mut paths: Vec<_> = self
            .resolved_values_paths_from_expr(expr)
            .into_iter()
            .collect();
        if paths.len() == 1 { paths.pop() } else { None }
    }

    pub(crate) fn single_direct_iterable_range_path_expr(
        &self,
        expr: &TemplateExpr,
    ) -> Option<String> {
        if !is_direct_path_expr(expr, self.root_bindings) {
            return None;
        }
        self.single_resolved_values_path_expr(expr)
    }

    fn fragment_binding_from_expr(
        &self,
        expr: &TemplateExpr,
        current_dot: Option<&AbstractValue>,
    ) -> Option<AbstractValue> {
        let mut seen = HashSet::new();
        self.fragment_context.fragment_binding_from_expr(
            expr,
            self.template_bindings,
            current_dot,
            &mut seen,
        )
    }

    pub(super) fn resolved_values_paths_in_expr_tree(
        &self,
        expr: &TemplateExpr,
    ) -> BTreeSet<String> {
        let mut locals = self.template_bindings.clone();
        for (key, value) in self.root_bindings {
            locals.insert(key.clone(), value.to_context_value());
        }

        let mut paths = BTreeSet::new();
        walk_expr_maximal_paths_excluding_helper_call_args(expr, &mut |node| {
            if expr_contains_helper_call(node) {
                return false;
            }
            let outer_binding = fragment_binding_from_outer_expr(
                node,
                Some(&locals),
                Some(self.root_bindings),
                self.current_dot_binding.as_ref(),
            );
            let fragment_binding =
                self.fragment_binding_from_expr(node, self.current_dot_fragment.as_ref());
            let resolved_paths = outer_binding
                .into_iter()
                .chain(fragment_binding)
                .flat_map(|binding| binding.fragment_source_paths())
                .filter(|path| !path.trim().is_empty())
                .collect::<BTreeSet<_>>();
            let found = !resolved_paths.is_empty();
            paths.extend(resolved_paths);
            found
        });
        paths
    }
}

fn walk_expr_maximal_paths_excluding_helper_call_args<F>(expr: &TemplateExpr, visit: &mut F)
where
    F: FnMut(&TemplateExpr) -> bool,
{
    if visit(expr) {
        return;
    }

    match expr {
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => {}
        TemplateExpr::Selector { operand, .. } | TemplateExpr::Parenthesized(operand) => {
            walk_expr_maximal_paths_excluding_helper_call_args(operand, visit);
        }
        TemplateExpr::Call { function, args } => {
            if matches!(function.as_str(), "include" | "template") {
                return;
            }
            for arg in args {
                walk_expr_maximal_paths_excluding_helper_call_args(arg, visit);
            }
        }
        TemplateExpr::Pipeline(stages) => {
            for stage in stages {
                walk_expr_maximal_paths_excluding_helper_call_args(stage, visit);
            }
        }
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            walk_expr_maximal_paths_excluding_helper_call_args(value, visit);
        }
    }
}

fn is_direct_path_expr(expr: &TemplateExpr, bindings: &HashMap<String, AbstractValue>) -> bool {
    match expr {
        TemplateExpr::Parenthesized(inner) => is_direct_path_expr(inner, bindings),
        TemplateExpr::Field(_) => true,
        TemplateExpr::Selector { .. } => {
            resolve_expr_to_values_path(expr, Some(bindings), None).is_some()
        }
        _ => false,
    }
}
