use crate::{Literal, TemplateExpr};

use crate::{is_provenance_preserving_function, is_string_transform_function};

/// Infers the exact JSON Schema type produced by a literal or known total transform.
pub fn expression_schema_type(expr: &TemplateExpr) -> Option<&'static str> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(_) | Literal::RawString(_)) => Some("string"),
        TemplateExpr::Literal(Literal::Int(_)) => Some("integer"),
        TemplateExpr::Literal(Literal::Float(_)) => Some("number"),
        TemplateExpr::Literal(Literal::Bool(_)) => Some("boolean"),
        TemplateExpr::Call { function, args } => match function.as_str() {
            "include" | "template" | "printf" => Some("string"),
            "default" if args.len() == 2 => args.first().and_then(expression_schema_type),
            function if is_string_transform_function(function) => Some("string"),
            function if is_provenance_preserving_function(function) => {
                args.first().and_then(expression_schema_type)
            }
            _ => None,
        },
        TemplateExpr::Pipeline(stages) => expression_schema_type_for_pipeline(stages),
        TemplateExpr::Literal(Literal::Nil)
        | TemplateExpr::Parenthesized(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Selector { .. }
        | TemplateExpr::Unknown(_)
        | TemplateExpr::VariableDefinition { .. }
        | TemplateExpr::Assignment { .. } => None,
    }
}

fn expression_schema_type_for_pipeline(stages: &[TemplateExpr]) -> Option<&'static str> {
    let mut current = stages.first().and_then(expression_schema_type);
    for stage in stages.iter().skip(1) {
        let TemplateExpr::Call { function, args } = stage.deparen() else {
            current = expression_schema_type(stage);
            continue;
        };
        current = match function.as_str() {
            "default" => args.first().and_then(expression_schema_type).or(current),
            function if is_string_transform_function(function) => Some("string"),
            function if is_provenance_preserving_function(function) => current,
            _ => None,
        };
    }
    current
}
