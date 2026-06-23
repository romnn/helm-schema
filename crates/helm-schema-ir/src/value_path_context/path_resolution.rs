use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::Effects;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, eval_exprs_effects};
use crate::fragment_expr_eval::{FragmentEvalContext, context_value_from_outer_expr};
use crate::helper_summary::HelperOutputMeta;
use crate::value_path_extraction::values_path_from_expr;

use super::ValuePathContext;

pub(crate) struct ValuePathExpressionFacts {
    pub(crate) values: BTreeSet<String>,
    pub(crate) default_fallback_values: BTreeSet<String>,
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(crate) encoded_output_values: BTreeSet<String>,
    pub(crate) local_output_meta: BTreeMap<String, HelperOutputMeta>,
}

pub(crate) fn computed_with_body_fragment_value_expr(
    expr: &TemplateExpr,
    root_bindings: &HashMap<String, AbstractValue>,
    template_bindings: &HashMap<String, AbstractValue>,
    fragment_context: FragmentEvalContext<'_>,
    current_dot_fragment: Option<&AbstractValue>,
    current_dot_binding: Option<&AbstractValue>,
) -> Option<AbstractValue> {
    let locals = locals_with_roots(template_bindings, root_bindings);

    context_value_from_outer_expr(
        expr,
        Some(&locals),
        Some(root_bindings),
        current_dot_binding,
    )
    .or_else(|| {
        fragment_context.fragment_value_from_expr(
            expr,
            template_bindings,
            current_dot_fragment,
            &mut HashSet::new(),
        )
    })
}

impl ValuePathContext<'_> {
    pub(crate) fn expression_path_facts(&self, exprs: &[TemplateExpr]) -> ValuePathExpressionFacts {
        let effects = self.expression_effects(exprs);
        let local_effects = self.local_expression_effects(exprs);
        let mut values = output_value_paths(effects.clone());
        let mut default_fallback_values = effects.defaults.clone();
        default_fallback_values.extend(local_effects.local_default_paths.iter().cloned());
        values.extend(default_fallback_values.iter().cloned());
        let mut type_hint_effects = effects.clone();
        type_hint_effects.merge(local_effects.clone());
        ValuePathExpressionFacts {
            values,
            default_fallback_values,
            type_hints: type_hint_effects.schema_type_hints(),
            encoded_output_values: effects.encoded_paths,
            local_output_meta: local_effects.local_output_meta,
        }
    }

    fn expression_effects(&self, exprs: &[TemplateExpr]) -> Effects {
        let mut effects = Effects::default();
        let env = self.expression_eval_env();
        for expr in exprs {
            effects.merge(eval_expr(expr, &env).effects);
        }
        effects
    }

    fn expression_eval_env(&self) -> EvalEnv {
        let current_dot = self
            .current_dot_fragment
            .as_ref()
            .map(AbstractValue::to_context_value)
            .or_else(|| self.current_dot_binding.clone());
        let mut env = EvalEnv::from_helper_context(Some(self.root_bindings), current_dot.as_ref())
            .without_helper_call_args();
        env.locals = locals_with_roots(self.template_bindings, self.root_bindings);
        env = env.with_local_facts(self.template_default_paths, self.template_output_meta);
        env
    }

    pub(crate) fn resolved_values_paths_from_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        output_value_paths(eval_expr(expr, &self.expression_eval_env()).effects)
    }

    pub(crate) fn resolved_default_fallback_paths_in_exprs(
        &self,
        exprs: &[TemplateExpr],
    ) -> BTreeSet<String> {
        let effects = self.expression_effects(exprs);
        let local_effects = self.local_expression_effects(exprs);
        let mut paths = effects.defaults.clone();
        paths.extend(local_effects.local_default_paths.iter().cloned());
        paths
    }

    fn local_expression_effects(&self, exprs: &[TemplateExpr]) -> Effects {
        let env = EvalEnv {
            locals: self.template_bindings.clone(),
            skip_helper_call_args: true,
            ..EvalEnv::default()
        }
        .with_local_facts(self.template_default_paths, self.template_output_meta);
        eval_exprs_effects(exprs, &env)
    }

    pub(super) fn paths_for_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        self.resolved_values_paths_from_expr(expr)
    }

    pub(super) fn expr_needs_context_value_resolution(&self, expr: &TemplateExpr) -> bool {
        values_path_from_expr(expr).is_none()
            && !self.resolved_values_paths_from_expr(expr).is_empty()
    }

    pub(super) fn truthy_paths_for_condition_expr(&self, expr: &TemplateExpr) -> BTreeSet<String> {
        self.resolved_values_paths_from_expr(expr)
    }

    pub(crate) fn with_body_fragment_value_expr(
        &self,
        expr: &TemplateExpr,
    ) -> Option<AbstractValue> {
        computed_with_body_fragment_value_expr(
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
}

fn output_value_paths(effects: Effects) -> BTreeSet<String> {
    effects
        .output_paths
        .into_iter()
        .chain(effects.local_source_paths)
        .filter(|path| !path.trim().is_empty())
        .collect()
}

fn locals_with_roots(
    template_bindings: &HashMap<String, AbstractValue>,
    root_bindings: &HashMap<String, AbstractValue>,
) -> HashMap<String, AbstractValue> {
    let mut locals = template_bindings.clone();
    for (key, value) in root_bindings {
        locals.insert(key.clone(), value.to_context_value());
    }
    locals
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

fn resolve_expr_to_values_path(
    expr: &TemplateExpr,
    bindings: Option<&HashMap<String, AbstractValue>>,
    current_dot: Option<&AbstractValue>,
) -> Option<String> {
    if let Some(path) = values_path_from_expr(expr) {
        return Some(path);
    }

    let env = EvalEnv::from_helper_context(bindings, current_dot);
    eval_expr(expr, &env)
        .value
        .as_ref()
        .and_then(AbstractValue::unique_path)
}
