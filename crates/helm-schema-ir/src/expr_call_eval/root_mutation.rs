use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{
    HelperCallValueResolver, direct_values_path, eval_expr, eval_expr_with_helper_calls,
};
use helm_schema_core::{Guard, GuardValue, Predicate};

use super::value_facts::{value_paths, value_strings};

pub(super) fn eval_set_call(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let mut effects = Effects::default();
    let target_result = args
        .first()
        .map(|expr| eval_expr_with_helper_calls(expr, env, resolver))
        .unwrap_or_else(EvalResult::none);
    let target_paths = value_paths(&target_result.value);
    let root_target = matches!(target_result.value, Some(AbstractValue::RootContext));
    effects.merge(target_result.effects);
    let target = match args.first().map(TemplateExpr::deparen) {
        Some(TemplateExpr::Variable(name)) => {
            let name = name.trim_start_matches('$');
            if !name.is_empty() && env.locals.contains_key(name) {
                Some(name.to_string())
            } else {
                None
            }
        }
        _ => None,
    };
    // `set $copy.Values KEY V` on a context-copy local (`$copy := deepCopy
    // .`) replaces one root values member for everything that later reads
    // the copy (`with $copy` blocks): observed as a mutation of the LOCAL
    // whose `Values` member overlays KEY over the real values root
    // (airflow's per-worker-set `set $globals.Values "workers" $workers`).
    let values_member_target = match args.first().map(TemplateExpr::deparen) {
        Some(TemplateExpr::Selector { operand, path })
            if path.as_slice() == ["Values"]
                && matches!(
                    operand.deparen(),
                    TemplateExpr::Variable(name)
                        if !name.trim_start_matches('$').is_empty()
                ) =>
        {
            match operand.deparen() {
                TemplateExpr::Variable(name) => {
                    let name = name.trim_start_matches('$');
                    env.locals
                        .get(name)
                        .and_then(|local| local.apply_to_path(&["Values".to_string()]))
                        .map(|old_values| (name.to_string(), old_values))
                }
                _ => None,
            }
        }
        _ => None,
    };
    let mut keys = BTreeSet::new();
    if let Some(expr) = args.get(1) {
        let key = eval_expr_with_helper_calls(expr, env, resolver);
        keys = value_strings(&key.value);
        effects.merge(key.effects);
    }
    let assigned_predicate = args
        .get(2)
        .and_then(|expr| root_set_truthy_predicate(expr, env));
    let assigned_value = if let Some(expr) = args.get(2) {
        let result = eval_expr_with_helper_calls(expr, env, resolver);
        let value = result.value.clone().unwrap_or(AbstractValue::Unknown);
        effects.merge(result.effects);
        value
    } else {
        AbstractValue::Unknown
    };
    for target_path in &target_paths {
        for key in &keys {
            let defaulted_path = helm_schema_core::append_value_path(target_path, key);
            if effects.defaults.contains(&defaulted_path) {
                effects.chart_default_paths.insert(defaulted_path);
            }
        }
    }
    let value = target
        .as_ref()
        .and_then(|target| env.locals.get(target))
        .cloned()
        .map(|value| {
            let entries = keys
                .iter()
                .map(|key| (key.clone(), assigned_value.clone()))
                .collect();
            value.with_overlay_entries(entries)
        });
    if let Some(target) = target {
        effects.add_local_set_mutation(target, keys, assigned_value);
    } else if let Some((local, old_values)) = values_member_target {
        // An unresolvable assigned value must not replace the member: the
        // copy would then resolve `.Values.KEY.…` to nothing where the
        // unobserved form still reached the plain path.
        if !keys.is_empty()
            && !matches!(assigned_value, AbstractValue::Unknown | AbstractValue::Top)
        {
            let entries = keys
                .iter()
                .map(|key| (key.clone(), assigned_value.clone()))
                .collect();
            effects.add_local_set_mutation(
                local,
                BTreeSet::from(["Values".to_string()]),
                old_values.with_overlay_entries(entries),
            );
        }
    } else if root_target {
        if keys.contains("Values")
            && let Some(assigned) = args.get(2)
            && let Some(source) = root_values_default_source(assigned, env)
        {
            effects.values_default_sources.insert(source);
        }
        for key in keys {
            if let Some(predicate) = &assigned_predicate {
                effects
                    .root_set_predicates
                    .insert(key.clone(), predicate.clone());
            }
            effects
                .root_set_mutations
                .insert(key, assigned_value.clone());
        }
    }
    EvalResult::with_effects(value, effects)
}

