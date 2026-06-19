use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

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

fn guard_value_literal(expr: &TemplateExpr) -> Option<GuardValue> {
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

fn type_is_schema_type(expr: Option<&TemplateExpr>) -> Option<String> {
    let TemplateExpr::Literal(
        helm_schema_ast::Literal::String(type_name)
        | helm_schema_ast::Literal::RawString(type_name),
    ) = expr?.deparen()
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

fn single(paths: &BTreeSet<String>) -> Option<String> {
    if paths.len() == 1 {
        paths.iter().next().cloned()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::parse_condition_expr;
    use crate::{Guard, GuardValue};

    fn parse_condition(text: &str) -> Vec<Guard> {
        let wrapped = format!("{{{{ {text} }}}}");
        let Some(top) = helm_schema_ast::parse_action_expressions(&wrapped)
            .into_iter()
            .next()
        else {
            return Vec::new();
        };
        parse_condition_expr(&top)
    }

    #[test]
    fn truthy_simple_path() {
        assert_eq!(
            parse_condition(".Values.X"),
            vec![Guard::Truthy { path: "X".into() }],
        );
    }

    #[test]
    fn not_simple_path() {
        assert_eq!(
            parse_condition("not .Values.X"),
            vec![Guard::Not { path: "X".into() }],
        );
    }

    #[test]
    fn not_with_nested_helper_call() {
        assert_eq!(
            parse_condition(r#"not (has (quote .Values.global.logLevel) (list "" (quote "")))"#),
            vec![Guard::Not {
                path: "global.logLevel".into(),
            }],
        );
    }

    #[test]
    fn or_with_two_paths_emits_or_guard() {
        assert_eq!(
            parse_condition("or .Values.A .Values.B"),
            vec![Guard::Or {
                paths: vec!["A".into(), "B".into()],
            }],
        );
    }

    #[test]
    fn or_paths_are_sorted() {
        assert_eq!(
            parse_condition("or .Values.z .Values.a"),
            vec![Guard::Or {
                paths: vec!["a".into(), "z".into()],
            }],
        );
    }

    #[test]
    fn or_with_nested_helper_calls() {
        assert_eq!(
            parse_condition("or (has .Values.A 1) (has .Values.B 2)"),
            vec![Guard::Or {
                paths: vec!["A".into(), "B".into()],
            }],
        );
    }

    #[test]
    fn or_with_equality_preserves_typed_alternative() {
        assert_eq!(
            parse_condition(r#"or (eq .Values.mode "prod") .Values.enabled"#),
            vec![Guard::AnyOf {
                alternatives: vec![
                    vec![Guard::Truthy {
                        path: "enabled".into(),
                    }],
                    vec![Guard::Eq {
                        path: "mode".into(),
                        value: GuardValue::string("prod"),
                    }],
                ],
            }],
        );
    }

    #[test]
    fn or_with_nested_and_preserves_conjunctive_alternative() {
        assert_eq!(
            parse_condition(r#"or (and .Values.a .Values.b) (eq .Values.mode "prod")"#),
            vec![Guard::AnyOf {
                alternatives: vec![
                    vec![
                        Guard::Truthy { path: "a".into() },
                        Guard::Truthy { path: "b".into() },
                    ],
                    vec![Guard::Eq {
                        path: "mode".into(),
                        value: GuardValue::string("prod"),
                    }],
                ],
            }],
        );
    }

    #[test]
    fn eq_with_string_literal() {
        assert_eq!(
            parse_condition(r#"eq .Values.X "value""#),
            vec![Guard::Eq {
                path: "X".into(),
                value: GuardValue::string("value"),
            }],
        );
    }

    #[test]
    fn eq_with_string_literal_containing_phantom_path() {
        assert_eq!(
            parse_condition(r#"eq .Values.X ".Values.fake""#),
            vec![Guard::Eq {
                path: "X".into(),
                value: GuardValue::string(".Values.fake"),
            }],
        );
    }

    #[test]
    fn eq_with_bool_literal_preserves_exact_comparison() {
        assert_eq!(
            parse_condition("eq .Values.enabled false"),
            vec![Guard::Eq {
                path: "enabled".into(),
                value: GuardValue::Bool(false),
            }],
        );
    }

    #[test]
    fn eq_with_int_literal_preserves_exact_comparison() {
        assert_eq!(
            parse_condition("eq .Values.replicas 3"),
            vec![Guard::Eq {
                path: "replicas".into(),
                value: GuardValue::Int(3),
            }],
        );
    }

    #[test]
    fn eq_with_nil_literal_preserves_exact_comparison() {
        assert_eq!(
            parse_condition("eq .Values.image.tag nil"),
            vec![Guard::Eq {
                path: "image.tag".into(),
                value: GuardValue::Null,
            }],
        );
    }

    #[test]
    fn eq_compare_two_values_falls_through_to_truthy() {
        assert_eq!(
            parse_condition("eq .Values.X .Values.Y"),
            vec![
                Guard::Truthy { path: "X".into() },
                Guard::Truthy { path: "Y".into() },
            ],
        );
    }

    #[test]
    fn ne_with_string_literal_emits_not_eq() {
        assert_eq!(
            parse_condition(r#"ne .Values.X "value""#),
            vec![Guard::NotEq {
                path: "X".into(),
                value: GuardValue::string("value"),
            }],
        );
    }

    #[test]
    fn not_eq_literal_projects_to_not_eq() {
        assert_eq!(
            parse_condition(r#"not (eq .Values.mode "disabled")"#),
            vec![Guard::NotEq {
                path: "mode".into(),
                value: GuardValue::string("disabled"),
            }],
        );
    }

    #[test]
    fn not_ne_literal_projects_to_eq() {
        assert_eq!(
            parse_condition(r#"not (ne .Values.mode "disabled")"#),
            vec![Guard::Eq {
                path: "mode".into(),
                value: GuardValue::string("disabled"),
            }],
        );
    }

    #[test]
    fn and_falls_through_to_per_path_truthy() {
        assert_eq!(
            parse_condition("and .Values.A .Values.B"),
            vec![
                Guard::Truthy { path: "A".into() },
                Guard::Truthy { path: "B".into() },
            ],
        );
    }

    #[test]
    fn and_with_parens_falls_through_to_per_path_truthy() {
        assert_eq!(
            parse_condition("and (.Values.A) (.Values.B)"),
            vec![
                Guard::Truthy { path: "A".into() },
                Guard::Truthy { path: "B".into() },
            ],
        );
    }

    #[test]
    fn and_preserves_nested_not_guard() {
        assert_eq!(
            parse_condition(
                "and .Values.prometheus.enabled (not .Values.prometheus.podmonitor.enabled)"
            ),
            vec![
                Guard::Truthy {
                    path: "prometheus.enabled".into()
                },
                Guard::Not {
                    path: "prometheus.podmonitor.enabled".into()
                },
            ],
        );
    }

    #[test]
    fn and_preserves_nested_or_guard() {
        assert_eq!(
            parse_condition(
                "and .Values.ldap.enabled (or .Values.ldap.bind_password .Values.ldap.bindpw)"
            ),
            vec![
                Guard::Truthy {
                    path: "ldap.enabled".into()
                },
                Guard::Or {
                    paths: vec!["ldap.bind_password".into(), "ldap.bindpw".into()]
                },
            ],
        );
    }

    #[test]
    fn empty_path_is_falsey_guard() {
        assert_eq!(
            parse_condition("empty .Values.service.loadBalancerIP"),
            vec![Guard::Not {
                path: "service.loadBalancerIP".into()
            }],
        );
    }

    #[test]
    fn not_empty_path_is_truthy_guard() {
        assert_eq!(
            parse_condition("not (empty .Values.service.loadBalancerIP)"),
            vec![Guard::Truthy {
                path: "service.loadBalancerIP".into()
            }],
        );
    }

    #[test]
    fn empty_condition_returns_empty() {
        assert!(parse_condition("").is_empty());
        assert!(parse_condition("   ").is_empty());
    }

    #[test]
    fn condition_without_values_reference_returns_empty() {
        assert!(parse_condition(".Chart.Name").is_empty());
        assert!(parse_condition("not (empty $var)").is_empty());
    }

    #[test]
    fn eq_value_preserves_literal_dot_star_substring() {
        assert_eq!(
            parse_condition(r#"eq .Values.X "match.*foo""#),
            vec![Guard::Eq {
                path: "X".into(),
                value: GuardValue::string("match.*foo"),
            }],
        );
    }

    #[test]
    fn eq_value_preserves_dot_values_substring_inside_string() {
        assert_eq!(
            parse_condition(r#"eq .Values.X ".Values.fake""#),
            vec![Guard::Eq {
                path: "X".into(),
                value: GuardValue::string(".Values.fake"),
            }],
        );
    }
}
