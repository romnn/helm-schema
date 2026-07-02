use helm_schema_ast::TemplateHeader;

use crate::abstract_value::AbstractValue;
use crate::value_path_context::ValuePathContext;
use helm_schema_core::Predicate;

#[derive(Clone)]
pub(crate) struct ConditionActionPlan {
    pub(crate) predicate: Predicate,
    pub(crate) bound_values: Vec<String>,
    pub(crate) dot_binding: Option<AbstractValue>,
}

pub(crate) fn plan_if_condition(
    header: &TemplateHeader,
    value_path_context: &ValuePathContext<'_>,
) -> ConditionActionPlan {
    ConditionActionPlan {
        predicate: value_path_context.condition_predicate_expr(header.expr()),
        bound_values: value_path_context.bound_output_paths_expr(header.expr()),
        dot_binding: None,
    }
}

pub(crate) fn plan_with_condition(
    header: &TemplateHeader,
    value_path_context: &ValuePathContext<'_>,
) -> ConditionActionPlan {
    ConditionActionPlan {
        predicate: value_path_context.with_condition_predicate_expr(header.expr()),
        bound_values: value_path_context.bound_output_paths_expr(header.expr()),
        dot_binding: value_path_context.with_body_fragment_value_expr(header.expr()),
    }
}
