use std::collections::{BTreeSet, HashMap};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_env::EvalEnv;
use crate::expr_eval::eval_expr;
use crate::expression_output_facts::encoded_output_paths_from_exprs;
use crate::helper_summary::HelperFragmentOutputUse;
use crate::predicate::Predicate;
use crate::template_expr_analysis::expr_contains_helper_call;
use crate::{ValueKind, YamlPath};

#[derive(Clone, Copy)]
pub(crate) struct HelperOutputExprContext<'a> {
    pub(crate) bindings: &'a HashMap<String, AbstractValue>,
    pub(crate) current_dot: Option<&'a AbstractValue>,
    pub(crate) relative_path: &'a YamlPath,
    pub(crate) kind: ValueKind,
    pub(crate) active_output_predicates: &'a BTreeSet<Predicate>,
    pub(crate) defaulted_paths: &'a BTreeSet<String>,
}

pub(crate) fn expression_output_use_is_keyed_map_projection(
    output: &HelperFragmentOutputUse,
    expression_base: &YamlPath,
) -> bool {
    let suffix = if output.relative_path.0.starts_with(&expression_base.0) {
        &output.relative_path.0[expression_base.0.len()..]
    } else {
        output.relative_path.0.as_slice()
    };
    !suffix.is_empty() && suffix.iter().all(|segment| !segment.ends_with("[*]"))
}

pub(crate) fn collect_output_uses_from_expr(
    expr: &TemplateExpr,
    context: HelperOutputExprContext<'_>,
    outputs: &mut Vec<HelperFragmentOutputUse>,
) {
    if expr_contains_helper_call(expr) {
        return;
    }

    let env = EvalEnv::from_helper_context(Some(context.bindings), context.current_dot);
    if let Some(value) = eval_expr(expr, &env).value {
        let encoded_paths = encoded_output_paths_from_exprs(std::slice::from_ref(expr), |expr| {
            eval_expr(expr, &env)
                .value
                .map(|value| {
                    value
                        .paths()
                        .into_iter()
                        .filter(|path| !path.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        });
        value.collect_output_uses_with_encoding(
            outputs,
            context.relative_path,
            context.kind,
            &encoded_paths,
            context.active_output_predicates,
            context.defaulted_paths,
        );
        return;
    }

    match expr {
        TemplateExpr::Call { args, .. } => {
            for arg in args {
                collect_output_uses_from_expr(arg, context, outputs);
            }
        }
        TemplateExpr::Selector { operand, .. } => {
            collect_output_uses_from_expr(operand, context, outputs);
        }
        TemplateExpr::Pipeline(stages) => {
            for stage in stages {
                collect_output_uses_from_expr(stage, context, outputs);
            }
        }
        TemplateExpr::Parenthesized(inner)
        | TemplateExpr::VariableDefinition { value: inner, .. }
        | TemplateExpr::Assignment { value: inner, .. } => {
            collect_output_uses_from_expr(inner, context, outputs);
        }
        TemplateExpr::Literal(_)
        | TemplateExpr::Field(_)
        | TemplateExpr::Variable(_)
        | TemplateExpr::Unknown(_) => {}
    }
}
