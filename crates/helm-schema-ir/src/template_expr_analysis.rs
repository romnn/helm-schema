use helm_schema_ast::TemplateExpr;

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

pub(crate) fn exprs_start_with_helper_call(exprs: &[TemplateExpr]) -> bool {
    matches!(exprs, [expr] if expr_starts_with_helper_call(expr))
}

pub(crate) fn exprs_pipeline_merges_into_var(exprs: &[TemplateExpr], var: &str) -> bool {
    let [TemplateExpr::Pipeline(stages)] = exprs else {
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
