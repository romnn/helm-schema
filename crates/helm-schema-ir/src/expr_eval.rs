use std::collections::{BTreeSet, HashMap};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_call_eval::{eval_call_with_helper_calls, eval_pipeline_with_helper_calls};
use helm_schema_ast::is_merge_function;

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
                return with_bound_selector_paths(
                    local_value_result(var, value, Some(&selected_paths), env),
                    expr,
                    env,
                );
            }
            if let TemplateExpr::Variable(var) = operand.as_ref()
                && !var.is_empty()
                && path.first().is_some_and(|segment| segment == "Values")
            {
                return with_bound_selector_paths(
                    EvalResult::from_value(values_path_value(path)),
                    expr,
                    env,
                );
            }
            if let TemplateExpr::Variable(var) = operand.as_ref()
                && var.is_empty()
                && let Some((head, tail)) = path.split_first()
                && let Some(value) = env
                    .root_fields
                    .get(head)
                    .and_then(|value| value.apply_to_path(tail))
            {
                return with_bound_selector_paths(EvalResult::from_value(value), expr, env);
            }
            let base = eval_expr_with_helper_calls(operand, env, resolver);
            let value = base
                .value
                .as_ref()
                .and_then(|value| value.apply_to_path(path));
            let mut effects = base.effects;
            effects.output_paths.clear();
            effects
                .bound_output_paths
                .extend(env.bound_values.selector_paths(expr));
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

pub(crate) fn eval_helper_exprs_direct_effects(
    exprs: &[TemplateExpr],
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
) -> Effects {
    let env = EvalEnv::from_helper_context(Some(bindings), current_dot).without_helper_call_args();
    eval_exprs_effects(exprs, &env)
}

pub(crate) fn bindings_for_helper_arg_with(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, AbstractValue>>,
    mut eval_binding: impl FnMut(&TemplateExpr) -> Option<AbstractValue>,
) -> HashMap<String, AbstractValue> {
    let Some(arg) = arg else {
        return HashMap::new();
    };

    match arg.deparen() {
        TemplateExpr::Parenthesized(inner) => {
            bindings_for_helper_arg_with(Some(inner), outer, eval_binding)
        }
        TemplateExpr::Field(path) if path.is_empty() => outer.cloned().unwrap_or_default(),
        TemplateExpr::Variable(var) if var.is_empty() => outer.cloned().unwrap_or_default(),
        TemplateExpr::Call { function, args } if is_merge_function(function) => {
            let mut merged = HashMap::new();
            for arg in args {
                merged.extend(bindings_from_helper_arg_value(eval_binding(arg), outer));
            }
            merged
        }
        _ => bindings_from_helper_arg_value(eval_binding(arg), outer),
    }
}

fn bindings_from_helper_arg_value(
    value: Option<AbstractValue>,
    outer: Option<&HashMap<String, AbstractValue>>,
) -> HashMap<String, AbstractValue> {
    match value {
        Some(AbstractValue::Dict(map)) => map.into_iter().collect(),
        Some(AbstractValue::RootContext) => outer.cloned().unwrap_or_default(),
        Some(AbstractValue::Overlay { entries, fallback }) => {
            let mut bindings = bindings_from_helper_arg_value(Some(*fallback), outer);
            bindings.extend(entries);
            bindings
        }
        _ => HashMap::new(),
    }
}

pub(crate) fn expr_starts_with_helper_call(expr: &TemplateExpr) -> bool {
    match expr.deparen() {
        TemplateExpr::Call { function, .. } => is_helper_call_function(function),
        TemplateExpr::Pipeline(stages) => stages.first().is_some_and(expr_starts_with_helper_call),
        _ => false,
    }
}

pub(crate) fn literal_helper_call_callee<'a>(
    function: &str,
    args: &'a [TemplateExpr],
) -> Option<&'a str> {
    if !is_helper_call_function(function) {
        return None;
    }
    let Some(TemplateExpr::Literal(lit)) = args.first().map(TemplateExpr::deparen) else {
        return None;
    };
    lit.as_string()
}

pub(crate) fn expr_literal_helper_call_callee(expr: &TemplateExpr) -> Option<&str> {
    let TemplateExpr::Call { function, args } = expr.deparen() else {
        return None;
    };
    literal_helper_call_callee(function, args)
}

/// Leading `$var` of an expression or pipeline, after unwrapping parens.
pub(crate) fn expr_leading_variable(expr: &TemplateExpr) -> Option<&str> {
    match expr.deparen() {
        TemplateExpr::Variable(name) if !name.is_empty() => Some(name.as_str()),
        TemplateExpr::Pipeline(stages) => stages.first().and_then(expr_leading_variable),
        _ => None,
    }
}

pub(crate) fn is_helper_call_function(function: &str) -> bool {
    matches!(function, "include" | "template")
}

fn local_value_result(
    var: &str,
    value: AbstractValue,
    selected_paths: Option<&BTreeSet<String>>,
    env: &EvalEnv,
) -> EvalResult {
    let mut result = EvalResult::from_value(value.clone());
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

fn with_bound_selector_paths(
    mut result: EvalResult,
    expr: &TemplateExpr,
    env: &EvalEnv,
) -> EvalResult {
    result
        .effects
        .bound_output_paths
        .extend(env.bound_values.selector_paths(expr));
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
