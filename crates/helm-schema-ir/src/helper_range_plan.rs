use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateHeader;

use crate::abstract_value::AbstractValue;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::fragment_range_scope::{range_iterable_binding_expr, range_variable_name_expr};
use crate::helper_range_frame::RangeFrame;

pub(crate) struct HelperRangeBindingPlan {
    range_fragment_value: Option<AbstractValue>,
    range_helper_value: Option<AbstractValue>,
    exact_iterations: Option<Vec<HelperRangeIteration>>,
    non_exact_variable_binding: Option<(String, AbstractValue)>,
}

#[derive(Clone, Copy)]
pub(crate) enum NonExactRangeVariableBinding {
    Bind,
    Skip,
}

#[derive(Clone)]
pub(crate) struct HelperRangeIteration {
    pub(crate) helper_dot_binding: Option<AbstractValue>,
    pub(crate) fragment_dot_binding: Option<AbstractValue>,
    pub(crate) variable_binding: Option<(String, AbstractValue)>,
}

pub(crate) fn plan_helper_range_binding(
    header: &TemplateHeader,
    local_bindings: &HashMap<String, AbstractValue>,
    current_dot_fragment: Option<&AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    non_exact_variable_binding: NonExactRangeVariableBinding,
) -> HelperRangeBindingPlan {
    let range_fragment_value = range_iterable_binding_expr(
        header.expr(),
        local_bindings,
        current_dot_fragment,
        context,
        seen,
    );
    let range_helper_value = range_fragment_value
        .as_ref()
        .map(AbstractValue::to_context_value);

    let exact_iterations = if let Some(AbstractValue::List(items)) = &range_fragment_value {
        let range_variable = range_variable_name_expr(header.expr());
        Some(
            items
                .iter()
                .map(|item| HelperRangeIteration {
                    helper_dot_binding: Some(item.to_context_value()),
                    fragment_dot_binding: Some(item.clone()),
                    variable_binding: range_variable
                        .as_ref()
                        .map(|variable| (variable.clone(), item.clone())),
                })
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };

    let should_bind_non_exact_variable = matches!(
        non_exact_variable_binding,
        NonExactRangeVariableBinding::Bind
    );
    let non_exact_variable_binding = if exact_iterations.is_none() && should_bind_non_exact_variable
    {
        range_variable_name_expr(header.expr()).zip(
            range_fragment_value
                .as_ref()
                .and_then(AbstractValue::fragment_range_item)
                .map(|binding| binding.to_context_value()),
        )
    } else {
        None
    };

    HelperRangeBindingPlan {
        range_fragment_value,
        range_helper_value,
        exact_iterations,
        non_exact_variable_binding,
    }
}

impl HelperRangeBindingPlan {
    pub(crate) fn range_fragment_value(&self) -> Option<&AbstractValue> {
        self.range_fragment_value.as_ref()
    }

    pub(crate) fn take_non_exact_variable_binding(&mut self) -> Option<(String, AbstractValue)> {
        self.non_exact_variable_binding.take()
    }

    pub(crate) fn apply_dot_binding(&self) -> bool {
        self.exact_iterations.is_none()
    }

    pub(crate) fn helper_value_body_dot(&self) -> Option<AbstractValue> {
        self.range_helper_value
            .as_ref()
            .and_then(AbstractValue::helper_range_item)
            .map(|binding| binding.to_context_value())
    }

    pub(crate) fn fragment_output_body_dot(&self) -> Option<AbstractValue> {
        self.range_fragment_value
            .as_ref()
            .and_then(AbstractValue::fragment_range_item)
            .map(|binding| binding.to_context_value())
    }

    pub(crate) fn helper_value_frame(&self) -> RangeFrame<HelperRangeIteration> {
        RangeFrame::new(
            self.range_helper_value
                .as_ref()
                .is_some_and(AbstractValue::definitely_nonempty_iterable),
            self.exact_iterations.clone(),
        )
    }

    pub(crate) fn fragment_output_frame(&self) -> RangeFrame<HelperRangeIteration> {
        RangeFrame::new(
            self.range_fragment_value
                .as_ref()
                .is_some_and(AbstractValue::definitely_nonempty_iterable),
            self.exact_iterations.clone(),
        )
    }
}
