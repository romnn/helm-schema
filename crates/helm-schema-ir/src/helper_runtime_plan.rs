use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateHeader;

use crate::abstract_value::AbstractValue;
use crate::condition_action_plan::ConditionActionPlan;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_range_frame::RangeFrame;
use crate::helper_range_plan::{
    HelperRangeIteration, NonExactRangeVariableBinding, plan_helper_range_binding,
};
use crate::helper_runtime_guards::{branch_condition_facts_for_expr, branch_guard_paths_for_expr};
use crate::helper_summary::{HelperOutputMeta, HelperSummary};
use crate::helper_walk_state::{HelperRuntimeControlState, HelperRuntimeLocals};
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

impl HelperConditionPlan {
    pub(crate) fn record_guard_paths_into(&self, analysis: &mut HelperSummary) {
        for path in &self.guard_paths {
            analysis.add_guard_path(path.clone());
        }
    }
}

#[derive(Clone)]
pub(crate) struct HelperRangeRuntimePlan {
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) action: RangeActionPlan,
    pub(crate) frame: RangeFrame<HelperRangeIteration>,
    pub(crate) non_exact_variable_binding: Option<(String, AbstractValue)>,
    pub(crate) range_fragment_value: Option<AbstractValue>,
}

pub(crate) struct ActivatedHelperRange {
    pub(crate) guard_paths: BTreeSet<String>,
    pub(crate) action: RangeActionPlan,
    pub(crate) range_fragment_value: Option<AbstractValue>,
}

impl HelperRangeRuntimePlan {
    pub(crate) fn activate(
        self,
        control: &mut HelperRuntimeControlState,
        locals: &mut HelperRuntimeLocals,
    ) -> ActivatedHelperRange {
        control.extend_truthy_predicates(self.guard_paths.iter().cloned());
        if let Some((variable, binding)) = self.non_exact_variable_binding {
            locals.bindings.insert(variable, binding);
        }
        control.push_range_frame(self.frame);

        ActivatedHelperRange {
            guard_paths: self.guard_paths,
            action: self.action,
            range_fragment_value: self.range_fragment_value,
        }
    }
}

impl ActivatedHelperRange {
    pub(crate) fn record_guard_paths_into(&self, analysis: &mut HelperSummary) {
        for path in &self.guard_paths {
            analysis.add_guard_path(path.clone());
        }
    }
}

pub(crate) fn helper_if_condition_plan(
    header: &TemplateHeader,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    local_bindings: &HashMap<String, AbstractValue>,
    local_default_paths: &HashMap<String, BTreeSet<String>>,
    local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    semantics: HelperRuntimeSemantics,
) -> HelperConditionPlan {
    let facts = branch_condition_facts_for_expr(
        header.expr(),
        bindings,
        current_dot,
        local_bindings,
        local_default_paths,
        local_output_meta,
        context,
        seen,
    );
    HelperConditionPlan {
        action: ConditionActionPlan {
            predicate: facts.predicate,
            bound_values: Vec::new(),
            dot_binding: None,
            apply_alternative_predicate: semantics.apply_alternative_predicate,
        },
        guard_paths: facts.guard_paths,
    }
}

pub(crate) fn helper_with_condition_plan(
    header: &TemplateHeader,
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
    current_dot_fragment: Option<&AbstractValue>,
    local_bindings: &HashMap<String, AbstractValue>,
    local_default_paths: &HashMap<String, BTreeSet<String>>,
    local_output_meta: &HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    semantics: HelperRuntimeSemantics,
) -> HelperConditionPlan {
    let facts = branch_condition_facts_for_expr(
        header.expr(),
        bindings,
        current_dot,
        local_bindings,
        local_default_paths,
        local_output_meta,
        context,
        seen,
    );
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
            predicate: facts.predicate,
            bound_values: Vec::new(),
            dot_binding: body_dot,
            apply_alternative_predicate: semantics.apply_alternative_predicate,
        },
        guard_paths: facts.guard_paths,
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

    let guard_paths = branch_guard_paths_for_expr(
        header.expr(),
        bindings,
        current_dot,
        local_bindings,
        context,
        seen,
    );
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

#[cfg(test)]
#[path = "tests/helper_runtime_plan.rs"]
mod tests;
