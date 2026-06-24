use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_call_eval::eval_call_with_helper_calls;
use crate::expr_pipeline_eval::eval_pipeline_with_helper_calls;

pub(crate) trait HelperCallValueResolver {
    fn resolve_helper_call(&mut self, name: &str, arg: Option<&TemplateExpr>)
    -> Option<EvalResult>;
}

struct NoHelperCallResolver;

impl HelperCallValueResolver for NoHelperCallResolver {
    fn resolve_helper_call(
        &mut self,
        _name: &str,
        _arg: Option<&TemplateExpr>,
    ) -> Option<EvalResult> {
        None
    }
}

pub(crate) fn eval_expr(expr: &TemplateExpr, env: &EvalEnv) -> EvalResult {
    let mut resolver = NoHelperCallResolver;
    eval_expr_with_helper_calls(expr, env, &mut resolver)
}

pub(crate) fn direct_values_path(expr: &TemplateExpr) -> Option<String> {
    if !matches!(
        expr.deparen(),
        TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
    ) {
        return None;
    }

    eval_expr(expr, &EvalEnv::default())
        .value
        .as_ref()
        .and_then(AbstractValue::unique_path)
}

pub(crate) fn eval_expr_with_helper_calls(
    expr: &TemplateExpr,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    match expr {
        TemplateExpr::Parenthesized(inner) => eval_expr_with_helper_calls(inner, env, resolver),
        TemplateExpr::Field(path) if path.first().is_some_and(|segment| segment == "Values") => {
            EvalResult::from_value(values_path_value(path))
        }
        TemplateExpr::Field(path) if path.is_empty() => {
            EvalResult::from_value(env.dot.clone().unwrap_or(AbstractValue::RootContext))
        }
        TemplateExpr::Field(path) => {
            let value = env.dot.as_ref().and_then(|value| value.apply_to_path(path));
            let value = value.or_else(|| {
                if !env.allow_field_root_lookup {
                    return None;
                }
                let (head, tail) = path.split_first()?;
                env.root_fields
                    .get(head)
                    .and_then(|value| value.apply_to_path(tail))
            });
            value
                .map(EvalResult::from_value)
                .unwrap_or_else(EvalResult::none)
        }
        TemplateExpr::Selector { operand, path }
            if matches!(operand.as_ref(), TemplateExpr::Variable(var) if var.is_empty())
                && path.first().is_some_and(|segment| segment == "Values") =>
        {
            EvalResult::from_value(values_path_value(path))
        }
        TemplateExpr::Variable(var) if var.is_empty() => {
            EvalResult::from_value(AbstractValue::RootContext)
        }
        TemplateExpr::Variable(var) if !var.is_empty() => env
            .locals
            .get(var)
            .cloned()
            .map(|value| local_value_result(var, value, None, env))
            .unwrap_or_else(EvalResult::none),
        TemplateExpr::Selector { operand, path } => {
            if let TemplateExpr::Variable(var) = operand.as_ref()
                && !var.is_empty()
                && let Some(value) = env
                    .locals
                    .get(var)
                    .and_then(|binding| binding.apply_to_path(path))
            {
                let selected_paths = value.fragment_source_paths();
                return local_value_result(var, value, Some(&selected_paths), env);
            }
            if let TemplateExpr::Variable(var) = operand.as_ref()
                && !var.is_empty()
                && path.first().is_some_and(|segment| segment == "Values")
            {
                return EvalResult::from_value(values_path_value(path));
            }
            if let TemplateExpr::Variable(var) = operand.as_ref()
                && var.is_empty()
                && let Some((head, tail)) = path.split_first()
                && let Some(value) = env
                    .root_fields
                    .get(head)
                    .and_then(|value| value.apply_to_path(tail))
            {
                return EvalResult::from_value(value);
            }
            let base = eval_expr_with_helper_calls(operand, env, resolver);
            let value = base
                .value
                .as_ref()
                .and_then(|value| value.apply_to_path(path));
            let mut effects = base.effects;
            effects.reads.clear();
            effects.output_paths.clear();
            if let Some(value) = &value {
                effects.reads.extend(value.paths());
            }
            EvalResult::with_effects(value, effects)
        }
        TemplateExpr::Call { function, args } => {
            eval_call_with_helper_calls(function, args, env, resolver)
        }
        TemplateExpr::Pipeline(stages) => eval_pipeline_with_helper_calls(stages, env, resolver),
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
            EvalResult::from_value(AbstractValue::StringSet(
                [value.clone()].into_iter().collect(),
            ))
        }
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            EvalResult::with_effects(
                None,
                eval_expr_with_helper_calls(value, env, resolver).effects,
            )
        }
        TemplateExpr::Literal(_) | TemplateExpr::Variable(_) | TemplateExpr::Unknown(_) => {
            EvalResult::none()
        }
    }
}

