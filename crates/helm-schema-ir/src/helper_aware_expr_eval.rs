use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{
    eval_expr, literal_printf_format, pipeline_preserves_current, render_printf_string_sets,
    transform_source_arg,
};
use crate::template_expr_analysis::{expr_contains_helper_call, is_merge_function};

pub(crate) trait HelperCallValueResolver {
    fn resolve_helper_call(
        &mut self,
        name: &str,
        arg: Option<&TemplateExpr>,
    ) -> Option<AbstractValue>;
}

pub(crate) fn eval_expr_with_helper_calls(
    expr: &TemplateExpr,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> Option<AbstractValue> {
    if !expr_contains_helper_call(expr) {
        return eval_expr(expr, env).value;
    }

    match expr {
        TemplateExpr::Parenthesized(inner) => eval_expr_with_helper_calls(inner, env, resolver),
        TemplateExpr::Selector { operand, path } => {
            let base = eval_expr_with_helper_calls(operand, env, resolver)?;
            base.apply_to_path(path)
        }
        TemplateExpr::Call { function, args } => {
            eval_call_with_helper_calls(function, args, env, resolver)
        }
        TemplateExpr::Pipeline(stages) => eval_pipeline_with_helper_calls(stages, env, resolver),
        _ => eval_expr(expr, env).value,
    }
}

fn eval_call_with_helper_calls(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> Option<AbstractValue> {
    match function {
        "include" | "template" => {
            let TemplateExpr::Literal(Literal::String(name) | Literal::RawString(name)) =
                args.first()?.deparen()
            else {
                return None;
            };
            resolver.resolve_helper_call(name, args.get(1))
        }
        "dict" => eval_dict(args, env, resolver),
        "list" | "tuple" => Some(AbstractValue::List(
            args.iter()
                .map(|arg| {
                    eval_expr_with_helper_calls(arg, env, resolver)
                        .unwrap_or(AbstractValue::Unknown)
                })
                .collect(),
        )),
        "append" => eval_append(args, env, resolver),
        function if is_merge_function(function) => {
            let values = args
                .iter()
                .filter_map(|arg| eval_expr_with_helper_calls(arg, env, resolver))
                .collect();
            AbstractValue::merge_all(values)
        }
        "coalesce" => {
            let values = args
                .iter()
                .filter_map(|arg| eval_expr_with_helper_calls(arg, env, resolver))
                .collect();
            AbstractValue::choice(values)
        }
        "default" if args.len() == 2 => {
            let values = [args.get(1), args.first()]
                .into_iter()
                .flatten()
                .filter_map(|arg| eval_expr_with_helper_calls(arg, env, resolver))
                .collect();
            AbstractValue::choice(values)
        }
        "ternary" => {
            let values = args
                .iter()
                .take(2)
                .filter_map(|arg| eval_expr_with_helper_calls(arg, env, resolver))
                .collect();
            AbstractValue::choice(values)
        }
        "printf" => eval_printf(args, env, resolver),
        "index" => eval_index(args, env, resolver),
        function => transform_source_arg(function, args)
            .and_then(|arg| eval_expr_with_helper_calls(arg, env, resolver)),
    }
}

fn eval_dict(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> Option<AbstractValue> {
    let mut entries = std::collections::BTreeMap::new();
    let mut index = 0usize;
    while index + 1 < args.len() {
        let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
            args[index].deparen()
        else {
            index += 1;
            continue;
        };
        let value = eval_expr_with_helper_calls(&args[index + 1], env, resolver)
            .unwrap_or(AbstractValue::Unknown);
        entries.insert(key.clone(), value);
        index += 2;
    }
    Some(AbstractValue::Dict(entries))
}

fn eval_append(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> Option<AbstractValue> {
    let mut items = match args
        .first()
        .and_then(|arg| eval_expr_with_helper_calls(arg, env, resolver))
    {
        Some(AbstractValue::List(items)) => items,
        Some(value) => vec![value],
        None => Vec::new(),
    };
    for arg in &args[1..] {
        if let Some(value) = eval_expr_with_helper_calls(arg, env, resolver) {
            items.push(value);
        }
    }
    Some(AbstractValue::List(items))
}

fn eval_printf(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> Option<AbstractValue> {
    let values = args
        .iter()
        .map(|arg| eval_expr_with_helper_calls(arg, env, resolver))
        .collect::<Vec<_>>();
    let provenance_paths = values
        .iter()
        .flat_map(|value| value.as_ref().map(AbstractValue::paths).unwrap_or_default())
        .collect::<BTreeSet<_>>();

    let rendered = literal_printf_format(args).and_then(|format| {
        let arg_strings = values
            .iter()
            .skip(1)
            .map(|value| {
                value
                    .as_ref()
                    .map(AbstractValue::strings)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        render_printf_string_sets(format, &arg_strings)
    });

    let mut choices = Vec::new();
    if let Some(rendered) = rendered {
        choices.push(AbstractValue::StringSet(rendered));
    }
    if !provenance_paths.is_empty() {
        choices.push(AbstractValue::PathSet(provenance_paths));
    }
    AbstractValue::choice(choices)
}

fn eval_index(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> Option<AbstractValue> {
    let base = eval_expr_with_helper_calls(args.first()?, env, resolver)?;
    let mut values = vec![base];
    for arg in &args[1..] {
        let evaluated = eval_expr_with_helper_calls(arg, env, resolver);
        let options = path_segment_options(arg, evaluated.as_ref())?;
        let mut next_values = Vec::new();
        for value in &values {
            for option in &options {
                if let Some(next) = value.apply_to_path(std::slice::from_ref(option)) {
                    next_values.push(next);
                }
            }
        }
        values = next_values;
    }
    AbstractValue::choice(values)
}

fn eval_pipeline_with_helper_calls(
    stages: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> Option<AbstractValue> {
    let mut current = eval_expr_with_helper_calls(stages.first()?, env, resolver);
    for stage in &stages[1..] {
        let TemplateExpr::Call { function, args } = stage else {
            current = eval_expr_with_helper_calls(stage, env, resolver);
            continue;
        };
        current = match function.as_str() {
            "default" => {
                let values = current
                    .into_iter()
                    .chain(
                        args.iter()
                            .filter_map(|arg| eval_expr_with_helper_calls(arg, env, resolver)),
                    )
                    .collect();
                AbstractValue::choice(values)
            }
            function if is_merge_function(function) => {
                let values = current
                    .into_iter()
                    .chain(
                        args.iter()
                            .filter_map(|arg| eval_expr_with_helper_calls(arg, env, resolver)),
                    )
                    .collect();
                AbstractValue::merge_all(values)
            }
            "ternary" => {
                let values = args
                    .iter()
                    .take(2)
                    .filter_map(|arg| eval_expr_with_helper_calls(arg, env, resolver))
                    .collect();
                AbstractValue::choice(values)
            }
            function if pipeline_preserves_current(function) => current,
            _ => None,
        };
    }
    current
}

fn path_segment_options(
    expr: &TemplateExpr,
    evaluated_value: Option<&AbstractValue>,
) -> Option<Vec<String>> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
            Some(vec![value.clone()])
        }
        TemplateExpr::Literal(Literal::Int(value)) => Some(vec![value.to_string()]),
        _ => {
            let strings = evaluated_value
                .map(AbstractValue::strings)
                .unwrap_or_default();
            if strings.is_empty() {
                None
            } else {
                Some(strings.into_iter().collect())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use helm_schema_ast::parse_action_expressions;

    use super::*;

    struct StaticResolver;

    impl HelperCallValueResolver for StaticResolver {
        fn resolve_helper_call(
            &mut self,
            name: &str,
            _arg: Option<&TemplateExpr>,
        ) -> Option<AbstractValue> {
            match name {
                "common.name" => Some(AbstractValue::ValuesPath("nameOverride".to_string())),
                "common.labels" => Some(AbstractValue::Dict(BTreeMap::from([(
                    "app".to_string(),
                    AbstractValue::ValuesPath("labels.app".to_string()),
                )]))),
                _ => None,
            }
        }
    }

    fn single_expr(action: &str) -> TemplateExpr {
        let exprs = parse_action_expressions(&format!("{{{{ {action} }}}}"));
        assert_eq!(exprs.len(), 1, "expected exactly one parsed expression");
        exprs.into_iter().next().expect("expression exists")
    }

    fn eval(action: &str) -> Option<AbstractValue> {
        let mut resolver = StaticResolver;
        eval_expr_with_helper_calls(&single_expr(action), &EvalEnv::default(), &mut resolver)
    }

    #[test]
    fn dict_value_can_be_nested_helper_call() {
        assert_eq!(
            eval(r#"dict "name" (include "common.name" .)"#),
            Some(AbstractValue::Dict(BTreeMap::from([(
                "name".to_string(),
                AbstractValue::ValuesPath("nameOverride".to_string()),
            )])))
        );
    }

    #[test]
    fn printf_preserves_nested_helper_provenance_path() {
        assert_eq!(
            eval(r#"printf "%s-sfx" (include "common.name" .)"#),
            Some(AbstractValue::PathSet(
                ["nameOverride".to_string()].into_iter().collect()
            ))
        );
    }

    #[test]
    fn pipeline_merge_can_consume_nested_helper_call() {
        assert_eq!(
            eval(r#"dict "base" "static" | merge (include "common.labels" .)"#),
            Some(AbstractValue::Dict(BTreeMap::from([
                (
                    "app".to_string(),
                    AbstractValue::ValuesPath("labels.app".to_string()),
                ),
                (
                    "base".to_string(),
                    AbstractValue::StringSet(["static".to_string()].into_iter().collect()),
                ),
            ])))
        );
    }
}
