use std::collections::{HashMap, HashSet};

use crate::fragment_binding::FragmentBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::fragment_range_scope::{range_iterable_binding, range_variable_name};
use crate::helper_binding::HelperBinding;
use crate::helper_range_frame::RangeFrame;

pub(crate) struct HelperRangeBindingPlan {
    range_fragment_binding: Option<FragmentBinding>,
    range_helper_binding: Option<HelperBinding>,
    exact_iterations: Option<Vec<HelperRangeIteration>>,
    non_exact_variable_binding: Option<(String, FragmentBinding)>,
}

#[derive(Clone, Copy)]
pub(crate) enum NonExactRangeVariableBinding {
    Bind,
    Skip,
}

#[derive(Clone)]
pub(crate) struct HelperRangeIteration {
    pub(crate) helper_dot_binding: Option<HelperBinding>,
    pub(crate) fragment_dot_binding: Option<FragmentBinding>,
    pub(crate) variable_binding: Option<(String, FragmentBinding)>,
}

pub(crate) fn plan_helper_range_binding(
    header: &str,
    local_bindings: &HashMap<String, FragmentBinding>,
    current_dot_fragment: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    non_exact_variable_binding: NonExactRangeVariableBinding,
) -> HelperRangeBindingPlan {
    let range_fragment_binding =
        range_iterable_binding(header, local_bindings, current_dot_fragment, context, seen);
    let range_helper_binding = range_fragment_binding
        .as_ref()
        .and_then(FragmentBinding::to_helper_binding);

    let exact_iterations = if let Some(FragmentBinding::List(items)) = &range_fragment_binding {
        let range_variable = range_variable_name(header);
        Some(
            items
                .iter()
                .map(|item| HelperRangeIteration {
                    helper_dot_binding: item.to_helper_binding(),
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
        range_variable_name(header).and_then(|variable| {
            range_fragment_binding
                .as_ref()
                .and_then(FragmentBinding::item_binding)
                .map(|binding| (variable, binding))
        })
    } else {
        None
    };

    HelperRangeBindingPlan {
        range_fragment_binding,
        range_helper_binding,
        exact_iterations,
        non_exact_variable_binding,
    }
}

impl HelperRangeBindingPlan {
    pub(crate) fn range_fragment_binding(&self) -> Option<&FragmentBinding> {
        self.range_fragment_binding.as_ref()
    }

    pub(crate) fn take_non_exact_variable_binding(&mut self) -> Option<(String, FragmentBinding)> {
        self.non_exact_variable_binding.take()
    }

    pub(crate) fn apply_dot_binding(&self) -> bool {
        self.exact_iterations.is_none()
    }

    pub(crate) fn helper_value_body_dot(&self) -> Option<FragmentBinding> {
        self.range_helper_binding
            .as_ref()
            .and_then(HelperBinding::item_binding)
            .map(|binding| binding.to_fragment_binding())
    }

    pub(crate) fn fragment_output_body_dot(&self) -> Option<FragmentBinding> {
        self.range_fragment_binding
            .as_ref()
            .and_then(FragmentBinding::item_binding)
    }

    pub(crate) fn helper_value_frame(&self) -> RangeFrame<HelperRangeIteration> {
        RangeFrame::new(
            self.range_helper_binding
                .as_ref()
                .is_some_and(HelperBinding::definitely_nonempty_iterable),
            self.exact_iterations.clone(),
        )
    }

    pub(crate) fn fragment_output_frame(&self) -> RangeFrame<HelperRangeIteration> {
        RangeFrame::new(
            self.range_fragment_binding
                .as_ref()
                .is_some_and(FragmentBinding::definitely_nonempty_iterable),
            self.exact_iterations.clone(),
        )
    }
}
