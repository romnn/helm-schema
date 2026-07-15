use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::BoundValueContext;
use crate::eval_effect::Effects;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{direct_values_path, eval_expr, eval_exprs_effects};
use crate::fragment_expr_eval::fragment_context_value;

use super::ValuePathContext;

impl ValuePathContext<'_> {
    pub(crate) fn expression_output_effects(&self, exprs: &[TemplateExpr]) -> Effects {
        let effects = self.expression_effects(exprs);
        let mut values = effects.output_value_paths();
        let defaults = effects.default_paths_with_local();
        let type_hints = effects.type_hints.clone();
        values.extend(defaults.iter().cloned());
        Effects {
            output_paths: values,
            bound_output_paths: effects.bound_output_paths,
            defaults,
            type_hints,
            encoded_paths: effects.encoded_paths,
            shape_erased_paths: effects.shape_erased_paths,
            local_output_meta: effects.local_output_meta,
            ..Effects::default()
        }
    }

    pub(crate) fn bound_output_paths_expr(&self, expr: &TemplateExpr) -> Vec<String> {
        self.expression_effects(std::slice::from_ref(expr))
            .bound_output_paths
            .into_iter()
            .collect()
    }

    fn expression_effects(&self, exprs: &[TemplateExpr]) -> Effects {
        let env = self.expression_eval_env();
        eval_exprs_effects(exprs, &env)
    }

    pub(super) fn expression_eval_env(&self) -> EvalEnv {
        let current_dot = self
            .current_dot_fragment
            .as_ref()
            .map(AbstractValue::to_context_value)
            .or_else(|| self.current_dot_binding.clone());
        let mut env = EvalEnv::from_helper_context(Some(self.root_bindings), current_dot.as_ref())
            .without_helper_call_args();
        // Locals and root bindings are distinct namespaces (see the hole
        // evaluator); roots resolve through `root_fields` only.
        env.locals = self.template_bindings.clone();
        env.local_default_paths = self.template_default_paths.clone();
        env.local_output_meta = self.template_output_meta.clone();
        env.bound_values = BoundValueContext::new(self.range_domains, self.get_bindings);
        env
    }

    pub(crate) fn resolved_values_paths_from_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        eval_expr(expr, &self.expression_eval_env())
            .effects
            .output_value_paths()
    }

    pub(crate) fn paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        self.resolved_values_paths_from_expr(expr)
    }

    pub(super) fn expr_needs_context_value_resolution(&self, expr: &TemplateExpr) -> bool {
        direct_values_path(expr).is_none() && !self.resolved_values_paths_from_expr(expr).is_empty()
    }

    pub(crate) fn with_body_fragment_value_expr(
        &self,
        expr: &TemplateExpr,
    ) -> Option<AbstractValue> {
        fragment_context_value(
            expr,
            self.root_bindings,
            &self.template_bindings,
            self.fragment_context,
            self.current_dot_fragment.as_ref(),
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
        if !matches!(
            expr.deparen(),
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
        ) {
            return None;
        }
        self.single_resolved_values_path_expr(expr)
    }

    pub(crate) fn single_direct_json_decoded_range_path_expr(
        &self,
        expr: &TemplateExpr,
    ) -> Option<String> {
        if !matches!(
            expr.deparen(),
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
        ) {
            return None;
        }
        eval_expr(expr, &self.expression_eval_env())
            .value
            .as_ref()
            .and_then(AbstractValue::unique_json_decoded_path)
    }
}
