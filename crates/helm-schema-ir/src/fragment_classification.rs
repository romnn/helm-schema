use helm_schema_ast::TemplateExpr;

use crate::template_expr_cache::parse_expr_text;

/// Return true when a Helm expression structurally calls a function that emits
/// a YAML fragment rather than a single scalar.
#[must_use]
pub fn is_fragment_expr(text: &str) -> bool {
    parse_expr_text(text)
        .iter()
        .any(expr_contains_fragment_function)
}

fn expr_contains_fragment_function(expr: &TemplateExpr) -> bool {
    let mut found = false;
    expr.walk(|node| {
        if found {
            return;
        }
        if let TemplateExpr::Call { function, .. } = node
            && matches!(function.as_str(), "toYaml" | "nindent" | "indent" | "tpl")
        {
            found = true;
        }
    });
    found
}

#[cfg(test)]
mod tests {
    use super::is_fragment_expr;

    #[test]
    fn detects_fragment_functions_in_bare_and_wrapped_actions() {
        assert!(is_fragment_expr(".Values.labels | toYaml | nindent 4"));
        assert!(is_fragment_expr("{{ include \"labels\" . | nindent 4 }}"));
        assert!(is_fragment_expr("tpl .Values.extra ."));
    }

    #[test]
    fn include_without_fragment_transform_is_scalar() {
        assert!(!is_fragment_expr("include \"name\" ."));
        assert!(!is_fragment_expr("template \"name\" ."));
    }

    #[test]
    fn string_literals_do_not_create_fragment_false_positives() {
        assert!(!is_fragment_expr(r#""toYaml" | quote"#));
        assert!(!is_fragment_expr(r#"printf "%s" "nindent""#));
    }
}
