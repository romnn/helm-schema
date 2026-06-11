use std::collections::HashMap;

use crate::Guard;
use crate::binding::FragmentBinding;
use crate::bound_value_analysis::{GetBinding, extract_bound_values};
use crate::value_path_context::ValuePathContext;

pub(crate) struct ConditionActionPlan {
    pub(crate) guards: Vec<Guard>,
    pub(crate) bound_values: Vec<String>,
    pub(crate) dot_binding: Option<FragmentBinding>,
}

pub(crate) fn plan_if_condition(
    text: &str,
    value_path_context: &ValuePathContext<'_>,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
) -> ConditionActionPlan {
    ConditionActionPlan {
        guards: value_path_context.condition_guards(text),
        bound_values: extract_bound_values(text, range_domains, get_bindings),
        dot_binding: None,
    }
}

pub(crate) fn plan_with_condition(
    text: &str,
    value_path_context: &ValuePathContext<'_>,
    range_domains: &HashMap<String, Vec<String>>,
    get_bindings: &HashMap<String, GetBinding>,
) -> ConditionActionPlan {
    ConditionActionPlan {
        guards: value_path_context.with_condition_guards(text),
        bound_values: extract_bound_values(text, range_domains, get_bindings),
        dot_binding: value_path_context.with_body_fragment_binding(text),
    }
}
