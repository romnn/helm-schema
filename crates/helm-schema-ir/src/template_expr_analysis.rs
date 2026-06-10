use helm_schema_ast::TemplateExpr;

use crate::template_expr_cache::parse_expr_text;

pub(crate) fn expr_contains_helper_call(expr: &TemplateExpr) -> bool {
    let mut found = false;
    expr.walk(|node| {
        if let TemplateExpr::Call { function, .. } = node
            && matches!(function.as_str(), "include" | "template")
        {
            found = true;
        }
    });
    found
}

pub(crate) fn expr_starts_with_helper_call(expr: &TemplateExpr) -> bool {
    match expr {
        TemplateExpr::Parenthesized(inner) => expr_starts_with_helper_call(inner),
        TemplateExpr::Call { function, .. } => matches!(function.as_str(), "include" | "template"),
        TemplateExpr::Pipeline(stages) => stages.first().is_some_and(expr_starts_with_helper_call),
        _ => false,
    }
}

pub(crate) fn text_starts_with_helper_call(text: &str) -> bool {
    let exprs = parse_expr_text(text);
    matches!(exprs.as_slice(), [expr] if expr_starts_with_helper_call(expr))
}

pub(crate) fn text_pipeline_merges_into_var(text: &str, var: &str) -> bool {
    let exprs = parse_expr_text(text);
    let [TemplateExpr::Pipeline(stages)] = exprs.as_slice() else {
        return false;
    };
    stages.iter().skip(1).any(|stage| {
        let TemplateExpr::Call { function, args } = stage else {
            return false;
        };
        is_merge_function(function)
            && args
                .iter()
                .any(|arg| matches!(arg, TemplateExpr::Variable(name) if name == var))
    })
}

pub(crate) fn is_merge_function(function: &str) -> bool {
    matches!(
        function,
        "merge" | "mustMerge" | "mergeOverwrite" | "mustMergeOverwrite"
    )
}

pub(crate) fn walk_expr_excluding_helper_call_args<F>(expr: &TemplateExpr, visit: &mut F)
where
    F: FnMut(&TemplateExpr),
{
    visit(expr);
    match expr {
        TemplateExpr::Call { function, args } => {
            if matches!(function.as_str(), "include" | "template") {
                return;
            }
            for arg in args {
                walk_expr_excluding_helper_call_args(arg, visit);
            }
        }
        TemplateExpr::Selector { operand, .. }
        | TemplateExpr::VariableDefinition { value: operand, .. }
        | TemplateExpr::Assignment { value: operand, .. } => {
            walk_expr_excluding_helper_call_args(operand, visit);
        }
        TemplateExpr::Pipeline(stages) => {
            for stage in stages {
                walk_expr_excluding_helper_call_args(stage, visit);
            }
        }
        TemplateExpr::Parenthesized(inner) => {
            walk_expr_excluding_helper_call_args(inner, visit);
        }
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => {}
    }
}
