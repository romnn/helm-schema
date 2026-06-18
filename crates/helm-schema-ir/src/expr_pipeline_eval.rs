use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::EvalResult;
use crate::eval_env::EvalEnv;
use crate::expr_call_eval::value_paths;
use crate::expr_eval::{eval_expr, eval_expr_value};
use crate::expr_function_catalog::{
    is_provenance_preserving_function, is_string_transform_function,
};
use crate::literal_schema_type::expression_schema_type;
use crate::template_expr_analysis::is_merge_function;

pub(crate) fn eval_pipeline(stages: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let Some(first_stage) = stages.first() else {
        return EvalResult::none();
    };
    let mut current = eval_expr(first_stage, env);

    for stage in &stages[1..] {
        let TemplateExpr::Call { function, args } = stage else {
            current.effects.merge(eval_expr(stage, env).effects);
            continue;
        };

        current = match function.as_str() {
            "default" => {
                let mut effects = current.effects;
                let current_paths = value_paths(&current.value);
                effects.add_default_paths(current_paths);
                if let Some(schema_type) = args.first().and_then(expression_schema_type) {
                    effects.add_type_hints(value_paths(&current.value), schema_type);
                }
                let mut values = current.value.into_iter().collect::<Vec<_>>();
                for arg in args {
                    let arg_result = eval_expr(arg, env);
                    effects.merge(arg_result.effects);
                    if let Some(value) = arg_result.value {
                        values.push(value);
                    }
                }
                EvalResult::with_effects(AbstractValue::choice(values), effects)
            }
            function if is_merge_function(function) => {
                let mut effects = current.effects;
                let mut values = current.value.into_iter().collect::<Vec<_>>();
                for arg in args {
                    let arg_result = eval_expr(arg, env);
                    effects.merge(arg_result.effects);
                    if let Some(value) = arg_result.value {
                        values.push(value);
                    }
                }
                EvalResult::with_effects(AbstractValue::merge_all(values), effects)
            }
            "ternary" => {
                let mut effects = current.effects;
                let mut values = Vec::new();
                for arg in args {
                    let arg_result = eval_expr(arg, env);
                    effects.merge(arg_result.effects);
                }
                for arg in args.iter().take(2) {
                    if let Some(value) = eval_expr_value(arg, env) {
                        values.push(value);
                    }
                }
                EvalResult::with_effects(AbstractValue::choice(values), effects)
            }
            function if is_string_transform_function(function) => {
                let mut effects = current.effects;
                effects.add_string_hints(value_paths(&current.value));
                for arg in args {
                    effects.merge(eval_expr(arg, env).effects);
                }
                EvalResult::with_effects(current.value, effects)
            }
            function if is_provenance_preserving_function(function) => {
                let mut effects = current.effects;
                for arg in args {
                    effects.merge(eval_expr(arg, env).effects);
                }
                EvalResult::with_effects(current.value, effects)
            }
            _ => {
                let mut effects = current.effects;
                for arg in args {
                    effects.merge(eval_expr(arg, env).effects);
                }
                EvalResult::with_effects(None, effects)
            }
        };
    }

    current
}
