use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::EvalResult;
use crate::eval_env::EvalEnv;
use crate::expr_call_eval::value_paths;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::expr_function_catalog::{
    is_provenance_preserving_function, is_string_transform_function,
};
use crate::literal_schema_type::expression_schema_type;
use crate::template_expr_analysis::is_merge_function;

pub(crate) fn eval_pipeline_with_helper_calls(
    stages: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let Some(first_stage) = stages.first() else {
        return EvalResult::none();
    };
    let mut current = eval_expr_with_helper_calls(first_stage, env, resolver);

    for stage in &stages[1..] {
        let TemplateExpr::Call { function, args } = stage else {
            current
                .effects
                .merge(eval_expr_with_helper_calls(stage, env, resolver).effects);
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
                    let arg_result = eval_expr_with_helper_calls(arg, env, resolver);
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
                    let arg_result = eval_expr_with_helper_calls(arg, env, resolver);
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
                for (index, arg) in args.iter().enumerate() {
                    let arg_result = eval_expr_with_helper_calls(arg, env, resolver);
                    effects.merge(arg_result.effects);
                    if index < 2
                        && let Some(value) = arg_result.value
                    {
                        values.push(value);
                    }
                }
                EvalResult::with_effects(AbstractValue::choice(values), effects)
            }
            function if is_string_transform_function(function) => {
                let mut effects = current.effects;
                let current_paths = value_paths(&current.value);
                effects.add_string_hints(current_paths.clone());
                if function == "b64enc" {
                    effects.add_encoded_paths(current_paths);
                }
                for arg in args {
                    let arg_result = eval_expr_with_helper_calls(arg, env, resolver);
                    if function == "b64enc" {
                        effects.add_encoded_paths(value_paths(&arg_result.value));
                    }
                    effects.merge(arg_result.effects);
                }
                EvalResult::with_effects(current.value, effects)
            }
            function if is_provenance_preserving_function(function) => {
                let mut effects = current.effects;
                for arg in args {
                    effects.merge(eval_expr_with_helper_calls(arg, env, resolver).effects);
                }
                EvalResult::with_effects(current.value, effects)
            }
            _ => {
                let mut effects = current.effects;
                for arg in args {
                    effects.merge(eval_expr_with_helper_calls(arg, env, resolver).effects);
                }
                EvalResult::with_effects(None, effects)
            }
        };
    }

    current
}
