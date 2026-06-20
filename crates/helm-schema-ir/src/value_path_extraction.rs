use std::collections::BTreeSet;

/// If `expr` is a `.Values.X.Y...` reference rooted at the current context or
/// a root variable, return the dotted path with the leading `Values.` stripped.
pub(crate) fn values_path_from_expr(expr: &helm_schema_ast::TemplateExpr) -> Option<String> {
    use helm_schema_ast::TemplateExpr as E;

    let expr = expr.deparen();
    match expr {
        E::Field(path) => values_path_from_segments(path),
        E::Selector { operand, path } => {
            if let Some(base) = values_path_from_expr(operand) {
                let suffix = path.join(".");
                return Some(if suffix.is_empty() {
                    base
                } else if base.is_empty() {
                    suffix
                } else {
                    format!("{base}.{suffix}")
                });
            }
            if !matches!(operand.as_ref(), E::Variable(_)) {
                return None;
            }
            values_path_from_segments(path)
        }
        E::Literal(_)
        | E::Variable(_)
        | E::Call { .. }
        | E::Pipeline(_)
        | E::Parenthesized(_)
        | E::VariableDefinition { .. }
        | E::Assignment { .. }
        | E::Unknown(_) => None,
    }
}

fn values_path_from_segments(segments: &[String]) -> Option<String> {
    let mut iter = segments.iter();
    let head = iter.next()?;
    if head != "Values" {
        return None;
    }
    let tail: Vec<String> = iter.cloned().collect();
    if tail.is_empty() {
        return None;
    }
    Some(tail.join("."))
}

/// Extract `.Values.foo.bar` references from a condition/expression string.
#[must_use]
#[cfg(test)]
pub fn extract_values_paths(text: &str) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for top in parse_bare_expression_text(text) {
        collect_loose_values_paths(&top, &mut paths);
    }
    paths.into_iter().collect()
}

pub(crate) fn collect_loose_values_paths(
    expr: &helm_schema_ast::TemplateExpr,
    out: &mut BTreeSet<String>,
) {
    expr.walk(|node| {
        if let Some(path) = values_path_from_expr_loose(node) {
            out.insert(path);
        }
    });
}

/// Parse `text` as a bare Go template expression and return every top-level
/// expression the wrapped action produces.
#[cfg(test)]
pub(crate) fn parse_bare_expression_text(text: &str) -> Vec<helm_schema_ast::TemplateExpr> {
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

#[cfg(test)]
const WILDCARD_PLACEHOLDER: &str = "__hsast_wildcard_marker__";

#[cfg(test)]
fn restore_wildcards_in_expr(expr: &mut helm_schema_ast::TemplateExpr) {
    use helm_schema_ast::{Literal, TemplateExpr};

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

#[cfg(test)]
fn restore_segments(segments: &mut [String]) {
    for segment in segments {
        if segment == WILDCARD_PLACEHOLDER {
            "*".clone_into(segment);
        }
    }
}

fn values_path_from_expr_loose(expr: &helm_schema_ast::TemplateExpr) -> Option<String> {
    use helm_schema_ast::TemplateExpr as E;

    let expr = expr.deparen();
    let segments: &[String] = match expr {
        E::Field(path) | E::Selector { path, .. } => path,
        _ => return None,
    };
    let values_index = segments.iter().position(|segment| segment == "Values")?;
    let tail = &segments[values_index + 1..];
    if tail.first()?.as_str() == "*" {
        return None;
    }
    Some(tail.join("."))
}

#[cfg(test)]
mod tests {
    use super::extract_values_paths;
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn root_chain_extracted() {
        sim_assert_eq!(
            extract_values_paths(".Values.foo.bar"),
            vec!["foo.bar".to_string()]
        );
    }

    #[test]
    fn quoted_payload_does_not_create_phantom_path() {
        let text = r#"eq .Values.X ".Values.fake""#;
        sim_assert_eq!(extract_values_paths(text), vec!["X".to_string()]);
    }

    #[test]
    fn rooted_dollar_values_path() {
        sim_assert_eq!(extract_values_paths("$.Values.X"), vec!["X".to_string()]);
    }

    #[test]
    fn rooted_named_variable_values_path() {
        sim_assert_eq!(
            extract_values_paths("$root.Values.Y"),
            vec!["Y".to_string()]
        );
    }

    #[test]
    fn nested_selector_chain_keeps_full_values_descendant_path() {
        sim_assert_eq!(
            extract_values_paths("((.Values.appVersions).airtype).global"),
            vec!["appVersions.airtype.global".to_string()]
        );
    }

    #[test]
    fn embedded_values_in_helper_context_chain() {
        sim_assert_eq!(
            extract_values_paths(".context.Values.X"),
            vec!["X".to_string()],
        );
    }

    #[test]
    fn multiple_refs_are_sorted_and_deduped() {
        let text = ".Values.b .Values.a .Values.b";
        sim_assert_eq!(
            extract_values_paths(text),
            vec!["a".to_string(), "b".to_string()],
        );
    }

    #[test]
    fn wildcard_segment_in_rewritten_path() {
        sim_assert_eq!(
            extract_values_paths(".Values.someList.*.name"),
            vec!["someList.*.name".to_string()],
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
            extract_values_paths(r#"eq .Values.X "pattern.*foo""#),
            vec!["X".to_string()],
        );
    }

    #[test]
    fn dot_values_substring_inside_string_does_not_emit_phantom() {
        sim_assert_eq!(
            extract_values_paths(r#"eq .Values.X ".Values.fake""#),
            vec!["X".to_string()],
        );
    }
}
