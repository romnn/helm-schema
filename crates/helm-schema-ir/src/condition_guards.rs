use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::expr_function_catalog::type_is_schema_type;
use crate::value_path_extraction::{collect_loose_values_paths, values_path_from_expr};
use crate::{Guard, GuardValue};

#[must_use]
pub(crate) fn parse_condition_expr(top: &TemplateExpr) -> Vec<Guard> {
    if let Some(guards) = parse_structural_condition_expr(top) {
        return dedupe_guards(guards);
    }

    loose_truthy_guards(top)
}

fn parse_structural_condition_expr(expr: &TemplateExpr) -> Option<Vec<Guard>> {
    if let Some(path) = values_path_from_expr(expr) {
        return Some(vec![Guard::Truthy { path }]);
    }

    let TemplateExpr::Call { function, args } = expr.deparen() else {
        return None;
    };

    match function.as_str() {
        "and" => {
            let mut guards = Vec::new();
            for arg in args {
                let child_guards = parse_structural_condition_expr(arg)
                    .unwrap_or_else(|| loose_truthy_guards(arg));
                extend_unique_guards(&mut guards, child_guards);
            }
            (!guards.is_empty()).then_some(guards)
        }
        "not" => {
            let [arg] = args.as_slice() else {
                return None;
            };
            parse_negated_condition_expr(arg)
        }
        "empty" => {
            let [arg] = args.as_slice() else {
                return None;
            };
            single_loose_path(arg).map(|path| vec![Guard::Not { path }])
        }
        "or" => {
            let alternatives = parse_or_alternatives(args)?;
            Some(or_guard_from_alternatives(alternatives))
        }
        "eq" => {
            let [left, right] = args.as_slice() else {
                return None;
            };
            let (path, value) = comparison_path_and_literal(left, right)?;
            Some(vec![Guard::Eq { path, value }])
        }
        "ne" => {
            let [left, right] = args.as_slice() else {
                return None;
            };
            let (path, value) = comparison_path_and_literal(left, right)?;
            Some(vec![Guard::NotEq { path, value }])
        }
        "typeIs" => {
            let schema_type = type_is_schema_type(args.first())?;
            let mut paths = BTreeSet::new();
            for arg in args.iter().skip(1) {
                collect_loose_values_paths(arg, &mut paths);
            }
            let path = single(&paths)?;
            Some(vec![Guard::TypeIs { path, schema_type }])
        }
        _ => None,
    }
}

fn parse_or_alternatives(args: &[TemplateExpr]) -> Option<Vec<Vec<Guard>>> {
    let mut alternatives = Vec::new();
    for arg in args {
        let guards = parse_structural_condition_expr(arg)
            .or_else(|| single_loose_path(arg).map(|path| vec![Guard::Truthy { path }]))?;
        alternatives.push(guards);
    }
    (!alternatives.is_empty()).then_some(alternatives)
}

fn or_guard_from_alternatives(mut alternatives: Vec<Vec<Guard>>) -> Vec<Guard> {
    for alternative in &mut alternatives {
        alternative.sort();
        alternative.dedup();
    }
    alternatives.sort();
    alternatives.dedup();

    if alternatives.len() == 1 {
        return alternatives.pop().unwrap_or_default();
    }

    if let Some(paths) = truthy_or_paths(&alternatives) {
        return vec![Guard::Or { paths }];
    }

    vec![Guard::AnyOf { alternatives }]
}

fn truthy_or_paths(alternatives: &[Vec<Guard>]) -> Option<Vec<String>> {
    alternatives
        .iter()
        .map(|alternative| match alternative.as_slice() {
            [Guard::Truthy { path }] => Some(path.clone()),
            _ => None,
        })
        .collect()
}

fn parse_negated_condition_expr(expr: &TemplateExpr) -> Option<Vec<Guard>> {
    if let Some(path) = values_path_from_expr(expr) {
        return Some(vec![Guard::Not { path }]);
    }

    if let TemplateExpr::Call { function, args } = expr.deparen()
        && function == "empty"
    {
        let [arg] = args.as_slice() else {
            return None;
        };
        return single_loose_path(arg).map(|path| vec![Guard::Truthy { path }]);
    }

    if let TemplateExpr::Call { function, args } = expr.deparen()
        && matches!(function.as_str(), "eq" | "ne")
        && let [left, right] = args.as_slice()
        && let Some((path, value)) = comparison_path_and_literal(left, right)
    {
        return Some(match function.as_str() {
            "eq" => vec![Guard::NotEq { path, value }],
            "ne" => vec![Guard::Eq { path, value }],
            _ => unreachable!("function is restricted above"),
        });
    }

    let mut paths = BTreeSet::new();
    collect_loose_values_paths(expr, &mut paths);
    single(&paths).map(|path| vec![Guard::Not { path }])
}

fn loose_truthy_guards(expr: &TemplateExpr) -> Vec<Guard> {
    let mut paths = BTreeSet::new();
    collect_loose_values_paths(expr, &mut paths);

    paths
        .into_iter()
        .map(|path| Guard::Truthy { path })
        .collect()
}

fn single_loose_path(expr: &TemplateExpr) -> Option<String> {
    let mut paths = BTreeSet::new();
    collect_loose_values_paths(expr, &mut paths);
    single(&paths)
}

fn comparison_path_and_literal(
    left: &TemplateExpr,
    right: &TemplateExpr,
) -> Option<(String, GuardValue)> {
    match (
        single_loose_path(left),
        guard_value_literal(left),
        single_loose_path(right),
        guard_value_literal(right),
    ) {
        (Some(path), None, None, Some(value)) | (None, Some(value), Some(path), None) => {
            Some((path, value))
        }
        _ => None,
    }
}

pub(crate) fn guard_value_literal(expr: &TemplateExpr) -> Option<GuardValue> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
            Some(GuardValue::string(value))
        }
        TemplateExpr::Literal(Literal::Bool(value)) => Some(GuardValue::Bool(*value)),
        TemplateExpr::Literal(Literal::Int(value)) => Some(GuardValue::Int(*value)),
        TemplateExpr::Literal(Literal::Float(value)) => GuardValue::float(*value),
        TemplateExpr::Literal(Literal::Nil) => Some(GuardValue::Null),
        _ => None,
    }
}

fn dedupe_guards(guards: Vec<Guard>) -> Vec<Guard> {
    let mut out = Vec::new();
    extend_unique_guards(&mut out, guards);
    out
}

fn extend_unique_guards(out: &mut Vec<Guard>, guards: Vec<Guard>) {
    for guard in guards {
        if !out.contains(&guard) {
            out.push(guard);
        }
    }
}

fn single(paths: &BTreeSet<String>) -> Option<String> {
    if paths.len() == 1 {
        paths.iter().next().cloned()
    } else {
        None
    }
}

#[cfg(test)]
#[path = "tests/condition_guards.rs"]
mod tests;
