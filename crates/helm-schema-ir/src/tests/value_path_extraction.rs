use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};
use test_util::prelude::sim_assert_eq;

use super::collect_loose_values_paths;

const WILDCARD_PLACEHOLDER: &str = "__hsast_wildcard_marker__";

fn extract_values_paths(text: &str) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for top in parse_bare_expression_text(text) {
        collect_loose_values_paths(&top, &mut paths);
    }
    paths.into_iter().collect()
}

fn parse_bare_expression_text(text: &str) -> Vec<TemplateExpr> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let has_wildcards = trimmed.contains(".*");
    let normalised = if has_wildcards {
        trimmed.replace(".*", &format!(".{WILDCARD_PLACEHOLDER}"))
    } else {
        trimmed.to_string()
    };
    let wrapped = if normalised.trim_start().starts_with("{{") {
        normalised
    } else {
        format!("{{{{ {normalised} }}}}")
    };
    let mut exprs = helm_schema_ast::parse_action_expressions(&wrapped);
    if has_wildcards {
        for expr in &mut exprs {
            restore_wildcards_in_expr(expr);
        }
    }
    exprs
}

fn restore_wildcards_in_expr(expr: &mut TemplateExpr) {
    match expr {
        TemplateExpr::Field(path) => restore_segments(path),
        TemplateExpr::Selector { operand, path } => {
            restore_segments(path);
            restore_wildcards_in_expr(operand);
        }
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
            if value.contains(WILDCARD_PLACEHOLDER) {
                *value = value.replace(WILDCARD_PLACEHOLDER, "*");
            }
        }
        TemplateExpr::Call { args, .. } => {
            for arg in args {
                restore_wildcards_in_expr(arg);
            }
        }
        TemplateExpr::Pipeline(stages) => {
            for stage in stages {
                restore_wildcards_in_expr(stage);
            }
        }
        TemplateExpr::Parenthesized(inner) => restore_wildcards_in_expr(inner),
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            restore_wildcards_in_expr(value)
        }
        TemplateExpr::Literal(_) | TemplateExpr::Variable(_) | TemplateExpr::Unknown(_) => {}
    }
}

fn restore_segments(segments: &mut [String]) {
    for segment in segments {
        if segment == WILDCARD_PLACEHOLDER {
            "*".clone_into(segment);
        }
    }
}

#[test]
fn root_chain_extracted() {
    sim_assert_eq!(
        have: extract_values_paths(".Values.foo.bar"),
        want: vec!["foo.bar".to_string()]
    );
}

#[test]
fn quoted_payload_does_not_create_phantom_path() {
    let text = r#"eq .Values.X ".Values.fake""#;
    sim_assert_eq!(have: extract_values_paths(text), want: vec!["X".to_string()]);
}

#[test]
fn rooted_dollar_values_path() {
    sim_assert_eq!(have: extract_values_paths("$.Values.X"), want: vec!["X".to_string()]);
}

#[test]
fn rooted_named_variable_values_path() {
    sim_assert_eq!(
        have: extract_values_paths("$root.Values.Y"),
        want: vec!["Y".to_string()]
    );
}

#[test]
fn nested_selector_chain_keeps_full_values_descendant_path() {
    sim_assert_eq!(
        have: extract_values_paths("((.Values.appVersions).airtype).global"),
        want: vec!["appVersions.airtype.global".to_string()]
    );
}

#[test]
fn embedded_values_in_helper_context_chain() {
    sim_assert_eq!(
        have: extract_values_paths(".context.Values.X"),
        want: vec!["X".to_string()],
    );
}

#[test]
fn multiple_refs_are_sorted_and_deduped() {
    let text = ".Values.b .Values.a .Values.b";
    sim_assert_eq!(
        have: extract_values_paths(text),
        want: vec!["a".to_string(), "b".to_string()],
    );
}

#[test]
fn wildcard_segment_in_rewritten_path() {
    sim_assert_eq!(
        have: extract_values_paths(".Values.someList.*.name"),
        want: vec!["someList.*.name".to_string()],
    );
}

#[test]
fn empty_text_returns_empty() {
    assert!(extract_values_paths("").is_empty());
    assert!(extract_values_paths("   \n  ").is_empty());
}

#[test]
fn no_values_reference_returns_empty() {
    assert!(extract_values_paths(".Chart.Name").is_empty());
    assert!(extract_values_paths("printf \"%s\" $var").is_empty());
}

#[test]
fn dot_star_inside_string_literal_does_not_emit_phantom_wildcard_path() {
    sim_assert_eq!(
        have: extract_values_paths(r#"eq .Values.X "pattern.*foo""#),
        want: vec!["X".to_string()],
    );
}

#[test]
fn dot_values_substring_inside_string_does_not_emit_phantom() {
    sim_assert_eq!(
        have: extract_values_paths(r#"eq .Values.X ".Values.fake""#),
        want: vec!["X".to_string()],
    );
}
