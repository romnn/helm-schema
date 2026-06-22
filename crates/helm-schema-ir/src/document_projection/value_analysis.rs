use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::TemplateExpr;

use crate::ValueKind;
use crate::bound_value_analysis::{GetBinding, extract_bound_values_from_exprs};
use crate::expression_analysis::resolved_string_transform_paths_for_exprs_with_fragment_locals;
use crate::expression_analysis::resolved_type_hint_paths_for_exprs_with_fragment_locals;
use crate::helper_summary::HelperOutputMeta;
use crate::helper_summary_mutation::insert_type_hint;
use crate::literal_schema_type::expression_schema_type;
use crate::output_path;
use crate::value_path_context::ValuePathContext;

pub(crate) struct DocumentExpressionFacts {
    pub(super) default_fallback_values: BTreeSet<String>,
    pub(super) values: BTreeSet<String>,
    pub(super) encoded_output_values: BTreeSet<String>,
    pub(super) type_hints: BTreeMap<String, BTreeSet<String>>,
    pub(super) local_output_meta: BTreeMap<String, HelperOutputMeta>,
    pub(super) bound_values: Vec<String>,
}

pub(crate) fn collect_document_expression_facts(
    exprs: &[TemplateExpr],
    kind: ValueKind,
    value_path_context: &ValuePathContext<'_>,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
) -> DocumentExpressionFacts {
    let default_fallback_values =
        value_path_context.resolved_default_fallback_paths_in_exprs(exprs);
    let mut values: BTreeSet<String> = value_path_context
        .resolved_values_paths_in_exprs(exprs)
        .into_iter()
        .collect();
    let type_hints = collect_document_type_hints(exprs, value_path_context);
    let encoded_output_values = collect_encoded_output_values(exprs, value_path_context);
    let local_output_meta = value_path_context.local_alias_output_meta_for_exprs(exprs);
    values.extend(default_fallback_values.iter().cloned());
    if kind == ValueKind::Scalar {
        let all_values = values.clone();
        values.retain(|path| !output_path::values_path_has_descendant(path, &all_values));
    }

    let bound_values = extract_bound_values_from_exprs(exprs, range_domains, get_bindings);

    DocumentExpressionFacts {
        default_fallback_values,
        values,
        encoded_output_values,
        type_hints,
        local_output_meta,
        bound_values,
    }
}

fn collect_encoded_output_values(
    exprs: &[TemplateExpr],
    value_path_context: &ValuePathContext<'_>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for expr in exprs {
        collect_encoded_output_values_from_expr(expr, value_path_context, &mut out);
    }
    out
}

fn collect_encoded_output_values_from_expr(
    expr: &TemplateExpr,
    value_path_context: &ValuePathContext<'_>,
    out: &mut BTreeSet<String>,
) {
    match expr.deparen() {
        TemplateExpr::Call { function, args } => {
            if function == "b64enc" {
                for arg in args {
                    out.extend(value_path_context.resolve_expr_to_values_paths(arg));
                }
            }
            for arg in args {
                collect_encoded_output_values_from_expr(arg, value_path_context, out);
            }
        }
        TemplateExpr::Pipeline(stages) => {
            collect_pipeline_encoded_output_values(stages, value_path_context, out);
        }
        TemplateExpr::Selector { operand, .. } | TemplateExpr::Parenthesized(operand) => {
            collect_encoded_output_values_from_expr(operand, value_path_context, out);
        }
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            collect_encoded_output_values_from_expr(value, value_path_context, out);
        }
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => {}
    }
}

fn collect_pipeline_encoded_output_values(
    stages: &[TemplateExpr],
    value_path_context: &ValuePathContext<'_>,
    out: &mut BTreeSet<String>,
) {
    let mut prefix: Vec<TemplateExpr> = Vec::new();
    for stage in stages {
        let current = stage.deparen();
        if let TemplateExpr::Call { function, args } = current {
            for arg in args {
                collect_encoded_output_values_from_expr(arg, value_path_context, out);
            }
            if function == "b64enc" {
                if !prefix.is_empty() {
                    let prefix_expr = if prefix.len() == 1 {
                        prefix[0].clone()
                    } else {
                        TemplateExpr::Pipeline(prefix.clone())
                    };
                    out.extend(value_path_context.resolve_expr_to_values_paths(&prefix_expr));
                }
                for arg in args {
                    out.extend(value_path_context.resolve_expr_to_values_paths(arg));
                }
            }
        } else {
            collect_encoded_output_values_from_expr(current, value_path_context, out);
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    use test_util::prelude::sim_assert_eq;

    use helm_schema_ast::DefineIndex;
    use helm_schema_ast::parse_action_expressions;

    use super::collect_document_expression_facts;
    use crate::ValueKind;
    use crate::abstract_value::AbstractValue;
    use crate::define_body_cache::DefineBodyCache;
    use crate::fragment_expr_eval::FragmentEvalContext;
    use crate::helper_summary::HelperSummaryCache;
    use crate::value_path_context::ValuePathContext;

    fn empty_fragment_context<'a>(
        defines: &'a DefineIndex,
        define_bodies: &'a DefineBodyCache,
        helper_summaries: &'a HelperSummaryCache,
    ) -> FragmentEvalContext<'a> {
        FragmentEvalContext::new(defines, define_bodies, helper_summaries)
    }

    #[test]
    fn document_type_hints_resolve_template_local_aliases() {
        let exprs = parse_action_expressions("{{ $port | b64enc | quote }}");
        let root_bindings = HashMap::new();
        let template_bindings = HashMap::from([(
            "port".to_string(),
            AbstractValue::choice(vec![
                AbstractValue::ValuesPath("global.service.port".to_string()),
                AbstractValue::ValuesPath("service.port".to_string()),
            ])
            .expect("choice has paths"),
        )]);
        let template_default_paths = HashMap::new();
        let template_output_meta = HashMap::new();
        let defines = DefineIndex::new();
        let define_bodies = DefineBodyCache::new(&defines);
        let helper_summaries = HelperSummaryCache::new();
        let context = ValuePathContext {
            root_bindings: &root_bindings,
            template_bindings: &template_bindings,
            template_default_paths: &template_default_paths,
            template_output_meta: &template_output_meta,
            fragment_context: empty_fragment_context(&defines, &define_bodies, &helper_summaries),
            current_dot_fragment: None,
            current_dot_binding: None,
        };

        let analysis = collect_document_expression_facts(
            &exprs,
            ValueKind::Scalar,
            &context,
            &HashMap::new(),
            &HashMap::new(),
        );

        sim_assert_eq!(
            have: analysis.type_hints,
            want: BTreeMap::from([
                (
                    "global.service.port".to_string(),
                    BTreeSet::from(["string".to_string()])
                ),
                (
                    "service.port".to_string(),
                    BTreeSet::from(["string".to_string()])
                )
            ])
        );
        sim_assert_eq!(
            have: analysis.encoded_output_values,
            want: BTreeSet::from([
                "global.service.port".to_string(),
                "service.port".to_string()
            ])
        );
    }
}
