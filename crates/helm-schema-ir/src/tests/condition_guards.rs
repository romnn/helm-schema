use super::parse_condition_expr;
use crate::{Guard, GuardValue};
use test_util::prelude::sim_assert_eq;

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
    sim_assert_eq!(
        have: parse_condition(".Values.X"),
        want: vec![Guard::Truthy { path: "X".into() }],
    );
}

#[test]
fn not_simple_path() {
    sim_assert_eq!(
        have: parse_condition("not .Values.X"),
        want: vec![Guard::Not { path: "X".into() }],
    );
}

#[test]
fn not_with_nested_helper_call() {
    sim_assert_eq!(
        have: parse_condition(r#"not (has (quote .Values.global.logLevel) (list "" (quote "")))"#),
        want: vec![Guard::Not {
            path: "global.logLevel".into(),
        }],
    );
}

#[test]
fn or_with_two_paths_emits_or_guard() {
    sim_assert_eq!(
        have: parse_condition("or .Values.A .Values.B"),
        want: vec![Guard::Or {
            paths: vec!["A".into(), "B".into()],
        }],
    );
}

#[test]
fn or_paths_are_sorted() {
    sim_assert_eq!(
        have: parse_condition("or .Values.z .Values.a"),
        want: vec![Guard::Or {
            paths: vec!["a".into(), "z".into()],
        }],
    );
}

#[test]
fn or_with_nested_helper_calls() {
    sim_assert_eq!(
        have: parse_condition("or (has .Values.A 1) (has .Values.B 2)"),
        want: vec![Guard::Or {
            paths: vec!["A".into(), "B".into()],
        }],
    );
}

#[test]
fn or_with_equality_preserves_typed_alternative() {
    sim_assert_eq!(
        have: parse_condition(r#"or (eq .Values.mode "prod") .Values.enabled"#),
        want: vec![Guard::AnyOf {
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
    sim_assert_eq!(
        have: parse_condition(r#"or (and .Values.a .Values.b) (eq .Values.mode "prod")"#),
        want: vec![Guard::AnyOf {
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
    sim_assert_eq!(
        have: parse_condition(r#"eq .Values.X "value""#),
        want: vec![Guard::Eq {
            path: "X".into(),
            value: GuardValue::string("value"),
        }],
    );
}

#[test]
fn eq_with_string_literal_containing_phantom_path() {
    sim_assert_eq!(
        have: parse_condition(r#"eq .Values.X ".Values.fake""#),
        want: vec![Guard::Eq {
            path: "X".into(),
            value: GuardValue::string(".Values.fake"),
        }],
    );
}

#[test]
fn eq_with_bool_literal_preserves_exact_comparison() {
    sim_assert_eq!(
        have: parse_condition("eq .Values.enabled false"),
        want: vec![Guard::Eq {
            path: "enabled".into(),
            value: GuardValue::Bool(false),
        }],
    );
}

#[test]
fn eq_with_int_literal_preserves_exact_comparison() {
    sim_assert_eq!(
        have: parse_condition("eq .Values.replicas 3"),
        want: vec![Guard::Eq {
            path: "replicas".into(),
            value: GuardValue::Int(3),
        }],
    );
}

#[test]
fn eq_with_nil_literal_preserves_exact_comparison() {
    sim_assert_eq!(
        have: parse_condition("eq .Values.image.tag nil"),
        want: vec![Guard::Eq {
            path: "image.tag".into(),
            value: GuardValue::Null,
        }],
    );
}

#[test]
fn eq_compare_two_values_falls_through_to_truthy() {
    sim_assert_eq!(
        have: parse_condition("eq .Values.X .Values.Y"),
        want: vec![
            Guard::Truthy { path: "X".into() },
            Guard::Truthy { path: "Y".into() },
        ],
    );
}

#[test]
fn ne_with_string_literal_emits_not_eq() {
    sim_assert_eq!(
        have: parse_condition(r#"ne .Values.X "value""#),
        want: vec![Guard::NotEq {
            path: "X".into(),
            value: GuardValue::string("value"),
        }],
    );
}

#[test]
fn not_eq_literal_projects_to_not_eq() {
    sim_assert_eq!(
        have: parse_condition(r#"not (eq .Values.mode "disabled")"#),
        want: vec![Guard::NotEq {
            path: "mode".into(),
            value: GuardValue::string("disabled"),
        }],
    );
}

#[test]
fn not_ne_literal_projects_to_eq() {
    sim_assert_eq!(
        have: parse_condition(r#"not (ne .Values.mode "disabled")"#),
        want: vec![Guard::Eq {
            path: "mode".into(),
            value: GuardValue::string("disabled"),
        }],
    );
}

#[test]
fn and_falls_through_to_per_path_truthy() {
    sim_assert_eq!(
        have: parse_condition("and .Values.A .Values.B"),
        want: vec![
            Guard::Truthy { path: "A".into() },
            Guard::Truthy { path: "B".into() },
        ],
    );
}

#[test]
fn and_with_parens_falls_through_to_per_path_truthy() {
    sim_assert_eq!(
        have: parse_condition("and (.Values.A) (.Values.B)"),
        want: vec![
            Guard::Truthy { path: "A".into() },
            Guard::Truthy { path: "B".into() },
        ],
    );
}

#[test]
fn and_preserves_nested_not_guard() {
    sim_assert_eq!(
        have: parse_condition(
            "and .Values.prometheus.enabled (not .Values.prometheus.podmonitor.enabled)"
        ),
        want: vec![
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
    sim_assert_eq!(
        have: parse_condition(
            "and .Values.ldap.enabled (or .Values.ldap.bind_password .Values.ldap.bindpw)"
        ),
        want: vec![
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
    sim_assert_eq!(
        have: parse_condition("empty .Values.service.loadBalancerIP"),
        want: vec![Guard::Not {
            path: "service.loadBalancerIP".into()
        }],
    );
}

#[test]
fn not_empty_path_is_truthy_guard() {
    sim_assert_eq!(
        have: parse_condition("not (empty .Values.service.loadBalancerIP)"),
        want: vec![Guard::Truthy {
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
    sim_assert_eq!(
        have: parse_condition(r#"eq .Values.X "match.*foo""#),
        want: vec![Guard::Eq {
            path: "X".into(),
            value: GuardValue::string("match.*foo"),
        }],
    );
}

#[test]
fn eq_value_preserves_dot_values_substring_inside_string() {
    sim_assert_eq!(
        have: parse_condition(r#"eq .Values.X ".Values.fake""#),
        want: vec![Guard::Eq {
            path: "X".into(),
            value: GuardValue::string(".Values.fake"),
        }],
    );
}
