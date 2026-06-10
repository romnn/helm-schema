use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::template_expr_analysis::is_merge_function;

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

pub(crate) fn apply_assignment_expr(expr: &TemplateExpr, env: &mut EvalEnv) -> bool {
    match expr {
        TemplateExpr::VariableDefinition { name, value }
        | TemplateExpr::Assignment { name, value } => {
            let name = name.trim_start_matches('$');
            if let Some(value) = eval_expr_value(value, env) {
                env.locals.insert(name.to_string(), value);
            } else {
                env.locals.remove(name);
            }
            true
        }
        _ => false,
    }
}

fn eval_call(function: &str, args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    match function {
        "default" if args.len() == 2 => {
            let fallback = eval_expr(&args[0], env);
            let primary = eval_expr(&args[1], env);
            let primary_paths = primary
                .value
                .as_ref()
                .map(AbstractValue::paths)
                .unwrap_or_default();
            let mut effects = fallback.effects;
            effects.merge(primary.effects);
            effects.add_default_paths(primary_paths.clone());
            EvalResult::with_effects(
                AbstractValue::choice(
                    [primary.value, fallback.value]
                        .into_iter()
                        .flatten()
                        .collect(),
                ),
                effects,
            )
        }
        "dict" => {
            let mut map = BTreeMap::new();
            let mut effects = Effects::default();
            let mut index = 0usize;
            while index + 1 < args.len() {
                let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                    &args[index]
                else {
                    index += 1;
                    continue;
                };
                let value = eval_expr(&args[index + 1], env);
                effects.merge(value.effects);
                map.insert(key.clone(), value.value.unwrap_or(AbstractValue::Unknown));
                index += 2;
            }
            EvalResult::with_effects(Some(AbstractValue::Dict(map)), effects)
        }
        "list" | "tuple" => {
            let mut items = Vec::new();
            let mut effects = Effects::default();
            for arg in args {
                let item = eval_expr(arg, env);
                effects.merge(item.effects);
                items.push(item.value.unwrap_or(AbstractValue::Unknown));
            }
            EvalResult::with_effects(Some(AbstractValue::List(items)), effects)
        }
        "append" => eval_append(args, env),
        function if is_merge_function(function) => {
            let mut values = Vec::new();
            let mut effects = Effects::default();
            for arg in args {
                let value = eval_expr(arg, env);
                effects.merge(value.effects);
                if let Some(value) = value.value {
                    values.push(value);
                }
            }
            EvalResult::with_effects(AbstractValue::merge_all(values), effects)
        }
        "coalesce" => {
            let mut values = Vec::new();
            let mut effects = Effects::default();
            for arg in args {
                let value = eval_expr(arg, env);
                effects.merge(value.effects);
                if let Some(value) = value.value {
                    values.push(value);
                }
            }
            EvalResult::with_effects(AbstractValue::choice(values), effects)
        }
        "ternary" => {
            let mut values = Vec::new();
            let mut effects = Effects::default();
            for arg in args {
                let value = eval_expr(arg, env);
                effects.merge(value.effects);
            }
            for arg in args.iter().take(2) {
                if let Some(value) = eval_expr_value(arg, env) {
                    values.push(value);
                }
            }
            EvalResult::with_effects(AbstractValue::choice(values), effects)
        }
        "printf" => eval_printf(args, env),
        "index" => eval_index(args, env),
        "typeIs" if args.len() >= 2 => {
            let mut result = eval_all_args(args, env);
            if let Some(schema_type) = type_is_schema_type(args.first()) {
                let paths = eval_expr(&args[1], env)
                    .value
                    .as_ref()
                    .map(AbstractValue::paths)
                    .unwrap_or_default();
                result.effects.add_type_hints(paths, &schema_type);
            }
            EvalResult::with_effects(None, result.effects)
        }
        function if is_string_transform_function(function) => {
            let result = eval_all_args(args, env);
            let mut effects = result.effects;
            effects.add_string_hints(value_paths(&result.value));
            EvalResult::with_effects(result.value, effects)
        }
        function if is_provenance_preserving_function(function) => eval_all_args(args, env),
        _ => {
            let mut effects = Effects::default();
            for arg in args {
                effects.merge(eval_expr(arg, env).effects);
            }
            EvalResult::with_effects(None, effects)
        }
    }
}

