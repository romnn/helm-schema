use crate::{Literal, TemplateExpr};

use crate::{is_provenance_preserving_function, is_string_transform_function};

pub fn literal_schema_type(expr: &TemplateExpr) -> Option<&'static str> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(_) | Literal::RawString(_)) => Some("string"),
        TemplateExpr::Literal(Literal::Int(_)) => Some("integer"),
        TemplateExpr::Literal(Literal::Float(_)) => Some("number"),
        TemplateExpr::Literal(Literal::Bool(_)) => Some("boolean"),
        TemplateExpr::Literal(Literal::Nil)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Selector { .. }
        | TemplateExpr::Call { .. }
        | TemplateExpr::Pipeline(_)
        | TemplateExpr::Parenthesized(_)
        | TemplateExpr::Unknown(_)
        | TemplateExpr::VariableDefinition { .. }
        | TemplateExpr::Assignment { .. } => None,
    }
}

pub fn expression_schema_type(expr: &TemplateExpr) -> Option<&'static str> {
    match expr.deparen() {
        TemplateExpr::Literal(_) => literal_schema_type(expr),
        TemplateExpr::Parenthesized(inner) => expression_schema_type(inner),
        TemplateExpr::Call { function, args } => match function.as_str() {
            "include" | "template" | "printf" => Some("string"),
            "default" if args.len() == 2 => expression_schema_type(&args[0]),
            function if is_string_transform_function(function) => Some("string"),
            function if is_provenance_preserving_function(function) => {
                args.first().and_then(expression_schema_type)
            }
            _ => None,
        },
        TemplateExpr::Pipeline(stages) => expression_schema_type_for_pipeline(stages),
        TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Selector { .. }
        | TemplateExpr::Unknown(_)
        | TemplateExpr::VariableDefinition { .. }
        | TemplateExpr::Assignment { .. } => None,
    }
}

fn expression_schema_type_for_pipeline(stages: &[TemplateExpr]) -> Option<&'static str> {
    let mut current = stages.first().and_then(expression_schema_type);
    for stage in &stages[1..] {
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
