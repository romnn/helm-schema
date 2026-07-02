use std::collections::HashMap;
use test_util::prelude::sim_assert_eq;

use super::{
    BoundValueContext, GetBinding, GetBindingPlan, parse_get_binding_from_exprs,
    parse_literal_list_range_expr,
};
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_exprs_effects;
use crate::fragment_assignment::AssignmentKind;
use helm_schema_ast::parse_expr_text;

fn parse_literal_list_range(header: &str) -> Option<(String, Vec<String>)> {
    let header = header.trim();
    let exprs = parse_expr_text(header);
    let [expr] = exprs.as_slice() else {
        return None;
    };
    parse_literal_list_range_expr(expr)
}

fn parse_get_binding(text: &str) -> Option<GetBindingPlan> {
    parse_get_binding_from_exprs(&parse_expr_text(text))
}

fn extract_bound_values(
    text: &str,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
) -> Vec<String> {
    let env = EvalEnv {
        bound_values: BoundValueContext::new(range_domains, get_bindings),
        ..EvalEnv::default()
    };
    eval_exprs_effects(&parse_expr_text(text), &env)
        .bound_output_paths
        .into_iter()
        .collect()
}

#[test]
fn parse_get_binding_detects_declaration_from_ast() {
    sim_assert_eq!(
        have: parse_get_binding(r#"{{- $value := get $.Values.config $key -}}"#),
        want: Some(GetBindingPlan {
            variable: "value".to_string(),
            kind: AssignmentKind::Declaration,
            binding: GetBinding {
                base: "config".to_string(),
                key_var: "key".to_string(),
            },
        })
    );
}

#[test]
fn parse_get_binding_detects_assignment_from_ast() {
    sim_assert_eq!(
        have: parse_get_binding(r#"{{- $value = get .Values.config $key -}}"#),
        want: Some(GetBindingPlan {
            variable: "value".to_string(),
            kind: AssignmentKind::Assignment,
            binding: GetBinding {
                base: "config".to_string(),
                key_var: "key".to_string(),
            },
        })
    );
}

#[test]
fn parse_literal_list_range_detects_variable_definition_from_ast() {
    sim_assert_eq!(
        have: parse_literal_list_range(r#"$scope := list "frontend" "backend""#),
        want: Some((
            "scope".to_string(),
            vec!["frontend".to_string(), "backend".to_string()]
        ))
    );
}

#[test]
fn extract_bound_values_resolves_selector_reads_from_ast() {
    let mut range_domains = HashMap::new();
    range_domains.insert(
        "scope".to_string(),
        vec!["frontend".to_string(), "backend".to_string()],
    );
    let mut get_bindings = HashMap::new();
    get_bindings.insert(
        "config".to_string(),
        GetBinding {
            base: "config".to_string(),
            key_var: "scope".to_string(),
        },
    );

    sim_assert_eq!(
        have: extract_bound_values(
            r#"{{- printf "%s" $config.port -}}"#,
            &range_domains,
            &get_bindings
        ),
        want: vec![
            "config.backend.port".to_string(),
            "config.frontend.port".to_string()
        ]
    );
}

#[test]
fn extract_bound_values_respects_or_short_circuit_eq_predicate() {
    let mut range_domains = HashMap::new();
    range_domains.insert(
        "protocol".to_string(),
        vec!["nats".to_string(), "websocket".to_string()],
    );
    let mut get_bindings = HashMap::new();
    get_bindings.insert(
        "config".to_string(),
        GetBinding {
            base: "config".to_string(),
            key_var: "protocol".to_string(),
        },
    );

    sim_assert_eq!(
        have: extract_bound_values(
            r#"or (eq $protocol "nats") $config.enabled"#,
            &range_domains,
            &get_bindings
        ),
        want: vec!["config.websocket.enabled".to_string()]
    );
}

#[test]
fn extract_bound_values_respects_and_short_circuit_eq_predicate() {
    let mut range_domains = HashMap::new();
    range_domains.insert(
        "protocol".to_string(),
        vec!["nats".to_string(), "websocket".to_string()],
    );
    let mut get_bindings = HashMap::new();
    get_bindings.insert(
        "config".to_string(),
        GetBinding {
            base: "config".to_string(),
            key_var: "protocol".to_string(),
        },
    );

    sim_assert_eq!(
        have: extract_bound_values(
            r#"and (eq $protocol "nats") $config.enabled"#,
            &range_domains,
            &get_bindings
        ),
        want: vec!["config.nats.enabled".to_string()]
    );
}
