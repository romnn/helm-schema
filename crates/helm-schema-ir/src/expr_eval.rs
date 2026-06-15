use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::EvalResult;
use crate::eval_env::EvalEnv;
pub(crate) use crate::expr_call_eval::{apply_index_segment, path_segment_options};
use crate::expr_call_eval::{eval_call, eval_pipeline};
pub(crate) use crate::expr_function_catalog::{
    pipeline_preserves_current, transform_source_arg, type_is_schema_type,
};
pub(crate) use crate::printf_eval::{literal_printf_format, render_printf_string_sets};

pub(crate) fn eval_expr(expr: &TemplateExpr, env: &EvalEnv) -> EvalResult {
    match expr {
        TemplateExpr::Parenthesized(inner) => eval_expr(inner, env),
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
            let base = eval_expr(operand, env);
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
        TemplateExpr::Call { function, args } => eval_call(function, args, env),
        TemplateExpr::Pipeline(stages) => eval_pipeline(stages, env),
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

pub(crate) fn eval_expr_value(expr: &TemplateExpr, env: &EvalEnv) -> Option<AbstractValue> {
    eval_expr(expr, env).value
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