fn eval_index(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let Some(base_expr) = args.first() else {
        return EvalResult::none();
    };
    let base = eval_expr(base_expr, env);
    let mut effects = base.effects;
    let Some(value) = base.value else {
        return EvalResult::with_effects(None, effects);
    };

    let mut values = vec![value];
    for arg in &args[1..] {
        let arg_result = eval_expr(arg, env);
        effects.merge(arg_result.effects);
        let Some(options) = path_segment_options(arg, arg_result.value.as_ref()) else {
            return EvalResult::with_effects(None, effects);
        };
        let mut next_values = Vec::new();
        for value in &values {
            for segment in &options {
                if let Some(next) = value.apply_to_path(std::slice::from_ref(segment)) {
                    next_values.push(next);
                }
            }
        }
        values = next_values;
    }

    let value = AbstractValue::choice(values);
    if let Some(value) = &value {
        effects.reads.extend(value.paths());
    }
    EvalResult::with_effects(value, effects)
}

fn eval_append(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let mut effects = Effects::default();
    let mut items = match args.first().map(|expr| eval_expr(expr, env)) {
        Some(result) => {
            effects.merge(result.effects);
            match result.value {
                Some(AbstractValue::List(items)) => items,
                Some(value) => vec![value],
                None => Vec::new(),
            }
        }
        None => Vec::new(),
    };
    for arg in &args[1..] {
        let result = eval_expr(arg, env);
        effects.merge(result.effects);
        if let Some(value) = result.value {
            items.push(value);
        }
    }
    EvalResult::with_effects(Some(AbstractValue::List(items)), effects)
}

fn eval_printf(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let mut effects = Effects::default();
    let mut provenance_paths = BTreeSet::new();
    let mut values = Vec::with_capacity(args.len());

    for arg in args {
        let result = eval_expr(arg, env);
        provenance_paths.extend(value_paths(&result.value));
        effects.merge(result.effects);
        values.push(result.value);
    }

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

    effects.add_string_hints(provenance_paths.clone());
    let mut values = Vec::new();
    if let Some(rendered) = rendered {
        values.push(AbstractValue::StringSet(rendered));
    }
    if !provenance_paths.is_empty() {
        values.push(AbstractValue::PathSet(provenance_paths));
    }
    EvalResult::with_effects(AbstractValue::choice(values), effects)
}

pub(crate) fn literal_printf_format(args: &[TemplateExpr]) -> Option<&str> {
    match args.first()?.deparen() {
        TemplateExpr::Literal(Literal::String(format) | Literal::RawString(format)) => {
            Some(format.as_str())
        }
        _ => None,
    }
}

pub(crate) fn render_printf_string_sets(
    format: &str,
    arg_strings: &[BTreeSet<String>],
) -> Option<BTreeSet<String>> {
    let parts = parse_supported_printf_format(format)?;
    let substitutions = parts
        .iter()
        .filter(|part| matches!(part, PrintfPart::Substitution))
        .count();
    if substitutions != arg_strings.len() {
        return None;
    }

    let mut rendered: BTreeSet<String> = [String::new()].into_iter().collect();
    let mut arg_index = 0usize;
    for part in parts {
        match part {
            PrintfPart::Literal(literal) => {
                rendered = rendered
                    .into_iter()
                    .map(|mut current| {
                        current.push_str(literal);
                        current
                    })
                    .collect();
            }
            PrintfPart::Substitution => {
                let strings = arg_strings.get(arg_index)?;
                if strings.is_empty() {
                    return None;
                }
                let mut next = BTreeSet::new();
                for current in &rendered {
                    for value in strings {
                        let mut rendered_value = current.clone();
                        rendered_value.push_str(value);
                        next.insert(rendered_value);
                    }
                }
                rendered = next;
                arg_index += 1;
            }
        }
    }
    Some(rendered)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrintfPart<'a> {
    Literal(&'a str),
    Substitution,
}

fn parse_supported_printf_format(format: &str) -> Option<Vec<PrintfPart<'_>>> {
    let mut parts = Vec::new();
    let mut literal_start = 0usize;
    let bytes = format.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }

        if literal_start < index {
            parts.push(PrintfPart::Literal(format.get(literal_start..index)?));
        }

        match *bytes.get(index + 1)? {
            b'%' => {
                parts.push(PrintfPart::Literal("%"));
                index += 2;
                literal_start = index;
            }
            b's' => {
                parts.push(PrintfPart::Substitution);
                index += 2;
                literal_start = index;
            }
            _ => return None,
        }
    }

    if literal_start < format.len() {
        parts.push(PrintfPart::Literal(format.get(literal_start..)?));
    }

    Some(parts)
}

