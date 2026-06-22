use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::EvalResult;
use crate::eval_env::EvalEnv;
use crate::expr_call_eval::eval_call_with_helper_calls;
use crate::expr_pipeline_eval::eval_pipeline_with_helper_calls;

pub(crate) trait HelperCallValueResolver {
    fn resolve_helper_call(
        &mut self,
        name: &str,
        arg: Option<&TemplateExpr>,
    ) -> Option<AbstractValue>;
}

struct NoHelperCallResolver;

impl HelperCallValueResolver for NoHelperCallResolver {
    fn resolve_helper_call(
        &mut self,
        _name: &str,
        _arg: Option<&TemplateExpr>,
    ) -> Option<AbstractValue> {
        None
    }
}

pub(crate) fn eval_expr(expr: &TemplateExpr, env: &EvalEnv) -> EvalResult {
    let mut resolver = NoHelperCallResolver;
    eval_expr_with_helper_calls(expr, env, &mut resolver)
}

pub(crate) fn eval_expr_with_helper_calls(
    expr: &TemplateExpr,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    match expr {
        TemplateExpr::Parenthesized(inner) => eval_expr_with_helper_calls(inner, env, resolver),
        TemplateExpr::Field(path) if path.first().is_some_and(|segment| segment == "Values") => {
            if path.len() == 1 {
                EvalResult::from_value(AbstractValue::values_root())
            } else {
                EvalResult::from_value(AbstractValue::ValuesPath(path[1..].join(".")))
            }
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
            if path.len() == 1 {
                EvalResult::from_value(AbstractValue::values_root())
            } else {
                EvalResult::from_value(AbstractValue::ValuesPath(path[1..].join(".")))
            }
        }
        TemplateExpr::Variable(var) if var.is_empty() => {
            EvalResult::from_value(AbstractValue::RootContext)
        }
        TemplateExpr::Variable(var) if !var.is_empty() => env
            .locals
            .get(var)
            .cloned()
            .map(EvalResult::from_value)
            .unwrap_or_else(EvalResult::none),
        TemplateExpr::Selector { operand, path } => {
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
        TemplateExpr::Literal(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_)
        | TemplateExpr::VariableDefinition { .. }
        | TemplateExpr::Assignment { .. } => EvalResult::none(),
    }
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
