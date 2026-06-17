use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;

use crate::Guard;
use crate::value_path_extraction::collect_loose_values_paths;

#[must_use]
pub(crate) fn parse_condition_expr(top: &TemplateExpr) -> Vec<Guard> {
    let mut paths = BTreeSet::new();
    collect_loose_values_paths(top, &mut paths);

    if let TemplateExpr::Call { function, args } = top {
        match function.as_str() {
            "not" => {
                if let Some(path) = single(&paths) {
                    return vec![Guard::Not { path }];
                }
            }
            "or" if paths.len() >= 2 => {
                return vec![Guard::Or {
                    paths: paths.into_iter().collect(),
                }];
            }
            "eq" => {
                if let Some(path) = single(&paths)
                    && let Some(value) = first_string_literal(args)
                {
                    return vec![Guard::Eq { path, value }];
                }
            }
            "ne" => {
                if let Some(path) = single(&paths) {
                    return vec![Guard::Truthy { path }];
                }
            }
            "typeIs" => {
                if let Some(path) = single(&paths)
                    && let Some(schema_type) = type_is_schema_type(args.first())
                {
                    return vec![Guard::TypeIs { path, schema_type }];
                }
            }
            _ => {}
        }
    }

    paths
        .into_iter()
        .map(|path| Guard::Truthy { path })
        .collect()
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

fn first_string_literal(exprs: &[TemplateExpr]) -> Option<String> {
    let mut found = None;
    for expr in exprs {
        if found.is_some() {
            break;
        }
        expr.walk(|node| {
            if found.is_some() {
                return;
            }
            if let TemplateExpr::Literal(lit) = node
                && let Some(value) = lit.as_string()
            {
                found = Some(value.to_string());
            }
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::parse_condition_expr;
    use crate::Guard;

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
    fn eq_with_string_literal() {
        assert_eq!(
            parse_condition(r#"eq .Values.X "value""#),
            vec![Guard::Eq {
                path: "X".into(),
                value: "value".into(),
            }],
        );
    }

    #[test]
    fn eq_with_string_literal_containing_phantom_path() {
        assert_eq!(
            parse_condition(r#"eq .Values.X ".Values.fake""#),
            vec![Guard::Eq {
                path: "X".into(),
                value: ".Values.fake".into(),
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
    fn ne_with_string_literal_emits_truthy() {
        assert_eq!(
            parse_condition(r#"ne .Values.X "value""#),
            vec![Guard::Truthy { path: "X".into() }],
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
                value: "match.*foo".into(),
            }],
        );
    }

    #[test]
    fn eq_value_preserves_dot_values_substring_inside_string() {
        assert_eq!(
            parse_condition(r#"eq .Values.X ".Values.fake""#),
            vec![Guard::Eq {
                path: "X".into(),
                value: ".Values.fake".into(),
            }],
        );
    }
}
