use std::collections::HashMap;

use helm_schema_ast::TemplateHeader;

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::{GetBinding, extract_bound_values_expr};
use crate::predicate::Predicate;
use crate::value_path_context::ValuePathContext;

#[derive(Clone)]
pub(crate) struct ConditionActionPlan {
    pub(crate) predicate: Predicate,
    pub(crate) bound_values: Vec<String>,
    pub(crate) dot_binding: Option<AbstractValue>,
    pub(crate) apply_alternative_predicate: bool,
}

impl ConditionActionPlan {
    pub(crate) fn contract_guards(&self) -> Vec<crate::Guard> {
        self.predicate.contract_guards()
    }
}

pub(crate) fn plan_if_condition(
    header: &TemplateHeader,
    value_path_context: &ValuePathContext<'_>,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
) -> ConditionActionPlan {
    ConditionActionPlan {
        predicate: value_path_context.condition_predicate_expr(header.expr()),
        bound_values: extract_bound_values_expr(header.expr(), range_domains, get_bindings),
        dot_binding: None,
        apply_alternative_predicate: true,
    }
}

pub(crate) fn plan_with_condition(
    header: &TemplateHeader,
    value_path_context: &ValuePathContext<'_>,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
) -> ConditionActionPlan {
    ConditionActionPlan {
        predicate: value_path_context.with_condition_predicate_expr(header.expr()),
        bound_values: extract_bound_values_expr(header.expr(), range_domains, get_bindings),
        dot_binding: value_path_context.with_body_fragment_value_expr(header.expr()),
        apply_alternative_predicate: true,
    }
}