pub(super) fn root_set_truthy_predicate(expr: &TemplateExpr, env: &EvalEnv) -> Option<Predicate> {
    match expr.deparen() {
        TemplateExpr::Literal(literal) => Some(if root_set_literal_is_truthy(literal) {
            Predicate::True
        } else {
            Predicate::False
        }),
        TemplateExpr::Field(path) if path.len() == 1 => {
            env.root_truthy_predicates.get(&path[0]).cloned()
        }
        TemplateExpr::Selector { operand, path }
            if path.len() == 1
                && matches!(operand.as_ref(), TemplateExpr::Variable(variable) if variable.is_empty()) =>
        {
            env.root_truthy_predicates.get(&path[0]).cloned()
        }
        TemplateExpr::Field(_) | TemplateExpr::Selector { .. } => {
            direct_values_path(expr).map(Predicate::truthy_path)
        }
        TemplateExpr::Call { function, args } => match function.as_str() {
            "and" => args
                .iter()
                .map(|arg| root_set_truthy_predicate(arg, env))
                .collect::<Option<Vec<_>>>()
                .map(Predicate::all),
            "or" => args
                .iter()
                .map(|arg| root_set_truthy_predicate(arg, env))
                .collect::<Option<Vec<_>>>()
                .map(root_set_predicate_any),
            "not" => {
                let [arg] = args.as_slice() else {
                    return None;
                };
                root_set_truthy_predicate(arg, env).map(|predicate| predicate.negated())
            }
            "eq" | "ne" => root_set_stringified_comparison(args, function == "ne"),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn root_set_stringified_comparison(
    args: &[TemplateExpr],
    negated: bool,
) -> Option<Predicate> {
    let [left, right] = args else {
        return None;
    };
    let (subject, target) = match (
        stringified_values_path(left),
        root_set_string_literal(right),
        stringified_values_path(right),
        root_set_string_literal(left),
    ) {
        (Some(subject), Some(target), None, None) | (None, None, Some(subject), Some(target)) => {
            (subject, target)
        }
        _ => return None,
    };
    let predicate = match target {
        "true" => root_set_predicate_any(vec![
            Predicate::from(Guard::Eq {
                path: subject.clone(),
                value: GuardValue::string("true"),
            }),
            Predicate::from(Guard::Eq {
                path: subject,
                value: GuardValue::Bool(true),
            }),
        ]),
        "false" => root_set_predicate_any(vec![
            Predicate::from(Guard::Eq {
                path: subject.clone(),
                value: GuardValue::string("false"),
            }),
            Predicate::from(Guard::Eq {
                path: subject,
                value: GuardValue::Bool(false),
            }),
        ]),
        target => Predicate::from(Guard::Eq {
            path: subject,
            value: GuardValue::string(target),
        }),
    };
    Some(if negated {
        predicate.negated()
    } else {
        predicate
    })
}

pub(super) fn stringified_values_path(expr: &TemplateExpr) -> Option<String> {
    match expr.deparen() {
        TemplateExpr::Call { function, args } if function == "toString" => {
            let [subject] = args.as_slice() else {
                return None;
            };
            direct_values_path(subject)
        }
        TemplateExpr::Pipeline(stages) => {
            let [subject, stage] = stages.as_slice() else {
                return None;
            };
            let TemplateExpr::Call { function, args } = stage.deparen() else {
                return None;
            };
            if function != "toString" || !args.is_empty() {
                return None;
            }
            direct_values_path(subject)
        }
        _ => None,
    }
}

pub(super) fn root_set_string_literal(expr: &TemplateExpr) -> Option<&str> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => Some(value),
        _ => None,
    }
}

pub(super) fn root_set_literal_is_truthy(literal: &Literal) -> bool {
    match literal {
        Literal::Bool(value) => *value,
        Literal::Int(value) => *value != 0,
        Literal::Float(value) => *value != 0.0,
        Literal::String(value) | Literal::RawString(value) => !value.is_empty(),
        Literal::Nil => false,
    }
}

pub(super) fn root_set_predicate_any(predicates: Vec<Predicate>) -> Predicate {
    if predicates
        .iter()
        .any(|predicate| matches!(predicate, Predicate::True))
    {
        return Predicate::True;
    }
    let mut predicates = predicates
        .into_iter()
        .filter(|predicate| !matches!(predicate, Predicate::False))
        .collect::<Vec<_>>();
    match predicates.len() {
        0 => Predicate::False,
        1 => predicates.remove(0),
        _ => Predicate::Or(predicates),
    }
}

pub(super) fn root_values_default_source(
    assigned: &TemplateExpr,
    env: &EvalEnv,
) -> Option<crate::ValuesDefaultSource> {
    let TemplateExpr::Call { function, args } = assigned.deparen() else {
        return None;
    };
    let [first, second] = args.as_slice() else {
        return None;
    };
    let (source, effective) = match function.as_str() {
        "merge" | "mustMerge" => (second, first),
        "mergeOverwrite" | "mustMergeOverwrite" => (first, second),
        _ => return None,
    };
    let effective_path = eval_expr(effective, env).value?.unique_path()?;
    if !effective_path.is_empty() {
        return None;
    }
    let source_path = eval_expr(source, env).value?.unique_path()?;
    if source_path.is_empty() {
        return None;
    }
    Some(crate::ValuesDefaultSource {
        target_path: String::new(),
        source_path,
    })
}
