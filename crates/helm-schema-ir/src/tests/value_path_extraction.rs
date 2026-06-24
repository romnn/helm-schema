use helm_schema_ast::TemplateExpr;
use test_util::prelude::sim_assert_eq;

use super::values_path_from_expr;

fn parse_expr(text: &str) -> Option<TemplateExpr> {
    let wrapped = format!("{{{{ {text} }}}}");
    helm_schema_ast::parse_action_expressions(&wrapped)
        .into_iter()
        .next()
}

fn values_path(text: &str) -> Option<String> {
    parse_expr(text).as_ref().and_then(values_path_from_expr)
}

#[test]
fn root_chain_extracted() {
    sim_assert_eq!(
        have: values_path(".Values.foo.bar"),
        want: Some("foo.bar".to_string())
    );
}

#[test]
fn rooted_dollar_values_path() {
    sim_assert_eq!(
        have: values_path("$.Values.X"),
        want: Some("X".to_string())
    );
}

#[test]
fn rooted_named_variable_values_path() {
    sim_assert_eq!(
        have: values_path("$root.Values.Y"),
        want: Some("Y".to_string())
    );
}

#[test]
fn nested_selector_chain_keeps_full_values_descendant_path() {
    sim_assert_eq!(
        have: values_path("((.Values.appVersions).airtype).global"),
        want: Some("appVersions.airtype.global".to_string())
    );
}

#[test]
fn helper_context_values_segment_is_not_a_rooted_path() {
    sim_assert_eq!(have: values_path(".context.Values.X"), want: None);
}

#[test]
fn function_call_with_values_reference_is_not_a_single_root_path() {
    sim_assert_eq!(have: values_path(r#"eq .Values.X ".Values.fake""#), want: None);
}

#[test]
fn string_literal_dot_values_payload_is_not_a_root_path() {
    sim_assert_eq!(have: values_path(r#"" .Values.fake ""#), want: None);
}
