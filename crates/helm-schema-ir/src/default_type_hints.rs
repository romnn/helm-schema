use serde_json::{Map, Value};

use crate::template_comment_filter::strip_yaml_comment_lines;
use crate::value_path_extraction::values_path_from_expr;

#[derive(Debug, Clone, PartialEq, Eq)]
enum DefaultLiteralType {
    String,
    Integer,
    Number,
    Boolean,
}

impl DefaultLiteralType {
    fn schema(&self) -> Value {
        let ty = match self {
            DefaultLiteralType::String => "string",
            DefaultLiteralType::Integer => "integer",
            DefaultLiteralType::Number => "number",
            DefaultLiteralType::Boolean => "boolean",
        };
        let mut map = Map::new();
        map.insert("type".to_string(), Value::String(ty.to_string()));
        Value::Object(map)
    }
}

/// Extract type hints implied by `default <literal> .Values.X` and
/// `.Values.X | default <literal>` patterns in template text.
#[must_use]
pub fn extract_default_type_hints(text: &str) -> Vec<(String, Value)> {
    use helm_schema_ast::{TemplateExpr, parse_action_expressions};

    let cleaned = strip_yaml_comment_lines(text);
    let mut out = Vec::new();
    for top in parse_action_expressions(&cleaned) {
        top.walk(|expr| match expr {
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                let TemplateExpr::Literal(lit) = args[0].deparen() else {
                    return;
                };
                let Some(ty) = classify_literal_type(lit) else {
                    return;
                };
                let Some(path) = values_path_from_expr(&args[1]) else {
                    return;
                };
                out.push((path, ty.schema()));
            }
            TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
                for window in stages.windows(2) {
                    let Some(path) = values_path_from_expr(&window[0]) else {
                        continue;
                    };
                    let TemplateExpr::Call { function, args } = window[1].deparen() else {
                        continue;
                    };
                    if function != "default" || args.len() != 1 {
                        continue;
                    }
                    let TemplateExpr::Literal(lit) = args[0].deparen() else {
                        continue;
                    };
                    let Some(ty) = classify_literal_type(lit) else {
                        continue;
                    };
                    out.push((path, ty.schema()));
                }
            }
            _ => {}
        });
    }
    out
}

fn classify_literal_type(lit: &helm_schema_ast::Literal) -> Option<DefaultLiteralType> {
    match lit {
        helm_schema_ast::Literal::String(_) | helm_schema_ast::Literal::RawString(_) => {
            Some(DefaultLiteralType::String)
        }
        helm_schema_ast::Literal::Int(_) => Some(DefaultLiteralType::Integer),
        helm_schema_ast::Literal::Float(_) => Some(DefaultLiteralType::Number),
        helm_schema_ast::Literal::Bool(_) => Some(DefaultLiteralType::Boolean),
        helm_schema_ast::Literal::Nil => None,
    }
}

#[cfg(test)]
mod tests {
    use super::extract_default_type_hints;
    use serde_json::json;

    fn hints(src: &str) -> Vec<(String, serde_json::Value)> {
        extract_default_type_hints(src)
    }

    #[test]
    fn prefix_literal_emits_typed_hint() {
        assert_eq!(
            hints(r#"{{ default 5 .Values.replicas }}"#),
            vec![("replicas".to_string(), json!({"type": "integer"}))],
        );
    }

    #[test]
    fn pipeline_literal_emits_typed_hint() {
        assert_eq!(
            hints(r#"{{ .Values.replicas | default 5 }}"#),
            vec![("replicas".to_string(), json!({"type": "integer"}))],
        );
    }

    #[test]
    fn nested_default_inner_emits_hint_outer_does_not() {
        assert_eq!(
            hints(r#"{{ default 5 (default "x" .Values.X) }}"#),
            vec![("X".to_string(), json!({"type": "string"}))],
        );
    }

    #[test]
    fn chained_defaults_emit_one_hint_for_innermost_path() {
        assert_eq!(
            hints(r#"{{ .Values.X | default 5 | default 10 }}"#),
            vec![("X".to_string(), json!({"type": "integer"}))],
        );
    }

    #[test]
    fn intervening_call_breaks_pipeline_pattern() {
        assert!(hints(r#"{{ .Values.X | required "msg" | default 5 }}"#).is_empty(),);
    }

    #[test]
    fn rooted_dollar_dotvalues_path_is_recognised() {
        assert_eq!(
            hints(r#"{{ default 5 $.Values.X }}"#),
            vec![("X".to_string(), json!({"type": "integer"}))],
        );
    }

    #[test]
    fn rooted_named_variable_dotvalues_path_is_recognised() {
        assert_eq!(
            hints(r#"{{ default 5 $root.Values.X }}"#),
            vec![("X".to_string(), json!({"type": "integer"}))],
        );
    }

    #[test]
    fn default_with_non_values_target_no_hint() {
        assert!(hints(r#"{{ default 5 .NotValues.X }}"#).is_empty());
    }

    #[test]
    fn default_with_dot_only_no_hint() {
        assert!(hints(r#"{{ default 5 . }}"#).is_empty());
    }

    #[test]
    fn default_with_parenthesised_first_arg_no_hint() {
        assert!(hints(r#"{{ default (printf "%s" .Y) .Values.X }}"#).is_empty());
    }

    #[test]
    fn bool_literal_classified_as_boolean() {
        assert_eq!(
            hints(r#"{{ default true .Values.enabled }}"#),
            vec![("enabled".to_string(), json!({"type": "boolean"}))],
        );
    }

    #[test]
    fn nil_literal_emits_no_hint() {
        assert!(hints(r#"{{ default nil .Values.X }}"#).is_empty());
    }
}
