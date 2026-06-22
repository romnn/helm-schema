use std::collections::BTreeSet;

/// If `expr` is a `.Values.X.Y...` reference rooted at the current context or
/// a root variable, return the dotted path with the leading `Values.` stripped.
pub(crate) fn values_path_from_expr(expr: &helm_schema_ast::TemplateExpr) -> Option<String> {
    values_path_from_expr_with(expr, ValuesPathMode::Rooted)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValuesPathMode {
    Rooted,
    AnyValuesSegment,
}

fn values_path_from_expr_with(
    expr: &helm_schema_ast::TemplateExpr,
    mode: ValuesPathMode,
) -> Option<String> {
    use helm_schema_ast::TemplateExpr as E;

    let expr = expr.deparen();
    match expr {
        E::Field(path) => values_path_from_segments(path, mode),
        E::Selector { operand, path } if mode == ValuesPathMode::Rooted => {
            if let Some(base) = values_path_from_expr_with(operand, mode) {
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
            values_path_from_segments(path, mode)
        }
        E::Selector { path, .. } => {
            values_path_from_segments(path, ValuesPathMode::AnyValuesSegment)
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

fn values_path_from_segments(segments: &[String], mode: ValuesPathMode) -> Option<String> {
    let tail = match mode {
        ValuesPathMode::Rooted => {
            let (head, tail) = segments.split_first()?;
            (head == "Values").then_some(tail)?
        }
        ValuesPathMode::AnyValuesSegment => {
            let values_index = segments.iter().position(|segment| segment == "Values")?;
            &segments[values_index + 1..]
        }
    };
    if tail.is_empty() {
        return None;
    }
    if mode == ValuesPathMode::AnyValuesSegment && tail.first()?.as_str() == "*" {
        return None;
    }
    Some(tail.join("."))
}

pub(crate) fn collect_loose_values_paths(
    expr: &helm_schema_ast::TemplateExpr,
    out: &mut BTreeSet<String>,
) {
    expr.walk(|node| {
        if let Some(path) = values_path_from_expr_with(node, ValuesPathMode::AnyValuesSegment) {
            out.insert(path);
        }
    });
}

#[cfg(test)]
#[path = "tests/value_path_extraction.rs"]
mod tests;