fn values_path_value(path: &[String]) -> AbstractValue {
    if path.len() == 1 {
        AbstractValue::values_root()
    } else {
        AbstractValue::ValuesPath(path[1..].join("."))
    }
}

pub(crate) fn eval_exprs_effects(exprs: &[TemplateExpr], env: &EvalEnv) -> Effects {
    let mut effects = Effects::default();
    for expr in exprs {
        effects.merge(eval_expr(expr, env).effects);
    }
    effects
}

pub(crate) fn eval_output_expr_effects(expr: &TemplateExpr, env: &EvalEnv) -> Effects {
    let result = eval_expr(expr, env);
    if let Some(value) = result.value {
        let mut effects = Effects::default();
        effects.encoded_paths = result.effects.encoded_paths;
        effects.rendered_output_values.push(value);
        return effects;
    }

    let mut effects = Effects::default();
    match expr {
        TemplateExpr::Call { args, .. } => {
            for arg in args {
                effects.merge(eval_output_expr_effects(arg, env));
            }
        }
        TemplateExpr::Selector { operand, .. } => {
            effects.merge(eval_output_expr_effects(operand, env));
        }
        TemplateExpr::Pipeline(stages) => {
            for stage in stages {
                effects.merge(eval_output_expr_effects(stage, env));
            }
        }
        TemplateExpr::Parenthesized(inner)
        | TemplateExpr::VariableDefinition { value: inner, .. }
        | TemplateExpr::Assignment { value: inner, .. } => {
            effects.merge(eval_output_expr_effects(inner, env));
        }
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => {}
    }
    effects
}

fn local_value_result(
    var: &str,
    value: AbstractValue,
    selected_paths: Option<&BTreeSet<String>>,
    env: &EvalEnv,
) -> EvalResult {
    let mut result = EvalResult::from_value(value.clone());
    result
        .effects
        .local_source_paths
        .extend(value.fragment_source_paths());
    result
        .effects
        .local_rendered_paths
        .extend(value.fragment_rendered_paths());
    result.effects.local_output_values.push(value);
    if let Some(default_paths) = env.local_default_paths.get(var) {
        result
            .effects
            .local_default_paths
            .extend(default_paths.iter().cloned());
        result.effects.add_default_paths(default_paths.clone());
    }
    if let Some(meta_by_path) = env.local_output_meta.get(var) {
        match selected_paths {
            Some(paths) => result.effects.merge_local_output_meta(
                meta_by_path
                    .iter()
                    .filter(|(path, _meta)| paths.contains(*path)),
            ),
            None => result.effects.merge_local_output_meta(meta_by_path.iter()),
        }
    }
    result
}

pub(crate) fn apply_local_set_mutations_expr(expr: &TemplateExpr, env: &mut EvalEnv) -> bool {
    let mutation_expr = match expr {
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            value.as_ref()
        }
        _ => expr,
    };
    let result = eval_expr(mutation_expr, env);
    env.apply_local_set_mutations(&result.effects.local_set_mutations)
}
