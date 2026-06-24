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
    let (head, tail) = segments.split_first()?;
    if head != "Values" || tail.is_empty() {
        return None;
    }
    Some(tail.join("."))
}

#[cfg(test)]
#[path = "tests/value_path_extraction.rs"]
mod tests;
