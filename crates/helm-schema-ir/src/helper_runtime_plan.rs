use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateHeader;

use crate::abstract_value::AbstractValue;
use crate::condition_action_plan::ConditionActionPlan;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_range_frame::RangeFrame;
use crate::helper_range_plan::{
    HelperRangeIteration, NonExactRangeVariableBinding, plan_helper_range_binding,
};
use crate::helper_runtime_guards::{branch_guard_paths_for_expr, truthy_predicate_for_paths};
use crate::range_action_plan::RangeActionPlan;
use crate::value_path_context::computed_with_body_fragment_value_expr;

#[derive(Clone, Copy)]
pub(crate) enum HelperRangeDotSource {
    HelperValue,
    FragmentValue,
}

#[derive(Clone, Copy)]
pub(crate) struct HelperRuntimeSemantics {
    pub(crate) apply_alternative_predicate: bool,
    pub(crate) non_exact_range_variable_binding: NonExactRangeVariableBinding,
    pub(crate) range_dot_source: HelperRangeDotSource,
}

#[derive(Clone)]
pub(crate) struct HelperConditionPlan {
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) action: ConditionActionPlan,
}

#[derive(Clone)]
pub(crate) struct HelperRangeRuntimePlan {
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) action: RangeActionPlan,
    pub(crate) frame: RangeFrame<HelperRangeIteration>,
    pub(crate) non_exact_variable_binding: Option<(String, AbstractValue)>,
    pub(crate) range_fragment_value: Option<AbstractValue>,
}

pub(crate) fn helper_if_condition_plan(
    header: &TemplateHeader,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    local_bindings: &HashMap<String, AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    semantics: HelperRuntimeSemantics,
) -> HelperConditionPlan {
    let guard_paths =
        helper_branch_guard_paths(header, bindings, current_dot, local_bindings, context, seen);
    HelperConditionPlan {
        action: ConditionActionPlan {
            predicate: truthy_predicate_for_paths(&guard_paths),
            bound_values: Vec::new(),
            dot_binding: None,
            apply_alternative_predicate: semantics.apply_alternative_predicate,
        },
        guard_paths,
    }
}

pub(crate) fn helper_with_condition_plan(
    header: &TemplateHeader,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    current_dot_fragment: Option<&AbstractValue>,
    local_bindings: &HashMap<String, AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    semantics: HelperRuntimeSemantics,
) -> HelperConditionPlan {
    let guard_paths =
        helper_branch_guard_paths(header, bindings, current_dot, local_bindings, context, seen);
    let body_dot = computed_with_body_fragment_value_expr(
        header.expr(),
        bindings,
        local_bindings,
        context,
        current_dot_fragment,
        current_dot,
    );
    HelperConditionPlan {
        action: ConditionActionPlan {
            predicate: truthy_predicate_for_paths(&guard_paths),
            bound_values: Vec::new(),
            dot_binding: body_dot,
            apply_alternative_predicate: semantics.apply_alternative_predicate,
        },
        guard_paths,
    }
}

pub(crate) fn helper_range_runtime_plan(
    header: Option<&TemplateHeader>,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    current_dot_fragment: Option<&AbstractValue>,
    local_bindings: &HashMap<String, AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    semantics: HelperRuntimeSemantics,
) -> HelperRangeRuntimePlan {
    let Some(header) = header else {
        return HelperRangeRuntimePlan {
            guard_paths: BTreeSet::new(),
            action: RangeActionPlan::empty(),
            frame: RangeFrame::unknown(),
            non_exact_variable_binding: None,
            range_fragment_value: None,
        };
    };

    let guard_paths =
        helper_branch_guard_paths(header, bindings, current_dot, local_bindings, context, seen);
    let mut range_plan = plan_helper_range_binding(
        header,
        local_bindings,
        current_dot_fragment,
        context,
        seen,
        semantics.non_exact_range_variable_binding,
    );
    let non_exact_variable_binding = range_plan.take_non_exact_variable_binding();
    let range_fragment_value = range_plan.range_fragment_value().cloned();
    let (frame, dot_binding) = match semantics.range_dot_source {
        HelperRangeDotSource::HelperValue => (
            range_plan.helper_value_frame(),
            range_plan.helper_value_body_dot(),
        ),
        HelperRangeDotSource::FragmentValue => (
            range_plan.fragment_output_frame(),
            range_plan.fragment_output_body_dot(),
        ),
    };

    HelperRangeRuntimePlan {
        action: RangeActionPlan::dot_binding(dot_binding, range_plan.apply_dot_binding()),
        frame,
        guard_paths,
        non_exact_variable_binding,
        range_fragment_value,
    }
}

fn helper_branch_guard_paths(
    header: &TemplateHeader,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    local_bindings: &HashMap<String, AbstractValue>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> BTreeSet<String> {
    branch_guard_paths_for_expr(
        header.expr(),
        bindings,
        current_dot,
        local_bindings,
        context,
        seen,
    )
}
