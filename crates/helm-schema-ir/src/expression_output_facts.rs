use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::TemplateExpr;

use crate::expression_analysis::{
    resolved_string_transform_paths_for_exprs_with_fragment_locals,
    resolved_type_hint_paths_for_exprs_with_fragment_locals,
};
use crate::helper_summary::{HelperOutputMeta, insert_type_hint};
use crate::literal_schema_type::expression_schema_type;
use crate::value_path_context::ValuePathContext;

pub(crate) struct DocumentExpressionOutputFacts {
    pub(crate) values: BTreeSet<String>,
    pub(crate) default_fallback_values: BTreeSet<String>,
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(crate) encoded_output_values: BTreeSet<String>,
    pub(crate) local_output_meta: BTreeMap<String, HelperOutputMeta>,
}

impl DocumentExpressionOutputFacts {
    pub(crate) fn collect(
        exprs: &[TemplateExpr],
        value_path_context: &ValuePathContext<'_>,
    ) -> Self {
        let path_facts = value_path_context.expression_path_facts(exprs);
        let type_hints = collect_document_type_hints(exprs, value_path_context);
        let encoded_output_values = encoded_output_paths_from_exprs(exprs, |expr| {
            value_path_context.resolve_expr_to_values_paths(expr)
        });
        let local_output_meta = value_path_context.local_alias_output_meta_for_exprs(exprs);
        Self {
            values: path_facts.values,
            default_fallback_values: path_facts.default_fallback_values,
            type_hints,
            encoded_output_values,
            local_output_meta,
        }
    }
}

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

fn collect_document_type_hints(
    exprs: &[TemplateExpr],
    value_path_context: &ValuePathContext<'_>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut hints = resolved_type_hint_paths_for_exprs_with_fragment_locals(
        exprs,
        Some(value_path_context.root_bindings),
        value_path_context.current_dot_binding.as_ref(),
        value_path_context.template_bindings,
    );
    for (path, schema_types) in resolved_string_transform_paths_for_exprs_with_fragment_locals(
        exprs,
        Some(value_path_context.root_bindings),
        value_path_context.current_dot_binding.as_ref(),
        value_path_context.template_bindings,
    ) {
        for schema_type in schema_types {
            insert_type_hint(&mut hints, path.clone(), &schema_type);
        }
    }

    for expr in exprs {
        expr.walk(|node| match node {
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                let Some(schema_type) = expression_schema_type(&args[0]) else {
                    return;
                };
                for path in value_path_context.resolve_expr_to_values_paths(&args[1]) {
                    insert_type_hint(&mut hints, path, schema_type);
                }
            }
            TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
                for window in stages.windows(2) {
                    let Some(schema_type) = pipeline_default_expression_schema_type(&window[1])
                    else {
                        continue;
                    };
                    for path in value_path_context.resolve_expr_to_values_paths(&window[0]) {
                        insert_type_hint(&mut hints, path, schema_type);
                    }
                }
            }
            _ => {}
        });
    }

    hints
}

fn pipeline_default_expression_schema_type(expr: &TemplateExpr) -> Option<&'static str> {
    let TemplateExpr::Call { function, args } = expr.deparen() else {
        return None;
    };
    if function != "default" {
        return None;
    }
    args.first().and_then(expression_schema_type)
}
