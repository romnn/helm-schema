use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;

pub(crate) fn encoded_output_paths_from_exprs<F>(
    exprs: &[TemplateExpr],
    mut resolve_paths: F,
) -> BTreeSet<String>
where
    F: FnMut(&TemplateExpr) -> BTreeSet<String>,
{
    let mut out = BTreeSet::new();
    for expr in exprs {
        append_encoded_output_paths(expr, &mut resolve_paths, &mut out);
    }
    out
}

fn append_encoded_output_paths<F>(
    expr: &TemplateExpr,
    resolve_paths: &mut F,
    out: &mut BTreeSet<String>,
) where
    F: FnMut(&TemplateExpr) -> BTreeSet<String>,
{
    match expr.deparen() {
        TemplateExpr::Call { function, args } => {
            if function == "b64enc" {
                for arg in args {
                    out.extend(resolve_paths(arg));
                }
            }
            for arg in args {
                append_encoded_output_paths(arg, resolve_paths, out);
            }
        }
        TemplateExpr::Pipeline(stages) => {
            append_pipeline_encoded_output_paths(stages, resolve_paths, out);
        }
        TemplateExpr::Selector { operand, .. } | TemplateExpr::Parenthesized(operand) => {
            append_encoded_output_paths(operand, resolve_paths, out);
        }
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            append_encoded_output_paths(value, resolve_paths, out);
        }
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => {}
    }
}

fn append_pipeline_encoded_output_paths<F>(
    stages: &[TemplateExpr],
    resolve_paths: &mut F,
    out: &mut BTreeSet<String>,
) where
    F: FnMut(&TemplateExpr) -> BTreeSet<String>,
{
    let mut prefix: Vec<TemplateExpr> = Vec::new();
    for stage in stages {
        let current = stage.deparen();
        if let TemplateExpr::Call { function, args } = current {
            for arg in args {
                append_encoded_output_paths(arg, resolve_paths, out);
            }
            if function == "b64enc" {
                if !prefix.is_empty() {
                    let prefix_expr = if prefix.len() == 1 {
                        prefix[0].clone()
                    } else {
                        TemplateExpr::Pipeline(prefix.clone())
                    };
                    out.extend(resolve_paths(&prefix_expr));
                }
                for arg in args {
                    out.extend(resolve_paths(arg));
                }
            }
        } else {
            append_encoded_output_paths(current, resolve_paths, out);
        }
        prefix.push(stage.clone());
    }
}