fn eval_pipeline(stages: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
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

fn eval_all_args(args: &[TemplateExpr], env: &EvalEnv) -> EvalResult {
    let mut values = Vec::new();
    let mut effects = Effects::default();
    for arg in args {
        let result = eval_expr(arg, env);
        effects.merge(result.effects);
        if let Some(value) = result.value {
            values.push(value);
        }
    }
    EvalResult::with_effects(AbstractValue::choice(values), effects)
}

fn value_paths(value: &Option<AbstractValue>) -> BTreeSet<String> {
    value.as_ref().map(AbstractValue::paths).unwrap_or_default()
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

pub(crate) fn type_is_schema_type(expr: Option<&TemplateExpr>) -> Option<String> {
    let TemplateExpr::Literal(Literal::String(type_name) | Literal::RawString(type_name)) =
        expr?.deparen()
    else {
        return None;
    };
    let schema_type = match type_name.as_str() {
        "bool" | "boolean" => "boolean",
        "float64" | "number" => "number",
        "int" | "int64" | "integer" => "integer",
        "list" | "slice" | "array" => "array",
        "map" | "dict" | "object" => "object",
        "string" => "string",
        _ => return None,
    };
    Some(schema_type.to_string())
}

fn is_string_transform_function(function: &str) -> bool {
    matches!(
        function,
        "quote"
            | "squote"
            | "toString"
            | "trunc"
            | "trim"
            | "trimAll"
            | "trimPrefix"
            | "trimSuffix"
            | "replace"
    )
}

fn is_provenance_preserving_function(function: &str) -> bool {
    matches!(
        function,
        "toYaml" | "fromYaml" | "deepCopy" | "tpl" | "indent" | "nindent" | "printf" | "int"
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use helm_schema_ast::parse_action_expressions;

    use super::*;

    fn single_expr(action: &str) -> TemplateExpr {
        let exprs = parse_action_expressions(&format!("{{{{ {action} }}}}"));
        assert_eq!(exprs.len(), 1, "expected exactly one parsed expression");
        exprs.into_iter().next().expect("expression exists")
    }

    #[test]
    fn string_transform_pipeline_preserves_all_printf_argument_paths() {
        let expr = single_expr(r#"printf "%s-%s" .Values.primary.name .Values.suffix | trunc 63"#);
        let result = eval_expr(&expr, &EvalEnv::default());

        assert!(
            result.effects.string_hints.contains("primary.name"),
            "primary.name should remain visible through printf before trunc"
        );
        assert!(
            result.effects.string_hints.contains("suffix"),
            "suffix should remain visible through printf before trunc"
        );
    }

    #[test]
    fn printf_exact_rendering_only_accepts_supported_string_formats() {
        let values = [BTreeSet::from(["x".to_string()])];

        assert_eq!(
            render_printf_string_sets("prefix-%s-%%", &values),
            Some(BTreeSet::from(["prefix-x-%".to_string()]))
        );
        assert_eq!(render_printf_string_sets("%d", &values), None);
        assert_eq!(
            render_printf_string_sets("literal", &[BTreeSet::from(["unused".to_string()])]),
            None
        );
        assert_eq!(render_printf_string_sets("%s-%s", &values), None);
    }

    #[test]
    fn unsupported_printf_format_preserves_string_hint_without_exact_string() {
        let expr = single_expr(r#"printf "%d" .Values.count"#);
        let result = eval_expr(&expr, &EvalEnv::default());

        assert!(
            result.effects.string_hints.contains("count"),
            "unsupported printf formats still prove scalar string-context use"
        );
        assert!(
            result
                .value
                .as_ref()
                .map(AbstractValue::strings)
                .unwrap_or_default()
                .is_empty(),
            "unsupported printf formats must not synthesize exact strings"
        );
    }

    #[test]
    fn helper_argument_fields_resolve_from_dot_root() {
        let expr = single_expr(r#"default "generated" .config.name"#);
        let env = EvalEnv {
            root_fields: HashMap::from([(
                "config".to_string(),
                AbstractValue::ValuesPath("serviceAccount".to_string()),
            )]),
            allow_field_root_lookup: true,
            ..EvalEnv::default()
        };

        let result = eval_expr(&expr, &env);

        assert!(
            result.effects.defaults.contains("serviceAccount.name"),
            "default should attach to the values path reached through .config.name"
        );
    }
}
