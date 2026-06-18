use helm_schema_ast::{Literal, TemplateExpr};

#[cfg(test)]
use crate::template_expr_cache::parse_expr_text;

pub(super) fn fragment_indent_width_from_exprs(exprs: &[TemplateExpr]) -> Option<usize> {
    exprs.iter().rev().find_map(call_indent_width)
}

fn call_indent_width(expr: &TemplateExpr) -> Option<usize> {
    match expr {
        TemplateExpr::Call { function, args }
            if matches!(function.as_str(), "indent" | "nindent") =>
        {
            match args.first() {
                Some(TemplateExpr::Literal(Literal::Int(width))) => usize::try_from(*width).ok(),
                Some(TemplateExpr::Parenthesized(inner)) => call_indent_width(inner),
                _ => None,
            }
        }
        TemplateExpr::Parenthesized(inner) => call_indent_width(inner),
        TemplateExpr::Pipeline(stages) => stages.iter().rev().find_map(call_indent_width),
        _ => None,
    }
}

#[cfg(test)]
pub(super) fn fragment_indent_width(text: &str) -> Option<usize> {
    fragment_indent_width_from_exprs(&parse_expr_text(text))
}
